package fyi.slot.konkr

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.SystemClock
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
 * The private files stay
 * slot.'s working copy: without the card slot. plays as before, and the
 * copy catches up when the folder is back.
 */
/**
 * The id of the folder a file at `path` ("Saves/mGBA/x.srm") goes in, under
 * `root`: each folder on the way is looked up in `known`, else found or made
 * by `make(parent id, name)`, and remembered. It once asked for the folder of
 * "<parent>/x", the same parent again, without end: a new slot. folder with
 * subfolders to write overflowed the stack (2.5.0-beta34, KPA, 18 Nov).
 */
internal fun folderFor(path: String, root: String, known: MutableMap<String, String>,
                       make: (String, String) -> String): String {
    if (!path.contains('/')) return root
    val parent = path.substringBeforeLast('/')
    known[parent]?.let { return it }
    val above = folderFor(parent, root, known, make)
    val id = make(above, parent.substringAfterLast('/'))
    known[parent] = id
    return id
}

internal class SlotFolder(private val activity: MainActivity) {
    companion object {
        /** Whether the storage a folder chosen through Android lives on is
         *  there: internal always, a card only while mounted. */
        fun volumeMounted(context: android.content.Context, volume: String?): Boolean {
            volume ?: return false
            if (volume == "primary") return true
            return try {
                context.getSystemService(StorageManager::class.java).storageVolumes.any {
                    it.uuid.equals(volume, ignoreCase = true) && it.state == android.os.Environment.MEDIA_MOUNTED
                }
            } catch (_: Exception) { false }
        }

        /** The storage of a folder chosen through Android ("primary", or a card's id). */
        fun volumeOf(tree: Uri): String? = try {
            DocumentsContract.getTreeDocumentId(tree).substringBefore(':', "")
        } catch (_: Exception) { null }

        private const val TAG = "SlotFolder"
        private const val PREFS = "slot-folder"
        private const val TREE = "tree"
        private const val ROOT = "root"
        private const val ENV = "env"
        const val REQUEST = 4720
        private const val JSON = "slot.json"
        /** The folder's deletions (FolderSync.Tomb), for every handheld. */
        private const val TOMBS = "deleted.json"
        /** The systems' folders made in CustomCheats/ for the player's files. */
        private val CHEAT_SYSTEMS = Systems.PLATFORMS
        private const val CHEAT_README = "CustomCheats/README.txt"
        private val CHEAT_HELP = ("Your own cheats, in RetroArch's .cht format: put each file in its " +
            "system's folder, named exactly like the game's ROM with .cht at the end, for example " +
            "GBA/Golden Sun (USA).cht. slot. shows them first in the game's Cheats, as My Cheats.\n")
        private const val FORMAT = 1
        private const val EXTERNAL = "com.android.externalstorage.documents"
        private const val WRITE_RETRY_MS = 30_000L
    }

    private val prefs get() = activity.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
    private val worker = Executors.newSingleThreadExecutor()
    private val baseFile get() = File(File(activity.filesDir, "SlotFolder"), "base.json")
    /** Saves and states removed from the folder by hand, held back until
     *  the player says at the game's launch (SaveDeletes). */
    private val goneFile get() = File(File(activity.filesDir, "SlotFolder"), "gone.txt")
    /** Of those, the ones the player chose to put back. */
    private val restoreFile get() = File(File(activity.filesDir, "SlotFolder"), "restore.txt")
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

    /** The copy queued and not started yet: 0 none, 1 sending only, 2 both
     *  ways. Asking again while it waits only makes it do more (beta39: on
     *  the KPA, three passes of 10 s each ran one after the other for
     *  nothing after a long one). */
    private val queued = java.util.concurrent.atomic.AtomicInteger(0)
    /** Copies that bring files in, queued or running, whose saves, states,
     *  cheats and game settings aren't settled yet: a launch waits for those
     *  only (beta39), never for labels and pictures behind them. */
    private val launchPending = java.util.concurrent.atomic.AtomicInteger(0)
    @Volatile private var pullRunning = false
    @Volatile private var syncRunning = false
    @Volatile private var syncHasChanges = false
    private val saveCopyQueued = java.util.concurrent.atomic.AtomicBoolean(false)
    private val confirmAfterSync = java.util.concurrent.atomic.AtomicBoolean(false)
    @Volatile private var retryTree: String? = null
    @Volatile private var retryAt = 0L
    /** The next copy follows the card's return: it says so when done (beta41). */
    @Volatile private var confirmNextPull = false
    /** The whole copy into a folder just chosen, in percent; -1 when none. */
    @Volatile private var copyAt = -1

    fun pulling(): Boolean = queued.get() == 2 || pullRunning

    /** The folder's labels are on their way in (3.0.1-beta11): a new handheld
     *  doesn't look for them on the Internet while they're coming. Settled
     *  by the first copy, by the first two-way pass (done or failed), or
     *  after two minutes at most. */
    @Volatile private var artworkIn = false
    private val createdAt = SystemClock.elapsedRealtime()
    fun artworkSettled(): Boolean = copyAt < 0 &&
        (tree() == null || artworkIn || SystemClock.elapsedRealtime() - createdAt > 120_000L)
    private fun artworkArrived() {
        if (artworkIn) return
        artworkIn = true
        ui { activity.folderArtworkIn() }
    }
    fun copyPercent(): Int? = copyAt.takeIf { it >= 0 }

