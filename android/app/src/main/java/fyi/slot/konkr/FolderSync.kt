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
 * (slot. 2.5). Pure logic over two
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
 *    but only if the folder's copy is the one slot. last wrote, and noted
 *    in the folder (a [Tomb]) so other handhelds remove theirs too;
 *  - a game's save or state removed from the folder by hand: held back
 *    until the player says, at that game's launch, whether to put it back
 *    or delete it everywhere (Siham 2026-11-18, SaveDeletes);
 *  - anything else removed from the folder by hand: put back (rule 3).
 * Only what travels is taken from the folder: nothing a Mac or another app
 * leaves there (._ files, .DS_Store, copies Android renamed).
 * Every copy goes through a temporary file, so a cut never leaves half a
 * file, and is checked by size before it counts.
 */
internal object FolderSync {
    /** A file's size and last change, in ms. */
    data class Meta(val size: Long, val modified: Long)

    /** Two different saves of a game, met in a copy: [other] is the one set
     *  aside, the handheld's when [handheldSetAside]. */
    class TwoSaves(val path: String, val other: ByteArray, val handheldSetAside: Boolean,
                   val handheldModified: Long, val folderModified: Long)

    /** One side: the private files or the folder. Paths are relative,
     *  with '/' (`Saves/mGBA/Pokemon.srm`). */
    enum class Scope {
        ALL, ARTWORK, SAVES, LAUNCH, REST;
        fun includes(path: String): Boolean = travels(path) && when (this) {
            ALL -> true
            ARTWORK -> path.startsWith("Labels/") || path.startsWith("ArtNames/") || PortableArtwork.travels(path)
            SAVES -> path.startsWith("Saves/") || path.startsWith("States/")
            LAUNCH -> forLaunch(path)
            REST -> !forLaunch(path)
        }
        fun descends(path: String): Boolean = when (this) {
            ALL -> path.substringBefore('/') in WALKED_DIRS
            ARTWORK -> path.substringBefore('/') in ARTWORK_DIRS
            SAVES -> path.substringBefore('/') in SAVE_DIRS
            LAUNCH -> path.substringBefore('/') in LAUNCH_DIRS
            REST -> path.substringBefore('/') in WALKED_DIRS && path.substringBefore('/') !in LAUNCH_DIRS
        }
    }

    interface Side {
        fun list(): Map<String, Meta>
        fun list(scope: Scope): Map<String, Meta> = list().filterKeys(scope::includes)
        fun read(path: String): ByteArray
        /** Writes through a temporary file; returns the file's new meta. */
        fun write(path: String, bytes: ByteArray): Meta
        fun delete(path: String)
        /** Removes what the folder should no longer hold (beta39: the
         *  RetroAchievements cache and slot.'s old RetroArch copies); how
         *  many it removed. Never anything that travels. */
        fun dropStale(): Int = 0
        /** The folder answers [path] by the file it listed as [listed]: the
         *  same file, its folders' names in other letter case (3.1.1). */
        fun alias(path: String, listed: String) {}
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

    /** A file deleted on purpose: what it was then, and when. Kept in the
     *  folder, so every handheld knows not to put its own copy back. */
    data class Tomb(val sha1: String, val at: Long)

    /** Tombs are forgotten after this long (a handheld away a year puts
     *  its copies back, as before). */
    private const val TOMB_MS = 365L * 24 * 3600 * 1000

    fun tombsToJson(tombs: Map<String, Tomb>): String {
        val json = JSONObject()
        for ((path, t) in tombs) json.put(path, JSONArray().put(t.sha1).put(t.at))
        return json.toString()
    }

    fun tombsFromJson(text: String?): MutableMap<String, Tomb> {
        val tombs = HashMap<String, Tomb>()
        if (text.isNullOrBlank()) return tombs
        try {
            val json = JSONObject(text)
            for (path in json.keys()) {
                val a = json.getJSONArray(path)
                tombs[path] = Tomb(a.getString(0), a.getLong(1))
            }
        } catch (_: Exception) {}
        return tombs
    }

