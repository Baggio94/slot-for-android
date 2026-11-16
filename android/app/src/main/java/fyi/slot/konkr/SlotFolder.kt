package fyi.slot.konkr

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.storage.StorageManager
import android.provider.DocumentsContract
import android.util.Log
import android.widget.Toast
import org.json.JSONObject
import java.io.File
import java.io.IOException
import java.util.UUID
import java.util.concurrent.Executors

/**
 * The slot. folder (slot. 2.5, Library > Choose slot. Folder): a folder the
 * player picks, in the handheld's storage or on a card, that holds a mirror
 * of slot.'s saves, states, settings, favourites, labels and per-game
 * choices (FolderSync.travels), so another handheld picking it gets
 * everything back. ROMs are never copied into it.
 *
 * Scenarios: claude/slot-portable-scenarios-v1.md. The private files stay
 * slot.'s working copy: without the card slot. plays as before, and the
 * copy catches up when the folder is back.
 */
internal class SlotFolder(private val activity: MainActivity) {
    companion object {
        private const val TAG = "SlotFolder"
        private const val PREFS = "slot-folder"
        private const val TREE = "tree"
        private const val ROOT = "root"
        private const val ENV = "env"
        const val REQUEST = 4720
        private const val JSON = "slot.json"
        private const val FORMAT = 1
        private const val EXTERNAL = "com.android.externalstorage.documents"
    }

    private val prefs get() = activity.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
    private val worker = Executors.newSingleThreadExecutor()
    private val baseFile get() = File(File(activity.filesDir, "SlotFolder"), "base.json")
    @Volatile private var warnedMissing = false
    /** The storage the player chose in slot.'s window, for the picker. */
    private var wantedVolume: String? = null

    /** Library's value: 0 none, 1 internal storage, 2 SD card. */
    fun place(): Int {
        val root = prefs.getString(ROOT, null) ?: return 0
        return if (root.startsWith("primary:")) 1 else 2
    }

    private fun tree(): Uri? = prefs.getString(TREE, null)?.let(Uri::parse)
    /** The storage the folder is on: `primary`, or the card's id. */
    fun volume(): String? = prefs.getString(ROOT, null)?.substringBefore(':', "")?.takeIf { it.isNotEmpty() }
    private fun rootDoc(): String? = prefs.getString(ROOT, null)

    // -----------------------------------------------------------------
    // Choosing it

    /** Library > Choose slot. Folder: slot.'s window, then Android's picker. */
    fun choose() {
        val volumes = removableVolume()
        val dialog = SlotModalDialog(activity, "SLOT. FOLDER",
            "Where should slot. keep your saves, states and settings? Already have a slot " +
                "folder from another handheld? Pick it to get everything back.",
            onDismiss = { if (!picking) activity.nativeExternalReturn() })
        dialog.addButton("INTERNAL STORAGE") { picking = true; dialog.dismiss(); pick("primary") }
        if (volumes != null) dialog.addButton("SD CARD") { picking = true; dialog.dismiss(); pick(volumes) }
        if (tree() != null) dialog.addButton("NONE") { dialog.dismiss(); stopUsing() }
        picking = false
        dialog.show()
    }
    private var picking = false

    /** The SD card's volume id, if one is in. */
    private fun removableVolume(): String? = try {
        val storage = activity.getSystemService(StorageManager::class.java)
        storage.storageVolumes.firstOrNull { it.isRemovable && it.state == android.os.Environment.MEDIA_MOUNTED }
            ?.uuid
    } catch (_: Exception) { null }

