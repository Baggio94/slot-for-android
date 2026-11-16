package fyi.slot.konkr

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.nio.file.Files

/** The slot. folder's two-way copy (slot. 2.5), on plain folders. */
class FolderSyncTest {
    /** A folder side without the travel filter, like a card. */
    private class Card(val root: File) : FolderSync.Side {
        override fun list(): Map<String, FolderSync.Meta> = root.walkTopDown().filter { it.isFile }
            .associate { it.relativeTo(root).path.replace('\\', '/') to FolderSync.Meta(it.length(), it.lastModified()) }
        override fun read(path: String) = File(root, path).readBytes()
        override fun write(path: String, bytes: ByteArray): FolderSync.Meta {
            val f = File(root, path); f.parentFile?.mkdirs(); f.writeBytes(bytes)
            return FolderSync.Meta(f.length(), f.lastModified())
        }
        override fun delete(path: String) { File(root, path).delete() }
    }

    private fun dirs(): Pair<File, File> =
        Files.createTempDirectory("slot-private").toFile() to Files.createTempDirectory("slot-card").toFile()

    private fun put(root: File, path: String, text: String, modified: Long = 1_000_000L) {
        val f = File(root, path); f.parentFile?.mkdirs(); f.writeText(text); f.setLastModified(modified)
    }

    @Test fun theFirstCopyTakesWhatTravelsAndNoSecrets() {
        val (mine, card) = dirs()
        put(mine, "Saves/mGBA/Emerald.srm", "save")
        put(mine, "konkr-favorites.json", "[]")
        put(mine, "Config/romm.json", "token")
        put(mine, "Config/retroachievements.toml", "token")
        put(mine, "Saves/abc.mgba.skip-resume.once", "1")
        put(mine, "Labels/old.png.carried", "x")
        val report = FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), FolderSync.Base(), FolderSync.Start.MERGE)
        assertEquals(2, report.toFolder)
        assertTrue(File(card, "Saves/mGBA/Emerald.srm").isFile)
        assertFalse("secrets never travel", File(card, "Config/romm.json").exists())
        assertFalse(File(card, "Config/retroachievements.toml").exists())
        assertFalse("one-shot markers stay", File(card, "Saves/abc.mgba.skip-resume.once").exists())
        assertFalse(File(card, "Labels/old.png.carried").exists())
    }

    @Test fun whatAnotherHandheldChangedComesBack() {
        val (mine, card) = dirs()
        put(mine, "Saves/mGBA/Emerald.srm", "first")
        val base = FolderSync.Base()
        FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), base, FolderSync.Start.MERGE)
        // The other handheld played.
        put(card, "Saves/mGBA/Emerald.srm", "played elsewhere", 2_000_000L)
        val report = FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), base)
        assertEquals(1, report.toPrivate)
        assertEquals("played elsewhere", File(mine, "Saves/mGBA/Emerald.srm").readText())
        assertTrue(report.changedPrivate)
    }

    @Test fun duringAGameNothingComesIn() {
        val (mine, card) = dirs()
        put(mine, "Saves/mGBA/Emerald.srm", "first")
        val base = FolderSync.Base()
        FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), base, FolderSync.Start.MERGE)
        put(card, "Saves/mGBA/Emerald.srm", "elsewhere", 2_000_000L)
        FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), base, pull = false)
        assertEquals("first", File(mine, "Saves/mGBA/Emerald.srm").readText())
    }

    @Test fun bothChangedTheNewerWinsAndTheOtherIsKept() {
        val (mine, card) = dirs()
        put(mine, "Saves/mGBA/Emerald.srm", "first")
        val base = FolderSync.Base()
        FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), base, FolderSync.Start.MERGE)
        put(mine, "Saves/mGBA/Emerald.srm", "here", 3_000_000L)
        put(card, "Saves/mGBA/Emerald.srm", "there", 2_000_000L)
        val report = FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), base)
        assertEquals(1, report.conflicts)
        assertEquals("here", File(card, "Saves/mGBA/Emerald.srm").readText())
        val kept = File(card, "Conflicts/Saves/mGBA").listFiles()!!.single()
        assertEquals("there", kept.readText())
    }

    @Test fun aPolaroidDeletedInSlotGoesFromTheFolderButOneDeletedByHandComesBack() {
        val (mine, card) = dirs()
        put(mine, "Saves/ManualExports/a.mgba.1.state", "polaroid")
        put(mine, "Saves/mGBA/Emerald.srm", "save")
        val base = FolderSync.Base()
        FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), base, FolderSync.Start.MERGE)
        File(mine, "Saves/ManualExports/a.mgba.1.state").delete()
        File(card, "Saves/mGBA/Emerald.srm").delete()
        val report = FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), base)
        assertEquals(1, report.removed)
        assertFalse(File(card, "Saves/ManualExports/a.mgba.1.state").exists())
        assertTrue("put back (rule 3)", File(card, "Saves/mGBA/Emerald.srm").isFile)
    }

    @Test fun usingTheFolderReplacesTheHandheldsCopy() {
        val (mine, card) = dirs()
        put(mine, "konkr-favorites.json", "[\"mine\"]", 5_000_000L)
        put(card, "konkr-favorites.json", "[\"theirs\"]", 1_000_000L)
        put(card, "Labels/x.png", "label")
        val report = FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), FolderSync.Base(),
            FolderSync.Start.FOLDER_WINS)
        assertEquals("[\"theirs\"]", File(mine, "konkr-favorites.json").readText())
        assertTrue(File(mine, "Labels/x.png").isFile)
        assertEquals(2, report.toPrivate)
    }

    @Test fun theBaseSurvivesARestart() {
        val base = FolderSync.Base()
        base.entries["Saves/a.srm"] = FolderSync.Entry(1, 2, 3, 4, "sha")
        assertEquals(base.entries, FolderSync.Base.parse(base.toJson()).entries)
    }
}
