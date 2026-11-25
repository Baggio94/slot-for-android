package fyi.slot.konkr

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test

/** SYNCING over a copy's passes (3.0.1-beta12). */
class TaskPillPassesTest {
    private val sync = TaskPill.Task(TaskPill.SYNC, "SYNCING 1/4", 0.25f)

    /** Runs [passes] in turn, each (running ms, gap ms after it), polling
     *  every 100 ms as the activity does; what showed at each poll. */
    private fun run(vararg passes: Pair<Long, Long>, tail: Long = 4_000L): List<Pair<Long, Boolean>> {
        val pill = TaskPill.Passes()
        val out = ArrayList<Pair<Long, Boolean>>()
        var now = 0L
        for ((on, off) in passes) {
            val end = now + on
            while (now < end) { out.add(now to (pill.step(now, sync) != null)); now += 100 }
            val gapEnd = now + off
            while (now < gapEnd) { out.add(now to (pill.step(now, null) != null)); now += 100 }
        }
        val end = now + tail
        while (now < end) { out.add(now to (pill.step(now, null) != null)); now += 100 }
        return out
    }

    @Test fun aQuickCopyNeverShows() {
        assertEquals(emptyList<Long>(), run(400L to 0L).filter { it.second }.map { it.first })
    }

    @Test fun passesAddUpAndShowOnceWithoutAFlash() {
        // Three passes of 500 ms, 300 ms apart: a second of copying in all.
        val shown = run(500L to 300L, 500L to 300L, 500L to 0L)
        val on = shown.filter { it.second }.map { it.first }
        assertEquals("shows after a second of running, in the third pass", 1_800L, on.first())
        // One stretch, no hiding between passes, at least 1.5 s.
        assertEquals((on.first()..on.last() step 100).toList(), on)
        assert(on.last() - on.first() >= TaskPill.Passes.LEAST_MS - 100)
    }

    @Test fun aLongPauseStartsOver() {
        val pill = TaskPill.Passes()
        var now = 0L
        repeat(6) { pill.step(now, sync); now += 100 }
        now += 2_000L
        assertNull(pill.step(now, null))
        repeat(6) { assertNull(pill.step(now, sync)); now += 100 }
        repeat(5) { pill.step(now, sync); now += 100 }
        assertNotNull(pill.step(now, sync))
    }
}