    fun launchFilesPending(): Boolean = launchPending.get() > 0

    /** The launch's own files, asked for first by the copy bringing files in. */
    private val priority = java.util.concurrent.atomic.AtomicReference<FolderSync.Priority?>(null)

    /** Waits, at most [millis], until this game's own files ([tokens]: see
     *  FolderSync.Priority) have come in: the copy takes them first, the
     *  rest goes on after the launch. True when they have, or when nothing
     *  comes in (3.0.1-beta11: no more launching anyway after 15 s). */
    fun awaitGameFiles(tokens: Set<String>, millis: Long): Boolean {
        if (launchPending.get() == 0) return true
        val want = FolderSync.Priority(tokens)
        priority.set(want)
        try {
            val until = SystemClock.elapsedRealtime() + millis
            while (launchPending.get() > 0 && want.served.count > 0L) {
                if (SystemClock.elapsedRealtime() >= until) return false
                want.served.await(20L, java.util.concurrent.TimeUnit.MILLISECONDS)
            }
            return true
        } finally { priority.compareAndSet(want, null) }
    }

    /** A copy bringing files in, as the task pill shows it: of the files
     *  the folder brings, how many are in; null when none runs or nothing
     *  comes in (3.0.1-beta12: a push, saves after a game or labels found,
     *  blocks nothing and shows nothing). */
    @Volatile private var syncDone = 0
    @Volatile private var syncTotal = 0
    /** The pass under way was asked for by the player (Refresh Library):
     *  it shows even when nothing comes in, as he waits for it. */
    @Volatile private var syncAsked = false
    fun syncProgress(): IntArray? =
        if (syncRunning && (syncTotal > 0 || syncAsked)) intArrayOf(syncDone, syncTotal) else null

    /** Runs [block] once no game is on (a window, the ROM picker, a reload
     *  never land on a game, beta38). */
    private fun whenFree(block: () -> Unit) {
        if (activity.isDestroyed) return
        if (!activity.nativeIsPlaying()) block()
        else activity.window.decorView.postDelayed({ whenFree(block) }, 2_000L)
    }

    /** Waits, at most [millis], for the copy under way, so a game starts from
     *  what the folder just brought (beta37). True when none is left. */
    fun awaitIdle(millis: Long): Boolean = try {
        worker.submit { }.get(millis, java.util.concurrent.TimeUnit.MILLISECONDS)
        true
    } catch (_: Exception) { false }

    /** Whether the folder's storage is in (Android's volumes only: cheap). */
    private fun mounted(): Boolean = volumeMounted(activity, volume())

    private var wasMounted: Boolean? = null
    private var pullWhenFree = false

    /** Every 2 seconds: the card came back while slot. was open, what changed
     *  in the folder comes in, once no game is on ([canPull]). */
    fun cardCheck(canPull: Boolean) {
        if (tree() == null) { wasMounted = null; pullWhenFree = false; return }
        val now = mounted()
        if (wasMounted == false && now) {
            SlotLog.i(TAG, "Slot folder's card is back")
            retryAt = 0L
            pullWhenFree = true
        }
        wasMounted = now
        if (pullWhenFree && now && canPull) {
            pullWhenFree = false
            confirmNextPull = true
            sync(pull = true)
        }
    }

    // -----------------------------------------------------------------
    // Choosing it

