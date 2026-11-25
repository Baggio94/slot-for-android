package fyi.slot.konkr

import android.os.SystemClock
import java.util.Locale

/**
 * The task pill (2.5.0-beta39, Siham 2026-11-19): what slot. is busy with,
 * drawn by the engine by the battery or at the top right of a menu. Only
 * what takes time: a task shows after a second, never for a quick one. One
 * pill, the most important task, "+1" for the others. A problem shows red
 * a few seconds. Never during a game (the engine draws it on the shelf and
 * the menus only). Reading it costs a few fields, every 100 ms.
 */
internal object TaskPill {
    const val PROBLEM = 0
    const val COPY = 1
    const val SYNC = 2
    const val DOWNLOADS = 3
    const val ROMM_SAVE = 4
    const val SCAN = 5
    const val LABELS = 6
    /** Something done the player wants to know about (beta41). */
    const val DONE = 7

    /** Before a task earns the pill. */
    private const val SHOW_AFTER_MS = 1_000L
    /** How long a problem shows. */
    private const val PROBLEM_MS = 4_000L
    /** How long a confirmation shows, however quick its task (beta41,
     *  Siham 2026-11-19), and how soon the same one may come back. */
    private const val DONE_MS = 1_500L
    private const val DONE_AGAIN_MS = 10_000L

    class Task(val kind: Int, val words: String, val progress: Float)

    /**
     * SYNCING across a copy's passes (LAUNCH, REST, ARTWORK; 3.0.1-beta12,
     * Siham 2026-11-25): it shows once the passes have run a second in all,
     * the short gaps between them neither restart that second nor hide it,
     * and once shown it stays at least [LEAST_MS] (no flash, never twice).
     */
    internal class Passes {
        private var last: Task? = null
        private var ran = 0L
        private var tick = -1L
        private var seen = 0L
        private var shownAt = -1L
        private var wasRunning = false

        /** What to show at [now] for [running] (null: nothing). */
        fun step(now: Long, running: Task?): Task? {
            val dt = if (tick < 0) 0L else (now - tick).coerceIn(0L, 500L)
            tick = now
            if (running != null) {
                last = running
                seen = now
                // Only time the copy ran counts, not the gap before it.
                if (wasRunning) ran += dt
            }
            wasRunning = running != null
            val shown = shownAt >= 0
            val keep = shown && (now - seen < GAP_MS || now - shownAt < LEAST_MS)
            val show = (running != null && (shown || ran >= SHOW_AFTER_MS)) || keep
            if (show) {
                if (!shown) shownAt = now
                return running ?: last
            }
            if (running == null && now - seen >= GAP_MS) {
                last = null
                ran = 0L
                shownAt = -1L
            }
            return null
        }

        companion object {
            /** A pause between passes shorter than this is the same copy. */
            const val GAP_MS = 1_500L
            const val LEAST_MS = 1_500L
        }
    }
    private val passes = Passes()

    /** One confirmation after the save and state upload attempts finish.
     *  Never claim a transfer happened when it was unchanged or deferred. */
    internal fun rommSyncWords(saveUploaded: Boolean, stateUploaded: Boolean): String? = when {
        saveUploaded && stateUploaded -> "SAVE + STATE SYNCED"
        saveUploaded -> "SAVE SYNCED"
        stateUploaded -> "STATE SYNCED"
        else -> null
    }

    @Volatile private var problem: String? = null
    @Volatile private var problemUntil = 0L
    @Volatile private var done: String? = null
    @Volatile private var doneUntil = 0L
    private val doneLast = HashMap<String, Long>()
    private val since = HashMap<Int, Long>()
    private var sent: String? = null

    /** A problem the player should see (ROMM OFFLINE, SAVE NOT SENT...). */
    fun problem(words: String) {
        problem = words.uppercase(Locale.ROOT)
        problemUntil = SystemClock.uptimeMillis() + PROBLEM_MS
    }

    /** Something done the player is glad to see ("SAVED TO ROMM"), even
     *  when it took no time: 1.5 s, not twice in 10 s. */
    @Synchronized
    fun confirm(words: String) {
        val now = SystemClock.uptimeMillis()
        val key = words.uppercase(Locale.ROOT)
        if (now - (doneLast[key] ?: Long.MIN_VALUE / 2) < DONE_AGAIN_MS) return
        doneLast[key] = now
        done = key
        doneUntil = now + DONE_MS
    }

    /** The pill's lines for [running] (any order), or null when unchanged
     *  since the last call. Called on the UI thread only. During a game
     *  ([playing]) a problem waits: it shows once the shelf is back. */
    fun spec(running: List<Task>, playing: Boolean): String? {
        val now = SystemClock.uptimeMillis()
        if (playing && problem != null) problemUntil = maxOf(problemUntil, now + PROBLEM_MS)
        if (playing && done != null) doneUntil = maxOf(doneUntil, now + DONE_MS)
        val shown = ArrayList<Task>()
        problem?.let { words ->
            if (now < problemUntil) shown.add(Task(PROBLEM, words, -1f)) else problem = null
        }
        done?.let { words ->
            if (now < doneUntil) shown.add(Task(DONE, words, 1f)) else done = null
        }
        val kinds = running.map { it.kind }.toSet()
        since.keys.retainAll(kinds)
        val sync = passes.step(now, running.firstOrNull { it.kind == SYNC })
        for (task in (running.filter { it.kind != SYNC } + listOfNotNull(sync)).sortedBy { it.kind }) {
            if (task.kind == SYNC) { shown.add(task); continue }
            val first = since.getOrPut(task.kind) { now }
            if (now - first >= SHOW_AFTER_MS) shown.add(task)
        }
        val spec = shown.joinToString("\n") {
            "${it.kind}\t${String.format(Locale.ROOT, "%.3f", it.progress)}\t${it.words.replace('\t', ' ')}"
        }
        if (spec == sent) return null
        sent = spec
        return spec
    }
}
