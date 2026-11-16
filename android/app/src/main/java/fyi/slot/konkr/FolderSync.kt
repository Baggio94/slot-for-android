package fyi.slot.konkr

import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.security.MessageDigest
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.TimeZone

/**
 * The two-way copy between slot.'s private files and the slot. folder
 * (slot. 2.5, claude/slot-portable-scenarios-v1.md). Pure logic over two
 * [Side]s, so it is tested on plain folders.
 *
 * The private files stay slot.'s working copy; the folder is their mirror,
 * on the card that goes from one handheld to another. What each file was at
 * the last copy (its size and date on both sides, and its fingerprint) is
 * kept in a [Base], so each side's changes are known:
 *  - changed on one side only: copied to the other;
 *  - changed on both: the newer one wins, the other is kept in the
 *    folder's Conflicts/ (nothing is lost, rule 4);
 *  - removed from slot. (a polaroid deleted): removed from the folder too,
 *    but only if the folder's copy is the one slot. last wrote;
 *  - removed from the folder by hand: put back (rule 3).
 * Every copy goes through a temporary file, so a cut never leaves half a
 * file, and is checked by size before it counts.
 */
internal object FolderSync {
    /** A file's size and last change, in ms. */
    data class Meta(val size: Long, val modified: Long)

    /** One side: the private files or the folder. Paths are relative,
     *  with '/' (`Saves/mGBA/Pokemon.srm`). */
    interface Side {
        fun list(): Map<String, Meta>
        fun read(path: String): ByteArray
        /** Writes through a temporary file; returns the file's new meta. */
        fun write(path: String, bytes: ByteArray): Meta
        fun delete(path: String)
    }

    /** What a file was on both sides at the last copy. */
    data class Entry(val privateSize: Long, val privateModified: Long,
                     val folderSize: Long, val folderModified: Long, val sha1: String)

    class Base(val entries: MutableMap<String, Entry> = HashMap()) {
        fun toJson(): String {
            val json = JSONObject()
            for ((path, e) in entries) json.put(path, JSONArray().put(e.privateSize).put(e.privateModified)
                .put(e.folderSize).put(e.folderModified).put(e.sha1))
            return json.toString()
        }
        companion object {
            fun parse(text: String?): Base {
                val base = Base()
                if (text.isNullOrBlank()) return base
                try {
                    val json = JSONObject(text)
                    for (path in json.keys()) {
                        val a = json.getJSONArray(path)
                        base.entries[path] = Entry(a.getLong(0), a.getLong(1), a.getLong(2), a.getLong(3), a.getString(4))
                    }
                } catch (_: Exception) {}
                return base
            }
        }
    }

    /** How the first copy into a folder decides. */
    enum class Start {
        /** Later copies: each side's changes since the last one. */
        SYNC,
        /** A new, empty folder, or Merge: no base yet, newer wins. */
        MERGE,
        /** "Use this folder's": the folder wins (the device was backed up). */
        FOLDER_WINS,
    }

    class Report {
        var toFolder = 0
        var toPrivate = 0
        var conflicts = 0
        var removed = 0
        val failures = ArrayList<String>()
        val changedPrivate: Boolean get() = toPrivate > 0
    }

    /** [pull]: false copies to the folder only (during a game, at pause). */
    fun sync(private: Side, folder: Side, base: Base, start: Start = Start.SYNC,
             pull: Boolean = true, now: Long = System.currentTimeMillis()): Report {
        val report = Report()
        val mine = private.list()
        val theirs = folder.list()
        for (path in (mine.keys + theirs.keys).sorted()) {
            if (path.startsWith("Conflicts/")) continue
            try {
                step(path, mine[path], theirs[path], base, private, folder, start, pull, now, report)
            } catch (error: Exception) {
                report.failures.add("$path: ${error.message}")
            }
        }
        // Forget what neither side holds any more.
        base.entries.keys.retainAll(mine.keys + theirs.keys)
        return report
    }