    /** Library > Choose slot. Folder: slot.'s window, then Android's picker. */
    fun choose() {
        copyPercent()?.let { percent ->
            // One copy at a time: the row shows how far it is.
            activity.nativeExternalReturn()
            toast("Copying to your slot. folder… $percent%")
            return
        }
        val volumes = removableVolume()
        val dialog = SlotModalDialog(activity, "SLOT. FOLDER",
            "Where should slot. keep its own data? Already have a slot. folder from " +
                "another handheld? Pick it to get everything back.",
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
        storage.storageVolumes.firstOrNull { it.isRemovable &&
            (it.state == android.os.Environment.MEDIA_MOUNTED ||
                it.state == android.os.Environment.MEDIA_MOUNTED_READ_ONLY) }?.uuid
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
            SlotToast.makeText(activity, "Open your slot. folder, or create one named slot, then tap Use this folder.",
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
        SlotToast.makeText(activity, "slot. keeps everything on this handheld only. Your slot. folder is untouched.",
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
            SlotLog.w(TAG, "No lasting access to the slot folder", error)
            TaskPill.problem(storageProblem(uri, listOf(SlotStorageProblem.describe(error))))
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
        val lookedAt = SystemClock.elapsedRealtime()
        val side = SafSide(activity, tree, doc)
        val children = try { side.children(doc) } catch (error: Exception) {
            SlotLog.w(TAG, "Slot folder unreadable", error)
            ui { activity.nativeExternalReturn()
                TaskPill.problem(storageProblem(tree, listOf(SlotStorageProblem.describe(error))))
                toast("Couldn't read this folder.") }
            return
        }
        val json = children.firstOrNull { it.name == JSON }?.let { side.readDoc(it.id) }
            ?.let { try { JSONObject(String(it, Charsets.UTF_8)) } catch (_: Exception) { null } }
        val folderName = side.name(doc)
        // The Rotate took about a minute here once, on a card it had just
        // mounted (beta38): the time it takes, for the next time.
        SlotLog.i(TAG, "Slot folder chosen: read in ${SystemClock.elapsedRealtime() - lookedAt} ms, " +
            "${children.size} entries, ${if (json != null) "a slot folder" else "no slot.json"}")
        when {
            json != null -> {
                if (json.optInt("format", 1) > FORMAT) {
                    // Scenario A8: written by a newer slot.
                    ui { activity.nativeExternalReturn(); toast("This slot. folder comes from a newer slot. Update slot. first.") }
                    return
                }
                val ours = prefs.getString(ENV, null) == json.optString("env")
                when {
                    ours || !hasData() -> use(tree, doc, json, FolderSync.Start.FOLDER_WINS, backup = hasData())
                    else -> ui {
                        // Scenario A5: this handheld has its own too.
                        val dialog = SlotModalDialog(activity, "SLOT. FOLDER",
                            "This slot. folder comes from another handheld, and this one has its own saves " +
                                "and settings. Keep only the folder's, or merge both? This handheld's are " +
                                "backed up first.",
                            onDismiss = { if (!chosen) activity.nativeExternalReturn() })
                        dialog.addButton("CANCEL") { dialog.dismiss() }
                        dialog.addButton("FOLDER ONLY") {
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
                            SlotLog.w(TAG, "Couldn't create the slot folder", error)
                            ui { activity.nativeExternalReturn()
                                TaskPill.problem(storageProblem(tree, listOf(SlotStorageProblem.describe(error))))
                                toast("Couldn't create the slot. folder.") }
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
        val side = SafSide(activity, tree, doc)
        // Before touching the private copy or making a backup, test the
        // actual SAF write path rather than trusting a writable-looking URI.
        try {
            checkStorageWritable(tree)
            side.probeWrite()
        } catch (error: Exception) {
            val problem = storageProblem(tree, listOf(SlotStorageProblem.describe(error)))
            SlotLog.w(TAG, "Slot folder write test failed: $problem", error)
            ui { activity.nativeExternalReturn(); TaskPill.problem(problem)
                toast("Couldn't write to this slot. folder. Check its storage and try again.") }
            return
        }
        if (backup) backupPrivate()
        val base = if (first) FolderSync.Base() else FolderSync.Base.parse(baseFile.takeIf { it.isFile }?.readText())
        // The copy can take minutes (labels): Library shows how far it is,
        // and the menus are free meanwhile (beta38: A did nothing until the
        // copy ended).
        copyAt = 0
        ui { activity.nativeExternalReturn(); toast("Copying to your slot. folder…") }
        val report = try {
            val tombs = readTombs(side)
            FolderSync.sync(privateSide(), side, base, if (first) start else FolderSync.Start.SYNC,
                tombs = tombs, restore = paths(restoreFile), progress = progress()).also {
                if (it.tombsChanged) writeTombs(side, tombs)?.let { error -> it.failures.add(error) }
                keepGone(it)
                keepTwoSaves(it)
                if (it.failures.isEmpty()) side.makeCheatFolders()?.let { error -> it.failures.add(error) }
            }
        } catch (error: Exception) {
            SlotLog.w(TAG, "First copy into the slot folder failed", error)
            ui { TaskPill.problem(storageProblem(tree, listOf(SlotStorageProblem.describe(error)))) }
            ui { activity.nativeSetSlotFolderCopy(-1); activity.nativeExternalReturn()
                toast("Couldn't copy into this folder: ${error.message}") }
            return
        } finally {
            copyAt = -1
            ui { activity.nativeSetSlotFolderCopy(-1) }
        }
        SlotLog.i(TAG, "Slot folder copy: ${report.toFolder} out, ${report.toPrivate} in")
        // Scenario A13: the folder only counts once the copy is complete.
        if (report.failures.isNotEmpty()) {
            SlotLog.w(TAG, "Slot folder copy: " + report.failures.joinToString(" | "))
            ui { TaskPill.problem(storageProblem(tree, report.failures)) }
            ui { activity.nativeExternalReturn(); toast("Some files couldn't be copied. Check the storage and try again.") }
            return
        }
        val env = json?.optString("env")?.takeIf { it.isNotBlank() }
            ?: prefs.getString(ENV, null) ?: UUID.randomUUID().toString()
        try {
            writeJson(SafSide(activity, tree, doc), doc, env, json)
            saveBase(base)
        } catch (error: Exception) {
            SlotLog.w(TAG, "Slot folder metadata not written", error)
            ui { activity.nativeExternalReturn()
                TaskPill.problem(storageProblem(tree, listOf(SlotStorageProblem.describe(error)))) }
            return
        }
        retryAt = 0L
        prefs.edit().putString(TREE, tree.toString()).putString(ROOT, doc).putString(ENV, env).apply()
        artworkArrived()
        ui {
            activity.nativeSetSlotFolder(place())
            // Files came in (labels, favourites, settings): the shelf shows
            // them. Nothing new: Library stays on its row (Siham 2026-11-16).
            if (report.changedPrivate) activity.nativeExternalClose() else activity.nativeExternalReturn()
            toast(if (report.changedPrivate) "slot. folder ready: your saves and settings are back."
                else "slot. folder ready.")
            // The rest never over a game (beta38): the player may have
            // left Library during the copy.
            whenFree {
                if (report.changedPrivate) activity.nativeReloadAll()
                // Scenario D1: the ROM folder next to it, if this handheld has
                // none, then its BIOS folder (3.0.1-beta12).
                val roms = json?.optString("roms")?.takeIf { it.isNotBlank() }
                val bios = json?.optString("bios")?.takeIf { it.isNotBlank() }
                if (roms != null || bios != null) activity.suggestRomFolder(roms, bios)
                // Scenario A10: the old folder, once the new one is complete.
                val (oldTree, oldDoc) = old
                if (first && oldTree != null && oldDoc != null && oldDoc != doc) offerToClear(oldTree, oldDoc)
            }
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
                SlotLog.w(TAG, "Backup of $path skipped", error)
            }
        }
    }

    private fun writeJson(side: SafSide, doc: String, env: String, old: JSONObject?) {
        val json = (old ?: JSONObject()).put("format", FORMAT).put("env", env)
            .put("device", Build.MANUFACTURER + " " + Build.MODEL)
            .put("written", System.currentTimeMillis())
        activity.romFolderPath()?.let { json.put("roms", it) }
        activity.biosFolderPath()?.let { json.put("bios", it) }
        side.write(JSON, json.toString(2).toByteArray(Charsets.UTF_8))
    }

    private fun offerToClear(tree: Uri, doc: String) {
        val dialog = SlotModalDialog(activity, "OLD SLOT FOLDER",
            "Everything is now in the new slot. folder. Delete the old one's files?")
        dialog.addButton("KEEP IT") { dialog.dismiss() }
        dialog.addButton("DELETE") {
            dialog.dismiss()
            worker.execute {
                try {
                    SafSide(activity, tree, doc).clear()
                } catch (error: Exception) {
                    SlotLog.w(TAG, "Old slot folder not cleared", error)
                    ui { toast("Couldn't delete the old slot. folder.") }
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
    fun sync(pull: Boolean, confirm: Boolean = false) {
        if (tree() == null) return
        if (confirm) confirmAfterSync.set(true)
        val want = if (pull) 2 else 1
        while (true) {
            val was = queued.get()
            if (was >= want) return
            if (!queued.compareAndSet(was, want)) continue
            if (pull) launchPending.incrementAndGet()
            // One is queued already: it now does this too.
            if (was != 0) return
            break
        }
        try {
            worker.execute {
                val asPull = queued.getAndSet(0) == 2
                val announce = confirmAfterSync.getAndSet(false)
                syncDone = 0; syncTotal = 0; syncHasChanges = false
                syncRunning = true
                if (asPull) pullRunning = true
                val settled = java.util.concurrent.atomic.AtomicBoolean(!asPull)
                val settle = { if (settled.compareAndSet(false, true)) launchPending.decrementAndGet() }
                // Persist launch data and conflict choices before release.
                val ok = try { runSync(asPull, scope = FolderSync.Scope.LAUNCH) }
                finally { syncRunning = false; pullRunning = false; settle() }
                // Labels come in with the rest: then the Internet's turn.
                if (ok) queueSecondary(FolderSync.Scope.REST, asPull, announce) { if (asPull) artworkArrived() }
                else {
                    if (announce) confirmAfterSync.set(true)
                    if (asPull) artworkArrived()
                }
            }
        } catch (error: Exception) {
            if (queued.getAndSet(0) == 2) launchPending.decrementAndGet()
            SlotLog.w(TAG, "Slot folder copy not queued", error)
        }
    }

    /** Artwork never needs to wait for, enumerate or modify saves and states.
     *  [asked]: Refresh Library's own pass, shown as SYNCING throughout. */
    fun syncArtwork(pull: Boolean, asked: Boolean = false, finished: (Boolean) -> Unit = {}) {
        if (tree() == null) { finished(true); return }
        queueSecondary(FolderSync.Scope.ARTWORK, pull, false, asked, finished)
    }

    /** Single writer, with checkpoints before handing over to urgent work. */
    private fun queueSecondary(scope: FolderSync.Scope, pull: Boolean, announce: Boolean,
                               asked: Boolean = false, finished: (Boolean) -> Unit) {
        val expectedTree = tree()
        if (expectedTree == null) { finished(false); return }
        fun enqueue() {
            if (activity.isDestroyed || tree() != expectedTree) { finished(false); return }
            try {
                worker.execute {
                    if (activity.isDestroyed || tree() != expectedTree) { finished(false); return@execute }
                    var yielded = false
                    syncDone = 0; syncTotal = 0; syncHasChanges = false; syncAsked = asked; syncRunning = true
                    val ok = try {
                        runSync(pull && activity.nativeCanRefreshLibrary(), announce = announce, scope = scope,
                            shouldYield = { queued.get() != 0 || launchPending.get() > 0 ||
                                saveCopyQueued.get() || activity.nativeIsPlaying() },
                            onYield = { yielded = true })
                    } catch (error: Exception) {
                        SlotLog.w(TAG, "Secondary folder copy failed", error); false
                    } finally { syncRunning = false; syncAsked = false }
                    if (yielded && ok) {
                        ui {
                            fun resumeCopy() {
                                if (activity.isDestroyed || tree() != expectedTree) { finished(false); return }
                                if (activity.nativeIsPlaying()) activity.window.decorView.postDelayed({ resumeCopy() }, 500L)
                                else enqueue()
                            }
                            resumeCopy()
                        }
                    } else finished(ok)
                }
            } catch (error: Exception) { finished(false) }
        }
        enqueue()
    }

    /** A normal in-game save never needs to walk the artwork folders. */
    fun syncSaves() {
        if (tree() == null || !saveCopyQueued.compareAndSet(false, true)) return
        try {
            worker.execute {
                saveCopyQueued.set(false)
                syncDone = 0; syncTotal = 0; syncHasChanges = false; syncRunning = true
                try { runSync(false, scope = FolderSync.Scope.SAVES) }
                catch (error: Exception) { SlotLog.w(TAG, "Save folder copy failed", error) }
                finally { syncRunning = false }
            }
        } catch (error: Exception) { saveCopyQueued.set(false) }
    }

    private fun runSync(pull: Boolean, launchFilesDone: () -> Unit = {}, announce: Boolean = false,
                        scope: FolderSync.Scope = FolderSync.Scope.ALL,
                        shouldYield: () -> Boolean = { false }, onYield: () -> Unit = {}): Boolean =
        run {
            // Which folder, read when the copy runs: one asked for just
            // before the player chose another went to the old one with the
            // new one's base (2.5.0-beta35: 540 files copied back for nothing).
            val tree = tree() ?: return@run false
            val doc = rootDoc() ?: return@run false
            if (retryTree == tree.toString() && SystemClock.elapsedRealtime() < retryAt) {
                // A later normal sync or card reinsert retries; don't hammer
                // the provider while the same storage error persists.
                if (announce) confirmAfterSync.set(true)
                return@run false
            }
            val base = FolderSync.Base.parse(baseFile.takeIf { it.isFile }?.readText())
            // A pause with no local changes needs no expensive SAF listing.
            // A two-way pass on resume/card insertion still checks external edits.
            if (!pull && scope == FolderSync.Scope.ALL && baseFile.isFile && paths(restoreFile).isEmpty()) {
                val local = privateSide().list()
                if (local.keys == base.entries.keys && local.all { (path, meta) ->
                        base.entries[path]?.let { it.privateSize == meta.size && it.privateModified == meta.modified } == true
                    }) return@run true
            }
            val side = SafSide(activity, tree, doc)
            try { checkStorageWritable(tree) } catch (error: Exception) {
                syncFailed(tree, listOf(SlotStorageProblem.describe(error)))
                if (announce) confirmAfterSync.set(true)
                return@run false
            }
            if (!side.reachable()) {
                // Scenario E1/E2: the card is away; slot. plays on its copy.
                if (!warnedMissing) {
                    warnedMissing = true
                    ui { toast("slot. folder unavailable: playing from this handheld's copy.") }
                }
                if (announce) confirmAfterSync.set(true)
                return@run false
            }
            warnedMissing = false
            val stranger = side.readJsonEnv()
            if (stranger != null && stranger != prefs.getString(ENV, null)) {
                // Scenario E3: another card's slot folder in the same place.
                SlotLog.w(TAG, "Another slot folder is in place: nothing written")
                return@run false
            }
            val started = SystemClock.elapsedRealtime()
            val report = try {
                val tombs = readTombs(side)
                FolderSync.sync(privateSide(), side, base, pull = pull, tombs = tombs,
                    restore = paths(restoreFile), launchFilesDone = launchFilesDone,
                    progress = { done, total -> syncDone = done; syncTotal = total },
                    scope = scope, changed = { syncHasChanges = true }, shouldYield = shouldYield,
                    // Only a copy bringing the launch files in answers a
                    // launch: a push alone would let it start before them.
                    priority = if (pull && scope == FolderSync.Scope.LAUNCH) { { priority.get() } } else { { null } }).also {
                    if (it.tombsChanged) writeTombs(side, tombs)?.let { error -> it.failures.add(error) }
                    if (scope == FolderSync.Scope.ALL || scope == FolderSync.Scope.LAUNCH || scope == FolderSync.Scope.SAVES) { keepGone(it); keepTwoSaves(it) }
                    if (pull && (scope == FolderSync.Scope.ALL || scope == FolderSync.Scope.REST) && !it.yielded && it.failures.isEmpty()) side.makeCheatFolders()?.let { error -> it.failures.add(error) }
                }
            } catch (error: Exception) {
                SlotLog.w(TAG, "Slot folder sync failed", error)
                syncFailed(tree, listOf(SlotStorageProblem.describe(error)))
                if (announce) confirmAfterSync.set(true)
                return@run false
            }
            // Keep completed entries, but never confirm a partial or failed copy.
            saveBase(base)
            if (report.toFolder > 0 || report.toPrivate > 0) {
                try { writeJson(side, doc, prefs.getString(ENV, null) ?: return@run false, side.readJson()) }
                catch (error: Exception) {
                    SlotLog.w(TAG, "slot.json not updated", error)
                    report.failures.add("slot.json: ${SlotStorageProblem.describe(error)}")
                }
            }
            if (report.yielded && report.failures.isEmpty()) {
                SlotLog.i(TAG, "Slot folder (${scope.name}) yielded after ${report.toFolder} out, ${report.toPrivate} in")
                onYield()
            } else if (report.failures.isNotEmpty()) {
                syncFailed(tree, report.failures)
                if (announce) confirmAfterSync.set(true)
            } else {
                retryAt = 0L
                shownProblem = null
                if (pull && (scope == FolderSync.Scope.ALL || scope == FolderSync.Scope.REST) && (announce || confirmNextPull)) TaskPill.confirm("FOLDER UP TO DATE")
                // A push while paused/playing may defer incoming changes.
                // Confirm the whole folder only after a two-way pass.
                if (!pull && announce) confirmAfterSync.set(true)
                if (pull && (scope == FolderSync.Scope.ALL || scope == FolderSync.Scope.REST)) confirmNextPull = false
            }
            if (report.changedShelf) ui { whenFree { if (activity.nativeCanRefreshLibrary()) activity.nativeReloadAll() } }
            SlotLog.i(TAG, "Slot folder (${scope.name}): ${report.toFolder} out, ${report.toPrivate} in, " +
                "${report.conflicts} conflicts, ${report.removed} removed, " +
                "${report.deletedHere.size} deleted here, ${report.gone.size} held back" +
                (if (report.stale > 0) ", ${report.stale} old files cleared" else "") +
                " in ${SystemClock.elapsedRealtime() - started} ms")
            report.failures.isEmpty()
        }

    // -----------------------------------------------------------------
    // Deletions (SaveDeletes)

    /** slot.'s files, whose saves and states deleted because another
     *  handheld deleted them are kept in a backup, and their RetroArch
     *  copies deleted at the game's next launch. */
    private fun privateSide(): FolderSync.PrivateSide {
        val files = activity.filesDir
        return object : FolderSync.PrivateSide(files) {
            override fun delete(path: String) {
                if (SaveDeletes.kindOf(path) != null) {
                    SaveDeletes.retire(files, File(files, path), SaveDeletes.coreOf(path), System.currentTimeMillis())
                    SaveDeletes.markRetroArch(files, path)
                } else {
                    super.delete(path)
                }
            }
        }
    }

    private fun readTombs(side: SafSide): MutableMap<String, FolderSync.Tomb> =
        FolderSync.tombsFromJson(side.readRootText(TOMBS))

    private fun writeTombs(side: SafSide, tombs: Map<String, FolderSync.Tomb>): String? = try {
        side.write(TOMBS, FolderSync.tombsToJson(tombs).toByteArray(Charsets.UTF_8))
        null
    } catch (error: Exception) {
        SlotLog.w(TAG, "Deletions not noted in the slot folder", error)
        "$TOMBS: ${SlotStorageProblem.describe(error)}"
    }

    private fun paths(file: File): Set<String> = try {
        if (file.isFile) file.readLines().filter { it.isNotBlank() }.toSet() else emptySet()
    } catch (_: Exception) { emptySet() }

    private fun writePaths(file: File, paths: Collection<String>) {
        try {
            file.parentFile?.mkdirs()
            if (paths.isEmpty()) file.delete() else file.writeText(paths.joinToString("\n"))
        } catch (error: Exception) {
            SlotLog.w(TAG, "Couldn't keep ${file.name}", error)
        }
    }

    /** What the copy held back is what the launch asks about; what was
     *  put back is no longer to restore. */
    private fun keepGone(report: FolderSync.Report) {
        writePaths(goneFile, report.gone)
        writePaths(restoreFile, paths(restoreFile).filter { it in report.gone })
    }

    // -----------------------------------------------------------------
    // Two different saves of a game (beta39): the newer one is in place,
    // the player chooses at the game's launch.

    private val twoDir get() = File(File(activity.filesDir, "SlotFolder"), "TwoSaves")

    class TwoSavesFound(val path: String, val key: String, val handheldSetAside: Boolean,
                        val handheldModified: Long, val folderModified: Long)

    private fun keepTwoSaves(report: FolderSync.Report) {
        for (two in report.twoSaves) {
            try {
                twoDir.mkdirs()
                val key = FolderSync.sha1(two.path.toByteArray(Charsets.UTF_8))
                File(twoDir, "$key.srm").writeBytes(two.other)
                File(twoDir, "$key.json").writeText(JSONObject().put("path", two.path)
                    .put("handheldSetAside", two.handheldSetAside).put("handheld", two.handheldModified)
                    .put("folder", two.folderModified).toString(), Charsets.UTF_8)
                SlotLog.i(TAG, "Two saves of ${two.path}: the " +
                    (if (two.handheldSetAside) "folder's" else "handheld's") + " is in place, asked at launch")
            } catch (error: Exception) {
                SlotLog.w(TAG, "Two saves of ${two.path} not kept for the choice", error)
            }
        }
    }

    /** This game's two saves, if a copy met them. */
    fun twoSavesFor(target: RetroArchStorage.Target): TwoSavesFound? {
        if (!twoDir.isDirectory) return null
        val cores = if (target.isGba && target.shareGbaSaves) listOf("mGBA", "gpSP") else listOf(target.core)
        for (core in cores) {
            val path = "Saves/$core/${target.stem}.${Systems.saveExtension(core)}"
            val key = FolderSync.sha1(path.toByteArray(Charsets.UTF_8))
            val json = File(twoDir, "$key.json").takeIf { it.isFile } ?: continue
            return try {
                val j = JSONObject(json.readText(Charsets.UTF_8))
                TwoSavesFound(path, key, j.getBoolean("handheldSetAside"), j.getLong("handheld"), j.getLong("folder"))
            } catch (_: Exception) { null }
        }
        return null
    }

    /** The player's choice: [useSetAside] puts the other save in place, the
     *  one in place kept beside it; the folder follows. */
    fun settleTwoSaves(found: TwoSavesFound, useSetAside: Boolean) {
        val other = File(twoDir, "${found.key}.srm")
        if (useSetAside && other.isFile) {
            val file = File(activity.filesDir, found.path)
            file.takeIf { it.isFile }?.let { File(it.parentFile, it.name + ".before-choice").writeBytes(it.readBytes()) }
            RetroArchStorage.stage(file, other.readBytes())
        }
        other.delete()
        File(twoDir, "${found.key}.json").delete()
        if (useSetAside) sync(pull = false)
    }

    /** Saves and states removed from the folder by hand, still held back. */
    fun gonePaths(): Set<String> = if (tree() == null) emptySet() else paths(goneFile)

    /** PUT IT BACK: these go back into the folder now. */
    fun putBack(paths: Collection<String>) {
        if (paths.isEmpty()) return
        writePaths(restoreFile, paths(restoreFile) + paths)
        sync(pull = false)
    }

    /** DELETE IT: slot.'s copies are gone; the folder follows now. */
    fun deleted(paths: Collection<String>) {
        writePaths(goneFile, paths(goneFile) - paths.toSet())
        sync(pull = false)
    }

    /** The first copy's progress on Library's row, a few times a second. */
    private fun progress(): (Int, Int) -> Unit {
        var shown = -1
        var at = 0L
        return { done, all ->
            val percent = if (all == 0) 100 else done * 100 / all
            val now = android.os.SystemClock.elapsedRealtime()
            if (percent != shown && (now - at >= 250 || percent == 100)) {
                shown = percent
                at = now
                copyAt = percent.coerceAtMost(99)
                activity.nativeSetSlotFolderCopy(percent)
            }
        }
    }

    private fun ui(block: () -> Unit) = activity.runOnUiThread { if (!activity.isDestroyed) block() }

    /** Android's volume state can lag a filesystem that remounts itself
     *  read-only. Inspect mountinfo when available, but never guess from a
     *  generic provider mkdir failure alone. */
    private fun readOnly(tree: Uri): Boolean {
        val volume = volumeOf(tree)?.takeIf { it != "primary" && it.isNotEmpty() } ?: return false
        val reported = try {
            activity.getSystemService(StorageManager::class.java).storageVolumes.any {
                it.uuid.equals(volume, ignoreCase = true) &&
                    it.state == android.os.Environment.MEDIA_MOUNTED_READ_ONLY
            }
        } catch (_: Exception) { false }
        if (reported) return true
        return try {
            File("/proc/self/mountinfo").useLines { lines ->
                lines.any { SlotStorageProblem.readOnlyMount(it, volume) }
            }
        } catch (_: Exception) { false }
    }

    private fun checkStorageWritable(tree: Uri) {
        if (readOnly(tree)) throw IOException("EROFS: Read-only file system")
        if (!activity.contentResolver.persistedUriPermissions.any {
                it.uri == tree && it.isReadPermission && it.isWritePermission
            }) throw SecurityException("No persistent write access to slot folder")
    }

    private fun storageProblem(tree: Uri, failures: List<String>): String =
        SlotStorageProblem.message(failures, readOnly(tree))

    /** The problem last shown and the files it was about: the same one
     *  again, retried every 30 s, is logged but not shown again until a copy
     *  goes through (3.1.0: "COPY FAILED" kept coming back). */
    @Volatile private var shownProblem: String? = null

    private fun syncFailed(tree: Uri, failures: List<String>) {
        val problem = storageProblem(tree, failures)
        retryTree = tree.toString()
        retryAt = SystemClock.elapsedRealtime() + WRITE_RETRY_MS
        SlotLog.w(TAG, "Slot folder: $problem (${failures.size} failures): " +
            failures.take(8).joinToString(" | "))
        val key = problem + "|" + failures.map { it.substringBefore(": ") }.sorted().joinToString("|")
        if (key == shownProblem) return
        shownProblem = key
        ui { TaskPill.problem(problem) }
    }
    private fun toast(text: String) = SlotToast.makeText(activity, text, Toast.LENGTH_LONG).show()

    // -----------------------------------------------------------------
    // The folder through Android's document provider

    class Child(val id: String, val name: String, val dir: Boolean, val size: Long, val modified: Long)

    class SafSide(private val context: Context, private val tree: Uri, private val root: String) : FolderSync.Side {
        private val resolver = context.contentResolver
        private val dirs = HashMap<String, String>()
        private val failedDirs = HashMap<Pair<String, String>, Exception>()

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

        override fun list(): Map<String, FolderSync.Meta> = list(FolderSync.Scope.ALL)
        override fun list(scope: FolderSync.Scope): Map<String, FolderSync.Meta> {
            val out = HashMap<String, FolderSync.Meta>()
            fun walk(doc: String, prefix: String) {
                for (child in children(doc)) {
                    val path = prefix + child.name
                    if (child.dir && path in FolderSync.UNWALKED_DIRS) { staleIds.add(child.id); continue }
                    if (child.dir) { dirs[path] = child.id; if (scope == FolderSync.Scope.ALL || scope.descends(path)) walk(child.id, "$path/") }
                    else if (FolderSync.stale(path)) staleIds.add(child.id)
                    else if (!child.name.endsWith(".slot-tmp")) {
                        // Every file's id, slot.json's too: written again,
                        // it replaces the old one rather than making
                        // "slot (1).json" beside it (2.5.0-beta35).
                        files[path] = child.id
                        if (path != JSON && scope.includes(path)) out[path] = FolderSync.Meta(child.size, child.modified)
                    }
                }
            }
            dirs.clear(); files.clear(); staleIds.clear(); failedDirs.clear()
            walk(root, "")
            listed = true
            return out
        }
        private val files = HashMap<String, String>()
        override fun alias(path: String, listed: String) { files[listed]?.let { files[path] = it } }
        /** What the last listing found that the folder should no longer hold. */
        private val staleIds = ArrayList<String>()

        override fun dropStale(): Int {
            var dropped = 0
            for (id in staleIds) {
                try {
                    if (DocumentsContract.deleteDocument(resolver, DocumentsContract.buildDocumentUriUsingTree(tree, id))) dropped++
                } catch (error: Exception) {
                    SlotLog.w(TAG, "An old file stays in the slot folder", error)
                }
            }
            staleIds.clear()
            return dropped
        }
        private var listed = false
        /** Pictures, written straight when new (see write). */
        private val DIRECT = listOf("Labels/", "ArtNames/", "Studio/", "Wallpapers/", "home-real-labels/", "ArtworkHistory/")

        private fun dirOf(path: String): String = folderFor(path, root, dirs) { above, name ->
            val key = above to name
            failedDirs[key]?.let { throw it }
            try {
                // The folder of that name in another letter case is the same
                // one where letter case isn't told apart (internal storage,
                // SD cards): asked to make it, Android made "POKEMINI (1)",
                // "(2)"... each copy, until it refused (3.0.0).
                val here = children(above).filter { it.dir }
                (here.firstOrNull { it.name == name } ?: here.firstOrNull { it.name.equals(name, ignoreCase = true) })
                    ?.id ?: createDir(above, name)
            } catch (error: Exception) {
                failedDirs[key] = error
                throw error
            }
        }

        /** One disposable byte, through the exact provider used by sync. */
        fun probeWrite() {
            val parent = DocumentsContract.buildDocumentUriUsingTree(tree, root)
            val name = ".write-${UUID.randomUUID()}.slot-tmp"
            val probe = DocumentsContract.createDocument(resolver, parent,
                "application/octet-stream", name) ?: throw IOException("Cannot create write probe")
            try {
                resolver.openOutputStream(probe, "wt")?.use { it.write(0) }
                    ?: throw IOException("Cannot write probe")
            } catch (error: Exception) {
                try { DocumentsContract.deleteDocument(resolver, probe) } catch (_: Exception) {}
                throw error
            }
            check(DocumentsContract.deleteDocument(resolver, probe)) { "Cannot remove write probe" }
        }

        /** A file at the folder's top, as text; null if absent. */
        fun readRootText(name: String): String? = try {
            children(root).firstOrNull { !it.dir && it.name == name }?.let {
                files[name] = it.id
                String(readDoc(it.id), Charsets.UTF_8)
            }
        } catch (_: Exception) { null }

        /** CustomCheats/ and a folder per system, ready for the player's
         *  .cht files, with a word on how to name them. */
        fun makeCheatFolders(): String? {
            try {
                for (system in CHEAT_SYSTEMS) dirOf("CustomCheats/$system/x")
                val readme = dirOf(CHEAT_README)
                if (children(readme).none { it.name == "README.txt" }) {
                    write(CHEAT_README, CHEAT_HELP.toByteArray(Charsets.UTF_8))
                }
                return null
            } catch (error: Exception) {
                SlotLog.w(TAG, "CustomCheats folders not made", error)
                return "CustomCheats: ${SlotStorageProblem.describe(error)}"
            }
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
                ?: if (!listed || path.startsWith("Conflicts/")) children(parent).firstOrNull { !it.dir && it.name == name }?.id else null
            // A new label or picture straight under its name: three calls to
            // Android instead of six, the first copy of hundreds of labels
            // took minutes (beta38). Saves, states and settings still go
            // through a temporary file.
            if (existing == null && DIRECT.any { path.startsWith(it) }) {
                val made = DocumentsContract.createDocument(resolver, parentUri, "application/octet-stream", name)
                    ?: throw IOException("couldn't write $path")
                try {
                    resolver.openOutputStream(made, "wt")?.use { it.write(bytes) } ?: throw IOException("couldn't write $path")
                } catch (error: Exception) {
                    try { DocumentsContract.deleteDocument(resolver, made) } catch (_: Exception) {}
                    throw error
                }
                val id = DocumentsContract.getDocumentId(made)
                files[path] = id
                return meta(id) ?: FolderSync.Meta(bytes.size.toLong(), System.currentTimeMillis())
            }
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
                // Some SD cards' providers refuse the rename: the file is then
                // written under its own name from the bytes in hand, and the
                // temporary one goes (3.1.0). A refused rename used to leave
                // "<name>.slot-tmp" in the folder.
                val renamed = try { DocumentsContract.renameDocument(resolver, temp, name) } catch (_: Exception) { null }
                val placed = renamed ?: run {
                    SlotLog.w(TAG, "Slot folder: rename refused for $path, written under its name instead")
                    val made = DocumentsContract.createDocument(resolver, parentUri, "application/octet-stream", name)
                        ?: throw IOException("couldn't write $path")
                    try {
                        resolver.openOutputStream(made, "wt")?.use { it.write(bytes) } ?: throw IOException("couldn't write $path")
                    } catch (error: Exception) {
                        try { DocumentsContract.deleteDocument(resolver, made) } catch (_: Exception) {}
                        throw error
                    }
                    try { DocumentsContract.deleteDocument(resolver, temp) } catch (_: Exception) {}
                    made
                }
                val id = DocumentsContract.getDocumentId(placed)
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
