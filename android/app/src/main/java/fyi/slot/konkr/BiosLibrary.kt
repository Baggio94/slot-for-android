package fyi.slot.konkr

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract
import java.io.File

/**
 * Safely stages user-owned BIOS into the cores' private libretro system directory.
 * The selected SAF tree is read-only; only known original Slot BIOS names
 * are considered. Called from the same single-thread executor as ROM staging.
 */
internal object BiosLibrary {
    /** Each BIOS slot. takes, by its lower-case name: the name the cores
     *  look for (case matters on Android) and the largest size accepted. */
    private class Kind(val name: String, val max: Int, val platform: String,
                       val valid: (ByteArray) -> Boolean)

    private fun sized(vararg sizes: Int): (ByteArray) -> Boolean = { it.size in sizes }

    /** A Master System BIOS: 8 KB (Japan, 1.3) up to the 256 KB ones with a
     *  built-in game. */
    private val smsBios: (ByteArray) -> Boolean = { it.size in setOf(8_192, 16_384, 32_768,
        65_536, 131_072, 262_144) }

    private val KINDS = listOf(
        Kind("gba_bios.bin", 16_384, Systems.GBA) { it.size == 16_384 && it[0] == 0x18.toByte() },
        Kind("gbc_bios.bin", 2_304, Systems.GBC, sized(2_304)),
        Kind("gb_bios.bin", 256, Systems.GB, sized(256)),
        // Beetle Lynx needs it; Handy uses it when there.
        Kind("lynxboot.img", 512, Systems.LYNX, sized(512)),
        Kind("bios.gg", 1_024, Systems.GG, sized(1_024)),
        Kind("bios_U.sms", 262_144, Systems.SMS, smsBios),
        Kind("bios_E.sms", 262_144, Systems.SMS, smsBios),
        Kind("bios_J.sms", 262_144, Systems.SMS, smsBios),
        // TMSS: Genesis Plus GX reads its first 2 KB and checks "GENESIS OS".
        Kind("bios_MD.bin", 16_384, Systems.MD) { bytes ->
            bytes.size in 2_048..16_384 &&
                String(bytes, 0x120, 10, Charsets.US_ASCII) == "GENESIS OS" },
        Kind("bios.min", 4_096, Systems.POKEMINI, sized(4_096)),
    )

    private fun kindOf(name: String): Kind? =
        KINDS.firstOrNull { it.name.equals(name, ignoreCase = true) }

    /** The BIOS a system's games can use (only the Lynx's Beetle Lynx needs one). */
    fun namesFor(platform: String): List<String> =
        KINDS.filter { it.platform == platform }.map { it.name }

    /** Short system names for "BIOS added: GBA." */
    fun systemOf(name: String): String = when (kindOf(name)?.platform) {
        Systems.GBA -> "GBA"
        Systems.GBC -> "GBC"
        Systems.GB -> "GB"
        Systems.LYNX -> "Lynx"
        Systems.GG -> "Game Gear"
        Systems.SMS -> "Master System"
        Systems.MD -> "Mega Drive"
        Systems.POKEMINI -> "Pokemon mini"
        else -> name
    }

    /** Which of these systems' BIOS slot. doesn't have yet: a Master System
     *  with any of its region BIOS has one. */
    fun missing(context: Context, platforms: Set<String>): Set<String> {
        val dest = File(context.filesDir, "BIOS")
        return platforms.flatMapTo(linkedSetOf()) { platform ->
            val names = namesFor(platform)
            if (names.any { File(dest, it).isFile }) emptyList() else names.map { it.lowercase() }
        }
    }

    internal fun valid(name: String, bytes: ByteArray): Boolean =
        kindOf(name)?.valid?.invoke(bytes) == true

    /** A BIOS from the player's RomM server, kept only where slot. has none:
     *  one already there, from the BIOS folder or anywhere, is never
     *  replaced (Siham 2026-11-17). */
    fun addIfMissing(context: Context, name: String, bytes: ByteArray): Boolean {
        val kind = kindOf(name) ?: return false
        if (!kind.valid(bytes)) return false
        val dest = File(context.filesDir, "BIOS").apply { mkdirs() }
        val target = File(dest, kind.name)
        if (target.exists()) return false
        val temp = File(dest, ".${kind.name}.tmp")
        try {
            temp.outputStream().use { out -> out.write(bytes); out.fd.sync() }
            check(temp.renameTo(target)) { "Could not install ${kind.name}" }
        } finally {
            temp.delete()
        }
        return true
    }

    fun importFrom(context: Context, folder: Uri): List<String> {
        val resolver = context.contentResolver
        val found = linkedMapOf<String, ByteArray>()
        val columns = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
            DocumentsContract.Document.COLUMN_SIZE
        )
        val children = DocumentsContract.buildChildDocumentsUriUsingTree(
            folder, DocumentsContract.getTreeDocumentId(folder)
        )
        resolver.query(children, columns, null, null, null)?.use { cursor ->
            val idCol = cursor.getColumnIndexOrThrow(columns[0])
            val nameCol = cursor.getColumnIndexOrThrow(columns[1])
            val mimeCol = cursor.getColumnIndexOrThrow(columns[2])
            val sizeCol = cursor.getColumnIndexOrThrow(columns[3])
            while (cursor.moveToNext()) {
                if (cursor.getString(mimeCol) ==
                    DocumentsContract.Document.MIME_TYPE_DIR) continue
                val kind = kindOf(cursor.getString(nameCol) ?: continue) ?: continue
                val name = kind.name
                val expected = kind.max
                if (cursor.isNull(sizeCol).not() &&
                    cursor.getLong(sizeCol) > expected.toLong()) {
                    continue
                }
                val uri = DocumentsContract.buildDocumentUriUsingTree(
                    folder, cursor.getString(idCol)
                )
                val bytes = resolver.openInputStream(uri)?.use { stream ->
                    // Android 12 compatibility: bounded read, no Java 9
                    // InputStream.readNBytes API dependency.
                    val data = ByteArray(expected + 1)
                    var count = 0
                    while (count < data.size) {
                        val n = stream.read(data, count, data.size - count)
                        if (n < 0) break
                        if (n == 0) continue
                        count += n
                    }
                    data.copyOf(count)
                } ?: continue
                if (bytes.size > expected || !kind.valid(bytes)) continue
                found[name] = bytes
            }
        } ?: error("Cannot read selected BIOS folder")

        require(found.isNotEmpty()) {
            "No BIOS slot. knows found (gba_bios.bin, gb_bios.bin, gbc_bios.bin, " +
                "lynxboot.img, bios.gg, bios_U.sms, bios_E.sms, bios_J.sms, bios_MD.bin, bios.min)"
        }
        val dest = File(context.filesDir, "BIOS").apply { mkdirs() }
        check(dest.isDirectory) { "BIOS destination unavailable" }

        // Stage every available BIOS first; only publish complete validated
        // files. Never write to the user-owned folder or erase the ROM library.
        val pending = linkedMapOf<String, File>()
        try {
            for ((name, bytes) in found) {
                val temp = File(dest, ".$name.tmp")
                temp.outputStream().use { out -> out.write(bytes) }
                pending[name] = temp
            }
            for ((name, temp) in pending) {
                val target = File(dest, name)
                check(temp.renameTo(target)) { "Could not install $name" }
            }
            // Switching folders should not silently retain a BIOS that no
            // longer exists in the newly selected folder.
            for (name in KINDS.map { it.name } - found.keys) {
                File(dest, name).delete()
            }
        } finally {
            pending.values.forEach { it.delete() }
        }
        return found.keys.toList()
    }
}
