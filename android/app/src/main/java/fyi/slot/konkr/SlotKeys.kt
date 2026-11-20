package fyi.slot.konkr

import android.view.KeyEvent

/**
 * A controller key as slot.'s menus read it, for the pages Android draws
 * (Apps, slot.'s dialogs, the RetroAchievements account). The shelf follows
 * Settings > Controls; these pages read Android's codes as they came, so a
 * player with A and B swapped launched an app with B (issue #123).
 */
internal object SlotKeys {
    /** A controller key with no slot. button: the page ignores it. */
    const val NONE = -1

    @JvmStatic private external fun nativeMenuKey(device: Int, code: Int): Int

    /**
     * The slot. button (its Android code) that `event`'s key is, as on the
     * shelf. Keys not from a controller (touch Back, a keyboard) and D-pad
     * keys Android made from a stick stay as they are.
     */
    fun menuCode(event: KeyEvent): Int {
        val code = event.keyCode
        val device = event.device ?: return code
        if (device.isVirtual || event.deviceId <= 0) return code
        if ((event.flags and KeyEvent.FLAG_FALLBACK) != 0) return code
        val dpad = code == KeyEvent.KEYCODE_DPAD_UP || code == KeyEvent.KEYCODE_DPAD_DOWN ||
            code == KeyEvent.KEYCODE_DPAD_LEFT || code == KeyEvent.KEYCODE_DPAD_RIGHT
        if (!dpad && !KeyEvent.isGamepadButton(code)) return code
        return try {
            nativeMenuKey(event.deviceId, code)
        } catch (_: UnsatisfiedLinkError) {
            code
        }
    }
}