    private val LAUNCH_ONLY = listOf("Saves/", "States/", "Cheats/", "CustomCheats/", "GameEmulation/")

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
        /** Saves and states removed from the folder by hand: held back. */
        val gone = LinkedHashSet<String>()
        /** Removed here because another handheld deleted them on purpose. */
        val deletedHere = ArrayList<String>()
        var tombsChanged = false
        var yielded = false
        /** What came in from the folder. */
        val pulled = ArrayList<String>()
        /** Old leftovers removed from the folder ([Side.dropStale]). */
        var stale = 0
        /** A game's two different saves: the newer one is in place, the
         *  player chooses at the game's launch (beta39). */
        val twoSaves = ArrayList<TwoSaves>()
        val changedPrivate: Boolean get() = toPrivate > 0 || deletedHere.isNotEmpty()
        /** Whether the shelf must be rebuilt: not for saves, states, cheats or
         *  a game's settings, read when a game starts (beta38: a rebuild
         *  froze the shelf for seconds). */
        val changedShelf: Boolean get() = (pulled + deletedHere).any { path ->
            LAUNCH_ONLY.none { path.startsWith(it) }
        }
    }

    /** A game's own files (save, states, settings, cheats), asked for first
     *  by its launch while a copy brings files in (3.0.1-beta11): a path
     *  is the game's when one of its parts is a [tokens] name, or starts
     *  with one and a dot (`Pokemon Emerald.srm`, `<digest>.mgba.state`). */
    class Priority(tokens: Set<String>) {
        private val tokens = tokens.filter { it.isNotBlank() }.toSet()
        val served = java.util.concurrent.CountDownLatch(1)
        fun wants(path: String): Boolean = path.split('/').any { part ->
            tokens.any { part == it || part.startsWith("$it.") }
        }
    }

    /**
     * [pull]: false copies to the folder only (during a game, at pause).
     * [tombs]: the folder's deletions, read and updated ([Report.tombsChanged]).
     * [restore]: held-back paths the player chose to put back.
     * [progress]: of the files the folder brings in (new or changed there
     *  since the last copy), how many are done (3.0.1-beta12: SYNCING
     *  counts these, not every file compared). None for a push.
     */
    fun sync(private: Side, folder: Side, base: Base, start: Start = Start.SYNC,
             pull: Boolean = true, now: Long = System.currentTimeMillis(),
             tombs: MutableMap<String, Tomb> = HashMap(), restore: Set<String> = emptySet(),
             progress: (Int, Int) -> Unit = { _, _ -> },
             launchFilesDone: () -> Unit = {}, scope: Scope = Scope.ALL,
             changed: () -> Unit = {}, shouldYield: () -> Boolean = { false },
             priority: () -> Priority? = { null }): Report {
        val report = Report()
        // What no longer travels is forgotten, never taken for a deletion
        // (beta39: the RetroAchievements cache left the folder).
        base.entries.keys.removeAll { !travels(it) }
        if (tombs.keys.removeAll { !travels(it) }) report.tombsChanged = true
        val mine = private.list(scope).filterKeys(scope::includes)
        val theirs = sameInAnyCase(mine.keys, folder.list(scope).filterKeys(scope::includes), folder)
        // What a game reads at its launch first (beta39): a launch waits for
        // those only, never for the labels behind them.
        val paths = (mine.keys + theirs.keys).sortedWith(compareBy({ !forLaunch(it) }, { it }))
        val inbound = if (!pull) emptySet() else paths.filterTo(HashSet()) { path ->
            val f = theirs[path] ?: return@filterTo false
            val b = if (start == Start.SYNC) base.entries[path] else null
            !path.startsWith("Conflicts/") && when {
                b != null -> f.size != b.folderSize || f.modified != b.folderModified
                else -> mine[path]?.let { it.size != f.size } ?: true
            }
        }
        var inboundDone = 0
        progress(0, inbound.size)
        var launchDone = false
        fun copy(path: String) {
            if (path in inbound) {
                inboundDone++
                progress(inboundDone, inbound.size)
            }
            if (path.startsWith("Conflicts/")) return
            try {
                val before = report.toFolder + report.toPrivate + report.removed + report.deletedHere.size
                step(path, mine[path], theirs[path], base, private, folder, start, pull, now, report,
                    tombs, restore)
                if (report.toFolder + report.toPrivate + report.removed + report.deletedHere.size > before) changed()
            } catch (error: Exception) {
                report.failures.add("$path: ${SlotStorageProblem.describe(error)}")
            }
        }
        // A launch's own files go first, by the same steps (conflicts,
        // deletions, the one writer): the rest follows in its order.
        val early = HashSet<String>()
        var asked: Priority? = null
        for ((done, path) in paths.withIndex()) {
            priority()?.takeIf { it !== asked }?.let { want ->
                asked = want
                for (next in paths.subList(done, paths.size)) {
                    if (next !in early && want.wants(next)) { early.add(next); copy(next) }
                }
                want.served.countDown()
            }
            if (!launchDone && !forLaunch(path)) { launchDone = true; launchFilesDone() }
            if (!forLaunch(path) && shouldYield()) {
                report.yielded = true
                return report
            }
            if (path in early) continue
            copy(path)
        }
        if (!launchDone) launchFilesDone()
        // Old leftovers go last (beta40): removing thousands of badges from
        // a card takes minutes, and held up a launch when it came first.
        if (pull && (scope == Scope.ALL || scope == Scope.REST)) report.stale = folder.dropStale()
        // Gone from both since the last copy: deleted here on purpose (DELETE
        // IT, at a game's launch). Other handhelds follow it.
        for ((path, entry) in base.entries) {
            if (scope.includes(path) && path !in mine && path !in theirs && tombs[path]?.sha1 != entry.sha1) {
                tombs[path] = Tomb(entry.sha1, now)
                report.tombsChanged = true
            }
        }
        // Forget what neither side holds any more, and old tombs.
        base.entries.keys.removeAll { scope.includes(it) && it !in mine && it !in theirs }
        if (tombs.entries.removeAll { scope.includes(it.key) && now - it.value.at > TOMB_MS }) report.tombsChanged = true
        return report
    }

