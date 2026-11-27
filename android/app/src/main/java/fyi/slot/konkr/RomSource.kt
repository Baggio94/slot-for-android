package fyi.slot.konkr

import java.io.BufferedInputStream
import java.io.FilterInputStream
import java.io.InputStream
import java.util.Locale
import java.util.zip.ZipInputStream

/**
 * One ROM per cartridge source: a plain cartridge file (Systems.ROM_EXTENSIONS), or the FIRST
 * matching cartridge within a .zip. Non-ROM extras (artwork, README) are
 * ignored. Archives with multiple games should be separated by the user:
 * one cartridge is displayed per ZIP, never a synthetic multi-ROM shelf.
 *
 * Streams only from the SAF provider. The original source is never modified.
 * Bounded uncompressed bytes prevent ZIP bombs and excessive cache storage.
 */
internal object RomSource {
    const val MAX_ROM_BYTES: Long = 64L * 1024 * 1024
    const val MIN_ROM_BYTES = 0x150
    private const val MAX_ENTRIES = 256
    private val supported = Systems.ROM_EXTENSIONS

    data class Header(val extension: String, val bytes: ByteArray, val battery: Boolean? = null)

    /** A Neo Geo game: the .zip is the cartridge, whole (FinalBurn Neo
     *  reads the set by its name), never a ROM to take out of it. */
    class NeoGeoSet : IllegalArgumentException("A Neo Geo set: the ZIP is the cartridge")

    /** The biggest Neo Geo set or .neo file slot. copies (the biggest
     *  commercial ones are about 100 MB). */
    const val MAX_NEOGEO_BYTES: Long = 256L * 1024 * 1024

    /** The most a cartridge of this extension may hold. */
    fun maxRomBytes(ext: String): Long =
        if (Systems.platformFor(ext) == Systems.NEOGEO) MAX_NEOGEO_BYTES else MAX_ROM_BYTES

    /** A Neo Geo set's ROM chip as MAME and FinalBurn Neo name them:
     *  201-p1.p1, 201-c1.c1, 232-m1.m1, k2k4s-p1.bin... */
    private val NEOGEO_CHIP = Regex("""^(p|c|m|s|v|sp|ep)[0-9]{1,2}$""")
    private val NEOGEO_CHIP_STEM = Regex("""[-_](p|c|m|s|v)[0-9]{1,2}[a-z]?$""")
    internal fun neoGeoChip(entryName: String): Boolean {
        val name = entryName.substringAfterLast('/').lowercase(Locale.ROOT)
        val ext = name.substringAfterLast('.', "")
        val stem = name.substringBeforeLast('.')
        return NEOGEO_CHIP.matches(ext) ||
            (ext in setOf("bin", "rom") && NEOGEO_CHIP_STEM.containsMatchIn(stem))
    }

    fun isContainer(filename: String): Boolean =
        filename.substringAfterLast('.', "").lowercase(Locale.ROOT) == "zip"

    fun isAcceptedFile(filename: String): Boolean =
        filename.substringAfterLast('.', "").lowercase(Locale.ROOT) in supported + "zip"

    /**
     * The save/RTC/auto-state basename must remain the OUTER ZIP basename
     * (RetroArch convention) while the actual cartridge extension must be
     * the embedded one. In particular, a .zip containing a .gba must be
     * eligible for opt-in shared GBA saves, never treated as a GB cartridge.
     * The ROM identity itself remains the original SAF URI.
     */
    fun archiveSaveFilename(zipFilename: String, embeddedExtension: String): String {
        require(isContainer(zipFilename)) { "Expected a cartridge ZIP" }
        val ext = embeddedExtension.lowercase(Locale.ROOT)
        require(ext in supported) { "Unsupported embedded cartridge type" }
        return "${zipFilename.substringBeforeLast('.')}.$ext"
    }

    private fun extension(filename: String): String =
        filename.substringAfterLast('.', "").lowercase(Locale.ROOT)

