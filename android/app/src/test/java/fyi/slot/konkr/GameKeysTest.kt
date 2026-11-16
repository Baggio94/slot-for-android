package fyi.slot.konkr

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.file.Files

/** Slot. 2.5: a game's private files take its portable name. */
class GameKeysTest {
    @Test fun withoutTheNativeLibraryAGameKeepsItsAddress() {
        val uri = "content://roms/emerald"
        assertEquals(GameKeys.legacyKey(uri), GameKeys.key(uri))
        assertEquals(GameKeys.legacyDigest(uri), GameKeys.digest(uri))
        // The same FNV-1a as core_selection.rs.
        assertEquals("cd3adfea5b691b9d", GameKeys.fnv("content://roms/1"))
    }

    @Test fun savesAndStatesMoveToTheNewNameAndNameThemselves() {
        val saves = Files.createTempDirectory("slot-saves").toFile()
        try {
            val (old, new) = "a".repeat(64) to "b".repeat(64)
            java.io.File(saves, "$old.mgba.state").writeBytes(ByteArray(2048) { 7 })
            java.io.File(saves, "$old.mgba.state-generation.active").writeText("$old.mgba.gen-42")
            java.io.File(saves, "$old.mgba.gen-42").writeBytes(ByteArray(1024) { 3 })
            java.io.File(saves, "ManualExports").mkdirs()
            java.io.File(saves, "ManualExports/$old.mgba.1700000000.state").writeBytes(ByteArray(900))
            java.io.File(saves, "${"c".repeat(64)}.mgba.state").writeBytes(ByteArray(10))

            GameKeys.carryGameFiles(saves, old, new)

            assertTrue(java.io.File(saves, "$new.mgba.state").isFile)
            assertEquals("$new.mgba.gen-42",
                java.io.File(saves, "$new.mgba.state-generation.active").readText())
            assertTrue(java.io.File(saves, "$new.mgba.gen-42").isFile)
            assertTrue(java.io.File(saves, "ManualExports/$new.mgba.1700000000.state").isFile)
            assertFalse(java.io.File(saves, "$old.mgba.state").exists())
            assertTrue("other games are left alone",
                java.io.File(saves, "${"c".repeat(64)}.mgba.state").isFile)
        } finally {
            saves.deleteRecursively()
        }
    }

    @Test fun aFileAlreadyUnderTheNewNameIsNeverReplaced() {
        val saves = Files.createTempDirectory("slot-saves").toFile()
        try {
            val (old, new) = "a".repeat(64) to "b".repeat(64)
            java.io.File(saves, "$old.mgba.state").writeBytes(byteArrayOf(1))
            java.io.File(saves, "$new.mgba.state").writeBytes(byteArrayOf(2))
            GameKeys.carryGameFiles(saves, old, new)
            assertEquals(2, java.io.File(saves, "$new.mgba.state").readBytes()[0].toInt())
            assertTrue(java.io.File(saves, "$old.mgba.state").isFile)
        } finally {
            saves.deleteRecursively()
        }
    }

    @Test fun anOldLabelIsCarriedAndKept() {
        val labels = Files.createTempDirectory("slot-labels").toFile()
        try {
            val old = java.io.File(labels, "old.png").apply { writeBytes(byteArrayOf(9)) }
            val new = java.io.File(labels, "new.png")
            GameKeys.carry(old, new)
            assertEquals(9, new.readBytes()[0].toInt())
            assertTrue(java.io.File(labels, "old.png.carried").isFile)
            new.delete()
            GameKeys.carry(old, new)
            assertFalse("a reset label stays reset", new.exists())
        } finally {
            labels.deleteRecursively()
        }
    }
}
