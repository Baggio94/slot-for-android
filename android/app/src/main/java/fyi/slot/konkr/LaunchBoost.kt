package fyi.slot.konkr

import android.content.Context
import android.os.Build
import android.os.PerformanceHintManager
import android.os.Process

/**
 * Asks Android for more CPU while a cart goes in (3.2 perf): Android's own
 * performance hint (ADPF) for slot.'s screen thread and the thread that
 * prepares the game: a work time reported well over the frame's target,
 * the public way to say the load just went up. Android 13 and later; elsewhere, or when
 * the device offers no hints, nothing happens and nothing changes.
 */
object LaunchBoost {
    private const val TAG = "SlotKonkr"
    private const val FRAME_NS = 16_666_667L
    private const val LOAD_UP_FRAMES = 4L
    private val tids = linkedSetOf<Int>()
    private var session: PerformanceHintManager.Session? = null

    /** The calling thread takes part (the screen's, the game loader). */
    @Synchronized
    fun join(context: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        if (!tids.add(Process.myTid())) return
        try {
            val manager = context.getSystemService(PerformanceHintManager::class.java) ?: return
            session?.close()
            session = manager.createHintSession(tids.toIntArray(), FRAME_NS)
        } catch (error: RuntimeException) {
            session = null
            SlotLog.w(TAG, "Performance hints unavailable", error)
        }
    }

    /** A cart is going in: more CPU now, if Android offers it. */
    @Synchronized
    fun loadUp() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        try {
            session?.reportActualWorkDuration(FRAME_NS * LOAD_UP_FRAMES)
        } catch (error: RuntimeException) {
            session = null
        }
    }

    /** The thread that joined is gone (a new GL surface thread): start over. */
    @Synchronized
    fun reset() {
        try { session?.close() } catch (_: RuntimeException) { }
        session = null
        tids.clear()
    }
}
