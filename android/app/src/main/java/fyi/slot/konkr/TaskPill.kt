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
        for (task in running.sortedBy { it.kind }) {
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
