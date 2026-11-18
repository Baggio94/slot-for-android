package fyi.slot.konkr

import java.io.File
import java.util.Locale

/**
 * A game's save or save state deleted outside slot. (slot. 2.5, Siham
 * 2026-11-18): in the RetroArch folders or by hand in the slot. folder.
 * slot. no longer puts it back behind the player's back: at that game's
 * launch it asks, PUT IT BACK or DELETE IT. Deleting removes it everywhere
 * (slot.'s own copy, the RetroArch folders, the slot. folder, RomM), with a
 * hidden backup kept on this handheld only (Saves/FolderBackups, which
 * never travels). Not a deletion: a card that is out, a folder slot. can't
 * read, a file slot. never synced.
 */
internal object SaveDeletes {
    enum class Kind { SAVE, STATE }
    enum class Where { RETROARCH, SLOT_FOLDER }
    enum class Choice { PUT_BACK, DELETE }

    /** What was deleted for one game, and where. */
    data class Gone(val kinds: Set<Kind>, val where: Set<Where>, val folderPaths: Set<String>) {
        fun isEmpty() = kinds.isEmpty()
        operator fun plus(other: Gone) = Gone(kinds + other.kinds, where + other.where,
            folderPaths + other.folderPaths)
        companion object { val NONE = Gone(emptySet(), emptySet(), emptySet()) }
    }

    private val HASHED = Regex("[0-9a-f]{16,64}")

    /**
     * A slot.-folder path that is a game's save or auto-state, and which:
     * Saves/<core>/<rom>.srm|.rtc, States/<core>/<rom>.state.auto, and
     * slot.'s own states under Saves/ (<game>.<core>.state, its generations
     * and RetroArch copies). Polaroids, links and the rest: null.
     */
    fun kindOf(path: String): Kind? {
        val parts = path.split('/')
        if (parts.size == 3 && parts[0] == "Saves" && parts[1] in RetroArchStorage.CORES &&
            (parts[2].endsWith(".srm") || parts[2].endsWith(".rtc"))) return Kind.SAVE
        if (parts.size == 3 && parts[0] == "States" && parts[1] in RetroArchStorage.CORES &&
            parts[2].endsWith(".state.auto")) return Kind.STATE
        if (parts.size == 2 && parts[0] == "Saves") {
            val name = parts[1]
            val id = name.substringBefore('.')
            if (id.matches(HASHED) && (name.endsWith(".state") || name.endsWith(".state.auto"))) return Kind.STATE
        }
        return null
    }

    /** The slot.-folder paths among [paths] that are this game's, by kind. */
    fun forGame(paths: Collection<String>, core: String, stem: String, id: String,
                shared: Boolean): Map<String, Kind> {
        val lower = RetroArchStorage.coreLower(core)
        val cores = if (shared) setOf("mGBA", "gpSP") else setOf(core)
        val out = LinkedHashMap<String, Kind>()
        for (path in paths) {
            val kind = kindOf(path) ?: continue
            val parts = path.split('/')
            val ours = when {
                parts.size == 3 -> parts[1] in (if (kind == Kind.SAVE) cores else setOf(core)) &&
                    (parts[2] == "$stem.srm" || parts[2] == "$stem.rtc" || parts[2] == "$stem.state.auto")
                else -> parts[1].startsWith("$id.$lower.") ||
                    (core == "mGBA" && parts[1] == "$id.state")
            }
            if (ours) out[path] = kind
        }
        return out
    }

    /** The window's words: what was deleted, of which game, from where. */
    /** [withState]: only the save is gone, and the game has a save state,
     *  which holds the save too: DELETE IT takes it as well (beta38). */
    fun message(title: String, gone: Gone, withState: Boolean = false): String {
        val what = when {
            gone.kinds.size == 2 -> "The save and save state"
            Kind.SAVE in gone.kinds -> "The save"
            else -> "The save state"
        }
        val verb = if (gone.kinds.size == 2) "were" else "was"
        val from = when {
            gone.where.size == 2 -> "your RetroArch and slot. folders"
            Where.RETROARCH in gone.where -> "your RetroArch folder"
            else -> "your slot. folder"
        }
        val too = if (withState) "Delete it from slot. too, with the game's save state, or put it back?"
            else "Delete it from slot. too, or put it back?"
        return "$what of $title $verb deleted from $from. $too"
    }

    /** Where a file deleted from slot. is kept, on this handheld only. */
    fun backupOf(files: File, core: String, name: String, now: Long): File {
        val stamp = java.text.SimpleDateFormat("yyyyMMdd-HHmmss", Locale.ROOT).format(java.util.Date(now))
        return File(files, "Saves/FolderBackups/$core/$stamp/$name")
    }

    /** Moves a private file to its backup (copy, then delete). */
    fun retire(files: File, file: File, core: String, now: Long) {
        if (!file.isFile) return
        val backup = backupOf(files, core, file.name, now)
        try {
            backup.parentFile?.mkdirs()
            file.copyTo(backup, overwrite = true)
        } catch (_: Exception) {}
        file.delete()
    }

    /**
     * A save or state another handheld deleted (the slot. folder's tomb):
     * this handheld's RetroArch folders still hold their copy, deleted at
     * the game's next launch if it is still what slot. left there
     * (RetroArchStorage.applyPendingDeletes).
     */
    fun markRetroArch(files: File, path: String) {
        val parts = path.split('/')
        val marks = when {
            parts.size == 3 && parts[2].endsWith(".srm") ->
                listOf("${parts[1]}/${parts[2]}", "${parts[1]}/${parts[2].removeSuffix(".srm")}.rtc")
            parts.size == 3 -> listOf("${parts[1]}/${parts[2]}")
            // slot.'s own state (or a generation of it), not its copies.
            parts.size == 2 && parts[1].endsWith(".state") -> {
                val bits = parts[1].split('.')
                if (bits.size >= 3) listOf("id/${bits[0]}.${bits[1]}") else emptyList()
            }
            else -> emptyList()
        }
        for (mark in marks) {
            try {
                val file = File(files, "SaveDeletes/$mark")
                file.parentFile?.mkdirs()
                file.writeBytes(byteArrayOf(1))
            } catch (_: Exception) {}
        }
    }

    /** A slot.-folder path's core folder name, for its backup. */
    fun coreOf(path: String): String {
        val parts = path.split('/')
        if (parts.size == 3 && parts[1] in RetroArchStorage.CORES) return parts[1]
        val lower = parts.last().split('.').getOrNull(1).orEmpty()
        return RetroArchStorage.CORES.firstOrNull { RetroArchStorage.coreLower(it) == lower } ?: "Other"
    }
}
