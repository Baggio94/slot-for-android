package fyi.slot.konkr

import org.junit.Assert.*
import org.junit.Test

class BiosProvenanceTest {
    @Test fun removesOnlyUnchangedKnownLocalCopiesMissingFromFolder() {
        assertTrue(BiosLibrary.canRemove("local", "local", false))
        assertFalse(BiosLibrary.canRemove(null, "romm", false))
        assertFalse(BiosLibrary.canRemove("local", "changed", false))
        assertFalse(BiosLibrary.canRemove("local", "local", true))
        assertFalse(BiosLibrary.canRemove("local", null, false))
    }
}
