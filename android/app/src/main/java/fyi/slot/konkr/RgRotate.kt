package fyi.slot.konkr

import android.animation.ValueAnimator
import android.app.admin.DevicePolicyManager
import android.content.ComponentName
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.util.Log
import android.view.InputDevice
import android.view.KeyEvent
import android.view.WindowManager

/**
 * Anbernic RG Rotate: a square screen that swivels shut over the controls.
 *
 * Its keys, read on Siham's Rotate (2026-11-14, getevent + logcat):
 * - middle button: KEYCODE_BACK from "retrogame_joypad". Android takes a hold
 *   of 0.5 s or more to go Home, which brings slot. back as the home app.
 * - round side button: KEYCODE_F10 from "retrogame_joypad". The system takes
 *   a hold of 0.5 s or more to switch launchers; slot. leaves it alone
 *   (Siham, 2026-11-14: a tap there inserted the cart before the switch).
 * - lid (hall sensor): KEYCODE_F12 from "gpio-keys", down when it closes and
 *   up when it opens.
 */
internal object RgRotate {
    private const val PAD = "retrogame_joypad"
    private const val LID = "gpio-keys"
    /** The middle button's hold, ejecting before Android's own at 0.5 s. */
    const val HOLD_MS = 400L

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
 * The closed RG Rotate. The native shelf draws the cover (the game's cartridge,
 * or the shelf), grows the status line's clock and battery and fades the
 * other icons; this dims the screen, then turns it off.
 *
 * Android never ends its screen timeout while the Rotate is shut: the lid is
 * a held key whose repeats count as user activity every 100 ms (measured on
 * Siham's Rotate, 2026-11-14). So slot. turns the screen off itself, with the
 * "Lock the screen" device admin right, asked once.
 */
internal class LidCover(
    private val activity: MainActivity,
    private val icons: () -> SlotStatusIndicators?,
) {
    companion object {
        private const val DIM_LEVEL = 0.03f
        /** A shortened screen timeout 2.4.0-menus6 may have left behind. */
        private const val SAVED_TIMEOUT = "rotate_saved_screen_off_timeout"
        private const val ASKED_ADMIN = "rotate_asked_device_admin"
        private const val STEP_MS = 6_000L
    }

    var closed = false
        private set
    private val handler = Handler(Looper.getMainLooper())
    private var dimmer: ValueAnimator? = null
    private var screenReceiver: BroadcastReceiver? = null
    private val admin = ComponentName(activity, SlotDeviceAdmin::class.java)
    private val policy get() = activity.getSystemService(DevicePolicyManager::class.java)

    init { restoreOldTimeout() }

    val canSleep: Boolean get() = policy?.isAdminActive(admin) == true

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

    private val sleep = Runnable {
        if (!closed) return@Runnable
        if (canSleep) {
            runCatching { policy?.lockNow() }
                .onFailure { Log.w("SlotKonkr", "Rotate: lockNow refused", it) }
        }
    }

    fun close() {
        if (closed) return
        closed = true
        activity.nativeLidCover(true)
        icons()?.fadeForLid(true, animations())
        activity.wakeCarouselFor(1_500L)
        handler.postDelayed(step, STEP_MS)
        listenScreen()
        awake()
    }

    fun open() {
        if (!closed) return
        closed = false
        handler.removeCallbacks(step)
        handler.removeCallbacks(dim)
        handler.removeCallbacks(sleep)
        dimmer?.cancel()
        setBrightness(WindowManager.LayoutParams.BRIGHTNESS_OVERRIDE_NONE)
        stopListeningScreen()
        activity.nativeLidCover(false)
        icons()?.fadeForLid(false, animations())
        activity.wakeCarouselFor(1_500L)
        askAdminOnce()
    }

    /** A touch, or the screen back on: full brightness and a new countdown,
     *  with the delays of Settings > Interface > RG Rotate. */
    fun awake() {
        if (!closed) return
        handler.removeCallbacks(dim)
        handler.removeCallbacks(sleep)
        dimmer?.cancel()
        setBrightness(WindowManager.LayoutParams.BRIGHTNESS_OVERRIDE_NONE)
        val config = activity.nativeRotateDelays()
        val (dimS, sleepS) = config[0] to config[1]
        // No game and the Black cover: straight to sleep.
        if (config[2] != 0 && !activity.nativeIsPlaying()) {
            handler.postDelayed(sleep, 800L)
            return
        }
        val dimMs = dimS * 1_000L
        handler.postDelayed(dim, dimMs)
        // 0: never sleep on its own.
        if (sleepS > 0) handler.postDelayed(sleep, dimMs + sleepS * 1_000L)
    }

    private fun animations() = activity.nativeRotateDelays().getOrElse(3) { 1 } != 0

    fun onResume() {
        activity.nativeRotateCanSleep(canSleep)
        if (closed) awake()
    }

    fun onDestroy() {
        handler.removeCallbacksAndMessages(null)
        stopListeningScreen()
    }

    /** Settings > Interface > RG Rotate > Sleep When Closed. */
    fun requestAdmin() {
        runCatching {
            activity.startActivity(Intent(DevicePolicyManager.ACTION_ADD_DEVICE_ADMIN)
                .putExtra(DevicePolicyManager.EXTRA_DEVICE_ADMIN, admin)
                .putExtra(DevicePolicyManager.EXTRA_ADD_EXPLANATION,
                    "Lets slot. turn the screen off once your closed RG Rotate has dimmed. " +
                        "You can turn it off again in Settings > Interface > RG Rotate."))
        }.onFailure { Log.w("SlotKonkr", "Rotate: device admin page refused", it) }
    }

    fun removeAdmin() {
        runCatching { policy?.removeActiveAdmin(admin) }
        handler.postDelayed({ activity.nativeRotateCanSleep(canSleep) }, 500L)
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

    /** 2.4.0-menus6 shortened Android's screen timeout while closed; put the
     *  player's own back if it was left behind (it can no longer write it
     *  without that permission: then just forget it). */
    private fun restoreOldTimeout() {
        if (!prefs().contains(SAVED_TIMEOUT)) return
        runCatching {
            if (Settings.System.canWrite(activity)) {
                Settings.System.putInt(activity.contentResolver, Settings.System.SCREEN_OFF_TIMEOUT,
                    prefs().getInt(SAVED_TIMEOUT, 30 * 60_000))
            }
        }
        prefs().edit().remove(SAVED_TIMEOUT).apply()
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

    /** The first time the Rotate is opened without the right: ask once. */
    private fun askAdminOnce() {
        if (canSleep) return
        if (prefs().getBoolean(ASKED_ADMIN, false)) return
        prefs().edit().putBoolean(ASKED_ADMIN, true).apply()
        handler.postDelayed({
            if (activity.isFinishing || activity.isDestroyed) return@postDelayed
            val dialog = SlotModalDialog(activity, "SLEEP WHEN CLOSED",
                "To let your RG Rotate go to sleep after you close it, slot. needs to be " +
                    "able to turn the screen off. Android will ask you to activate it.")
            dialog.addButton("NOT NOW") { dialog.dismiss() }
            dialog.addButton("ALLOW") {
                dialog.dismiss()
                requestAdmin()
            }
            dialog.show()
        }, 600L)
    }
}
