package fyi.slot.konkr

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.file.Files

/** "Keep folder saves": the next start of that game begins from the
 *  folder's save, not from slot.'s older save state (amadeus12, 2026-11-15). */
class FolderSaveMarkTest {
    @Test fun aKeptFolderSaveStartsTheGameFromItOnce() {
        val files = Files.createTempDirectory("slot-files").toFile()
        try {
            RetroArchStorage.markFolderSave(files, "mGBA", "Pokemon - Emerald Version (USA)")
            val stale = java.io.File(files, "Saves/abc123.mgba.state-stale.once")
            assertTrue(RetroArchStorage.applyFolderSaveMark(files, "mGBA",
                "Pokemon - Emerald Version (USA)", "abc123"))
            assertTrue("the one-shot guard game.rs reads", stale.isFile)
            assertFalse("only once", RetroArchStorage.applyFolderSaveMark(files, "mGBA",
                "Pokemon - Emerald Version (USA)", "abc123"))
        } finally {
            files.deleteRecursively()
        }
    }

    @Test fun otherGamesAndCoresAreLeftAlone() {
        val files = Files.createTempDirectory("slot-files").toFile()
        try {
            RetroArchStorage.markFolderSave(files, "mGBA", "Game A")
            assertFalse(RetroArchStorage.applyFolderSaveMark(files, "mGBA", "Game B", "b"))
            assertFalse(RetroArchStorage.applyFolderSaveMark(files, "gpSP", "Game A", "a"))
            assertFalse(java.io.File(files, "Saves").exists())
        } finally {
            files.deleteRecursively()
        }
    }
}
