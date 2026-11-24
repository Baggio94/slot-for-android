package fyi.slot.konkr

import org.junit.Assert.*
import org.junit.Test

class GbFamilyArtworkTest {
    @Test fun onlyGameBoyAndColorCrossTheArtworkBoundary() {
        assertEquals("GB", GbFamilyArtwork.other("GBC"))
        assertEquals("GBC", GbFamilyArtwork.other("GB"))
        assertNull(GbFamilyArtwork.other("GBA"))
        assertEquals(listOf("GBA" to "Super Mario Land DX"), GbFamilyArtwork.names("GBA", "Super Mario Land DX"))
    }

    @Test fun numberedSequelsKeepTheirIdentityAndTags() {
        assertEquals("Super Mario Land (World)", GbFamilyArtwork.originalTitle("Super Mario Land DX (World)"))
        assertEquals("Super Mario Land 2 - 6 Golden Coins (USA)",
            GbFamilyArtwork.originalTitle("Super Mario Land 2 DX (USA)"))
        assertEquals("Kirby's Dream Land", GbFamilyArtwork.originalTitle("Kirby's Dream Land DX"))
        assertEquals("Metroid II - Return of Samus", GbFamilyArtwork.originalTitle("Metroid II: Return of Samus - Color"))
        assertNull(GbFamilyArtwork.originalTitle("Super Mario Bros. Deluxe"))
        assertNull(GbFamilyArtwork.originalTitle("Super Mario Land 2"))
        val catalog = OfficialArtMatcher.Catalog.fromNames("GB", mapOf(
            "11111111" to "Super Mario Land (World)",
            "22222222" to "Super Mario Land 2 - 6 Golden Coins (USA)"))
        val title = GbFamilyArtwork.originalTitle("Super Mario Land 2 DX (USA)")!!
        assertEquals("22222222", OfficialArtMatcher.inArtSet("GB", title, catalog, relaxed = false))
        assertNull(OfficialArtMatcher.inArtSet("GB", "Super Mario Land 3 DX", catalog, relaxed = true))
    }

    @Test fun oldMissesAreReconsideredOnlyForGbFamily() {
        assertEquals("gb-miss-v2", GbFamilyArtwork.missKind("GB"))
        assertEquals("gb-miss-v2", GbFamilyArtwork.missKind("GBC"))
        assertEquals("gb-miss", GbFamilyArtwork.missKind("GBA"))
        assertTrue(FolderSync.travels("ArtworkHistory/gb-miss-v2/0123456789abcdef.txt"))
    }
}
