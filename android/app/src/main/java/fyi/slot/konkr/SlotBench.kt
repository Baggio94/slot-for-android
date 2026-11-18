package fyi.slot.konkr

import android.os.Handler
import android.os.Looper
import android.util.Log

/**
 * The shelf's bench (2.5.0-beta37): with `adb shell setprop debug.slot.bench 1`
 * (an app can't set that, only adb), slot. scrolls the carousel, changes
 * shelves and opens menus by itself, then says in its log how its frames
 * went (frame_watch.rs). It only moves and looks: no game goes in, nothing
 * is chosen, no setting changes. tools/slot-bench.sh runs it on each
 * handheld plugged in and keeps the report.
 */
internal class SlotBench(private val activity: MainActivity, private val canRun: () -> Boolean) {
    companion object {
        private const val TAG = "SlotKonkr"
        private const val PROPERTY = "debug.slot.bench"
        private const val RIGHT = 22
        private const val LEFT = 21
        private const val UP = 19
        private const val DOWN = 20
        private const val START = 108
        private const val SELECT = 109
        private const val B = 97
        private const val L1 = 102
        private const val R1 = 103
        /** Once each time the property goes to 1. */
        @Volatile private var ran = false
    }

    private val handler = Handler(Looper.getMainLooper())

    /** A step: after [wait] ms, the key [code] goes down or up. */
    private class Step(val wait: Long, val code: Int, val down: Boolean)

    private fun steps(): List<Step> = buildList {
        fun tap(code: Int, gap: Long) { add(Step(gap, code, true)); add(Step(40, code, false)) }
        fun hold(code: Int, ms: Long) { add(Step(800, code, true)); add(Step(ms, code, false)) }
        // Out of any page first: B on the shelf does nothing.
        repeat(3) { tap(B, 300) }
        repeat(30) { tap(RIGHT, 100) }
        repeat(30) { tap(LEFT, 100) }
        hold(RIGHT, 2_500)
        hold(LEFT, 2_500)
        repeat(6) { tap(DOWN, 400) }
        repeat(6) { tap(UP, 400) }
        repeat(6) { tap(R1, 700) }
        repeat(6) { tap(L1, 700) }
        tap(START, 1_000)
        repeat(7) { tap(DOWN, 250) }
        repeat(7) { tap(UP, 250) }
        tap(B, 600)
        tap(B, 600)
        tap(SELECT, 1_000)
        tap(B, 900)
        tap(B, 900)
    }

    /** At each resume: starts when the property is 1 (read off the UI thread). */
    fun maybeStart() {
        Thread({
            val wanted = try {
                val process = ProcessBuilder("getprop", PROPERTY).redirectErrorStream(true).start()
                val text = process.inputStream.bufferedReader().use { it.readText() }.trim()
                process.waitFor()
                text == "1"
            } catch (_: Exception) { false }
            // Back to 0 (tools/slot-bench.sh's end): the next 1 runs it again.
            if (!wanted) { ran = false; return@Thread }
            if (ran) return@Thread
            ran = true
            handler.postDelayed({ run(steps(), 0) }, 4_000L)
        }, "slot-bench").start()
    }

    private fun run(steps: List<Step>, at: Int) {
        if (at == 0) {
            Log.i(TAG, "Slot bench: started")
            activity.nativeFrameWatchFlush()
        }
        if (at >= steps.size) {
            handler.postDelayed({
                activity.nativeFrameWatchFlush()
                Log.i(TAG, "Slot bench: done")
            }, 1_500L)
            return
        }
        if (!canRun()) {
            Log.i(TAG, "Slot bench: stopped (a game, or slot. in the background)")
            activity.nativeResetInput()
            return
        }
        val step = steps[at]
        handler.postDelayed({
            if (canRun()) activity.nativeKey(step.code, step.down)
            run(steps, at + 1)
        }, step.wait)
    }
}
