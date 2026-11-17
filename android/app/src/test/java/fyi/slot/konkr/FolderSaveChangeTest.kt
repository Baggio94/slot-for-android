package fyi.slot.konkr

import fyi.slot.konkr.RetroArchStorage.FolderSaveAction
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.file.Files

/** Issue #116: a save put in the save folder outside slot. is not silently
 *  covered by slot.'s own state any more. */
class FolderSaveChangeTest {
    private val slotSave = ByteArray(8192) { 1 }
    private val pasted = ByteArray(8192) { 2 }
    private val print = { bytes: ByteArray -> RetroArchStorage.digestOf(bytes) }

    @Test fun aPastedSaveAsksWhenSlotWouldResumeItsOwnState() {
        assertEquals(FolderSaveAction.ASK, RetroArchStorage.folderSaveAction(
            pasted, slotSave, print(slotSave), resumable = true, stateOutside = false))
    }

    @Test fun aGameNeverPlayedInSlotJustUsesTheFolderSave() {
        assertEquals(FolderSaveAction.NONE, RetroArchStorage.folderSaveAction(
            pasted, null, null, resumable = false, stateOutside = false))
    }

    @Test fun anOlderSaveIsSeenToo() {
        // Contents, never dates: an older save pasted back still asks.
        val older = ByteArray(8192) { 0 }
        assertEquals(FolderSaveAction.ASK, RetroArchStorage.folderSaveAction(
            older, slotSave, print(slotSave), resumable = true, stateOutside = false))
    }

    @Test fun retroArchWithItsOwnAutoStateResumesItWithoutAsking() {
        assertEquals(FolderSaveAction.NONE, RetroArchStorage.folderSaveAction(
            pasted, slotSave, print(slotSave), resumable = true, stateOutside = true))
    }

    @Test fun whatSlotWroteItselfNeverAsks() {
        assertEquals(FolderSaveAction.NONE, RetroArchStorage.folderSaveAction(
            slotSave, slotSave, print(slotSave), resumable = true, stateOutside = false))
    }

    @Test fun aTradeWhoseExportFailedIsNotLostAtTheNextStart() {
        val beforeTrade = slotSave
        val afterTrade = ByteArray(8192) { 3 }
        // The folder still holds what slot. last synced; slot.'s copy has the trade.
        assertEquals(FolderSaveAction.KEEP_SLOT_SAVE, RetroArchStorage.folderSaveAction(
            beforeTrade, afterTrade, print(beforeTrade), resumable = true, stateOutside = false))
    }

    @Test fun shareCoreSavesMirrorsNeverAskNorKeepTheOlderPrivateCopy() {
        // gpSP played; slot. wrote the mGBA mirror; mGBA's private copy is older.
        val mirror = pasted
        assertEquals(FolderSaveAction.NONE, RetroArchStorage.folderSaveAction(
            mirror, slotSave, print(mirror), resumable = true, stateOutside = false,
            shared = true))
        // Before 2.4.2 (no print), the shared journal says it is slot.'s.
        val journal = ByteArray(131072) { 0xff.toByte() }.also { mirror.copyInto(it) }
        assertEquals(FolderSaveAction.NONE, RetroArchStorage.folderSaveAction(
            mirror, slotSave, null, resumable = true, stateOutside = false,
            shared = true, journal = journal))
    }

    @Test fun beforeAnyPrintTheFolderIsComparedWithSlotsCopy() {
        assertEquals(FolderSaveAction.NONE, RetroArchStorage.folderSaveAction(
            slotSave, slotSave.copyOf(), null, resumable = true, stateOutside = false))
        assertEquals(FolderSaveAction.ASK, RetroArchStorage.folderSaveAction(
            pasted, slotSave, null, resumable = true, stateOutside = false))
    }

    @Test fun aStatePutInTheFolderIsSeenWhateverItsDate() {
        val state = ByteArray(64) { 4 }
        val other = ByteArray(64) { 5 }
        assertFalse(RetroArchStorage.stateChangedOutside(null, print(state), null))
        assertFalse(RetroArchStorage.stateChangedOutside(state, print(state), null))
        assertTrue(RetroArchStorage.stateChangedOutside(other, print(state), null))
        // No print yet: slot.'s own export decides.
        assertFalse(RetroArchStorage.stateChangedOutside(state, null, matchesSlotExport = true))
        assertTrue(RetroArchStorage.stateChangedOutside(state, null, matchesSlotExport = false))
        assertTrue(RetroArchStorage.stateChangedOutside(state, null, matchesSlotExport = null))
    }

    @Test fun printsBelongToOneFolder() {
        val files = Files.createTempDirectory("slot-files").toFile()
        try {
            RetroArchStorage.rememberSynced(files, "tree-a", "mGBA", "Game.srm", slotSave)
            assertEquals(print(slotSave),
                RetroArchStorage.lastSynced(files, "tree-a", "mGBA", "Game.srm"))
            assertNull(RetroArchStorage.lastSynced(files, "tree-b", "mGBA", "Game.srm"))
            assertNull(RetroArchStorage.lastSynced(files, "tree-a", "gpSP", "Game.srm"))
        } finally {
            files.deleteRecursively()
        }
    }
}
