package fyi.slot.konkr

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.util.AtomicFile
import android.util.Log
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.io.File
import java.net.HttpURLConnection
import java.net.URL

/**
 * slot. for Android's own cart art set (tools/home-art), served from R2 at
 * art.slot-android.app: per platform an index, <PLATFORM>.json, mapping a
 * dump's CRC32 to [type, region, file] entries (label scans, front renders,
 * a logo), and the files themselves under files/, named by their SHA-1.
 *
 * Asked first for every cart; the earlier sources (slot.'s art set for the
 * Game Boy family, ScreenScraper for the home consoles) stay behind it for
 * what it doesn't hold yet. Indexes are cached and refreshed every few hours,
 * files kept for good: a file's name is its content.
 */
internal object SlotArtSet {
    private const val TAG = "SlotArtSet"
    const val BASE = "https://art.slot-android.app/"
    private const val INDEX_FRESH_MS = 6L * 60 * 60 * 1000
    private const val MAX_INDEX = 16 * 1024 * 1024
    private const val MAX_FILE = 6 * 1024 * 1024
    private val filePath = Regex("files/[0-9a-f]{2}/[0-9a-f]{40}\\.(png|webp|jpg)")
    private val platforms = setOf(Systems.GB, Systems.GBC, Systems.GBA,
        Systems.NES, Systems.SNES, Systems.MD)

    class Entry(val type: String, val region: String, val file: String) {
        val url: String get() = BASE + file
    }

    private class Loaded(val at: Long, val index: JSONObject)
    private val loaded = HashMap<String, Loaded>()

    private fun dir(context: Context) = File(context.cacheDir, "slot-art").apply { mkdirs() }

    /** A dump's entries: null when the set doesn't know it (ask elsewhere),
     *  empty when it knows the dump has no art. */
    @Synchronized
    fun entries(context: Context, platform: String, crc: String): List<Entry>? {
        val index = index(context, platform) ?: return null
        val list = index.optJSONArray(crc.uppercase()) ?: return null
        return (0 until list.length()).mapNotNull { i ->
            val e = list.optJSONArray(i) ?: return@mapNotNull null
            val file = e.optString(2)
            if (e.length() < 3 || !filePath.matches(file)) null
            else Entry(e.optString(0), e.optString(1), file)
        }
    }

    private fun index(context: Context, platform: String): JSONObject? {
        if (platform !in platforms) return null
        val now = System.currentTimeMillis()
        loaded[platform]?.takeIf { now - it.at < INDEX_FRESH_MS }?.let { return it.index }
        val cache = File(dir(context), "$platform.json")
        fun cached(): JSONObject? = try {
            if (cache.isFile && cache.length() in 2L..MAX_INDEX.toLong()) JSONObject(cache.readText()) else null
        } catch (_: Exception) { null }
        if (cache.isFile && now - cache.lastModified() < INDEX_FRESH_MS) {
            cached()?.let { loaded[platform] = Loaded(cache.lastModified(), it); return it }
        }
        return try {
            val bytes = get("$platform.json", MAX_INDEX) ?: return cached()
            val json = JSONObject(String(bytes, Charsets.UTF_8))
            write(cache, bytes)
            loaded[platform] = Loaded(now, json)
            json
        } catch (error: Exception) {
            Log.w(TAG, "Art set index unavailable: $platform", error)
            // A stale index still names files that exist: they never change.
            cached()?.also { loaded[platform] = Loaded(now, it) }
        }
    }

    /** The names Cart Studio searches for a platform, written for slot. as
     *  ArtNames/<PLATFORM>.tsv (CRC, tab, name), refreshed every few hours. */
    fun writeNames(context: Context, platform: String) {
        if (platform !in platforms) return
        val out = File(File(context.filesDir, "ArtNames").apply { mkdirs() }, "$platform.tsv")
        if (out.isFile && System.currentTimeMillis() - out.lastModified() < INDEX_FRESH_MS) return
        try {
            val bytes = get("$platform.names.json", MAX_INDEX) ?: return
            val json = JSONObject(String(bytes, Charsets.UTF_8))
            val text = StringBuilder()
            for (crc in json.keys()) {
                val name = json.optString(crc).replace('\t', ' ').replace('\n', ' ')
                if (name.isNotBlank()) text.append(crc).append('\t').append(name).append('\n')
            }
            write(out, text.toString().toByteArray(Charsets.UTF_8))
        } catch (error: Exception) {
            Log.w(TAG, "Art set names unavailable: $platform", error)
        }
    }

    /** A file of the set, as served (PNG, WebP or JPEG), or null. */
    fun file(context: Context, entry: Entry): ByteArray? {
        if (!filePath.matches(entry.file)) return null
        val cache = File(dir(context), entry.file.substringAfterLast('/'))
        if (cache.isFile && cache.length() in 8L..MAX_FILE.toLong()) {
            try { return cache.readBytes() } catch (_: Exception) {}
        }
        val bytes = get(entry.file, MAX_FILE) ?: return null
        try { write(cache, bytes) } catch (error: Exception) { Log.w(TAG, "Art cache write failed", error) }
        return bytes
    }

    /** The label scan of a Game Boy family dump, as PNG, or null. */
    fun scanPng(context: Context, platform: String, crc: String): ByteArray? {
        val scan = entries(context, platform, crc)
            ?.firstOrNull { it.type == "support-texture" } ?: return null
        return file(context, scan)?.let(::toPng)
    }

    /** Any image the set serves as PNG, the format slot.'s label code reads. */
    fun toPng(bytes: ByteArray): ByteArray? {
        if (bytes.size >= 8 && bytes[0] == 0x89.toByte() && bytes[1] == 'P'.code.toByte()) return bytes
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
        if (bounds.outWidth !in 1..4096 || bounds.outHeight !in 1..4096) return null
        val bitmap = BitmapFactory.decodeByteArray(bytes, 0, bytes.size,
            BitmapFactory.Options().apply { inPreferredConfig = Bitmap.Config.ARGB_8888 }) ?: return null
        return try {
            val out = ByteArrayOutputStream()
            if (bitmap.compress(Bitmap.CompressFormat.PNG, 100, out)) out.toByteArray() else null
        } finally {
            bitmap.recycle()
        }
    }

    /** null for a 404: the set doesn't have it. */
    private fun get(path: String, limit: Int): ByteArray? {
        val connection = (URL(BASE + path).openConnection() as HttpURLConnection).apply {
            connectTimeout = 9_000
            readTimeout = 12_000
            instanceFollowRedirects = false
            setRequestProperty("User-Agent", "slot. for Android")
        }
        try {
            val status = connection.responseCode
            if (status == 404) return null
            check(status == 200) { "Art set returned HTTP $status for $path" }
            val out = ByteArrayOutputStream()
            connection.inputStream.use { input ->
                val buffer = ByteArray(32 * 1024)
                while (true) {
                    val n = input.read(buffer)
                    if (n < 0) break
                    require(out.size() + n <= limit) { "Art set response too large" }
                    out.write(buffer, 0, n)
                }
            }
            return out.toByteArray().takeIf { it.isNotEmpty() }
        } finally {
            connection.disconnect()
        }
    }

    private fun write(file: File, bytes: ByteArray) {
        val atomic = AtomicFile(file)
        val out = atomic.startWrite()
        try {
            out.write(bytes)
            atomic.finishWrite(out)
        } catch (error: Exception) {
            atomic.failWrite(out)
            throw error
        }
    }

}
