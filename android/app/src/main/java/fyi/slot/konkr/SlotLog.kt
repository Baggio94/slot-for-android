package fyi.slot.konkr

import android.util.Log

/**
 * slot.'s own journal (2.5.0-beta38): the lines that say what a launch, a
 * save, RomM or the slot. folder did, kept by slot. itself as well as in
 * Android's log. Some handhelds keep only errors in Android's log (the
 * Retroid Pocket Classic), so About's X Logs had nothing to show there.
 * A few hundred short lines in memory; written only when the player saves
 * the logs.
 */
internal object SlotLog {
    private const val KEEP = 800
    private val lines = ArrayDeque<Pair<Long, String>>()

    private fun keep(level: Char, tag: String, message: String?, error: Throwable?) {
        val text = buildString {
            append(level).append(' ').append(tag).append(": ").append(message)
            if (error != null) append(" — ").append(error.javaClass.simpleName).append(": ").append(error.message)
        }
        synchronized(lines) {
            if (lines.size >= KEEP) lines.removeFirst()
            lines.addLast(System.currentTimeMillis() to text)
        }
    }

    fun i(tag: String, message: String?, error: Throwable? = null): Int {
        keep('I', tag, message, error)
        return if (error == null) Log.i(tag, message ?: "") else Log.i(tag, message, error)
    }

    fun w(tag: String, message: String?, error: Throwable? = null): Int {
        keep('W', tag, message, error)
        return if (error == null) Log.w(tag, message ?: "") else Log.w(tag, message, error)
    }

    fun e(tag: String, message: String?, error: Throwable? = null): Int {
        keep('E', tag, message, error)
        return if (error == null) Log.e(tag, message ?: "") else Log.e(tag, message, error)
    }

    /** The journal, oldest first, with the native side's lines ([native]:
     *  "epoch-ms|line" each) in their place. */
    fun snapshot(native: String): List<Pair<Long, String>> {
        val mine = synchronized(lines) { lines.toList() }
        val theirs = native.lineSequence().mapNotNull { line ->
            val bar = line.indexOf('|')
            if (bar <= 0) null else line.substring(0, bar).toLongOrNull()?.let { it to line.substring(bar + 1) }
        }.toList()
        return (mine + theirs).sortedBy { it.first }
    }
}
