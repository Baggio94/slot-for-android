package fyi.slot.konkr

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Sync States with RomM (3.0): Siham's scenarios of 2026-11-22. */
class RommStatesTest {
    private fun gba(core: String): RetroArchStorage.Target {
        val f = java.io.File("x")
        return RetroArchStorage.Target("content://roms/gba/Emerald.gba", "Emerald", core, f, f, f, f, f, f,
            null, null, true, "id", true)
    }

    private val mine = "mine".toByteArray()
    private fun kept(sha: String = RommSaves.sha1(mine), stamp: String = "2026-11-22T10:00:00Z", id: Long = 7) =
        RommStates.Record(1, id, stamp, sha)

    private fun plan(kept: RommStates.Record?, server: String?, local: ByteArray? = mine,
                     pick: RommSaves.Pick = RommSaves.Pick.NONE, saveAt: String = "", stale: Boolean = false) =
        RommStates.plan(kept, server, local, pick, saveAt, stale)

    @Test fun onlyThisCoresStateWithRetroArchsName() {
        val states = listOf(
            Romm.ServerSave(1, "Emerald.state.auto", "2026-11-22T10:00:00Z", "mgba", ""),
            Romm.ServerSave(2, "Emerald.state.auto", "2026-11-22T12:00:00Z", "gpsp", ""),
            Romm.ServerSave(3, "Emerald.state1", "2026-11-22T13:00:00Z", "mgba", ""),
            Romm.ServerSave(4, "Ruby.state.auto", "2026-11-22T14:00:00Z", "mgba", ""),
            Romm.ServerSave(5, "Emerald [2026-11-22_11-00-00].state.auto", "2026-11-22T11:00:00Z", "mGBA", "",
                nameNoTags = "Emerald.state.auto"),
        )
        // D2/D3: gpSP's never for mGBA, even with Share Core Saves; RetroArch's mGBA counts.
        assertEquals(5L, RommStates.newest(states, gba("mGBA"))?.id)
        assertEquals(2L, RommStates.newest(states, gba("gpSP"))?.id)
        assertNull(RommStates.newest(states.filter { it.emulator != "gpsp" }, gba("gpSP")))
    }

    @Test fun datesCompareAsInstants() {
        assertTrue(RommStates.compareStamps("2026-11-22T12:00:00+01:00", "2026-11-22T11:30:00Z") < 0)
        assertTrue(RommStates.compareStamps("2026-11-22T12:00:00.5Z", "2026-11-22T12:00:00Z") > 0)
    }

    @Test fun launches() {
        // B1, B2: no state on the server.
        assertEquals(RommStates.Plan.NOTHING, plan(null, null, local = null))
        assertEquals(RommStates.Plan.NOTHING, plan(null, null))
        // B3: the server's, nothing here.
        assertEquals(RommStates.Plan.BRING, plan(null, "2026-11-22T10:00:00Z", local = null))
        // B4: the server unchanged.
        assertEquals(RommStates.Plan.NOTHING, plan(kept(), "2026-11-22T10:00:00Z", local = "new".toByteArray()))
        // B5: the server changed, this handheld didn't.
        assertEquals(RommStates.Plan.BRING, plan(kept(), "2026-11-22T11:00:00Z"))
        // B6: both changed; first exchange with a state on both sides too.
        assertEquals(RommStates.Plan.ASK, plan(kept(), "2026-11-22T11:00:00Z", local = "new".toByteArray()))
        assertEquals(RommStates.Plan.ASK, plan(null, "2026-11-22T11:00:00Z"))
    }

    @Test fun neverOverANewerSave() {
        // C2/C3: the server's save is newer than its state.
        assertEquals(RommStates.Plan.SAVE_IS_NEWER,
            plan(kept(), "2026-11-22T11:00:00Z", saveAt = "2026-11-22T11:00:05Z"))
        // C1: saved, then its state (slot.'s order at an eject).
        assertEquals(RommStates.Plan.BRING,
            plan(kept(), "2026-11-22T11:00:05Z", pick = RommSaves.Pick.BROUGHT, saveAt = "2026-11-22T11:00:00Z"))
        // C4: this handheld's save kept: its state too.
        assertEquals(RommStates.Plan.HANDHELD_SAVE_KEPT,
            plan(kept(), "2026-11-22T11:00:05Z", pick = RommSaves.Pick.KEPT_HANDHELD))
        // This handheld saved in-game after its own state.
        assertEquals(RommStates.Plan.HANDHELD_SAVE_KEPT, plan(kept(), "2026-11-22T11:00:00Z", stale = true))
        // ...unless the save came from the server with this state.
        assertEquals(RommStates.Plan.BRING,
            plan(kept(), "2026-11-22T11:00:05Z", pick = RommSaves.Pick.BROUGHT, stale = true))
    }

    @Test fun deletedElsewhere() {
        // F2: the server had it, has none now, this handheld has one.
        assertEquals(RommStates.Plan.DELETED_ELSEWHERE, plan(kept(), null))
        // Already settled (deleted or kept): nothing more.
        assertEquals(RommStates.Plan.NOTHING, plan(kept(id = -1), null))
        assertEquals(RommStates.Plan.NOTHING, plan(kept(), null, local = null))
    }

    @Test fun retroArchsFileName() {
        assertEquals("Emerald.state.auto", RommStates.fileName(gba("mGBA")))
    }
}
