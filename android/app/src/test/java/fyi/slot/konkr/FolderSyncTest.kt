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
        /** As the card's side: what the folder should no longer hold goes. */
        override fun dropStale(): Int {
            val stale = root.walkTopDown().filter { it.isFile }
                .filter { FolderSync.stale(it.relativeTo(root).path.replace('\\', '/')) }.toList()
            stale.forEach { it.delete() }
            return stale.size
        }
    }

    private fun dirs(): Pair<File, File> =
        Files.createTempDirectory("slot-private").toFile() to Files.createTempDirectory("slot-card").toFile()

    private fun put(root: File, path: String, text: String, modified: Long = 1_000_000L) {
        val f = File(root, path); f.parentFile?.mkdirs(); f.writeText(text); f.setLastModified(modified)
    }

    @Test fun syncingCountsOnlyWhatComesIn() {
        val (mine, card) = dirs()
        // Two saves come in; three of this handheld's go out.
        put(card, "Saves/mGBA/Zelda.srm", "z")
        put(card, "Saves/mGBA/Metroid.srm", "m")
        put(mine, "Saves/mGBA/Golden Sun.srm", "g")
        put(mine, "Saves/mGBA/Advance Wars.srm", "a")
        put(mine, "Saves/mGBA/Mario Kart.srm", "k")
        val seen = ArrayList<Pair<Int, Int>>()
        FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), FolderSync.Base(), FolderSync.Start.MERGE,
            progress = { done, total -> seen.add(done to total) })
        assertEquals(0 to 2, seen.first())
        assertEquals(2 to 2, seen.last())
        // A push brings nothing in: nothing to count, nothing to show.
        put(mine, "Saves/mGBA/Zelda.srm", "z2", 2_000_000L)
        val pushed = ArrayList<Pair<Int, Int>>()
        FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), FolderSync.Base(), pull = false,
            progress = { done, total -> pushed.add(done to total) })
        assertTrue(pushed.all { it.second == 0 })
    }

    @Test fun aLaunchedGamesOwnFilesComeInFirst() {
        val (mine, card) = dirs()
        // The folder brings three games' saves in; Zelda is launched.
        put(card, "Saves/mGBA/Advance Wars.srm", "a")
        put(card, "Saves/mGBA/Mario Kart.srm", "m")
        put(card, "Saves/mGBA/Zelda.srm", "z")
        put(card, "Saves/abc123.mgba.state", "zs")
        put(card, "GameEmulation/f00d.json", "{}")
        val order = ArrayList<String>()
        val private = object : FolderSync.PrivateSide(mine) {
            override fun write(path: String, bytes: ByteArray): FolderSync.Meta {
                order.add(path); return super.write(path, bytes)
            }
        }
        val want = FolderSync.Priority(setOf("Zelda", "abc123", "f00d", ""))
        val report = FolderSync.sync(private, Card(card), FolderSync.Base(), FolderSync.Start.MERGE,
            scope = FolderSync.Scope.LAUNCH, priority = { want })
        assertEquals(0L, want.served.count)
        assertEquals(setOf("Saves/mGBA/Zelda.srm", "Saves/abc123.mgba.state", "GameEmulation/f00d.json"),
            order.take(3).toSet())
        assertEquals("every file still comes in, once", 5, report.toPrivate)
        assertFalse("a name is whole: Mario isn't Mario Kart", FolderSync.Priority(setOf("Mario"))
            .wants("Saves/mGBA/Mario Kart.srm"))
    }

    @Test fun theFirstCopyTakesWhatTravelsAndNoSecrets() {
        val (mine, card) = dirs()
        put(mine, "Saves/mGBA/Emerald.srm", "save")
        put(mine, "konkr-favorites.json", "[]")
        put(mine, "Config/romm.json", "token")
        put(mine, "Config/retroachievements.toml", "token")
        put(mine, "Saves/abc.mgba.skip-resume.once", "1")
        put(mine, "Labels/old.png.carried", "x")
        put(mine, "Saves/RetroAchievements/0cc175b9/auth.json", "token")
        put(mine, "Saves/RetroAchievements/0cc175b9/badges/1234.png", "badge")
        val report = FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), FolderSync.Base(), FolderSync.Start.MERGE)
        assertEquals(2, report.toFolder)
        assertTrue(File(card, "Saves/mGBA/Emerald.srm").isFile)
        assertFalse("secrets never travel", File(card, "Config/romm.json").exists())
        assertFalse(File(card, "Config/retroachievements.toml").exists())
        assertFalse("RetroAchievements' sign-in stays", File(card, "Saves/RetroAchievements").exists())
        assertFalse("one-shot markers stay", File(card, "Saves/abc.mgba.skip-resume.once").exists())
        assertFalse(File(card, "Labels/old.png.carried").exists())
        assertFalse("this handheld's own", FolderSync.travels("Saves/abc.mgba.retroarch-import.state.auto"))
        assertFalse(FolderSync.travels("Saves/abc.mgba.state-export-protected"))
        assertTrue(FolderSync.travels("Saves/abc.mgba.state"))
    }

    @Test fun artworkSyncPreservesUnrelatedBaseAndDoesNotTouchSaves() {
        val (mine, card) = dirs()
        put(mine, "Saves/mGBA/Emerald.srm", "first")
        put(mine, "Labels/a.png", "old")
        val base = FolderSync.Base()
        val private = FolderSync.PrivateSide(mine)
        FolderSync.sync(private, Card(card), base)
        val saved = base.entries.getValue("Saves/mGBA/Emerald.srm")
        put(mine, "Saves/mGBA/Emerald.srm", "local new", 3_000_000L)
        put(card, "Saves/mGBA/Emerald.srm", "remote new", 2_000_000L)
        put(mine, "Labels/b.png", "new")
        val report = FolderSync.sync(private, Card(card), base, scope = FolderSync.Scope.ARTWORK)
        assertEquals(1, report.toFolder)
        assertEquals(saved, base.entries["Saves/mGBA/Emerald.srm"])
        assertEquals("remote new", File(card, "Saves/mGBA/Emerald.srm").readText())
        assertEquals("local new", File(mine, "Saves/mGBA/Emerald.srm").readText())
        assertFalse(private.list(FolderSync.Scope.ARTWORK).containsKey("Saves/mGBA/Emerald.srm"))
        assertTrue(File(card, "Labels/b.png").isFile)
        assertEquals(1, FolderSync.sync(private, Card(card), base, scope = FolderSync.Scope.SAVES).conflicts)
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

    @Test fun theRetroAchievementsCacheLeavesTheFolderWithoutDeletingAnything() {
        val (mine, card) = dirs()
        put(mine, "Saves/mGBA/Emerald.srm", "save")
        put(mine, "Saves/RetroAchievements/0cc175b9/auth.json", "token")
        val base = FolderSync.Base()
        FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), base, FolderSync.Start.MERGE)
        // What a beta38 handheld left on the card, and in its base.
        put(card, "Saves/RetroAchievements/0cc175b9/auth.json", "token")
        put(card, "Saves/RetroAchievements/0cc175b9/badges/1234.png", "badge")
        put(card, "Saves/abc.mgba.retroarch-export.state.auto", "state")
        base.entries["Saves/RetroAchievements/0cc175b9/auth.json"] =
            FolderSync.Entry(5, 1_000_000L, 5, 1_000_000L, FolderSync.sha1("token".toByteArray()))
        val tombs = HashMap<String, FolderSync.Tomb>()
        val report = FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), base, tombs = tombs)
        assertEquals(3, report.stale)
        assertFalse(File(card, "Saves/RetroAchievements/0cc175b9/auth.json").exists())
        assertFalse(File(card, "Saves/abc.mgba.retroarch-export.state.auto").exists())
        assertTrue("the handheld keeps its own", File(mine, "Saves/RetroAchievements/0cc175b9/auth.json").isFile)
        assertTrue(File(card, "Saves/mGBA/Emerald.srm").isFile)
        assertTrue("no deletion noted for other handhelds", tombs.isEmpty())
        assertTrue(base.entries.keys.none { it.startsWith("Saves/RetroAchievements/") })
        assertEquals(0, report.removed)
    }

    @Test fun aGamesFilesComeInBeforeTheLabels() {
        val (mine, card) = dirs()
        put(card, "Labels/GBA/Emerald.png", "label")
        put(card, "Saves/mGBA/Emerald.srm", "save")
        put(card, "States/mGBA/Emerald.state.auto", "state")
        var seen: Pair<Boolean, Boolean>? = null
        FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), FolderSync.Base(), FolderSync.Start.MERGE,
            launchFilesDone = {
                seen = (File(mine, "Saves/mGBA/Emerald.srm").isFile && File(mine, "States/mGBA/Emerald.state.auto").isFile) to
                    File(mine, "Labels/GBA/Emerald.png").isFile
            })
        assertEquals("saves and states in, labels not yet" , true to false, seen)
        assertTrue(File(mine, "Labels/GBA/Emerald.png").isFile)
    }

    @Test fun twoDifferentSavesAreKeptForTheChoice() {
        val (mine, card) = dirs()
        put(mine, "Saves/mGBA/Emerald.srm", "handheld", modified = 2_000_000L)
        put(card, "Saves/mGBA/Emerald.srm", "folder", modified = 3_000_000L)
        put(mine, "Labels/GBA/Emerald.png", "one", modified = 2_000_000L)
        put(card, "Labels/GBA/Emerald.png", "two", modified = 3_000_000L)
        val report = FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), FolderSync.Base(), FolderSync.Start.MERGE)
        // The newer save in place; the handheld's set aside for the choice.
        assertEquals("folder", File(mine, "Saves/mGBA/Emerald.srm").readText())
        assertEquals(1, report.twoSaves.size)
        val two = report.twoSaves.single()
        assertEquals("Saves/mGBA/Emerald.srm", two.path)
        assertTrue(two.handheldSetAside)
        assertEquals("handheld", String(two.other))
        // Not for labels: the newer one, as before.
        assertEquals(2, report.conflicts)
    }

    @Test fun secondaryCopyYieldsAndResumesWithoutLosingTheBase() {
        val (mine, card) = dirs()
        try {
            put(mine, "Labels/GBA/a.png", "a")
            put(mine, "Labels/GBA/b.png", "b")
            put(card, "Saves/mGBA/Emerald.srm", "new save")
            val base = FolderSync.Base()
            var checks = 0
            val first = FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), base,
                scope = FolderSync.Scope.ARTWORK, shouldYield = { checks++ >= 1 })
            assertTrue(first.yielded)
            assertEquals(1, first.toFolder)
            assertEquals(1, base.entries.size)
            assertFalse(File(mine, "Saves/mGBA/Emerald.srm").exists())
            val launch = FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), base,
                scope = FolderSync.Scope.LAUNCH)
            assertEquals(1, launch.toPrivate)
            assertEquals("new save", File(mine, "Saves/mGBA/Emerald.srm").readText())
            val rest = FolderSync.sync(FolderSync.PrivateSide(mine), Card(card), base,
                scope = FolderSync.Scope.REST)
            assertFalse(rest.yielded)
            assertEquals(1, rest.toFolder)
            assertEquals(3, base.entries.size)
            assertEquals("a", File(card, "Labels/GBA/a.png").readText())
            assertEquals("b", File(card, "Labels/GBA/b.png").readText())
        } finally { mine.deleteRecursively(); card.deleteRecursively() }
    }

    @Test fun launchAndRestCoverTheSamePathsAsAll() {
        for (path in listOf("Saves/mGBA/game.srm", "States/mGBA/game.state",
            "Cheats/game.cht", "CustomCheats/game.cht", "GameEmulation/game.json",
            "Labels/GBA/game.png", "Studio/game.shell", "slot-konkr-settings.json",
            "Config/romm.json", "Saves/RetroAchievements/auth.json")) {
            val launch = FolderSync.Scope.LAUNCH.includes(path)
            val rest = FolderSync.Scope.REST.includes(path)
            assertFalse(launch && rest)
            assertEquals(FolderSync.Scope.ALL.includes(path), launch || rest)
        }
        assertEquals("a9993e364706816aba3e25717850c26c9cd0d89d", FolderSync.sha1("abc".toByteArray()))
    }
}
