package fyi.slot.konkr

import org.junit.Assert.*
import org.junit.Test

class SlotGraphicsRecoveryTest {
    @Test fun aTransientFailureRetriesAndCanRecoverWithoutRestarting() {
        val recovery = SlotGraphicsRecovery()
        var calls = 0
        val restore = { ++calls >= 2 }
        assertFalse(recovery.attempt(1_000, restore))
        assertFalse(recovery.attempt(1_100, restore))
        assertEquals(1, calls)
        assertTrue(recovery.attempt(1_250, restore))
        assertEquals(2, calls)
    }

    @Test fun persistentFailureIsThrottledButDoesNotDisableFutureRecovery() {
        val recovery = SlotGraphicsRecovery()
        for (time in listOf(0L, 250L, 750L, 1_750L))
            assertFalse(recovery.attempt(time) { false })
        var called = false
        assertFalse(recovery.attempt(2_749) { called = true; true })
        assertFalse(called)
        assertTrue(recovery.attempt(2_750) { true })
        recovery.reset()
        assertTrue(recovery.attempt(0) { true })
    }
}
