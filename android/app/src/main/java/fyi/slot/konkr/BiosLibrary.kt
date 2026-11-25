package fyi.slot.konkr

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract
import android.util.AtomicFile
import java.security.MessageDigest
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

    /** The BIOS a system's games can use, including explicitly imported BIOS. */
    fun namesFor(platform: String): List<String> =
        KINDS.filter { it.platform == platform }.map { it.name }

    /** Download policy only: activation is independently decided by each core. */
    internal val ROMM_AUTO_PLATFORMS = KINDS.map { it.platform }.toSet()

    internal fun automaticNamesFor(platform: String): List<String> =
        if (platform in ROMM_AUTO_PLATFORMS) namesFor(platform) else emptyList()

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

    /** Check each regional firmware independently. */
    internal fun missingNames(platforms: Set<String>, exists: (String) -> Boolean): Set<String> =
        platforms.flatMapTo(linkedSetOf()) { platform ->
            automaticNamesFor(platform).filterNot(exists).map { it.lowercase(java.util.Locale.ROOT) }
        }

    fun missing(context: Context, platforms: Set<String>): Set<String> {
        val dest = File(context.filesDir, "BIOS")
        return missingNames(platforms) { File(dest, it).isFile }
    }

    internal fun valid(name: String, bytes: ByteArray): Boolean =
        kindOf(name)?.valid?.invoke(bytes) == true

    /** A BIOS from the player's RomM server, kept only where slot. has none:
     *  one already there, from the BIOS folder or anywhere, is never
     *  replaced (Siham 2026-11-17). */
    @Synchronized
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

    private fun fingerprint(bytes: ByteArray): String =
        MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it.toInt() and 255) }

    // Only copies known to have been installed from a local folder are
    // removable. An old untracked copy might be a RomM download.
    internal fun canRemove(owned: String?, current: String?, listed: Boolean): Boolean =
        !listed && owned != null && owned == current

    @Synchronized
    fun importFrom(context: Context, folder: Uri, allowEmpty: Boolean = false): List<String> {
        val resolver = context.contentResolver
        val found = linkedMapOf<String, ByteArray>()
        val listed = hashSetOf<String>()
        check(SlotFolder.volumeMounted(context, SlotFolder.volumeOf(folder))) { "BIOS volume unavailable" }
        val columns = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
            DocumentsContract.Document.COLUMN_SIZE
        )
        val rootId = DocumentsContract.getTreeDocumentId(folder)
        val root = DocumentsContract.buildDocumentUriUsingTree(folder, rootId)
        // An unavailable/deleted tree is not a valid empty BIOS folder.
        val rootReadable = resolver.query(root, arrayOf(DocumentsContract.Document.COLUMN_MIME_TYPE),
            null, null, null)?.use { it.moveToFirst() && it.getString(0) == DocumentsContract.Document.MIME_TYPE_DIR } == true
        check(rootReadable) { "Cannot read selected BIOS folder" }
        val children = DocumentsContract.buildChildDocumentsUriUsingTree(folder, rootId)
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
                listed.add(name)
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
                } ?: error("Cannot read BIOS $name")
                if (bytes.size > expected || !kind.valid(bytes)) continue
                found[name] = bytes
            }
        } ?: error("Cannot read selected BIOS folder")

        require(allowEmpty || found.isNotEmpty()) {
            "No BIOS slot. knows found (gba_bios.bin, gb_bios.bin, gbc_bios.bin, " +
                "lynxboot.img, bios.gg, bios_U.sms, bios_E.sms, bios_J.sms, bios_MD.bin, bios.min)"
        }
        val dest = File(context.filesDir, "BIOS").apply { mkdirs() }
        check(dest.isDirectory) { "BIOS destination unavailable" }

        val owners = context.getSharedPreferences("local-bios-provenance", Context.MODE_PRIVATE)
        // Never delete when the volume has vanished or the listing failed.
        check(SlotFolder.volumeMounted(context, SlotFolder.volumeOf(folder))) { "BIOS volume unavailable" }
        for ((name, bytes) in found) {
            val target = File(dest, name)
            val incoming = fingerprint(bytes)
            val existing = target.takeIf { it.isFile }?.readBytes()?.let(::fingerprint)
            if (existing != incoming) {
                val atomic = AtomicFile(target)
                val stream = atomic.startWrite()
                try {
                    stream.write(bytes)
                    atomic.finishWrite(stream)
                } catch (error: Exception) {
                    atomic.failWrite(stream)
                    throw error
                }
                check(owners.edit().putString(name, incoming).commit()) { "Could not record BIOS provenance" }
            }
            // Same bytes with no provenance: preserve the legacy/RomM copy
            // rather than silently declaring it folder-owned.
        }
        check(SlotFolder.volumeMounted(context, SlotFolder.volumeOf(folder))) { "BIOS volume unavailable" }
        for (kind in KINDS) {
            val name = kind.name
            val owned = owners.getString(name, null) ?: continue
            val target = File(dest, name)
            val current = target.takeIf { it.isFile }?.readBytes()?.let(::fingerprint)
            if (canRemove(owned, current, name in listed)) {
                check(target.delete()) { "Could not remove BIOS $name" }
                owners.edit().remove(name).commit()
            } else if (owned != current) {
                owners.edit().remove(name).commit()
            }
        }
        return found.keys.toList()
    }
}
