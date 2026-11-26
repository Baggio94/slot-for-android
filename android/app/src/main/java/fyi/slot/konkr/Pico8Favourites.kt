package fyi.slot.konkr

import android.content.Context
import android.net.Uri
import android.os.FileObserver
import android.provider.DocumentsContract
import android.util.Log
import java.io.File
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.util.Locale

/**
 * A cart marked as a favourite in Splore is saved in the ROM folder, so it
 * sits on the PICO-8 shelf like any other. PICO-8 keeps its favourites in
 * favourites.txt in its home (Saves/PICO-8/pico-8), one line a cart:
 * "|lootslime-1 |lootslime |1794 |moleman | |lootslime 1.01". The cart is
 * the one Splore cached in bbs/carts, or else the BBS's copy of it.
 *
 * Each cart is saved once: one taken off the shelf later stays off, even
 * while it is still a favourite.
 */
internal object Pico8Favourites {
    private const val TAG = "SlotPico8"
    private const val PREFS = "slot_pico8_favourites"
    private const val SAVED = "saved_ids"
    private const val LIST = "favourites.txt"
    private val FOLDERS = listOf("pico8", "pico-8", "pico")
    private val ID = Regex("[a-z0-9_-]{1,80}")

    data class Favourite(val id: String, val title: String)

    fun home(context: Context) = File(context.filesDir, "Saves/PICO-8/pico-8")

    /** The favourites in the order Splore lists them; malformed lines are skipped. */
    fun parse(text: String): List<Favourite> = text.lineSequence().mapNotNull { line ->
        val cols = line.split('|').map { it.trim() }
        if (cols.size < 3 || cols[0].isNotEmpty()) return@mapNotNull null
        val id = cols[1].lowercase(Locale.ROOT)
        if (!ID.matches(id)) return@mapNotNull null
        val title = cols.getOrNull(6)?.takeIf { it.isNotEmpty() } ?: cols[2].ifEmpty { id }
        Favourite(id, title)
    }.distinctBy { it.id }.toList()

    /** "lootslime 1.01" → "Lootslime 1.01.p8.png": PICO-8 shows titles in one
     *  case; the shelf reads better with capitals. Never a path. */
    fun fileName(fav: Favourite): String {
        val words = fav.title.replace(Regex("""[\\/:*?"<>|\u0000-\u001f]"""), " ")
            .split(' ').filter { it.isNotEmpty() }
            .joinToString(" ") { w -> w.replaceFirstChar { it.titlecase(Locale.ROOT) } }
            .trim('.', ' ')
        return (words.ifEmpty { fav.id }.take(80)) + ".p8.png"
    }

    /** Watches favourites.txt; [changed] runs on the observer's thread. */
    fun watch(context: Context, changed: () -> Unit): FileObserver {
        val dir = home(context).apply { mkdirs() }
        return object : FileObserver(dir, CLOSE_WRITE or MOVED_TO) {
            override fun onEvent(event: Int, path: String?) {
                if (path == LIST) changed()
            }
        }.also { it.startWatching() }
    }

    /**
     * Saves the favourites not saved before into the ROM folder's PICO-8
     * folder (made when there is none). The files written, for the shelf.
     */
    fun sync(context: Context, tree: Uri, onSaved: (String) -> Unit = {}): List<RomLibrary.SavedFile> {
        val list = File(home(context), LIST)
        if (!list.isFile) return emptyList()
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val saved = prefs.getStringSet(SAVED, emptySet())!!.toMutableSet()
        val wanted = parse(list.readText()).filter { it.id !in saved }
        if (wanted.isEmpty()) return emptyList()
        val (folder, folderName) = folder(context, tree)
        val existing = children(context, tree, folder)
        val written = ArrayList<RomLibrary.SavedFile>()
        for (fav in wanted) {
            val name = fileName(fav)
            if (name.lowercase(Locale.ROOT) in existing) {
                saved += fav.id
                continue
            }
            val bytes = try {
                cart(context, fav.id)
            } catch (e: IOException) {
                Log.w(TAG, "PICO-8 favourite ${fav.id} not saved", e)
                continue
            }
            val uri = write(context, folder, name, bytes)
            if (uri == null) { Log.w(TAG, "PICO-8 favourite ${fav.id}: couldn't write $name"); continue }
            saved += fav.id
            onSaved(name)
            written += RomLibrary.SavedFile(uri, folderName)
            Log.i(TAG, "PICO-8 favourite ${fav.id} saved as $name")
        }
        prefs.edit().putStringSet(SAVED, saved).apply()
        return written
    }