    private fun step(path: String, p: Meta?, f: Meta?, base: Base, private: Side, folder: Side,
                     start: Start, pull: Boolean, now: Long, report: Report,
                     tombs: MutableMap<String, Tomb>, restore: Set<String>) {
        var b = if (start == Start.SYNC) base.entries[path] else null
        var pChanged = p != null && (b == null || p.size != b.privateSize || p.modified != b.privateModified)
        var fChanged = f != null && (b == null || f.size != b.folderSize || f.modified != b.folderModified)
        // Only the time changed, not the size: the bytes say (beta38). An SD
        // card gives back other times than at the copy, and a save rewritten
        // as it was is not a change: copying hundreds of such files froze
        // the shelf, and a deleted save came back without asking.
        if (b != null && pChanged && p!!.size == b.privateSize && sha1(private.read(path)) == b.sha1) {
            b = b.copy(privateModified = p.modified)
            base.entries[path] = b
            pChanged = false
        }
        if (b != null && fChanged && f!!.size == b.folderSize && sha1(folder.read(path)) == b.sha1) {
            b = b.copy(folderModified = f.modified)
            base.entries[path] = b
            fChanged = false
        }
        when {
            p != null && f == null -> {
                val tomb = tombs[path]
                val deletedElsewhere = tomb != null &&
                    ((b != null && !pChanged && b.sha1 == tomb.sha1) || sha1(private.read(path)) == tomb.sha1)
                when {
                    deletedElsewhere -> {
                        // Another handheld deleted this very file on purpose:
                        // this one follows (the private side keeps a backup of
                        // a save or state). Not during a game.
                        if (!pull) return
                        private.delete(path)
                        base.entries.remove(path)
                        report.deletedHere.add(path)
                    }
                    // (A deletion made on purpose elsewhere, after this one
                    // played on: this handheld's save stays, as before.)
                    b != null && SaveDeletes.kindOf(path) != null && path !in restore &&
                        (tomb == null || !pChanged) -> {
                        // A save or state removed from the folder by hand:
                        // the player says at the game's launch what to do,
                        // even after playing on here (PUT IT BACK then puts
                        // this handheld's newer one back, beta38).
                        report.gone.add(path)
                    }
                    else -> {
                        // New in slot., or removed from the folder by hand: put back.
                        val bytes = private.read(path)
                        val written = folder.write(path, bytes)
                        check(written.size == bytes.size.toLong()) { "copy to the folder came out short" }
                        base.entries[path] = Entry(p.size, p.modified, written.size, written.modified, sha1(bytes))
                        if (tombs.remove(path) != null) report.tombsChanged = true
                        report.toFolder++
                    }
                }
            }
            p == null && f != null -> {
                if (b != null && !fChanged) {
                    // slot. removed it, and the folder's is the one it wrote:
                    // gone from the folder, and other handhelds are told.
                    folder.delete(path)
                    base.entries.remove(path)
                    tombs[path] = Tomb(b.sha1, now)
                    report.tombsChanged = true
                    report.removed++
                } else if (pull) {
                    val bytes = folder.read(path)
                    val written = private.write(path, bytes)
                    base.entries[path] = Entry(written.size, written.modified, f.size, f.modified, sha1(bytes))
                    report.toPrivate++
                    report.pulled.add(path)
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
                    if (start != Start.FOLDER_WINS && Systems.isSaveName(path) && SaveDeletes.kindOf(path) != null) {
                        report.twoSaves.add(TwoSaves(path, if (folderWins) mineBytes else theirBytes,
                            folderWins, p.modified, f.modified))
                    }
                    val stamp = stamp(now)
                    if (folderWins) {
                        if (start != Start.FOLDER_WINS) folder.write("Conflicts/$path.$stamp", mineBytes)
                        val written = private.write(path, theirBytes)
                        base.entries[path] = Entry(written.size, written.modified, f.size, f.modified, ts)
                        report.toPrivate++
                        report.pulled.add(path)
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
        report.pulled.add(path)
    }

    private fun stamp(now: Long): String = SimpleDateFormat("yyyyMMdd-HHmmss", Locale.ROOT)
        .apply { timeZone = TimeZone.getTimeZone("UTC") }.format(Date(now))

    fun sha1(bytes: ByteArray): String {
        val digest = MessageDigest.getInstance("SHA-1").digest(bytes)
        val hex = "0123456789abcdef"
        return buildString(digest.size * 2) {
            for (byte in digest) {
                val n = byte.toInt() and 0xff
                append(hex[n ushr 4]); append(hex[n and 15])
            }
        }
    }

    // -----------------------------------------------------------------
    // What travels in the slot. folder (scenarios §2), from slot.'s files.

    /** Folders that travel whole. */
    private val TRAVELLING_DIRS = listOf("Saves", "States", "Labels", "Studio", "GameEmulation",
        "Cheats", "CustomCheats", "Wallpapers", "ArtNames")
    private val ARTWORK_DIRS = setOf("Labels", "ArtNames", "ArtworkHistory", "home-real-labels")
    private val SAVE_DIRS = setOf("Saves", "States")
    private val LAUNCH_DIRS = LAUNCH_ONLY.map { it.trimEnd('/') }.toSet()
    private val WALKED_DIRS = TRAVELLING_DIRS.toSet() + ARTWORK_DIRS + setOf("Config")
    /** Single files that travel. */
    private val TRAVELLING_FILES = setOf("slot-konkr-settings.json", "slot-konkr-controls.json",
        "konkr-favorites.json", "konkr-recents.json", "Config/cart_shell.ini",
        "Config/default_core.ini", "Config/selected_core.ini", "Config/theme.txt")
    // Not Config/video_mode.ini: Screen Size depends on each screen's shape,
    // so every handheld keeps its own (Siham 2026-11-16).

    /** Read when a game starts: its save, state, cheats and settings. */
    fun forLaunch(path: String): Boolean = LAUNCH_ONLY.any { path.startsWith(it) }

    /** Folders that are never walked on either side: RetroAchievements'
     *  cache (thousands of badges) and its sign-in (beta39: they went to the
     *  card, a full copy took minutes and the token left the handheld). */
    val UNWALKED_DIRS = setOf("Saves/RetroAchievements")

    /** A folder path the folder should no longer hold (dropped once, beta39). */
    /**
     * The folder's files under the names slot. keeps them by: internal
     * storage and SD cards don't tell letter case apart, so slot.'s
     * "States/POKEMINI" and RetroArch's "States/PokeMini" are one folder
     * there, listed under one of the two names. Compared as they were
     * listed, the other name's files looked missing, and each copy asked
     * Android for another "POKEMINI (n)" until it refused (3.0.0,
     * BogWizard's Pocket Block). A name both sides hold as it is stays as it is.
     */
    internal fun sameInAnyCase(mine: Set<String>, theirs: Map<String, Meta>, folder: Side): Map<String, Meta> {
        if (theirs.keys.all { it in mine }) return theirs
        val byCase = mine.groupBy { it.lowercase(Locale.ROOT) }
        val out = HashMap<String, Meta>(theirs.size)
        for ((listed, meta) in theirs) {
            val own = if (listed in mine) listed
                else byCase[listed.lowercase(Locale.ROOT)]?.singleOrNull()?.takeIf { it !in theirs } ?: listed
            if (own != listed) folder.alias(own, listed)
            out[own] = meta
        }
        return out
    }

    /** "States/POKEMINI (3)": the folder Android made beside one whose name
     *  differs only in letter case, again at each copy, before 3.1.1. */
    private val CASE_COPY_DIR = Regex(""".+ \(\d{1,3}\)""")

    fun stale(path: String): Boolean = UNWALKED_DIRS.any { path == it || path.startsWith("$it/") } ||
        path.endsWith(".retroarch-export.state.auto") || path.endsWith(".retroarch-import.state.auto") ||
        path.endsWith(".state-export-protected")

    /** Whether a private file travels: never secrets (RomM, RetroAchievements),
     *  device links, caches, nor slot.'s own one-shot markers and copies. */
    fun travels(path: String): Boolean {
        if (path in TRAVELLING_FILES) return true
        // Only portable per-game records and CRC-keyed Studio images; never
        // the legacy URI-keyed databases or other private caches.
        if (PortableArtwork.travels(path)) return true
        if (UNWALKED_DIRS.any { path.startsWith("$it/") }) return false
        val top = path.substringBefore('/')
        if (top !in TRAVELLING_DIRS || !path.contains('/')) return false
        val name = path.substringAfterLast('/')
        if (name.endsWith(".tmp") || name.endsWith(".carried") || name.endsWith(".once")) return false
        // Never back into slot.: the player can delete those folders.
        if ((top == "States" || top == "Saves") && CASE_COPY_DIR.matches(path.split('/')[1])) return false
        // The RetroArch state decoded at each launch, and the guard that
        // protects this handheld's own RetroArch auto-state: both belong to
        // this handheld. Travelling, one handheld's routine deletion of them
        // reached the others (2.5).
        if (name.endsWith(".retroarch-import.state.auto") || name.endsWith(".state-export-protected")) return false
        // The copy staged for this handheld's RetroArch: made again from the
        // game's own state, it doubled the states' room in the folder (beta38).
        if (name.endsWith(".retroarch-export.state.auto")) return false
        if (name.startsWith(".")) return false
        if (path.startsWith("Saves/RetroArchBackups/") || path.startsWith("Saves/ManualImports/") ||
            path.startsWith("Saves/FolderBackups/")) return false
        if (path.startsWith("Saves/LinkBackups/") || path.startsWith("Saves/Link")) return false
        return true
    }

    /** slot.'s private files as a [Side], only those that travel. */
    open class PrivateSide(private val root: File) : Side {
        override fun list(): Map<String, Meta> = list(Scope.ALL)
        override fun list(scope: Scope): Map<String, Meta> {
            val out = HashMap<String, Meta>()
            fun walk(dir: File, prefix: String) {
                for (file in dir.listFiles() ?: return) {
                    val path = prefix + file.name
                    if (file.isDirectory) { if (path !in UNWALKED_DIRS && scope.descends(path)) walk(file, "$path/") }
                    else if (scope.includes(path)) out[path] = Meta(file.length(), file.lastModified())
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
