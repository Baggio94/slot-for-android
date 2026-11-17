package fyi.slot.konkr

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract
import java.io.File

/**
 * Safely stages user-owned BIOS into mGBA's private libretro system directory.
 * The selected SAF tree is read-only; only known original Slot BIOS names
 * are considered. Called from the same single-thread executor as ROM staging.
 */
internal object BiosLibrary {
    private val lengths = mapOf(
        "gba_bios.bin" to 16_384,
        "gbc_bios.bin" to 2_304,
        "gb_bios.bin" to 256
    )

    /** The BIOS a system's games can use (none is required). */
    fun namesFor(platform: String): List<String> = when (platform) {
        Systems.GBA -> listOf("gba_bios.bin")
        Systems.GBC -> listOf("gbc_bios.bin")
        Systems.GB -> listOf("gb_bios.bin")
        else -> emptyList()
    }

    /** Short system names for "BIOS added: GBA." */
    fun systemOf(name: String): String = when (name) {
        "gba_bios.bin" -> "GBA"
        "gbc_bios.bin" -> "GBC"
        else -> "GB"
    }

    /** Which of these systems' BIOS slot. doesn't have yet. */
    fun missing(context: Context, platforms: Set<String>): Set<String> {
        val dest = File(context.filesDir, "BIOS")
        return platforms.flatMap(::namesFor).filterTo(linkedSetOf()) { !File(dest, it).isFile }
    }

    internal fun valid(name: String, bytes: ByteArray): Boolean {
        val expected = lengths[name] ?: return false
        if (bytes.size != expected) return false
        return name != "gba_bios.bin" || bytes[0] == 0x18.toByte()
    }

    /** A BIOS from the player's RomM server, kept only where slot. has none:
     *  one already there, from the BIOS folder or anywhere, is never
     *  replaced (Siham 2026-11-17). */
    fun addIfMissing(context: Context, name: String, bytes: ByteArray): Boolean {
        if (!valid(name, bytes)) return false
        val dest = File(context.filesDir, "BIOS").apply { mkdirs() }
        val target = File(dest, name)
        if (target.exists()) return false
        val temp = File(dest, ".$name.tmp")
        try {
            temp.outputStream().use { out -> out.write(bytes); out.fd.sync() }
            check(temp.renameTo(target)) { "Could not install $name" }
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
                val name = cursor.getString(nameCol)?.lowercase() ?: continue
                val expected = lengths[name] ?: continue
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
                if (bytes.size != expected) continue
                // Original Slot also recognizes the standard GBA BIOS prefix.
                if (name == "gba_bios.bin" && bytes[0] != 0x18.toByte()) continue
                found[name] = bytes
            }
        } ?: error("Cannot read selected BIOS folder")

        require(found.isNotEmpty()) {
            "No valid gba_bios.bin, gbc_bios.bin or gb_bios.bin found"
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
            for (name in lengths.keys - found.keys) {
                File(dest, name).delete()
            }
        } finally {
            pending.values.forEach { it.delete() }
        }
        return found.keys.toList()
    }
}
