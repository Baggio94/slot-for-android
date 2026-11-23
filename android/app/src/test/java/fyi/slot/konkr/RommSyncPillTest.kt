package fyi.slot.konkr

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** RomM's two queued uploads yield one accurate, compact success pill. */
class RommSyncPillTest {
    @Test fun saveOnlyShowsSaveSynced() {
        assertEquals("SAVE SYNCED", TaskPill.rommSyncWords(saveUploaded = true, stateUploaded = false))
    }

    @Test fun stateOnlyShowsStateSynced() {
        assertEquals("STATE SYNCED", TaskPill.rommSyncWords(saveUploaded = false, stateUploaded = true))
    }

    @Test fun bothUploadsProduceOneCombinedPill() {
        assertEquals("SAVE + STATE SYNCED",
            TaskPill.rommSyncWords(saveUploaded = true, stateUploaded = true))
    }

    @Test fun nothingUploadedProducesNoMisleadingConfirmation() {
        assertNull(TaskPill.rommSyncWords(saveUploaded = false, stateUploaded = false))
    }
}