    /**
     * The ZIP entry is exposed only while the callback is running; callers
     * must consume it synchronously and never keep a reference to the stream.
     * No entry path is written to the filesystem (zip-slip is impossible).
     */
    fun <T> useRom(source: InputStream, filename: String,
                   operation: (InputStream, String) -> T): T {
        val ext = extension(filename)
        require(ext in supported + "zip") { "Unsupported cartridge type" }
        if (ext != "zip") {
            return source.use { operation(Limited(it, maxRomBytes(ext)), ext) }
        }
        return ZipInputStream(BufferedInputStream(source)).use { zip ->
            var visited = 0
            while (visited++ < MAX_ENTRIES) {
                val entry = zip.nextEntry ?: break
                if (entry.isDirectory) continue
                val romExt = extension(entry.name.substringAfterLast('/'))
                // A Neo Geo set's chips are no cartridge of their own: the
                // whole ZIP is the game, known by its first chip.
                if (romExt !in supported && neoGeoChip(entry.name)) throw NeoGeoSet()
                if (romExt !in supported || !Systems.acceptsName(entry.name)) continue
                require(entry.size == -1L || entry.size in Systems.minRomBytes(romExt).toLong()..maxRomBytes(romExt)) {
                    "ZIP ROM exceeds safe size limits"
                }
                // Preserve a deterministic first-ROM choice across scan,
                // warming, artwork/RA fingerprints and emulator launch.
                return@use operation(Limited(zip, maxRomBytes(romExt)), romExt)
            }
            throw IllegalArgumentException("ZIP does not contain a cartridge ROM")
        }
    }

    fun inspect(source: InputStream, filename: String): Header = inspectInternal(source, filename, false)

    fun inspectWithBattery(source: InputStream, filename: String): Header = inspectInternal(source, filename, true)

    private fun inspectInternal(source: InputStream, filename: String, battery: Boolean): Header =
        useRom(source, filename) { stream, ext ->
            // Home console headers sit further in (SNES up to 64 KiB).
            val head = ByteArray(Systems.headerBytes(ext))
            var filled = 0
            while (filled < head.size) {
                val n = stream.read(head, filled, head.size - filled)
                if (n < 0) break
                if (n == 0) continue
                filled += n
            }
            require(filled >= Systems.minRomBytes(ext)) { "Cartridge ROM header is too small" }
            val header = head.copyOf(filled)
            val hasBattery = if (!battery) null else when (Systems.platformFor(ext)) {
                Systems.WS, "GBA" -> {
                    val whole = java.io.SequenceInputStream(java.io.ByteArrayInputStream(header), stream)
                    if (Systems.platformFor(ext) == Systems.WS) Systems.wsBattery(whole)
                    else Systems.gbaBattery(whole)
                }
                else -> null
            }
            Header(ext, header, hasBattery)
        }

    /** A Neo Geo set copied whole into [target], at most MAX_NEOGEO_BYTES;
     *  [progress] (the bytes so far) false stops it. The bytes copied. */
    fun copyNeoGeoSet(source: InputStream, target: java.io.File,
                      progress: (Long) -> Boolean = { true }): Long {
        var copied = 0L
        target.outputStream().buffered().use { output ->
            val buffer = ByteArray(64 * 1024)
            while (true) {
                val n = source.read(buffer)
                if (n < 0) break
                copied += n
                require(copied <= MAX_NEOGEO_BYTES) { "Neo Geo set exceeds 256 MiB" }
                output.write(buffer, 0, n)
                if (!progress(copied)) throw InterruptedException()
            }
        }
        require(copied >= MIN_ROM_BYTES) { "Neo Geo set too small" }
        return copied
    }

    private class Limited(source: InputStream, private val limit: Long): FilterInputStream(source) {
        private var received = 0L
        override fun read(): Int {
            val byte = super.read()
            if (byte >= 0) {
                received++
                require(received <= limit) { "ROM exceeds ${limit shr 20} MiB uncompressed" }
            }
            return byte
        }
        override fun read(b: ByteArray, off: Int, len: Int): Int {
            val n = `in`.read(b, off, len)
            if (n > 0) {
                received += n
                require(received <= limit) { "ROM exceeds ${limit shr 20} MiB uncompressed" }
            }
            return n
        }
        // The owner, not this limited view, closes the SAF/ZIP stream.
        override fun close() {}
    }
}