    private fun pick(volume: String) {
        wantedVolume = volume
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
                Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                putExtra(DocumentsContract.EXTRA_INITIAL_URI, DocumentsContract.buildDocumentUri(EXTERNAL, "$volume:"))
            }
        }
        try {
            SlotToast.makeText(activity, "Open your slot folder, or create one named slot, then tap Use this folder.",
                Toast.LENGTH_LONG).show()
            @Suppress("DEPRECATION")
            activity.startActivityForResult(intent, REQUEST)
        } catch (error: ActivityNotFoundException) {
            activity.nativeExternalReturn()
            SlotToast.makeText(activity, "Couldn't open the folder picker.", Toast.LENGTH_LONG).show()
        }
    }

    /** NONE (scenario A11): slot. stops copying; the folder stays as it is. */
    private fun stopUsing() {
        prefs.edit().remove(TREE).remove(ROOT).apply()
        activity.nativeSetSlotFolder(0)
        activity.nativeExternalReturn()
        SlotToast.makeText(activity, "slot. keeps everything on this handheld only. Your slot folder is untouched.",
            Toast.LENGTH_LONG).show()
    }

    /** The picker's answer (MainActivity.onActivityResult). */
    fun picked(resultCode: Int, data: Intent?) {
        val uri = data?.data
        if (resultCode != android.app.Activity.RESULT_OK || uri == null) {
            activity.nativeExternalReturn()
            return
        }
        try {
            activity.contentResolver.takePersistableUriPermission(uri,
                Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
        } catch (error: SecurityException) {
            Log.w(TAG, "No lasting access to the slot folder", error)
            activity.nativeExternalReturn()
            SlotToast.makeText(activity, "Android didn't give slot. access to this folder.", Toast.LENGTH_LONG).show()
            return
        }
        // Scenario A7: never a folder slot. already uses for something else.
        if (uri.toString() in activity.otherFolders()) {
            activity.nativeExternalReturn()
            SlotToast.makeText(activity, "This folder is already your ROM, BIOS, save or state folder. " +
                "Choose another one.", Toast.LENGTH_LONG).show()
            return
        }
        worker.execute { look(uri, DocumentsContract.getTreeDocumentId(uri)) }
    }

    /** What the chosen folder holds decides what happens (scenarios A2-A6). */
    private fun look(tree: Uri, doc: String) {
        val side = SafSide(activity, tree, doc)
        val children = try { side.children(doc) } catch (error: Exception) {
            Log.w(TAG, "Slot folder unreadable", error)
            ui { activity.nativeExternalReturn(); toast("Couldn't read this folder.") }
            return
        }
        val json = children.firstOrNull { it.name == JSON }?.let { side.readDoc(it.id) }
            ?.let { try { JSONObject(String(it, Charsets.UTF_8)) } catch (_: Exception) { null } }
        val folderName = side.name(doc)
        when {
            json != null -> {
                if (json.optInt("format", 1) > FORMAT) {
                    // Scenario A8: written by a newer slot.
                    ui { activity.nativeExternalReturn(); toast("This slot folder comes from a newer slot. Update slot. first.") }
                    return
                }
                val ours = prefs.getString(ENV, null) == json.optString("env")
                when {
                    ours || !hasData() -> use(tree, doc, json, FolderSync.Start.FOLDER_WINS, backup = hasData())
                    else -> ui {
                        // Scenario A5: this handheld has its own too.
                        val dialog = SlotModalDialog(activity, "SLOT. FOLDER",
                            "This slot folder comes from another handheld, and this one has its own saves " +
                                "and settings. Use the folder's, or merge both? The handheld's are backed up first.",
                            onDismiss = { if (!chosen) activity.nativeExternalReturn() })
                        dialog.addButton("CANCEL") { dialog.dismiss() }
                        dialog.addButton("USE THE FOLDER'S") {
                            chosen = true; dialog.dismiss()
                            worker.execute { use(tree, doc, json, FolderSync.Start.FOLDER_WINS, backup = true) }
                        }
                        dialog.addButton("MERGE BOTH") {
                            chosen = true; dialog.dismiss()
                            worker.execute { use(tree, doc, json, FolderSync.Start.MERGE, backup = true) }
                        }
                        chosen = false
                        dialog.show()
                    }
                }
            }
            children.isEmpty() || folderName.equals("slot", ignoreCase = true) ->
                use(tree, doc, null, FolderSync.Start.MERGE, backup = false)
            else -> ui {
                // Scenario A6: a folder with other things in it.
                val dialog = SlotModalDialog(activity, "SLOT. FOLDER",
                    "This folder already holds other files. Create a folder named slot in it?",
                    onDismiss = { if (!chosen) activity.nativeExternalReturn() })
                dialog.addButton("CANCEL") { dialog.dismiss() }
                dialog.addButton("CREATE SLOT") {
                    chosen = true; dialog.dismiss()
                    worker.execute {
                        try {
                            val existing = children.firstOrNull { it.dir && it.name.equals("slot", ignoreCase = true) }
                            val child = existing?.id ?: side.createDir(doc, "slot")
                            look(tree, child)
                        } catch (error: Exception) {
                            Log.w(TAG, "Couldn't create the slot folder", error)
                            ui { activity.nativeExternalReturn(); toast("Couldn't create the slot folder.") }
                        }
                    }
                }
                chosen = false
                dialog.show()
            }
        }
    }
    private var chosen = false

    /** Something to keep: favourites, saves or settings of its own. */
    private fun hasData(): Boolean {
        val files = activity.filesDir
        return File(files, "konkr-favorites.json").isFile || File(files, "slot-konkr-settings.json").isFile ||
            (File(files, "Saves").listFiles()?.any { it.isDirectory && (it.list()?.isNotEmpty() == true) } == true)
    }

    /** Starts using a folder: the first copy, then slot.json. */
    private fun use(tree: Uri, doc: String, json: JSONObject?, start: FolderSync.Start, backup: Boolean) {
        val old = tree() to rootDoc()
        val first = old.second != doc
        if (backup) backupPrivate()
        val base = if (first) FolderSync.Base() else FolderSync.Base.parse(baseFile.takeIf { it.isFile }?.readText())
        val report = try {
            FolderSync.sync(FolderSync.PrivateSide(activity.filesDir), SafSide(activity, tree, doc), base,
                if (first) start else FolderSync.Start.SYNC)
        } catch (error: Exception) {
            Log.w(TAG, "First copy into the slot folder failed", error)
            ui { activity.nativeExternalReturn(); toast("Couldn't copy into this folder: ${error.message}") }
            return
        }
        // Scenario A13: the folder only counts once the copy is complete.
        if (report.failures.isNotEmpty()) {
            Log.w(TAG, "Slot folder copy: " + report.failures.joinToString(" | "))
            ui { activity.nativeExternalReturn(); toast("Some files couldn't be copied. Check the card's space and try again.") }
            return
        }
        val env = json?.optString("env")?.takeIf { it.isNotBlank() }
            ?: prefs.getString(ENV, null) ?: UUID.randomUUID().toString()
        writeJson(SafSide(activity, tree, doc), doc, env, json)
        saveBase(base)
        prefs.edit().putString(TREE, tree.toString()).putString(ROOT, doc).putString(ENV, env).apply()
        ui {
            activity.nativeSetSlotFolder(place())
            // Files came in (labels, favourites, settings): the shelf shows
            // them. Nothing new: Library stays on its row (Siham 2026-11-16).
            if (report.changedPrivate) activity.nativeExternalClose() else activity.nativeExternalReturn()
            toast(if (report.changedPrivate) "slot. folder ready: your saves and settings are back."
                else "slot. folder ready.")
            if (report.changedPrivate) activity.nativeReloadAll()
            // Scenario D1: the ROM folder next to it, if this handheld has none.
            json?.optString("roms")?.takeIf { it.isNotBlank() }?.let { activity.suggestRomFolder(it) }
            // Scenario A10: the old folder, once the new one is complete.
            val (oldTree, oldDoc) = old
            if (first && oldTree != null && oldDoc != null && oldDoc != doc) offerToClear(oldTree, oldDoc)
        }
    }

    /** Before the folder's files replace the handheld's: a dated copy. */
    private fun backupPrivate() {
        val stamp = java.text.SimpleDateFormat("yyyyMMdd-HHmmss", java.util.Locale.ROOT).format(java.util.Date())
        val to = File(File(activity.filesDir, "SlotFolder/Backups"), stamp)
        val side = FolderSync.PrivateSide(activity.filesDir)
        for (path in side.list().keys) {
            try {
                val target = File(to, path)
                target.parentFile?.mkdirs()
                File(activity.filesDir, path).copyTo(target, overwrite = true)
            } catch (error: Exception) {
                Log.w(TAG, "Backup of $path skipped", error)
            }
        }
    }

    private fun writeJson(side: SafSide, doc: String, env: String, old: JSONObject?) {
        val json = (old ?: JSONObject()).put("format", FORMAT).put("env", env)
            .put("device", Build.MANUFACTURER + " " + Build.MODEL)
            .put("written", System.currentTimeMillis())
        activity.romFolderPath()?.let { json.put("roms", it) }
        side.write(JSON, json.toString(2).toByteArray(Charsets.UTF_8))
    }

    private fun offerToClear(tree: Uri, doc: String) {
        val dialog = SlotModalDialog(activity, "OLD SLOT FOLDER",
            "Everything is now in the new slot folder. Delete the old one's files?")
        dialog.addButton("KEEP IT") { dialog.dismiss() }
        dialog.addButton("DELETE") {
            dialog.dismiss()
            worker.execute {
                try {
                    SafSide(activity, tree, doc).clear()
                } catch (error: Exception) {
                    Log.w(TAG, "Old slot folder not cleared", error)
                    ui { toast("Couldn't delete the old slot folder.") }
                }
            }
        }
        dialog.show()
    }

    private fun saveBase(base: FolderSync.Base) {
        baseFile.parentFile?.mkdirs()
        val temp = File(baseFile.parentFile, "base.json.tmp")
        temp.writeText(base.toJson())
        temp.renameTo(baseFile)
    }

    // -----------------------------------------------------------------
    // Keeping it in step

    /**
     * Copies both ways when slot. comes to the front, out of a game (B2, F1):
     * what another handheld changed comes in, then the shelf is built again.
     * [pull] false (during a game, at pause) only sends this one's changes.
     */
    fun sync(pull: Boolean) {
        val tree = tree() ?: return
        val doc = rootDoc() ?: return
        worker.execute {
            val side = SafSide(activity, tree, doc)
            if (!side.reachable()) {
                // Scenario E1/E2: the card is away; slot. plays on its copy.
                if (!warnedMissing) {
                    warnedMissing = true
                    ui { toast("slot. folder unavailable: playing from this handheld's copy.") }
                }
                return@execute
            }
            warnedMissing = false
            val stranger = side.readJsonEnv()
            if (stranger != null && stranger != prefs.getString(ENV, null)) {
                // Scenario E3: another card's slot folder in the same place.
                Log.w(TAG, "Another slot folder is in place: nothing written")
                return@execute
            }
            val base = FolderSync.Base.parse(baseFile.takeIf { it.isFile }?.readText())
            val report = try {
                FolderSync.sync(FolderSync.PrivateSide(activity.filesDir), side, base, pull = pull)
            } catch (error: Exception) {
                Log.w(TAG, "Slot folder sync failed", error)
                return@execute
            }
            if (report.failures.isNotEmpty()) Log.w(TAG, "Slot folder: " + report.failures.joinToString(" | "))
            saveBase(base)
            if (report.toFolder > 0 || report.toPrivate > 0) {
                try { writeJson(side, doc, prefs.getString(ENV, null) ?: return@execute, side.readJson()) }
                catch (error: Exception) { Log.w(TAG, "slot.json not updated", error) }
            }
            if (report.changedPrivate) ui { if (activity.nativeCanRefreshLibrary()) activity.nativeReloadAll() }
            Log.i(TAG, "Slot folder: ${report.toFolder} out, ${report.toPrivate} in, " +
                "${report.conflicts} conflicts, ${report.removed} removed")
        }
    }

    private fun ui(block: () -> Unit) = activity.runOnUiThread { if (!activity.isDestroyed) block() }
    private fun toast(text: String) = SlotToast.makeText(activity, text, Toast.LENGTH_LONG).show()

    // -----------------------------------------------------------------
    // The folder through Android's document provider

    class Child(val id: String, val name: String, val dir: Boolean, val size: Long, val modified: Long)

    class SafSide(private val context: Context, private val tree: Uri, private val root: String) : FolderSync.Side {
        private val resolver = context.contentResolver
        private val dirs = HashMap<String, String>()

        fun children(doc: String): List<Child> {
            val uri = DocumentsContract.buildChildDocumentsUriUsingTree(tree, doc)
            val out = ArrayList<Child>()
            resolver.query(uri, arrayOf(DocumentsContract.Document.COLUMN_DOCUMENT_ID,
                DocumentsContract.Document.COLUMN_DISPLAY_NAME, DocumentsContract.Document.COLUMN_MIME_TYPE,
                DocumentsContract.Document.COLUMN_SIZE, DocumentsContract.Document.COLUMN_LAST_MODIFIED),
                null, null, null)?.use { c ->
                while (c.moveToNext()) {
                    out.add(Child(c.getString(0), c.getString(1) ?: continue,
                        c.getString(2) == DocumentsContract.Document.MIME_TYPE_DIR,
                        if (c.isNull(3)) 0 else c.getLong(3), if (c.isNull(4)) 0 else c.getLong(4)))
                }
            } ?: throw IOException("folder unreadable")
            return out
        }

        fun reachable(): Boolean = try { children(root); true } catch (_: Exception) { false }

        fun name(doc: String): String =
            resolver.query(DocumentsContract.buildDocumentUriUsingTree(tree, doc),
                arrayOf(DocumentsContract.Document.COLUMN_DISPLAY_NAME), null, null, null)?.use { c ->
                if (c.moveToFirst()) c.getString(0) else null
            }.orEmpty()

        fun readDoc(id: String): ByteArray =
            resolver.openInputStream(DocumentsContract.buildDocumentUriUsingTree(tree, id))?.use { it.readBytes() }
                ?: throw IOException("unreadable")

        fun readJson(): JSONObject? = children(root).firstOrNull { it.name == JSON }?.let {
            try { JSONObject(String(readDoc(it.id), Charsets.UTF_8)) } catch (_: Exception) { null }
        }
        fun readJsonEnv(): String? = readJson()?.optString("env")?.takeIf { it.isNotBlank() }

        fun createDir(parent: String, name: String): String {
            val uri = DocumentsContract.createDocument(resolver,
                DocumentsContract.buildDocumentUriUsingTree(tree, parent), DocumentsContract.Document.MIME_TYPE_DIR, name)
                ?: throw IOException("couldn't create $name")
            return DocumentsContract.getDocumentId(uri)
        }

        override fun list(): Map<String, FolderSync.Meta> {
            val out = HashMap<String, FolderSync.Meta>()
            fun walk(doc: String, prefix: String) {
                for (child in children(doc)) {
                    val path = prefix + child.name
                    if (child.dir) { dirs[path] = child.id; walk(child.id, "$path/") }
                    else if (path != JSON && !child.name.endsWith(".slot-tmp")) {
                        out[path] = FolderSync.Meta(child.size, child.modified)
                        files[path] = child.id
                    }
                }
            }
            dirs.clear(); files.clear()
            walk(root, "")
            listed = true
            return out
        }
        private val files = HashMap<String, String>()
        private var listed = false

        private fun dirOf(path: String): String {
            if (!path.contains('/')) return root
            val parent = path.substringBeforeLast('/')
            dirs[parent]?.let { return it }
            val above = dirOf("$parent/x")
            val name = parent.substringAfterLast('/')
            val existing = children(above).firstOrNull { it.dir && it.name == name }?.id
            val id = existing ?: createDir(above, name)
            dirs[parent] = id
            return id
        }

        override fun read(path: String): ByteArray {
            val id = files[path] ?: run { list(); files[path] } ?: throw IOException("missing $path")
            return readDoc(id)
        }

        override fun write(path: String, bytes: ByteArray): FolderSync.Meta {
            val parent = dirOf(path)
            val name = path.substringAfterLast('/')
            val parentUri = DocumentsContract.buildDocumentUriUsingTree(tree, parent)
            val existing = files[path]
                ?: if (!listed) children(parent).firstOrNull { !it.dir && it.name == name }?.id else null
            // Through a temporary file: a cut never leaves half a save.
            val temp = DocumentsContract.createDocument(resolver, parentUri, "application/octet-stream",
                "$name.slot-tmp") ?: throw IOException("couldn't write $path")
            try {
                resolver.openOutputStream(temp, "wt")?.use { it.write(bytes) } ?: throw IOException("couldn't write $path")
                val written = meta(DocumentsContract.getDocumentId(temp))
                if (written != null && written.size != bytes.size.toLong()) throw IOException("$path came out short")
                if (existing != null) {
                    DocumentsContract.deleteDocument(resolver, DocumentsContract.buildDocumentUriUsingTree(tree, existing))
                }
                val renamed = DocumentsContract.renameDocument(resolver, temp, name) ?: temp
                val id = DocumentsContract.getDocumentId(renamed)
                files[path] = id
                return meta(id) ?: FolderSync.Meta(bytes.size.toLong(), System.currentTimeMillis())
            } catch (error: Exception) {
                try { DocumentsContract.deleteDocument(resolver, temp) } catch (_: Exception) {}
                throw error
            }
        }

        /** One document's size and date. */
        private fun meta(id: String): FolderSync.Meta? =
            resolver.query(DocumentsContract.buildDocumentUriUsingTree(tree, id),
                arrayOf(DocumentsContract.Document.COLUMN_SIZE, DocumentsContract.Document.COLUMN_LAST_MODIFIED),
                null, null, null)?.use { c ->
                if (c.moveToFirst()) FolderSync.Meta(if (c.isNull(0)) 0 else c.getLong(0),
                    if (c.isNull(1)) 0 else c.getLong(1)) else null
            }

        override fun delete(path: String) {
            val id = files[path] ?: return
            DocumentsContract.deleteDocument(resolver, DocumentsContract.buildDocumentUriUsingTree(tree, id))
            files.remove(path)
        }

        /** Deletes what slot. put in this folder (scenario A10, confirmed). */
        fun clear() {
            if (readJson() == null) return // Not a slot folder: never.
            for (child in children(root)) {
                DocumentsContract.deleteDocument(resolver, DocumentsContract.buildDocumentUriUsingTree(tree, child.id))
            }
        }
    }
}
