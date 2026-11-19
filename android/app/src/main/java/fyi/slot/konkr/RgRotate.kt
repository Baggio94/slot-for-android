package fyi.slot.konkr

import android.animation.ValueAnimator
import android.app.KeyguardManager
import android.app.admin.DevicePolicyManager
import android.content.ComponentName
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.hardware.display.DisplayManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.PowerManager
import android.os.SystemClock
import android.provider.Settings
import android.util.Log
import android.view.Display
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
 *
 * Asleep, Android hands the lid's repeats to the lock screen. With none
 * ("None" in Security), no window takes them and Android blames slot. after
 * 5 s ("isn't responding") and kills it (Siham's Rotate, 2026-11-14). With
 * Swipe, Anbernic lifts it when the lid opens: straight back to the game. So
 * slot. checks a lock screen came up; if not it turns the screen back on,
 * only dims from then on, and asks for Swipe.
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
        /** The Rotate slept without a lock screen: dim only until Swipe is set. */
        private const val NO_LOCK = "rotate_no_lock_screen"
        private const val ASKED_LOCK = "rotate_asked_lock_screen"
        private const val STEP_MS = 6_000L
        // Anbernic lifts the lock screen ~0.8 s after the lid opens (its
        // slideRunnable, measured on Siham's Rotate 2026-11-14): wait past it.
        private const val RELOCK_MS = 2_000L
        private const val OPENED_CHECK_MS = 1_200L
        private const val MAX_RELOCKS = 4
        /** Woken shut (power key), the cover shows this long, then sleeps. */
        private const val PEEK_MS = 8_000L
        /** Opened this soon after the screen came on from sleep: the lid's
         *  own opening woke it, straight back with no cover animation. */
        private const val WOKEN_OPEN_MS = 2_000L
        private const val SHUT_ROTATION = "rotate_shut_rotation"
        private const val OPEN_ROTATION = "rotate_open_rotation"
    }

    var closed = false
        private set
    /** The lid's sleep was requested. Android can call onResume while lockNow
     * is still removing the Activity window: do not regain the lock screen
     * or restart the countdown until the lid is physically opened. */
    private var sleepRequested = false
    /** slot. put the Rotate to sleep since the lid closed: the lock screen
     *  is under the cover until the lid really opens. */
    private var sleptShut = false
    /** When the screen came back on after that sleep (elapsedRealtime). */
    private var wokeAt = 0L
    private var relockAttempts = 0
    private val handler = Handler(Looper.getMainLooper())
    private var dimmer: ValueAnimator? = null
    private var screenReceiver: BroadcastReceiver? = null
    private var displayListener: DisplayManager.DisplayListener? = null
    private var displayOn = true
    /** The display's rotation with the lid open, and shut (learned from the
     *  lid's own key, kept): Android turns the display before it hands slot.
     *  the lid's key, held while it turns (videos, Siham 2026-11-19). */
    private var rotationWatcher: DisplayManager.DisplayListener? = null
    private var lastRotation = -1
    private val admin = ComponentName(activity, SlotDeviceAdmin::class.java)
    private val policy get() = activity.getSystemService(DevicePolicyManager::class.java)

    init {
        restoreOldTimeout()
        watchRotation()
    }

    private fun rotationNow(): Int =
        activity.getSystemService(DisplayManager::class.java)?.getDisplay(Display.DEFAULT_DISPLAY)?.rotation ?: -1

    /** Every turn of the display, timed; a turn to the shut rotation closes
     *  at once, before the lid's key comes (beta43). */
    private fun watchRotation() {
        if (!RgRotate.device || rotationWatcher != null) return
        val displays = activity.getSystemService(DisplayManager::class.java) ?: return
        lastRotation = rotationNow()
        val watcher = object : DisplayManager.DisplayListener {
            override fun onDisplayAdded(displayId: Int) {}
            override fun onDisplayRemoved(displayId: Int) {}
            override fun onDisplayChanged(displayId: Int) {
                if (displayId != Display.DEFAULT_DISPLAY) return
                val rotation = rotationNow()
                if (rotation == lastRotation || rotation < 0) return
                lastRotation = rotation
                val shut = prefs().getInt(SHUT_ROTATION, -1)
                val open = prefs().getInt(OPEN_ROTATION, -1)
                SlotLog.i("SlotKonkr", "Rotate: [${SystemClock.uptimeMillis()}] display turned to $rotation (shut $shut, open $open); lidClosed=$closed")
                if (!closed && shut >= 0 && shut != open && rotation == shut) {
                    SlotLog.i("SlotKonkr", "Rotate: [${SystemClock.uptimeMillis()}] closing on the display's turn, before the lid's key")
                    close()
                }
            }
        }
        runCatching { displays.registerDisplayListener(watcher, handler) }
            .onSuccess { rotationWatcher = watcher }
            .onFailure { SlotLog.w("SlotKonkr", "Rotate: rotation watcher refused", it) }
    }

    /** The lid's key said where it is: the display's rotation then is the
     *  shut (or open) one, read once the turn is done. */
    fun learnRotation(shut: Boolean) {
        handler.postDelayed({
            if (closed != shut) return@postDelayed
            val rotation = rotationNow()
            if (rotation < 0) return@postDelayed
            val key = if (shut) SHUT_ROTATION else OPEN_ROTATION
            if (prefs().getInt(key, -1) != rotation) {
                prefs().edit().putInt(key, rotation).apply()
                SlotLog.i("SlotKonkr", "Rotate: ${if (shut) "shut" else "open"} rotation is $rotation")
            }
        }, 1_500L)
    }

    val canSleep: Boolean get() = policy?.isAdminActive(admin) == true
    private val noLock: Boolean get() = prefs().getBoolean(NO_LOCK, false)
    private val keyguard get() = activity.getSystemService(KeyguardManager::class.java)

    /** Settings > Handheld (RG Rotate) > Sleep When Closed: 0 off, 1 on,
     *  2 waiting for a lock screen. */
    private fun sleepState() = when {
        !canSleep -> 0
        noLock -> 2
        else -> 1
    }

    /** Half a second after lockNow, a lock screen should be up. */
    private val checkLocked: Runnable = Runnable {
        if (!closed || keyguard?.isKeyguardLocked != false) return@Runnable
        SlotLog.w("SlotKonkr", "Rotate: [${SystemClock.uptimeMillis()}] no lock screen after lockNow; waking, dim only from now on")
        prefs().edit().putBoolean(NO_LOCK, true).remove(ASKED_LOCK).apply()
        sleepRequested = false
        handler.removeCallbacks(retrySleep)
        activity.nativeRotateSleepState(sleepState())
        // Back on before Android's 5 s: slot.'s window takes the lid's repeats.
        runCatching {
            @Suppress("DEPRECATION")
            activity.getSystemService(PowerManager::class.java)
                ?.newWakeLock(PowerManager.SCREEN_DIM_WAKE_LOCK or
                    PowerManager.ACQUIRE_CAUSES_WAKEUP, "slot:rotate-no-lock")
                ?.acquire(1_000L)
        }.onFailure { SlotLog.w("SlotKonkr", "Rotate: [${SystemClock.uptimeMillis()}] could not wake", it) }
        fade(0f)
    }

    /** Some Rotate firmware wakes the screen again on held F12 repeats.
     * Re-issue the lock only if the display really became interactive,
     * without ever putting slot. over the keyguard while the lid is shut.
     * A bounded retry avoids an infinite lock/wake loop. */
    private val retrySleep: Runnable = Runnable {
        if (!closed || !sleepRequested || !canSleep || noLock) return@Runnable
        if (activity.getSystemService(PowerManager::class.java)?.isInteractive != true) return@Runnable
        // Anbernic lifts the lock screen only when the lid opens: then the
        // lid's release went to the lock screen, not to slot.
        if (keyguard?.isKeyguardLocked == false) {
            SlotLog.i("SlotKonkr", "Rotate: [${SystemClock.uptimeMillis()}] lock screen lifted by the lid opening")
            open()
            return@Runnable
        }
        if (relockAttempts >= MAX_RELOCKS) {
            SlotLog.w("SlotKonkr", "Rotate: [${SystemClock.uptimeMillis()}] display keeps waking after lockNow; retries exhausted")
            fade(0f)
            return@Runnable
        }
        relockAttempts++
        SlotLog.i("SlotKonkr", "Rotate: [${SystemClock.uptimeMillis()}] screen woke with lid closed; lock retry $relockAttempts")
        activity.setShowWhenLocked(false)
        activity.window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        runCatching { policy?.lockNow() }
            .onFailure { SlotLog.w("SlotKonkr", "Rotate: [${SystemClock.uptimeMillis()}] repeated lockNow refused", it) }
        handler.removeCallbacks(checkLocked)
        handler.postDelayed(checkLocked, 500L)
    }

    /** The screen came back on after slot. put the shut Rotate to sleep:
     *  the power key, or the lid opening (Anbernic lifts the lock screen
     *  0.1-0.8 s later). The cover, fresh, over the lock screen meanwhile. */
    private fun wakeShut() {
        sleepRequested = false
        wokeAt = SystemClock.elapsedRealtime()
        handler.removeCallbacks(retrySleep)
        handler.removeCallbacks(checkLocked)
        activity.refreshSystemStatus()
        activity.wakeCarouselFor(1_500L)
        showOverLockScreen()
        handler.removeCallbacks(openedCheck)
        handler.postDelayed(openedCheck, OPENED_CHECK_MS)
        awake()
    }

    /** The screen went off shut without slot. putting it to sleep (the power
     *  key, Android's own timeout): the same hand-over as slot.'s sleep. The
     *  lock screen takes the focus and the lid's keys; left over it, slot.
     *  had the focus with no window, and the lid's keys made Android stop it
     *  (bg ANR, 455, Siham 2026-11-15). */
    private fun asleepShut() {
        if (!closed || sleepRequested || noLock) return
        SlotLog.i("SlotKonkr", "Rotate: [${SystemClock.uptimeMillis()}] screen off shut; the lock screen takes over")
        sleepRequested = true
        sleptShut = true
        relockAttempts = 0
        handler.removeCallbacks(dim)
        handler.removeCallbacks(sleep)
        dimmer?.cancel()
        activity.setShowWhenLocked(false)
        activity.window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
    }

    private fun scheduleRelock() {
        if (!closed || !sleepRequested || noLock) return
        handler.removeCallbacks(openedCheck)
        handler.postDelayed(openedCheck, OPENED_CHECK_MS)
        handler.removeCallbacks(retrySleep)
        handler.postDelayed(retrySleep, RELOCK_MS)
    }

    /** Woken after sleeping shut: if Anbernic already lifted the lock screen,
     *  the lid was opened (its release went to the lock screen). */
    private val openedCheck: Runnable = Runnable {
        if (!closed || !sleptShut) return@Runnable
        if (activity.getSystemService(PowerManager::class.java)?.isInteractive != true) return@Runnable
        if (keyguard?.isKeyguardLocked == false) {
            SlotLog.i("SlotKonkr", "Rotate: [${SystemClock.uptimeMillis()}] woke with the lock screen lifted: lid opened")
            open()
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

    private val dim: Runnable = Runnable { if (closed) fade(DIM_LEVEL) }

    private val sleep: Runnable = Runnable {
        if (!closed) return@Runnable
        if (canSleep && !noLock) {
            // Keep the keyguard in charge until F12 UP (actual opening).
            // On this firmware, lockNow briefly resumes MainActivity while
            // powering off. Showing over the keyguard in onResume or
            // ACTION_SCREEN_ON steals focus during that transition, leaving
            // no focused input window for the held F12 key -> ANR after 5 s.
            sleepRequested = true
            sleptShut = true
            relockAttempts = 0
            activity.setShowWhenLocked(false)
            activity.window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
            SlotLog.i("SlotKonkr", "Rotate: [${SystemClock.uptimeMillis()}] lockNow requested; lid remains closed")
            runCatching { policy?.lockNow() }
                .onFailure {
                    sleepRequested = false
                    SlotLog.w("SlotKonkr", "Rotate: [${SystemClock.uptimeMillis()}] lockNow refused", it)
                }
            handler.postDelayed(checkLocked, 500L)
        } else {
            // No sleep for slot. to give: as dark as the screen goes.
            fade(0f)
        }
    }

    fun close() {
        if (closed) return
        closed = true
        sleepRequested = false
        sleptShut = false
        relockAttempts = 0
        activity.window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        // Until slot. puts it to sleep, the cover stays over the lock screen
        // and keeps the focus (the lid's repeats come to slot.); it hands
        // over to the lock screen only right before lockNow.
        showOverLockScreen()
        SlotLog.i("SlotKonkr", "Rotate: [${SystemClock.uptimeMillis()}] lid closed; cover and sleep timer active")
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
        // The lid's opening woke it from sleep: no cover animation, straight
        // back to the game or the shelf (Siham 2026-11-14). Android's
        // screen-on can come after the opening itself (446 video D: 0.5 s
        // later): then slot. is still asleep here.
        val woken = sleptShut &&
            (sleepRequested || SystemClock.elapsedRealtime() - wokeAt < WOKEN_OPEN_MS)
        sleepRequested = false
        sleptShut = false
        relockAttempts = 0
        handler.removeCallbacks(retrySleep)
        handler.removeCallbacks(openedCheck)
        SlotLog.i("SlotKonkr", "Rotate: [${SystemClock.uptimeMillis()}] lid opened; restore previous session (woken=$woken)")
        handler.removeCallbacks(step)
        handler.removeCallbacks(dim)
        handler.removeCallbacks(sleep)
        handler.removeCallbacks(checkLocked)
        dimmer?.cancel()
        setBrightness(WindowManager.LayoutParams.BRIGHTNESS_OVERRIDE_NONE)
        stopListeningScreen()
        leaveLockScreen()
        if (woken) activity.nativeLidCoverNow(false) else activity.nativeLidCover(false)
        icons()?.fadeForLid(false, animations() && !woken)
        activity.wakeCarouselFor(1_500L)
        if (noLock) askLockOnce() else askAdminOnce()
    }

    /** Woken by the power key, slot. shows over a Swipe lock screen (the
     *  one slot. needs to sleep under), never over a PIN, pattern or password. */
    private fun showOverLockScreen() {
        // Never once slot. has put the lid to sleep: the lock screen must
        // own input focus throughout the lockNow transition and the sleep.
        if (!RgRotate.device || (closed && sleepRequested)) return
        activity.setShowWhenLocked(keyguard?.isDeviceSecure == false)
    }

    /** Anbernic lifts a Swipe lock screen as the lid opens, not on the power
     *  key: slot. lifts it itself, straight back to the game. A PIN keeps its
     *  own page. */
    private fun leaveLockScreen() {
        // Shut, the lock screen must keep the lid's repeats (never lift it).
        // Screen off (slot. started while the Rotate sleeps shut), lifting it
        // leaves no window focused: isn't responding (444, 2026-11-14).
        if (!RgRotate.device || closed || !interactive()) return
        showOverLockScreen()
        val guard = keyguard ?: return
        if (!guard.isKeyguardLocked || guard.isDeviceSecure) return
        runCatching { guard.requestDismissKeyguard(activity, null) }
            .onFailure { SlotLog.w("SlotKonkr", "Rotate: [${SystemClock.uptimeMillis()}] lock screen not lifted", it) }
    }

    /** Settings > Security, to set Swipe; the next sleep checks again. */
    fun openLockSettings() {
        prefs().edit().remove(NO_LOCK).apply()
        runCatching { activity.startActivity(Intent(Settings.ACTION_SECURITY_SETTINGS)) }
            .onFailure { SlotLog.w("SlotKonkr", "Rotate: [${SystemClock.uptimeMillis()}] security settings refused", it) }
        handler.postDelayed({ activity.nativeRotateSleepState(sleepState()) }, 300L)
    }

    /** A touch, or the screen back on: full brightness and a new countdown,
     *  with the delays of Settings > Handheld (RG Rotate). */
    fun awake() {
        if (!closed) return
        // A wake caused by the held lid sensor must never cancel the already
        // elapsed sleep deadline or restore full brightness.
        if (sleepRequested) {
            scheduleRelock()
            return
        }
        handler.removeCallbacks(dim)
        handler.removeCallbacks(sleep)
        dimmer?.cancel()
        setBrightness(WindowManager.LayoutParams.BRIGHTNESS_OVERRIDE_NONE)
        // Woken shut after a sleep (the power key): a look at the cover,
        // then back to sleep, like a phone.
        if (sleptShut && canSleep && !noLock) {
            handler.postDelayed(sleep, PEEK_MS)
            return
        }
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
        SlotLog.i("SlotKonkr", "Rotate: [${SystemClock.uptimeMillis()}] resumed; lidClosed=$closed sleeping=$sleepRequested")
        activity.nativeRotateSleepState(sleepState())
        if (closed) {
            // lockNow can invoke onResume between pause and display OFF.
            // Never steal focus from the keyguard once asleep. If the
            // firmware has really woken the display, schedule a bounded
            // lock retry. Before the sleep, the cover stays over it.
            // Asleep, slot. only comes back to the front once the lock screen
            // is going away (Anbernic lifting it as the lid opens). A swipe
            // on the lock screen while shut is not an opening (445 resumed
            // the game then).
            if (sleepRequested && interactive() && keyguard?.isKeyguardLocked == false) {
                SlotLog.i("SlotKonkr", "Rotate: [${SystemClock.uptimeMillis()}] resumed with the lock screen lifted: lid opened")
                open()
                return
            }
            if (sleepRequested) activity.setShowWhenLocked(false) else showOverLockScreen()
            when {
                sleepRequested -> scheduleRelock()
                noLock -> fade(0f)
                else -> awake()
            }
            SlotLog.i("SlotKonkr", "Rotate: [${SystemClock.uptimeMillis()}] resumed with lid closed; sleeping=$sleepRequested noLock=$noLock")
        } else {
            leaveLockScreen()
        }
    }

    private fun interactive() =
        activity.getSystemService(PowerManager::class.java)?.isInteractive == true

    fun onDestroy() {
        handler.removeCallbacksAndMessages(null)
        stopListeningScreen()
    }

    /** Settings > Handheld (RG Rotate) > Sleep When Closed. */
    fun requestAdmin() {
        runCatching {
            activity.startActivity(Intent(DevicePolicyManager.ACTION_ADD_DEVICE_ADMIN)
                .putExtra(DevicePolicyManager.EXTRA_DEVICE_ADMIN, admin)
                .putExtra(DevicePolicyManager.EXTRA_ADD_EXPLANATION,
                    "Lets slot. turn the screen off once your closed RG Rotate has dimmed. " +
                        "You can turn it off again in Settings > Handheld."))
        }.onFailure { SlotLog.w("SlotKonkr", "Rotate: [${SystemClock.uptimeMillis()}] device admin page refused", it) }
    }

    /** Settings > Handheld > Sleep When Closed, on to off: asked first, so a
     *  stray press can't take the sleep away (Siham 2026-11-15). */
    fun confirmRemoveAdmin() {
        if (activity.isFinishing || activity.isDestroyed) return
        val dialog = SlotModalDialog(activity, "SLEEP WHEN CLOSED",
            "Turn off Sleep When Closed? Your RG Rotate will only dim when you close it.")
        dialog.addButton("KEEP ON") { dialog.dismiss() }
        dialog.addButton("TURN OFF") {
            dialog.dismiss()
            removeAdmin()
        }
        dialog.show()
    }

    fun removeAdmin() {
        runCatching { policy?.removeActiveAdmin(admin) }
        handler.postDelayed({ activity.nativeRotateSleepState(sleepState()) }, 500L)
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
        listenDisplay()
        if (screenReceiver != null) return
        val receiver = object : BroadcastReceiver() {
            override fun onReceive(context: Context, intent: Intent) {
                if (intent.action == Intent.ACTION_USER_PRESENT) {
                    // The lock screen went away: asleep shut, that is
                    // Anbernic lifting it as the lid opens.
                    SlotLog.i("SlotKonkr", "Rotate: [${SystemClock.uptimeMillis()}] lock screen lifted; lidClosed=$closed sleeping=$sleepRequested")
                    if (closed && sleptShut) open()
                } else if (intent.action == Intent.ACTION_SCREEN_OFF) {
                    // Should the display listener miss it.
                    asleepShut()
                } else if (intent.action == Intent.ACTION_SCREEN_ON) {
                    // Screen-on alone is NOT evidence the lid opened: the
                    // power key wakes it shut too. Either way slot.'s cover
                    // shows at once over the lock screen (the screen is on,
                    // slot. keeps the focus: no ANR); the lid's release,
                    // or Anbernic lifting the lock screen, opens.
                    if (!closed) leaveLockScreen()
                    else if (sleepRequested) wakeShut()
                    else if (noLock) fade(0f)
                    else { showOverLockScreen(); awake() }
                    SlotLog.i("SlotKonkr", "Rotate: [${SystemClock.uptimeMillis()}] screen on; lidClosed=$closed sleeping=$sleepRequested noLock=$noLock")
                }
            }
        }
        activity.registerReceiver(receiver, IntentFilter(Intent.ACTION_SCREEN_ON).apply {
            addAction(Intent.ACTION_USER_PRESENT)
            addAction(Intent.ACTION_SCREEN_OFF)
        })
        screenReceiver = receiver
    }

    /** The display turning on, as it happens: Android's screen-on broadcast
     *  comes 0.3-0.65 s later, its lock screen's clock showing meanwhile
     *  (446 videos, Siham 2026-11-14). */
    private fun listenDisplay() {
        if (displayListener != null) return
        val displays = activity.getSystemService(DisplayManager::class.java) ?: return
        displayOn = displays.getDisplay(Display.DEFAULT_DISPLAY)?.state == Display.STATE_ON
        val listener = object : DisplayManager.DisplayListener {
            override fun onDisplayAdded(displayId: Int) {}
            override fun onDisplayRemoved(displayId: Int) {}
            override fun onDisplayChanged(displayId: Int) {
                if (displayId != Display.DEFAULT_DISPLAY) return
                val on = displays.getDisplay(displayId)?.state == Display.STATE_ON
                val turnedOn = on && !displayOn
                val turnedOff = !on && displayOn
                displayOn = on
                if (turnedOn && closed && sleepRequested) {
                    SlotLog.i("SlotKonkr", "Rotate: [${SystemClock.uptimeMillis()}] display on while asleep shut")
                    wakeShut()
                } else if (turnedOff) {
                    asleepShut()
                }
            }
        }
        runCatching { displays.registerDisplayListener(listener, handler) }
            .onSuccess { displayListener = listener }
            .onFailure { SlotLog.w("SlotKonkr", "Rotate: [${SystemClock.uptimeMillis()}] display listener refused", it) }
    }

    private fun stopListeningScreen() {
        displayListener?.let { listener ->
            runCatching { activity.getSystemService(DisplayManager::class.java)?.unregisterDisplayListener(listener) }
        }
        displayListener = null
        screenReceiver?.let { runCatching { activity.unregisterReceiver(it) } }
        screenReceiver = null
    }

    /** It slept with no lock screen: explain once, offer Security. */
    private fun askLockOnce() {
        if (prefs().getBoolean(ASKED_LOCK, false)) return
        prefs().edit().putBoolean(ASKED_LOCK, true).apply()
        handler.postDelayed({
            if (activity.isFinishing || activity.isDestroyed) return@postDelayed
            val dialog = SlotModalDialog(activity, "SLEEP WHEN CLOSED",
                "Your RG Rotate has no lock screen, so Android can't let it sleep while " +
                    "it is closed. Set Screen lock to Swipe (no code): you won't see it, " +
                    "opening the Rotate goes straight back to your game. Until then, slot. " +
                    "only dims the screen.")
            dialog.addButton("NOT NOW") { dialog.dismiss() }
            dialog.addButton("SETTINGS") {
                dialog.dismiss()
                openLockSettings()
            }
            dialog.show()
        }, 600L)
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
