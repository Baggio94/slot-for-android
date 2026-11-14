package fyi.slot.konkr

import android.app.Activity
import android.content.Intent
import android.os.Bundle

/**
 * slot. as the home app: brings back the slot. already running instead of
 * Android starting a second one in its home task. MainActivity is
 * singleTask, so it comes to the front with onNewIntent.
 */
class SlotHomeActivity : Activity() {
    companion object {
        /** Set on the intent MainActivity receives: Android's Home. */
        const val EXTRA_HOME = "fyi.slot.konkr.HOME"
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        startActivity(Intent(this, MainActivity::class.java)
            .setAction(Intent.ACTION_MAIN)
            .addCategory(Intent.CATEGORY_LAUNCHER)
            .putExtra(EXTRA_HOME, true)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        finish()
        @Suppress("DEPRECATION")
        overridePendingTransition(0, 0)
    }
}
