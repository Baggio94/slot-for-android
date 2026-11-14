package fyi.slot.konkr

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract
import java.io.File
import java.io.InputStream
import java.util.Locale
import java.util.zip.ZipInputStream

/**
 * Stages the player's own PICO-8 (the Raspberry Pi build) from the BIOS
 * folder into BIOS/pico-8, where slot.'s PICO-8 core (cores/pico8real)
 * loads it. It may sit at the folder's top (pico8_64 and pico8.dat), in a
 * pico-8 folder there, or still zipped as Lexaloffle ships it
 * (pico-8_0.2.7_raspi.zip). A stamp of the sources skips the copy on the
 * next start when nothing changed. Read-only on the user's folder.
 */
internal object Pico8Install {
    const val DIR = "pico-8"
    private val FILES = listOf("pico8_64", "pico8.dat")
    private const val MAX_BYTES = 64L * 1024 * 1024
    private const val STAMP = ".source"

    private data class Doc(val uri: Uri, val name: String, val size: Long, val modified: Long, val dir: Boolean)

    private fun list(context: Context, tree: Uri, id: String): List<Doc> {
        val columns = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
            DocumentsContract.Document.COLUMN_SIZE,
            DocumentsContract.Document.COLUMN_LAST_MODIFIED
        )
        val out = ArrayList<Doc>()
        val children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, id)
        // Null is a provider not ready yet, not a folder without PICO-8:
        // throwing keeps the staged copy (the startup restore retries).
        val cursor = context.contentResolver.query(children, columns, null, null, null)
            ?: error("Cannot read the BIOS folder")
        cursor.use { c ->
            while (c.moveToNext()) {
                val docId = c.getString(0) ?: continue
                val name = c.getString(1) ?: continue
                val size = if (c.isNull(3)) -1L else c.getLong(3)
                val modified = if (c.isNull(4)) -1L else c.getLong(4)
                out += Doc(DocumentsContract.buildDocumentUriUsingTree(tree, docId), name, size, modified,
                    c.getString(2) == DocumentsContract.Document.MIME_TYPE_DIR)
            }
        }
        return out
    }

    private fun docId(doc: Doc): String = DocumentsContract.getDocumentId(doc.uri)

    /** True when PICO-8 is staged (now or from before). */
    fun importFrom(context: Context, folder: Uri): Boolean {
        val dest = File(context.filesDir, "BIOS/$DIR")
        val top = list(context, folder, DocumentsContract.getTreeDocumentId(folder))
        fun lower(d: Doc) = d.name.lowercase(Locale.ROOT)
        // Loose files: at the top, else in a pico-8 (or pico8) folder.
        var loose = FILES.mapNotNull { f -> top.firstOrNull { !it.dir && lower(it) == f } }
        if (loose.size < FILES.size) {
            top.firstOrNull { it.dir && (lower(it) == "pico-8" || lower(it) == "pico8") }?.let { sub ->
                val inside = list(context, folder, docId(sub))
                loose = FILES.mapNotNull { f -> inside.firstOrNull { !it.dir && lower(it) == f } }
            }
        }
        val zip = top.filter { !it.dir && lower(it).endsWith(".zip") && lower(it).contains("raspi") &&
            (lower(it).startsWith("pico-8") || lower(it).startsWith("pico8")) }
            .maxByOrNull { lower(it) }
        val sources = when {
            loose.size == FILES.size -> loose
            zip != null -> listOf(zip)
            else -> {
                dest.deleteRecursively()
                return false
            }
        }
        val stamp = sources.joinToString("\n") { "${it.name}:${it.size}:${it.modified}" }
        val stampFile = File(dest, STAMP)
        if (FILES.all { File(dest, it).isFile } && stampFile.isFile && stampFile.readText() == stamp) return true
        dest.mkdirs()
        val staged = HashMap<String, File>()
        try {
            val resolver = context.contentResolver
            if (sources === loose) {
                for (doc in loose) {
                    resolver.openInputStream(doc.uri)?.use { stage(dest, lower(doc), it, staged) }
                }
            } else {
                resolver.openInputStream(zip!!.uri)?.use { raw ->
                    ZipInputStream(raw.buffered()).use { z ->
                        while (true) {
                            val entry = z.nextEntry ?: break
                            val name = entry.name.substringAfterLast('/').lowercase(Locale.ROOT)
                            if (!entry.isDirectory && name in FILES && name !in staged) stage(dest, name, z, staged)
                        }
                    }
                }
            }
            check(staged.keys.containsAll(FILES)) { "PICO-8: pico8_64 or pico8.dat missing" }
            check(isArm64Elf(staged.getValue("pico8_64"))) { "PICO-8: pico8_64 is not the 64-bit Raspberry Pi build" }
            for ((name, temp) in staged) check(temp.renameTo(File(dest, name))) { "Could not install $name" }
            staged.clear()
            File(dest, "pico8_64").setExecutable(true, true)
            stampFile.writeText(stamp)
            android.util.Log.i("SlotKonkr", "PICO-8 staged from ${sources.joinToString { it.name }}")
            return true
        } finally {
            staged.values.forEach { it.delete() }
        }
    }

    private fun stage(dest: File, name: String, input: InputStream, staged: MutableMap<String, File>) {
        val temp = File(dest, ".$name.tmp")
        var total = 0L
        temp.outputStream().use { out ->
            val buf = ByteArray(64 * 1024)
            while (true) {
                val n = input.read(buf)
                if (n < 0) break
                total += n
                check(total <= MAX_BYTES) { "PICO-8: $name is too large" }
                out.write(buf, 0, n)
            }
        }
        staged[name] = temp
    }

    private fun isArm64Elf(file: File): Boolean {
        val head = ByteArray(20)
        file.inputStream().use { if (it.read(head) != head.size) return false }
        return head[0] == 0x7f.toByte() && head[1] == 'E'.code.toByte() && head[2] == 'L'.code.toByte() &&
            head[3] == 'F'.code.toByte() && head[4] == 2.toByte() && head[18] == 0xb7.toByte() && head[19] == 0.toByte()
    }
}
