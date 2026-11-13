package fyi.slot.konkr

/** A failed EGL rebuild must not leave the presenter permanently black.
 * Called only on the GL thread with a current context. */
internal class SlotGraphicsRecovery {
    private var nextAttemptAt = 0L
    private var failures = 0

    fun reset() {
        nextAttemptAt = 0L
        failures = 0
    }

    fun attempt(now: Long, restore: () -> Boolean): Boolean {
        if (now < nextAttemptAt) return false
        if (restore()) {
            reset()
            return true
        }
        failures = (failures + 1).coerceAtMost(3)
        nextAttemptAt = now + when (failures) { 1 -> 250L; 2 -> 500L; else -> 1_000L }
        return false
    }
}
