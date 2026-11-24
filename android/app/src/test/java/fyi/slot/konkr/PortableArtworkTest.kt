package fyi.slot.konkr

import org.junit.Assert.*
import org.junit.Test

class PortableArtworkTest {
    @Test fun transferredOrCustomLabelsDoNotTriggerBackfill() {
        assertFalse(PortableArtwork.needsLookup(true, null, "md"))
        assertFalse(PortableArtwork.needsLookup(true, "md", "md"))
        assertTrue(PortableArtwork.needsLookup(true, "md", "md_ea"))
        assertTrue(PortableArtwork.needsLookup(false, null, "md"))
    }
    @Test fun onlyPortableArtworkFilesTravel() {
        assertTrue(FolderSync.travels("ArtworkHistory/home-cut/0123456789abcdef.txt"))
        assertTrue(FolderSync.travels("ArtworkHistory/home-miss/0123456789abcdef.txt"))
        assertTrue(FolderSync.travels("ArtworkHistory/gb-miss/0123456789abcdef.txt"))
        assertTrue(FolderSync.travels("home-real-labels/ABCDEF01.png"))
        assertFalse(FolderSync.travels("home-real-labels/done.json"))
        assertFalse(FolderSync.travels("home-labels-v1.json"))
        assertFalse(FolderSync.travels("AutoLabels/no-official-label.txt"))
        assertFalse(FolderSync.travels("ArtworkHistory/home-cut/0123456789abcdef.txt.bak"))
    }
    @Test fun manualRefreshAndWeeklyHomeExpiry() {
        val at = 1000L
        val record = PortableArtwork.homeMiss(32, 44, "md", at)
        assertTrue(PortableArtwork.validHomeMiss(record, 32, 44, "md", at + 1))
        assertFalse(PortableArtwork.validHomeMiss(record, 32, 44, "md", at + 1, at + 1))
        assertFalse(PortableArtwork.validHomeMiss(record, 32, 44, "md", at + PortableArtwork.MISS_TTL_MS))
        assertFalse(PortableArtwork.validHomeMiss("32\t44\tmd", 32, 44, "md", at + 1))
        assertFalse(PortableArtwork.validHomeMiss(record, 32, 44, "md_ea", at + 1))
        assertFalse(PortableArtwork.validGbMiss(PortableArtwork.gbMiss(32, 44, at), 32, 44, at + 1, 200, at + 1))
    }
    @Test fun negativeRecordsExpireAndDoNotMatchChangedRoms() {
        val record = PortableArtwork.gbMiss(32, 44, 1000)
        assertTrue(PortableArtwork.validGbMiss(record, 32, 44, 1100, 200))
        assertFalse(PortableArtwork.validGbMiss(record, 32, 44, 1200, 200))
        assertFalse(PortableArtwork.validGbMiss(record, 33, 44, 1100, 200))
        assertFalse(PortableArtwork.validGbMiss(record, 32, 45, 1100, 200))
    }
}
