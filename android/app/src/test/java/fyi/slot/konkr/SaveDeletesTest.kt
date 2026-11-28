package fyi.slot.konkr

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.nio.file.Files

/** A save or state deleted outside slot. (2.5): what counts, and for which game. */
class SaveDeletesTest {
    private val id = "2633759f15615fee4fe64bc17654470be4cfe952985703149b4f1f5cc7a2fcf0"

    @Test fun savesAndAutoStatesCountNotPolaroidsNorTheRest() {
        assertEquals(SaveDeletes.Kind.SAVE, SaveDeletes.kindOf("Saves/mGBA/Golden Sun.srm"))
        assertEquals(SaveDeletes.Kind.SAVE, SaveDeletes.kindOf("Saves/Gambatte/Tetris.rtc"))
        assertEquals(SaveDeletes.Kind.STATE, SaveDeletes.kindOf("States/Genesis Plus GX/Sonic.state.auto"))
        assertEquals(SaveDeletes.Kind.STATE, SaveDeletes.kindOf("Saves/$id.mgba.state"))
        assertEquals(SaveDeletes.Kind.STATE, SaveDeletes.kindOf("Saves/$id.mgba.retroarch-export.state.auto"))
        assertEquals(SaveDeletes.Kind.STATE, SaveDeletes.kindOf("Saves/$id.mgba.gen-123.state"))
        assertNull(SaveDeletes.kindOf("Saves/ManualExports/$id.mgba.1.state"))
        assertNull(SaveDeletes.kindOf("Saves/ManualLinks/$id.mgba.links"))
        assertNull(SaveDeletes.kindOf("Saves/$id.mgba.state-generation.active"))
        assertNull(SaveDeletes.kindOf("Saves/Unknown/Golden Sun.srm"))
        assertNull(SaveDeletes.kindOf("Labels/abc.png"))
        assertNull(SaveDeletes.kindOf("konkr-favorites.json"))
    }

    @Test fun lynxAndPokemonMiniSavesAreSaves() {
        assertEquals(SaveDeletes.Kind.SAVE, SaveDeletes.kindOf("Saves/Handy/Chip's Challenge.eeprom"))
        assertEquals(SaveDeletes.Kind.SAVE, SaveDeletes.kindOf("Saves/PokeMini/Pokemon Puzzle Collection.eep"))
        val mine = SaveDeletes.forGame(listOf("Saves/Handy/Chip's Challenge.eeprom",
            "Saves/Handy/Chip's Challenge.srm"), "Handy", "Chip's Challenge", id, shared = false)
        assertEquals(mapOf("Saves/Handy/Chip's Challenge.eeprom" to SaveDeletes.Kind.SAVE), mine)
        assertEquals("eeprom", Systems.saveExtension("Handy"))
        assertEquals("eep", Systems.saveExtension("PokeMini"))
        assertEquals("srm", Systems.saveExtension("Beetle Lynx"))
        assertEquals("flash", Systems.saveExtension("Beetle NeoPop"))
        assertEquals("ngf", Systems.saveExtension("RACE"))
        assertEquals("Game", Systems.saveStem("Game.eeprom"))
    }

    @Test fun neoGeoSavesAreSaves() {
        assertEquals(SaveDeletes.Kind.SAVE, SaveDeletes.kindOf("Saves/FinalBurn Neo/mslug.fs"))
        assertEquals(SaveDeletes.Kind.SAVE, SaveDeletes.kindOf("Saves/FinalBurn Neo/mslug.memcard"))
        assertEquals(SaveDeletes.Kind.SAVE, SaveDeletes.kindOf("Saves/Geolith/mslug.mcr"))
        assertEquals(SaveDeletes.Kind.SAVE, SaveDeletes.kindOf("Saves/Geolith/mslug.nv"))
        assertEquals("fs", Systems.saveExtension("FinalBurn Neo"))
        assertEquals("mcr", Systems.saveExtension("Geolith"))
        assertEquals("srm", Systems.saveExtension("Beetle PCE Fast"))
        assertEquals("mslug", Systems.saveStem("mslug.memcard"))
    }

