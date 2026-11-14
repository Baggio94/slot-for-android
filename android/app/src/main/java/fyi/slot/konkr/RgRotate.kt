package fyi.slot.konkr

import android.animation.ValueAnimator
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.graphics.Color
import android.net.Uri
import android.os.BatteryManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.text.format.DateFormat
import android.util.Log
import android.view.Gravity
import android.view.InputDevice
import android.view.KeyEvent
import android.view.View
import android.view.WindowManager
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import java.util.Date

/**
 * Anbernic RG Rotate: a square screen that swivels shut over the controls.
 *
 * Its keys, read on Siham's Rotate (2026-11-14, getevent + logcat):
 * - middle button: KEYCODE_BACK from "retrogame_joypad". Android takes a hold
 *   of 0.5 s or more to go Home, which brings slot. back as the home app.
 * - round side button: KEYCODE_F10 from "retrogame_joypad". The system takes
 *   a hold of 0.5 s or more to switch launchers; slot. can't stop it.
 * - lid (hall sensor): KEYCODE_F12 from "gpio-keys", down when it closes and
 *   up when it opens.
 */
internal object RgRotate {
    private const val PAD = "retrogame_joypad"
    private const val LID = "gpio-keys"

    val device: Boolean by lazy {
        // ro.product.model "RG Rotate", maker "Anbernic" (Siham's, 2026-11-14).
        Build.MODEL.contains("RG Rotate", ignoreCase = true)
    }

    private fun from(event: KeyEvent, name: String): Boolean {
        val source = event.device ?: return false
        return !source.isVirtual && source.name.equals(name, ignoreCase = true)
    }

    /** The middle button: the slot. button, eject included. */
    fun middleKey(keyCode: Int, event: KeyEvent): Boolean =
        device && keyCode == KeyEvent.KEYCODE_BACK && from(event, PAD)

    /** The round side button: the slot. button's taps only. */
    fun sideKey(keyCode: Int, event: KeyEvent): Boolean =
        device && keyCode == KeyEvent.KEYCODE_F10 && from(event, PAD)

    fun lidKey(keyCode: Int, event: KeyEvent): Boolean =
        device && keyCode == KeyEvent.KEYCODE_F12 && from(event, LID)

    /** Keys only reachable with the screen open: one means the lid is up. */
    fun opensLid(keyCode: Int, event: KeyEvent): Boolean =
        event.isFromSource(InputDevice.SOURCE_GAMEPAD) && keyCode in setOf(
            KeyEvent.KEYCODE_DPAD_UP, KeyEvent.KEYCODE_DPAD_DOWN,
            KeyEvent.KEYCODE_DPAD_LEFT, KeyEvent.KEYCODE_DPAD_RIGHT,
            KeyEvent.KEYCODE_BUTTON_A, KeyEvent.KEYCODE_BUTTON_B,
            KeyEvent.KEYCODE_BUTTON_X, KeyEvent.KEYCODE_BUTTON_Y,
            KeyEvent.KEYCODE_BUTTON_START, KeyEvent.KEYCODE_BUTTON_SELECT)
}

/**
 * The closed RG Rotate. The native shelf draws the cartridge (the game's,
 * seated, or a slowly drifting shelf); this adds the time and battery,
 * dims the screen, then lets Android put it to sleep.
 */
