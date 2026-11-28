package fyi.slot.konkr

import org.junit.Assert.assertEquals
import org.junit.Test

class NeoGeoSetsTest {
    private val sets = NeoGeoSets.parse(sequenceOf(
        "stakwin2\t\tStakes Winner 2",
        "spinmast\t\tSpin Master / Miracle Adventure",
        "mslug\t\tMetal Slug - Super Vehicle-001",
        "kof98a\tkof98\tThe King of Fighters '98 (alternate)",
        "twsoc96\t\tTecmo World Soccer '96",
        "kof94\t\tThe King of Fighters '94 (NGM-055)(NGH-055)",
        "savagere\t\tSavage Reign / Fu'un Mokushiroku - Kakutou Sousei",
        "savagereb\tsavagere\tSavage Reign / Fu'un Mokushiroku - Kakutou Sousei (Boss Hack)",
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

    @Test fun anUnknownSetNameFindsItsGameByTitle() {
        assertEquals("twsoc96", NeoGeoSets.setIn(sets, "Tecmo World Soccer '96 (tws96).neo"))
        assertEquals("kof94", NeoGeoSets.setIn(sets, "King of Fighters '94, The (kof94x).neo"))
        // A title shared with a hack: the parent.
        assertEquals("savagere", NeoGeoSets.setIn(sets, "Savage Reign (savagerx).neo"))
        assertEquals("unknown game (zz9)", NeoGeoSets.setIn(sets, "Unknown Game (zz9).neo"))
    }
}