    /** The cart's .p8.png: Splore's cached copy, or the BBS's. */
    private fun cart(context: Context, id: String): ByteArray {
        val cached = File(home(context), "bbs/carts/$id.p8.png")
        if (cached.isFile && isPng(cached.readBytes())) return cached.readBytes()
        val connection = URL("https://carts.lexaloffle.com/$id.p8.png").openConnection() as HttpURLConnection
        connection.connectTimeout = 15_000
        connection.readTimeout = 30_000
        try {
            if (connection.responseCode != 200) throw IOException("HTTP ${connection.responseCode}")
            val bytes = connection.inputStream.use { it.readBytes() }
            if (!isPng(bytes)) throw IOException("not a cart")
            return bytes
        } finally {
            connection.disconnect()
        }
    }

    private fun isPng(b: ByteArray) = b.size > 8 && b[0] == 0x89.toByte() && b[1] == 'P'.code.toByte() &&
        b[2] == 'N'.code.toByte() && b[3] == 'G'.code.toByte()

    /** The ROM folder's PICO-8 folder and its name, as a scan names it. */
    private fun folder(context: Context, tree: Uri): Pair<Uri, String> {
        val resolver = context.contentResolver
        val rootId = DocumentsContract.getTreeDocumentId(tree)
        val root = DocumentsContract.buildDocumentUriUsingTree(tree, rootId)
        val dirs = HashMap<String, Pair<String, String>>()
        resolver.query(DocumentsContract.buildChildDocumentsUriUsingTree(tree, rootId),
            arrayOf(DocumentsContract.Document.COLUMN_DOCUMENT_ID, DocumentsContract.Document.COLUMN_DISPLAY_NAME,
                DocumentsContract.Document.COLUMN_MIME_TYPE), null, null, null)?.use {
            while (it.moveToNext()) {
                if (it.getString(2) != DocumentsContract.Document.MIME_TYPE_DIR) continue
                val name = it.getString(1) ?: continue
                dirs[name.lowercase(Locale.ROOT)] = it.getString(0) to name
            }
        }
        FOLDERS.firstOrNull { it in dirs }?.let {
            val (id, name) = dirs.getValue(it)
            return DocumentsContract.buildDocumentUriUsingTree(tree, id) to name
        }
        val made = DocumentsContract.createDocument(resolver, root, DocumentsContract.Document.MIME_TYPE_DIR,
            FOLDERS.first()) ?: throw IOException("Couldn't make the ${FOLDERS.first()} folder")
        return made to FOLDERS.first()
    }

    private fun children(context: Context, tree: Uri, folder: Uri): Set<String> {
        val names = HashSet<String>()
        val children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, DocumentsContract.getDocumentId(folder))
        context.contentResolver.query(children, arrayOf(DocumentsContract.Document.COLUMN_DISPLAY_NAME),
            null, null, null)?.use {
            while (it.moveToNext()) it.getString(0)?.let { n -> names += n.lowercase(Locale.ROOT) }
        }
        return names
    }

    /** Written as NAME.part and renamed when whole, so a cut write never shows on the shelf. */
    private fun write(context: Context, folder: Uri, name: String, bytes: ByteArray): Uri? {
        val resolver = context.contentResolver
        val part = DocumentsContract.createDocument(resolver, folder, "application/octet-stream", "$name.part")
            ?: return null
        try {
            (resolver.openOutputStream(part, "w") ?: return null).use { it.write(bytes) }
            return DocumentsContract.renameDocument(resolver, part, name)
        } catch (e: Exception) {
            try { DocumentsContract.deleteDocument(resolver, part) } catch (_: Exception) {}
            Log.w(TAG, "PICO-8 favourite: writing $name failed", e)
            return null
        }
    }
}