    private fun step(path: String, p: Meta?, f: Meta?, base: Base, private: Side, folder: Side,
                     start: Start, pull: Boolean, now: Long, report: Report) {
        val b = if (start == Start.SYNC) base.entries[path] else null
        val pChanged = p != null && (b == null || p.size != b.privateSize || p.modified != b.privateModified)
        val fChanged = f != null && (b == null || f.size != b.folderSize || f.modified != b.folderModified)
        when {
            p != null && f == null -> {
                // New in slot., or removed from the folder by hand: put back.
                val bytes = private.read(path)
                val written = folder.write(path, bytes)
                check(written.size == bytes.size.toLong()) { "copy to the folder came out short" }
                base.entries[path] = Entry(p.size, p.modified, written.size, written.modified, sha1(bytes))
                report.toFolder++
            }
            p == null && f != null -> {
                if (b != null && !fChanged) {
                    // slot. removed it, and the folder's is the one it wrote.
                    folder.delete(path)
                    base.entries.remove(path)
                    report.removed++
                } else if (pull) {
                    val bytes = folder.read(path)
                    val written = private.write(path, bytes)
                    base.entries[path] = Entry(written.size, written.modified, f.size, f.modified, sha1(bytes))
                    report.toPrivate++
                }
            }
            p != null && f != null -> when {
                !pChanged && !fChanged -> {}
                pChanged && !fChanged -> toFolder(path, p, private, folder, base, report)
                !pChanged && fChanged -> if (pull) toPrivate(path, f, private, folder, base, report)
                else -> {
                    // Both changed, or the first copy: the same bytes are fine.
                    val mineBytes = private.read(path)
                    val theirBytes = folder.read(path)
                    val (ms, ts) = sha1(mineBytes) to sha1(theirBytes)
                    if (ms == ts) {
                        base.entries[path] = Entry(p.size, p.modified, f.size, f.modified, ms)
                        return
                    }
                    val folderWins = when (start) {
                        Start.FOLDER_WINS -> true
                        else -> f.modified > p.modified
                    }
                    if (folderWins && !pull) {
                        // A game is on: the choice waits for the next pull.
                        return
                    }
                    // "Use the folder's": the handheld's copy is in its backup.
                    if (start != Start.FOLDER_WINS) report.conflicts++
                    val stamp = stamp(now)
                    if (folderWins) {
                        if (start != Start.FOLDER_WINS) folder.write("Conflicts/$path.$stamp", mineBytes)
                        val written = private.write(path, theirBytes)
                        base.entries[path] = Entry(written.size, written.modified, f.size, f.modified, ts)
                        report.toPrivate++
                    } else {
                        folder.write("Conflicts/$path.$stamp", theirBytes)
                        val written = folder.write(path, mineBytes)
                        base.entries[path] = Entry(p.size, p.modified, written.size, written.modified, ms)
                        report.toFolder++
                    }
                }
            }
        }
    }

    private fun toFolder(path: String, p: Meta, private: Side, folder: Side, base: Base, report: Report) {
        val bytes = private.read(path)
        val written = folder.write(path, bytes)
        check(written.size == bytes.size.toLong()) { "copy to the folder came out short" }
        base.entries[path] = Entry(p.size, p.modified, written.size, written.modified, sha1(bytes))
        report.toFolder++
    }

    private fun toPrivate(path: String, f: Meta, private: Side, folder: Side, base: Base, report: Report) {
        val bytes = folder.read(path)
        val written = private.write(path, bytes)
        check(written.size == bytes.size.toLong()) { "copy from the folder came out short" }
        base.entries[path] = Entry(written.size, written.modified, f.size, f.modified, sha1(bytes))
        report.toPrivate++
    }

    private fun stamp(now: Long): String = SimpleDateFormat("yyyyMMdd-HHmmss", Locale.ROOT)
        .apply { timeZone = TimeZone.getTimeZone("UTC") }.format(Date(now))

    fun sha1(bytes: ByteArray): String = MessageDigest.getInstance("SHA-1").digest(bytes)
        .joinToString("") { "%02x".format(Locale.ROOT, it.toInt() and 0xff) }

    // -----------------------------------------------------------------
    // What travels in the slot. folder (scenarios §2), from slot.'s files.

    /** Folders that travel whole. */
    private val TRAVELLING_DIRS = listOf("Saves", "States", "Labels", "Studio", "GameEmulation",
        "Cheats", "CustomCheats", "Wallpapers", "ArtNames")
    /** Single files that travel. */
    private val TRAVELLING_FILES = setOf("slot-konkr-settings.json", "slot-konkr-controls.json",
        "konkr-favorites.json", "konkr-recents.json", "Config/cart_shell.ini",
        "Config/default_core.ini", "Config/selected_core.ini", "Config/theme.txt")
    // Not Config/video_mode.ini: Screen Size depends on each screen's shape,
    // so every handheld keeps its own (Siham 2026-11-16).

    /** Whether a private file travels: never secrets (RomM, RetroAchievements),
     *  device links, caches, nor slot.'s own one-shot markers and copies. */
    fun travels(path: String): Boolean {
        if (path in TRAVELLING_FILES) return true
        val top = path.substringBefore('/')
        if (top !in TRAVELLING_DIRS || !path.contains('/')) return false
        val name = path.substringAfterLast('/')
        if (name.endsWith(".tmp") || name.endsWith(".carried") || name.endsWith(".once")) return false
        if (name.startsWith(".")) return false
        if (path.startsWith("Saves/RetroArchBackups/") || path.startsWith("Saves/ManualImports/")) return false
        if (path.startsWith("Saves/LinkBackups/") || path.startsWith("Saves/Link")) return false
        return true
    }

    /** slot.'s private files as a [Side], only those that travel. */
    class PrivateSide(private val root: File) : Side {
        override fun list(): Map<String, Meta> {
            val out = HashMap<String, Meta>()
            fun walk(dir: File, prefix: String) {
                for (file in dir.listFiles() ?: return) {
                    val path = prefix + file.name
                    if (file.isDirectory) walk(file, "$path/")
                    else if (travels(path)) out[path] = Meta(file.length(), file.lastModified())
                }
            }
            walk(root, "")
            return out
        }
        override fun read(path: String): ByteArray = File(root, path).readBytes()
        override fun write(path: String, bytes: ByteArray): Meta {
            val file = File(root, path)
            file.parentFile?.mkdirs()
            val temp = File(file.parentFile, file.name + ".sync.tmp")
            temp.writeBytes(bytes)
            if (!temp.renameTo(file)) {
                temp.delete()
                throw java.io.IOException("couldn't replace $path")
            }
            return Meta(file.length(), file.lastModified())
        }
        override fun delete(path: String) { File(root, path).delete() }
    }
}
