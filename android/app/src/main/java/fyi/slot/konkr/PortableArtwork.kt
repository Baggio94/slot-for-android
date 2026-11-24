package fyi.slot.konkr

import android.content.Context
import android.util.AtomicFile
import java.io.File
import java.util.concurrent.atomic.AtomicLong

/** Each game has an independent record: no URI-keyed databases or
 *  whole-library last-writer-wins merges between handhelds. */
internal object PortableArtwork {
    private val change = AtomicLong()
    private val record = Regex("ArtworkHistory/(home-cut|home-miss|gb-miss|gb-miss-v2)/[0-9a-f]{16}\\.txt")
    private val studio = Regex("home-real-labels/[0-9A-F]{8}\\.png")
    fun travels(path: String): Boolean = record.matches(path) || studio.matches(path)
    fun revision(): Long = change.get()

    private fun file(context: Context, kind: String, uri: String): File? {
        val name = GameKeys.name(uri)
        if (name == uri || name.isBlank() || name.startsWith("content://")) return null
        val path = "ArtworkHistory/$kind/${GameKeys.fnv(name)}.txt"
        if (!travels(path)) return null
        return File(context.filesDir, path)
    }
    fun read(context: Context, kind: String, uri: String): String? = try {
        file(context, kind, uri)?.takeIf { it.isFile && it.length() in 1..256 }?.readText()
    } catch (_: Exception) { null }
    fun write(context: Context, kind: String, uri: String, value: String) {
        val file = file(context, kind, uri) ?: return
        if (value.length !in 1..256 || read(context, kind, uri) == value) return
        try {
            file.parentFile?.mkdirs()
            val atomic = AtomicFile(file)
            val stream = atomic.startWrite()
            try {
                stream.write(value.toByteArray(Charsets.UTF_8))
                atomic.finishWrite(stream)
                change.incrementAndGet()
            } catch (error: Exception) {
                atomic.failWrite(stream)
                throw error
            }
        } catch (error: Exception) {
            SlotLog.w("PortableArtwork", "Could not keep artwork history", error)
        }
    }
    fun needsLookup(labelExists: Boolean, cut: String?, shell: String): Boolean =
        !labelExists || (cut != null && cut != shell)
    fun homeMiss(size: Long, modified: Long, shell: String): String = "$size\t$modified\t$shell"
    fun gbMiss(size: Long, modified: Long, at: Long): String = "$size\t$modified\t$at"
    fun validGbMiss(value: String?, size: Long, modified: Long, now: Long, ttl: Long): Boolean {
        val fields = value?.split('\t') ?: return false
        if (fields.size != 3 || fields[0].toLongOrNull() != size || fields[1].toLongOrNull() != modified) return false
        val at = fields[2].toLongOrNull() ?: return false
        return at > 0 && at <= now && now - at < ttl
    }
}
