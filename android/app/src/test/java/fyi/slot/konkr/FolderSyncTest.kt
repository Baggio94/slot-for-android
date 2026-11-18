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
        assertFalse("this handheld's own", FolderSync.travels("Saves/abc.mgba.retroarch-import.state.auto"))
        assertFalse(FolderSync.travels("Saves/abc.mgba.state-export-protected"))
        assertTrue(FolderSync.travels("Saves/abc.mgba.state"))
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
        put(mine, "Labels/x.png", "label")
        val base = FolderSync.Base()
        val tombs = HashMap<String, FolderSync.Tomb>()
        FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), base, FolderSync.Start.MERGE, tombs = tombs)
        File(mine, "Saves/ManualExports/a.mgba.1.state").delete()
        File(card, "Labels/x.png").delete()
        val report = FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), base, tombs = tombs)
        assertEquals(1, report.removed)
        assertFalse(File(card, "Saves/ManualExports/a.mgba.1.state").exists())
        assertTrue("noted for the other handhelds", "Saves/ManualExports/a.mgba.1.state" in tombs)
        assertTrue("put back (rule 3)", File(card, "Labels/x.png").isFile)
    }

    @Test fun aSaveDeletedByHandIsHeldBackUntilThePlayerSays() {
        val (mine, card) = dirs()
        put(mine, "Saves/mGBA/Emerald.srm", "save")
        put(mine, "States/mGBA/Emerald.state.auto", "state")
        val base = FolderSync.Base()
        FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), base, FolderSync.Start.MERGE)
        File(card, "Saves/mGBA/Emerald.srm").delete()
        File(card, "States/mGBA/Emerald.state.auto").delete()
        val held = FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), base)
        assertEquals(setOf("Saves/mGBA/Emerald.srm", "States/mGBA/Emerald.state.auto"), held.gone)
        assertFalse("not put back behind the player's back", File(card, "Saves/mGBA/Emerald.srm").exists())
        assertTrue("slot.'s own copy is untouched", File(mine, "Saves/mGBA/Emerald.srm").isFile)
        // Still held at the next copy, during a game too.
        assertEquals(2, FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), base, pull = false).gone.size)
        // PUT IT BACK for the save; DELETE IT for the state.
        File(mine, "States/mGBA/Emerald.state.auto").delete()
        val tombs = HashMap<String, FolderSync.Tomb>()
        val after = FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), base, tombs = tombs,
            restore = setOf("Saves/mGBA/Emerald.srm"))
        assertTrue(after.gone.isEmpty())
        assertEquals("save", File(card, "Saves/mGBA/Emerald.srm").readText())
        assertTrue("the deletion is noted for the other handhelds", "States/mGBA/Emerald.state.auto" in tombs)
        assertTrue(after.tombsChanged)
    }

    @Test fun anotherHandheldFollowsADeletionMadeOnPurpose() {
        // Handheld A and B share the card, both with the same save.
        val (a, card) = dirs()
        val b = Files.createTempDirectory("slot-b").toFile()
        put(a, "Saves/mGBA/Emerald.srm", "save")
        val baseA = FolderSync.Base()
        val baseB = FolderSync.Base()
        val tombs = HashMap<String, FolderSync.Tomb>()
        FolderSync.sync(FolderSync.PrivateSide(a), Card(card), baseA, FolderSync.Start.MERGE, tombs = tombs)
        FolderSync.sync(FolderSync.PrivateSide(b), Card(card), baseB, FolderSync.Start.MERGE, tombs = tombs)
        assertTrue(File(b, "Saves/mGBA/Emerald.srm").isFile)
        // A deletes it (DELETE IT): gone from A, then from the card.
        File(a, "Saves/mGBA/Emerald.srm").delete()
        FolderSync.sync(FolderSync.PrivateSide(a), Card(card), baseA, tombs = tombs)
        assertFalse(File(card, "Saves/mGBA/Emerald.srm").exists())
        // During a game B waits: neither deleted nor put back.
        FolderSync.sync(FolderSync.PrivateSide(b), Card(card), baseB, pull = false, tombs = tombs)
        assertTrue(File(b, "Saves/mGBA/Emerald.srm").isFile)
        assertFalse(File(card, "Saves/mGBA/Emerald.srm").exists())
        // Then B follows, without asking.
        val report = FolderSync.sync(FolderSync.PrivateSide(b), Card(card), baseB, tombs = tombs)
        assertEquals(listOf("Saves/mGBA/Emerald.srm"), report.deletedHere)
        assertFalse(File(b, "Saves/mGBA/Emerald.srm").exists())
        assertTrue(report.changedPrivate)
    }

    @Test fun aHandheldThatPlayedOnKeepsItsSaveDespiteTheDeletion() {
        val (a, card) = dirs()
        val b = Files.createTempDirectory("slot-b").toFile()
        put(a, "Saves/mGBA/Emerald.srm", "save")
        val baseA = FolderSync.Base()
        val baseB = FolderSync.Base()
        val tombs = HashMap<String, FolderSync.Tomb>()
        FolderSync.sync(FolderSync.PrivateSide(a), Card(card), baseA, FolderSync.Start.MERGE, tombs = tombs)
        FolderSync.sync(FolderSync.PrivateSide(b), Card(card), baseB, FolderSync.Start.MERGE, tombs = tombs)
        File(a, "Saves/mGBA/Emerald.srm").delete()
        FolderSync.sync(FolderSync.PrivateSide(a), Card(card), baseA, tombs = tombs)
        // B played on before seeing the card again.
        put(b, "Saves/mGBA/Emerald.srm", "further", 9_000_000L)
        val report = FolderSync.sync(FolderSync.PrivateSide(b), Card(card), baseB, tombs = tombs)
        assertTrue(report.deletedHere.isEmpty())
        assertEquals("further", File(card, "Saves/mGBA/Emerald.srm").readText())
        assertFalse("a save made again clears the tomb", "Saves/mGBA/Emerald.srm" in tombs)
    }

    @Test fun whatAMacOrAnotherAppLeavesInTheFolderIsIgnored() {
        val (mine, card) = dirs()
        put(card, "._slot.json", "mac")
        put(card, ".DS_Store", "mac")
        put(card, "slot (1).json", "copy")
        put(card, "CustomCheats/GBA/._Golden Sun.cht", "mac")
        put(card, "CustomCheats/GBA/Golden Sun.cht", "cheats")
        val report = FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), FolderSync.Base(), FolderSync.Start.MERGE)
        assertEquals(1, report.toPrivate)
        assertTrue(File(mine, "CustomCheats/GBA/Golden Sun.cht").isFile)
        assertFalse(File(mine, "slot (1).json").exists())
        assertFalse(File(mine, ".DS_Store").exists())
        assertFalse(File(mine, "CustomCheats/GBA/._Golden Sun.cht").exists())
    }

    @Test fun tombsSurviveARestartAndAreForgottenAfterAYear() {
        val tombs = mapOf("Saves/mGBA/a.srm" to FolderSync.Tomb("sha", 5L))
        assertEquals(tombs, FolderSync.tombsFromJson(FolderSync.tombsToJson(tombs)))
        val (mine, card) = dirs()
        val old = HashMap(tombs)
        val report = FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), FolderSync.Base(),
            now = 400L * 24 * 3600 * 1000, tombs = old)
        assertTrue(old.isEmpty())
        assertTrue(report.tombsChanged)
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

    @Test fun aCardGivingOtherTimesForTheSameBytesCopiesNothing() {
        // beta38: an SD card's times moved after the copy; 194 files came
        // back for nothing and the shelf froze.
        val (mine, card) = dirs()
        put(mine, "Labels/GBA/a.png", "label")
        put(mine, "Saves/mGBA/Emerald.srm", "save")
        val base = FolderSync.Base()
        FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), base, FolderSync.Start.MERGE)
        File(card, "Labels/GBA/a.png").setLastModified(7_000_000L)
        File(mine, "Saves/mGBA/Emerald.srm").setLastModified(8_000_000L)
        val report = FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), base)
        assertEquals(0, report.toPrivate)
        assertEquals(0, report.toFolder)
        assertFalse(report.changedPrivate)
        // And the next copy doesn't read them again.
        assertEquals(7_000_000L, base.entries["Labels/GBA/a.png"]!!.folderModified)
        assertEquals(8_000_000L, base.entries["Saves/mGBA/Emerald.srm"]!!.privateModified)
    }

    @Test fun aSaveDeletedByHandIsAskedAboutEvenAfterPlayingOn() {
        val (mine, card) = dirs()
        put(mine, "Saves/mGBA/Emerald.srm", "save")
        val base = FolderSync.Base()
        FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), base, FolderSync.Start.MERGE)
        put(mine, "Saves/mGBA/Emerald.srm", "played on", 9_000_000L)
        File(card, "Saves/mGBA/Emerald.srm").delete()
        val held = FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), base)
        assertEquals(setOf("Saves/mGBA/Emerald.srm"), held.gone)
        assertFalse(File(card, "Saves/mGBA/Emerald.srm").exists())
        // PUT IT BACK: the newer one.
        FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), base, restore = setOf("Saves/mGBA/Emerald.srm"))
        assertEquals("played on", File(card, "Saves/mGBA/Emerald.srm").readText())
    }
}