internal class LidCover(
    private val activity: MainActivity,
    frame: FrameLayout,
) {
    companion object {
        private const val DIM_AFTER_MS = 15_000L
        /** Screen off this long after the screen dims. */
        private const val SLEEP_AFTER_DIM_MS = 60_000L
        private const val STEP_MS = 6_000L
        private const val DIM_LEVEL = 0.03f
        private const val SAVED_TIMEOUT = "rotate_saved_screen_off_timeout"
        private const val ASKED_SETTINGS = "rotate_asked_write_settings"
    }

    var closed = false
        private set
    private val handler = Handler(Looper.getMainLooper())
    private var dimmer: ValueAnimator? = null
    private var screenReceiver: BroadcastReceiver? = null

    private val clock = TextView(activity).apply {
        typeface = SlotUi.font(activity)
        textSize = 56f
        letterSpacing = .04f
        setTextColor(Color.argb(235, 242, 242, 245))
        setShadowLayer(12f, 0f, 2f, Color.argb(200, 0, 0, 0))
        gravity = Gravity.CENTER_HORIZONTAL
    }
    private val battery = TextView(activity).apply {
        typeface = SlotUi.font(activity)
        textSize = 15f
        letterSpacing = .08f
        setTextColor(Color.argb(200, 242, 242, 245))
        setShadowLayer(8f, 0f, 1f, Color.argb(200, 0, 0, 0))
        gravity = Gravity.CENTER_HORIZONTAL
    }
    private val face = LinearLayout(activity).apply {
        orientation = LinearLayout.VERTICAL
        gravity = Gravity.CENTER_HORIZONTAL
        isClickable = false
        isFocusable = false
        visibility = View.GONE
        addView(clock)
        addView(battery)
    }

    init {
        frame.addView(face, FrameLayout.LayoutParams(-1, -2, Gravity.TOP).apply {
            topMargin = SlotUi.dp(activity, 36)
        })
        restoreTimeout()
    }

    private val tick = object : Runnable {
        override fun run() {
            if (!closed) return
            showTime()
            handler.postDelayed(this, 1_000L)
        }
    }

    private val step = object : Runnable {
        override fun run() {
            if (!closed) return
            if (!activity.nativeIsPlaying()) {
                activity.nativeLidStep()
                activity.wakeCarouselFor(1_500L)
            }
            handler.postDelayed(this, STEP_MS)
        }
    }

    private val dim = Runnable { if (closed) fade(DIM_LEVEL) }

    fun close() {
        if (closed) return
        closed = true
        activity.nativeLidCover(true)
        activity.wakeCarouselFor(1_500L)
        face.visibility = View.VISIBLE
        showTime()
        handler.post(tick)
        handler.postDelayed(step, STEP_MS)
        shortenTimeout()
        listenScreen()
        awake()
    }

    fun open() {
        if (!closed) return
        closed = false
        handler.removeCallbacks(tick)
        handler.removeCallbacks(step)
        handler.removeCallbacks(dim)
        dimmer?.cancel()
        setBrightness(WindowManager.LayoutParams.BRIGHTNESS_OVERRIDE_NONE)
        face.visibility = View.GONE
        stopListeningScreen()
        restoreTimeout()
        activity.nativeLidCover(false)
        activity.wakeCarouselFor(1_500L)
        askSettingsOnce()
    }

    /** A touch, or the screen back on: full brightness and a new countdown. */
    fun awake() {
        if (!closed) return
        handler.removeCallbacks(dim)
        dimmer?.cancel()
        setBrightness(WindowManager.LayoutParams.BRIGHTNESS_OVERRIDE_NONE)
        handler.postDelayed(dim, DIM_AFTER_MS)
    }

    fun onResume() {
        if (!closed) return
        face.visibility = View.VISIBLE
        handler.removeCallbacks(tick)
        handler.post(tick)
        awake()
    }

    fun onDestroy() {
        handler.removeCallbacksAndMessages(null)
        stopListeningScreen()
        restoreTimeout()
    }

    private fun showTime() {
        clock.text = DateFormat.getTimeFormat(activity).format(Date())
        val manager = activity.getSystemService(Context.BATTERY_SERVICE) as? BatteryManager
        val level = manager?.getIntProperty(BatteryManager.BATTERY_PROPERTY_CAPACITY) ?: -1
        val charging = manager?.isCharging == true
        battery.text = if (level in 0..100) {
            "$level%" + if (charging) "  CHARGING" else ""
        } else ""
    }

    private fun fade(to: Float) {
        dimmer?.cancel()
        val from = activity.window.attributes.screenBrightness.let { if (it < 0f) 1f else it }
        dimmer = ValueAnimator.ofFloat(from, to).apply {
            duration = 1_200L
            addUpdateListener { setBrightness(it.animatedValue as Float) }
            start()
        }
    }

    private fun setBrightness(value: Float) {
        val params = activity.window.attributes
        if (params.screenBrightness == value) return
        params.screenBrightness = value
        activity.window.attributes = params
    }

    private fun prefs() = activity.getSharedPreferences(MainActivity.PREFS_NAME, Context.MODE_PRIVATE)

    /** Android's own screen timeout, made to end the countdown: it turns the
     *  screen off this long after the last touch, as slot. can't itself. */
    private fun shortenTimeout() {
        if (!Settings.System.canWrite(activity)) return
        runCatching {
            val resolver = activity.contentResolver
            if (!prefs().contains(SAVED_TIMEOUT)) {
                val current = Settings.System.getInt(resolver, Settings.System.SCREEN_OFF_TIMEOUT)
                prefs().edit().putInt(SAVED_TIMEOUT, current).commit()
            }
            Settings.System.putInt(resolver, Settings.System.SCREEN_OFF_TIMEOUT,
                (DIM_AFTER_MS + SLEEP_AFTER_DIM_MS).toInt())
        }.onFailure { Log.w("SlotKonkr", "Rotate: screen timeout not changed", it) }
    }

    /** Puts the player's own timeout back; also after a crash while closed. */
    private fun restoreTimeout() {
        if (!prefs().contains(SAVED_TIMEOUT)) return
        if (!Settings.System.canWrite(activity)) return
        runCatching {
            Settings.System.putInt(activity.contentResolver, Settings.System.SCREEN_OFF_TIMEOUT,
                prefs().getInt(SAVED_TIMEOUT, 30 * 60_000))
            prefs().edit().remove(SAVED_TIMEOUT).apply()
        }.onFailure { Log.w("SlotKonkr", "Rotate: screen timeout not restored", it) }
    }

    private fun listenScreen() {
        if (screenReceiver != null) return
        val receiver = object : BroadcastReceiver() {
            override fun onReceive(context: Context, intent: Intent) {
                if (intent.action == Intent.ACTION_SCREEN_ON) awake()
            }
        }
        activity.registerReceiver(receiver, IntentFilter(Intent.ACTION_SCREEN_ON))
        screenReceiver = receiver
    }

    private fun stopListeningScreen() {
        screenReceiver?.let { runCatching { activity.unregisterReceiver(it) } }
        screenReceiver = null
    }

    /** The first time the Rotate is opened without the permission: ask for it
     *  once, so a closed Rotate can go to sleep on its own. */
    private fun askSettingsOnce() {
        if (Settings.System.canWrite(activity)) return
        if (prefs().getBoolean(ASKED_SETTINGS, false)) return
        prefs().edit().putBoolean(ASKED_SETTINGS, true).apply()
        handler.postDelayed({
            if (activity.isFinishing || activity.isDestroyed) return@postDelayed
            val dialog = SlotModalDialog(activity, "SLEEP WHEN CLOSED",
                "To let the RG Rotate dim and then sleep a minute after you close it, " +
                    "allow slot. to modify system settings. Your screen timeout comes back " +
                    "as soon as you open it.")
            dialog.addButton("NOT NOW") { dialog.dismiss() }
            dialog.addButton("ALLOW") {
                dialog.dismiss()
                runCatching {
                    activity.startActivity(Intent(Settings.ACTION_MANAGE_WRITE_SETTINGS,
                        Uri.parse("package:" + activity.packageName)))
                }
            }
            dialog.show()
        }, 600L)
    }
}
