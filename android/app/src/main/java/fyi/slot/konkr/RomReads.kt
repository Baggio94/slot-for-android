package fyi.slot.konkr

import android.net.Uri
import java.util.concurrent.Semaphore

/** Whole-ROM reads for the labels' checksums (3.0.1-beta11): side by side,
 *  but bounded per storage. Internal storage keeps up with four; an SD card
 *  slows down past two (its reads go one after the other in the card). */
internal object RomReads {
    private val internal = Semaphore(4)
    private val card = Semaphore(2)

    internal fun onInternalStorage(uri: String): Boolean =
        Uri.decode(uri).let { it.contains("/document/primary:") || it.contains("/tree/primary:") }

    /** How many ROM headers a library scan reads at once (3.0.1-beta12):
     *  four on internal storage, two on an SD card, as the checksums. */
    internal fun headerReaders(treeUri: String): Int = if (onInternalStorage(treeUri)) 4 else 2

    fun <T> read(uri: String, block: () -> T): T {
        val gate = if (onInternalStorage(uri)) internal else card
        gate.acquire()
        try { return block() } finally { gate.release() }
    }
}
