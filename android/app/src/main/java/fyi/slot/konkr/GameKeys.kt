package fyi.slot.konkr

import android.util.Log
import java.io.File
import java.security.MessageDigest

/**
 * A game's names for the files slot. keeps about it (slot. 2.5).
 *
 * Its portable name is its system and file name (`gba/pokemon emerald
 * (usa).gba`, library.rs game_key), the same on every handheld, where the
 * Android address of the ROM changes with each device and card. Slot. 2.4
 * and older named the files after that address: they are carried over the
 * first time they're needed.
 */
internal object GameKeys {
    private const val TAG = "SlotKonkr"

    @JvmStatic private external fun nativeGameKey(uri: String): String?

    /** The game's portable name; its address when the native library
     *  doesn't know it (a game outside the shelves, or in unit tests). */
    fun name(uri: String): String = try {
        nativeGameKey(uri)?.takeIf { it.isNotEmpty() } ?: uri
    } catch (_: UnsatisfiedLinkError) {
        uri
    }

    /** Labels, Cart Studio shells: FNV-1a of the name (core_selection.rs). */
    fun key(uri: String): String = fnv(name(uri))
    fun legacyKey(uri: String): String = fnv(uri)

    /** Saves, states and the ROM cache: SHA-256 of the name. */
    fun digest(uri: String): String = sha256(name(uri))
    fun legacyDigest(uri: String): String = sha256(uri)

    fun fnv(text: String): String {
        var hash = -3750763034362895579L // 0xcbf29ce484222325
        for (byte in text.toByteArray(Charsets.UTF_8)) {
            hash = (hash xor (byte.toInt() and 0xff).toLong()) * 0x100000001b3L
        }
        return java.lang.Long.toUnsignedString(hash, 16).padStart(16, '0')
    }

    private fun sha256(text: String): String = MessageDigest.getInstance("SHA-256")
        .digest(text.toByteArray(Charsets.UTF_8))
        .joinToString("") { "%02x".format(java.util.Locale.ROOT, it.toInt() and 0xff) }

    /** `dir/<key><suffix>`, after carrying slot. 2.4's file there. */
    fun file(dir: File, uri: String, suffix: String): File {
        val file = File(dir, key(uri) + suffix)
        carry(File(dir, legacyKey(uri) + suffix), file)
        return file
    }

    /** Copies `old` to `new` when only the old one exists, then keeps the
     *  old one as `<name>.carried` (never deleted, never carried twice). */
    fun carry(old: File, new: File) {
        if (old == new || new.exists() || !old.isFile) return
        try {
            new.parentFile?.mkdirs()
            val temp = File(new.parentFile, new.name + ".carry.tmp")
            old.copyTo(temp, overwrite = true)
            if (!temp.renameTo(new)) { temp.delete(); return }
            old.renameTo(File(old.parentFile, old.name + ".carried"))
        } catch (error: Exception) {
            warn("Couldn't carry ${old.name} over", error)
        }
    }

    /**
     * A game's private saves and states, named `<digest>.<core>...` after
     * its address in 2.4, take its portable digest: each file moves to the
     * new name unless one is already there, and the few small records that
     * name their own files (state-generation.active) are rewritten with it.
     * False when a file couldn't be carried (it is tried again next time).
     */
    fun carryGameFiles(saves: File, old: String, new: String): Boolean {
        if (old == new || old.isEmpty() || new.isEmpty()) return true
        var failed = false
        val dirs = listOf(saves, File(saves, "ManualExports"), File(saves, "ManualImports"))
        var moved = 0
        for (dir in dirs) {
            val files = dir.listFiles { file -> file.isFile && file.name.startsWith("$old.") } ?: continue
            for (file in files) {
                val target = File(dir, new + file.name.removePrefix(old))
                if (target.exists()) continue
                try {
                    if (file.length() in 1..512) {
                        val text = file.readBytes().toString(Charsets.UTF_8)
                        if (text.contains(old)) {
                            val temp = File(dir, target.name + ".carry.tmp")
                            temp.writeBytes(text.replace(old, new).toByteArray(Charsets.UTF_8))
                            if (temp.renameTo(target)) { file.delete(); moved++ } else { temp.delete(); failed = true }
                            continue
                        }
                    }
                    if (file.renameTo(target)) moved++ else failed = true
                } catch (error: Exception) {
                    failed = true
                    warn("Couldn't carry ${file.name} over", error)
                }
            }
        }
        if (moved > 0) {
            try { Log.i(TAG, "Carried $moved save files to the game's portable name") }
            catch (_: RuntimeException) {} // JVM unit tests have no Android log
        }
        return !failed
    }

    private fun warn(message: String, error: Exception) {
        try { Log.w(TAG, message, error) } catch (_: RuntimeException) {}
    }
}
