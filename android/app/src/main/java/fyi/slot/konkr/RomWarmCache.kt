package fyi.slot.konkr

import android.content.Context
import android.net.Uri
import android.os.SystemClock
import android.util.Log
import java.io.File
import java.security.MessageDigest

/**
 * Best-effort cartridge ROM warming on an independent, low-priority executor.
 *
 * The real launcher retains sole ownership of core setup, RetroArch imports,
 * save conflict arbitration and all emulation state. This cache ONLY primes
 * the exact private file the original launch path already consumes.
 */
internal object RomWarmCache {
    private const val TAG = "SlotRomWarm"
    private const val MAX_ROM = 64L * 1024 * 1024
    private const val MAX_TOTAL = 256L * 1024 * 1024
    // Home-console ROMs share the same 256 MiB eviction budget as GB/GBA.
    private val CACHE_NAME = Regex("[a-f0-9]{64}\\.(${Systems.ROM_EXTENSIONS.joinToString("|")})")
    /** A key stops the reading for this long; a launch of the same ROM
     *  takes the copy over meanwhile (A is a key too). */
    private const val PAUSE_MS = 700L
    /** A launch waits for the copy only while it moves. */
    private const val STALL_MS = 2_000L
    /** Warmed, never launched: dated a year back, so the cache lets them go
     *  before the games played (a launch dates its ROM now). */
    private const val WARM_AGE_MS = 365L * 24 * 60 * 60 * 1000

    /** The ROM a launch wants: its warm copy goes on, at full speed. */
    val launching = java.util.concurrent.atomic.AtomicReference<String?>(null)
    /** The ROM being copied now, and how far it got. */
    @Volatile private var copying: String? = null
    @Volatile private var copied = 0L

    /** For a launch: when this ROM is being warmed, wait for that copy
     *  rather than make a second. True when the cached ROM is there. Gives
     *  up when the copy stops moving: the launch copies, as before. */
    fun awaitCopy(rawUri: String, target: File): Boolean {
        var last = -1L
        var moved = SystemClock.uptimeMillis()
        while (copying == rawUri) {
            val now = SystemClock.uptimeMillis()
            if (copied != last) { last = copied; moved = now }
            else if (now - moved > STALL_MS) return false
            Thread.sleep(10L)
        }
        return target.isFile && target.length() >= 0x150L
    }

    /** A ROM launched: the cache keeps it before the ones not played since. */
    fun played(target: File) {
        target.setLastModified(System.currentTimeMillis())
    }

    /** [superseded]: another cart is wanted now, the copy stops at once. */
    fun prepare(context: Context, rawUri: String, shouldContinue: () -> Boolean,
                superseded: () -> Boolean = { false }) {
        if (!rawUri.startsWith("content://") || !shouldContinue()) return
        val uri = Uri.parse(rawUri)
        val name = uri.lastPathSegment.orEmpty()
        if (!RomSource.isAcceptedFile(name)) return
        val ext = if (RomSource.isContainer(name)) {
            try {
                context.contentResolver.openInputStream(uri)?.use {
                    RomSource.inspect(it, name).extension
                } ?: return
            } catch (_: Exception) { return }
        } else name.substringAfterLast('.', "").lowercase(java.util.Locale.ROOT)
        val digest = MessageDigest.getInstance("SHA-256")
            .digest(rawUri.toByteArray(Charsets.UTF_8))
            .joinToString("") { "%02x".format(it.toInt() and 0xff) }
        val dir = File(context.cacheDir, "rom-cache").apply { mkdirs() }
        val target = File(dir, "$digest.$ext")
        if (target.isFile && target.length() >= 0x150L) return

        val temporary = File(dir, "$digest.warm.tmp")
        // A key pauses the copy (no reading while slot. answers it), then
        // it goes on if the cart is still the one, or a launch wants it.
        fun goOn(): Boolean {
            if (launching.get() == rawUri || shouldContinue()) return true
            val until = SystemClock.uptimeMillis() + PAUSE_MS
            while (SystemClock.uptimeMillis() < until) {
                Thread.sleep(10L)
                if (launching.get() == rawUri || shouldContinue()) return true
                if (superseded()) return false
            }
            return shouldContinue()
        }
        copying = rawUri
        copied = 0L
        try {
            if (!goOn()) return
            val source = context.contentResolver.openInputStream(uri) ?: return
            var count = 0L
            source.use { input ->
                RomSource.useRom(input, name) { raw, embeddedExt ->
                    require(embeddedExt == ext) { "Cartridge archive entry changed" }
                    temporary.outputStream().buffered().use { output ->
                        val buffer = ByteArray(64 * 1024)
                        while (true) {
                            if (Thread.currentThread().isInterrupted || !goOn())
                                throw InterruptedException()
                            val n = raw.read(buffer)
                            if (n < 0) break
                            count += n
                            copied = count
                            require(count <= MAX_ROM) { "ROM exceeds cache size limit" }
                            output.write(buffer, 0, n)
                            // Yield gently to core, audio, UI and cartridge
                            // artwork; not when a launch is waiting.
                            if ((count and 0x7ffffL) == 0L && launching.get() != rawUri) Thread.sleep(1L)
                        }
                    }
                }
            }
            if (count < 0x150 || !goOn()) return
            // Do not replace a file materialized concurrently by the real
            // launcher; all writes are atomic from the perspective of readers.
            if (!target.exists()) {
                temporary.setLastModified(System.currentTimeMillis() - WARM_AGE_MS)
                if (!temporary.renameTo(target)) return
                Log.i(TAG, "Warmed selected ROM ($count bytes)")
            }
            // Cache-only files. No saves, screenshots, labels or states touched.
            trim(dir, target)
        } catch (error: Exception) {
            Log.w(TAG, "Optional ROM warming skipped", error)
        } finally {
            temporary.delete()
            copying = null
        }
    }

    private fun trim(dir: File, selected: File) {
        val files = dir.listFiles()?.filter {
            it.isFile && CACHE_NAME.matches(it.name)
        }?.sortedByDescending { it.lastModified() } ?: return
        var kept = 0L
        for (file in files) {
            if (file == selected) { kept += file.length(); continue }
            if (kept + file.length() > MAX_TOTAL) file.delete()
            else kept += file.length()
        }
    }
}
