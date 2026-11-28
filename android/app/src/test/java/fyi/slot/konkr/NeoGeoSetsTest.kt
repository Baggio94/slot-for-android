package fyi.slot.konkr

import org.junit.Assert.assertEquals
import org.junit.Test

class NeoGeoSetsTest {
    private val sets = NeoGeoSets.parse(sequenceOf(
        "stakwin2\t\tStakes Winner 2",
        "spinmast\t\tSpin Master / Miracle Adventure",
        "mslug\t\tMetal Slug - Super Vehicle-001",
        "kof98a\tkof98\tThe King of Fighters '98 (alternate)",
    ))

    @Test fun aSetIsReadFromItsFileName() {
        assertEquals("mslug", NeoGeoSets.setIn(sets, "mslug.zip"))
        assertEquals("mslug", NeoGeoSets.setIn(sets, "roms/MSLUG.neo"))
    }

    @Test fun aTitledNeoGivesItsSetInBrackets() {
        assertEquals("stakwin2", NeoGeoSets.setIn(sets, "Stakes Winner 2 (stakwin2).neo"))
        assertEquals("spinmast", NeoGeoSets.setIn(sets, "Spin Master (spinmast).neo"))
        assertEquals("kof98a", NeoGeoSets.setIn(sets, "King of Fighters 98 (kof98a) .neo"))
    }

    @Test fun anUnknownBracketKeepsTheWholeName() {
        assertEquals("metal slug (usa)", NeoGeoSets.setIn(sets, "Metal Slug (USA).neo"))
        assertEquals("game (nope)", NeoGeoSets.setIn(sets, "Game (nope).neo"))
    }
}
