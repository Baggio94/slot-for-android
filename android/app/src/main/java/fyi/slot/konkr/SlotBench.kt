package fyi.slot.konkr

import android.os.Handler
import android.os.Looper

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
        /** The property's value the bench last ran for: each new value runs
         *  it once more, without closing slot. (beta38). */
        @Volatile private var ranFor: String? = null
        /** The report, where adb reads it: some handhelds keep only errors
         *  in Android's log (beta38). */
        const val REPORT = "slot-bench.txt"
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
            val wanted: String = try {
                val process = ProcessBuilder("getprop", PROPERTY).redirectErrorStream(true).start()
                val text = process.inputStream.bufferedReader().use { it.readText() }.trim()
                process.waitFor()
                text
            } catch (_: Exception) { "" }
            if (wanted.isEmpty() || wanted == "0" || wanted == ranFor) return@Thread
            ranFor = wanted
            handler.postDelayed({ run(steps(), 0) }, 4_000L)
        }, "slot-bench").start()
    }

    private var startedAt = 0L

    private fun report(): java.io.File? = activity.getExternalFilesDir(null)?.let { java.io.File(it, REPORT) }

    /** The journal's lines since the bench started, for tools/slot-bench.sh. */
    private fun writeReport() {
        val native = activity.nativeJournal()
        val from = startedAt - 10_000
        Thread({
            try {
                val stamp = java.text.SimpleDateFormat("MM-dd HH:mm:ss.SSS", java.util.Locale.US)
                report()?.writeText(SlotLog.snapshot(native).filter { it.first >= from }
                    .joinToString("\n") { (at, line) -> stamp.format(java.util.Date(at)) + " " + line } + "\n")
            } catch (error: Exception) {
                SlotLog.w(TAG, "Bench report not written", error)
            }
        }, "slot-bench-report").start()
    }

    private fun run(steps: List<Step>, at: Int) {
        if (at == 0) {
            startedAt = System.currentTimeMillis()
            report()?.delete()
            SlotLog.i(TAG, "Slot bench: started")
            activity.nativeFrameWatchFlush()
        }
        if (at >= steps.size) {
            handler.postDelayed({
                activity.nativeFrameWatchFlush()
                SlotLog.i(TAG, "Slot bench: done")
                writeReport()
            }, 1_500L)
            return
        }
        if (!canRun()) {
            SlotLog.i(TAG, "Slot bench: stopped (a game, or slot. in the background)")
            activity.nativeResetInput()
            writeReport()
            return
        }
        val step = steps[at]
        handler.postDelayed({
            if (canRun()) activity.nativeKey(step.code, step.down)
            run(steps, at + 1)
        }, step.wait)
    }
}
