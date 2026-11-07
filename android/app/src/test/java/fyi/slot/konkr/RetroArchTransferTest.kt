package fyi.slot.konkr

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** What a newly chosen folder is given: saves, RTCs, the auto-state and the
 *  manual slots that mirror the Polaroids, each with its picture. */
class RetroArchTransferTest {
    @Test fun savesAreTheSramAndTheClock() {
        assertTrue(RetroArchStorage.transferKind("Pokemon - Crystal Version (USA).srm", false))
        assertTrue(RetroArchStorage.transferKind("Pokemon - Crystal Version (USA).rtc", false))
        assertFalse(RetroArchStorage.transferKind("Game.srm.before-slot", false))
        assertFalse(RetroArchStorage.transferKind("Game.state.auto", false))
    }

    @Test fun statesAreTheAutoStateAndEveryPolaroidSlot() {
        for (name in listOf("Super Mario Bros. 3 (USA).state.auto", "Game.state", "Game.state1",
                "Game.state9", "Game.state.png", "Game.state4.png", "Game.state.auto.png")) {
            assertTrue(name, RetroArchStorage.transferKind(name, true))
        }
        for (name in listOf("Game.srm", "Game.state10", "Game.state.auto.before-slot",
                "Game.state1.before-transfer-20261107-230000", ".state", "Game.state.slot-20261107-230000")) {
            assertFalse(name, RetroArchStorage.transferKind(name, true))
        }
    }
}