    @Test fun onlyThisGamesFilesAreAskedAbout() {
        val paths = listOf("Saves/mGBA/Golden Sun.srm", "Saves/gpSP/Golden Sun.srm",
            "States/mGBA/Golden Sun.state.auto", "Saves/mGBA/Golden Sun 2.srm",
            "Saves/$id.mgba.state", "Saves/$id.gpsp.state", "Saves/$id.state")
        val mine = SaveDeletes.forGame(paths, "mGBA", "Golden Sun", id, shared = false)
        assertEquals(mapOf("Saves/mGBA/Golden Sun.srm" to SaveDeletes.Kind.SAVE,
            "States/mGBA/Golden Sun.state.auto" to SaveDeletes.Kind.STATE,
            "Saves/$id.mgba.state" to SaveDeletes.Kind.STATE,
            "Saves/$id.state" to SaveDeletes.Kind.STATE), mine)
        // Share Core Saves: the other core's save is this game's save too.
        val shared = SaveDeletes.forGame(paths, "mGBA", "Golden Sun", id, shared = true)
        assertTrue("Saves/gpSP/Golden Sun.srm" in shared)
        assertTrue("Saves/$id.gpsp.state" !in shared)
    }

    @Test fun theWindowSaysWhatWhereAndForWhichGame() {
        val both = SaveDeletes.Gone(setOf(SaveDeletes.Kind.SAVE, SaveDeletes.Kind.STATE),
            setOf(SaveDeletes.Where.RETROARCH), emptySet())
        assertEquals("The save and save state of Golden Sun were deleted from your RetroArch folder. " +
            "Delete it from slot. too, or put it back?", SaveDeletes.message("Golden Sun", both))
        val state = SaveDeletes.Gone(setOf(SaveDeletes.Kind.STATE), setOf(SaveDeletes.Where.SLOT_FOLDER), emptySet())
        assertEquals("The save state of Sonic was deleted from your slot. folder. " +
            "Delete it from slot. too, or put it back?", SaveDeletes.message("Sonic", state))
        val save = SaveDeletes.Gone(setOf(SaveDeletes.Kind.SAVE), setOf(SaveDeletes.Where.SLOT_FOLDER), emptySet())
        assertEquals("The save of F-Zero was deleted from your slot. folder. " +
            "Delete it from slot. too, with the game's save state, or put it back?",
            SaveDeletes.message("F-Zero", save, withState = true))
    }

    @Test fun aDeletedSaveIsKeptAsideAndItsRetroArchCopyMarked() {
        val files = Files.createTempDirectory("slot-files").toFile()
        val save = File(files, "Saves/mGBA/Golden Sun.srm").apply { parentFile.mkdirs(); writeText("progress") }
        SaveDeletes.retire(files, save, "mGBA", 0L)
        assertTrue(!save.exists())
        val kept = File(files, "Saves/FolderBackups/mGBA").walkTopDown().single { it.isFile }
        assertEquals("progress", kept.readText())
        assertTrue("backups never travel", !FolderSync.travels(kept.relativeTo(files).path))

        SaveDeletes.markRetroArch(files, "Saves/mGBA/Golden Sun.srm")
        SaveDeletes.markRetroArch(files, "Saves/$id.mgba.state")
        assertTrue(File(files, "SaveDeletes/mGBA/Golden Sun.srm").isFile)
        assertTrue(File(files, "SaveDeletes/mGBA/Golden Sun.rtc").isFile)
        assertTrue(File(files, "SaveDeletes/id/$id.mgba").isFile)
        SaveDeletes.markRetroArch(files, "Saves/$id.gpsp.retroarch-export.state.auto")
        assertTrue("a copy is not the state", !File(files, "SaveDeletes/id/$id.gpsp").exists())
        assertTrue("marks never travel", !FolderSync.travels("SaveDeletes/mGBA/Golden Sun.srm"))
        assertEquals("Genesis Plus GX", SaveDeletes.coreOf("Saves/$id.genesis_plus_gx.state"))
    }
}
