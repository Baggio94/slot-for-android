package fyi.slot.konkr

/** Storage errors are classified only from explicit evidence, not guesses. */
internal object SlotStorageProblem {
    fun describe(error: Throwable): String = generateSequence(error) { it.cause }
        .take(8).joinToString(": ") { "${it.javaClass.simpleName}: ${it.message.orEmpty()}" }

    fun message(failures: List<String>, readOnly: Boolean = false): String {
        val text = failures.joinToString("\n").lowercase(java.util.Locale.ROOT)
        return when {
            readOnly || "erofs" in text || "read-only file system" in text || "read-only filesystem" in text ->
                "SD CARD READ-ONLY"
            "securityexception" in text || "eacces" in text || "eperm" in text ||
                "permission denied" in text || "no persistent write access" in text -> "FOLDER ACCESS DENIED"
            "enospc" in text || "no space" in text || "disk full" in text -> "NOT ENOUGH SPACE"
            else -> "COPY FAILED"
        }
    }

    /** mountinfo has per-mount flags before the separator and filesystem
     *  flags after it. FUSE can be rw even when the backing exFAT is ro. */
    fun readOnlyMount(line: String, volume: String): Boolean {
        val halves = line.split(" - ", limit = 2)
        if (halves.size != 2) return false
        val mount = halves[0].split(' ')
        val fs = halves[1].split(' ')
        if (mount.size < 6 || fs.size < 3) return false
        val path = mount[4]
        val matches = path.equals("/storage/$volume", true) ||
            path.equals("/mnt/media_rw/$volume", true) ||
            (path.startsWith("/mnt/pass_through/") && path.substringAfterLast('/').equals(volume, true))
        return matches && ("ro" in mount[5].split(',') || "ro" in fs[2].split(','))
    }
}
