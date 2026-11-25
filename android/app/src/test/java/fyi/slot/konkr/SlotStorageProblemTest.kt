package fyi.slot.konkr

import org.junit.Assert.*
import org.junit.Test
import java.io.IOException

class SlotStorageProblemTest {
    @Test fun errorsAreSpecificOnlyWhenSupported() {
        assertEquals("SD CARD READ-ONLY", SlotStorageProblem.message(listOf("EROFS")))
        assertEquals("SD CARD READ-ONLY", SlotStorageProblem.message(listOf("Failed to mkdir"), true))
        assertEquals("FOLDER ACCESS DENIED", SlotStorageProblem.message(listOf("SecurityException: denied")))
        assertEquals("NOT ENOUGH SPACE", SlotStorageProblem.message(listOf("ENOSPC")))
        assertEquals("COPY FAILED", SlotStorageProblem.message(listOf("Failed to mkdir")))
        assertEquals("COPY FAILED", SlotStorageProblem.message(listOf("EIO")))
        assertEquals("SD CARD READ-ONLY", SlotStorageProblem.message(listOf(
            SlotStorageProblem.describe(IOException("provider", IOException("EROFS"))))))
    }

    @Test fun backingMountCanBeReadOnlyWhileFuseIsWritable() {
        val uuid = "6AFD-80FF"
        assertTrue(SlotStorageProblem.readOnlyMount(
            "28 1 179:25 / /mnt/media_rw/$uuid ro,nosuid - exfat /dev/block/mmcblk1p1 ro,errors=remount-ro", uuid))
        assertFalse(SlotStorageProblem.readOnlyMount(
            "29 1 0:44 / /storage/$uuid rw,nosuid - fuse /dev/fuse rw", uuid))
        assertFalse(SlotStorageProblem.readOnlyMount(
            "28 1 179:25 / /mnt/media_rw/$uuid rw,nosuid - exfat /dev/block/mmcblk1p1 rw,errors=remount-ro", uuid))
        assertFalse(SlotStorageProblem.readOnlyMount(
            "28 1 179:25 / /mnt/media_rw/OTHER ro - exfat /dev/block/mmcblk1p1 ro", uuid))
    }
}
