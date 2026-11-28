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
    /** The hack labels' indexes (HackLabels), read like a platform's. */
    private const val HACKS = "HACKS"
    private const val HACK_NAMES = "HACKS.names"
    private const val HACK_PLATFORMS = "HACKS.platforms"
    private val hackIndexes = listOf(HACKS, HACK_NAMES, HACK_PLATFORMS)
    // Every shelf with a label base: PICO-8's labels are its carts' own
    // pictures, so the art set has no PICO8 index to ask for.
    private val platforms = Systems.PLATFORMS.filter { it != Systems.PICO8 }
        .flatMap { Systems.artKeys(it) }.toSet() + hackIndexes

    class Entry(val type: String, val region: String, val file: String) {
        val url: String get() = BASE + file
    }

    private class Loaded(val at: Long, val index: JSONObject)
    private val loaded = java.util.concurrent.ConcurrentHashMap<String, Loaded>()
    private val failedIndexes = java.util.concurrent.ConcurrentHashMap<String, Long>()
    private val failedNames = java.util.concurrent.ConcurrentHashMap<String, Long>()
    private val indexLocks = java.util.concurrent.ConcurrentHashMap<String, Any>()
    private val namesLocks = java.util.concurrent.ConcurrentHashMap<String, Any>()
    private val refreshWorkers = java.util.concurrent.Executors.newFixedThreadPool(4) { task ->
        Thread(task, "slot-art-catalogue").apply { isDaemon = true }
    }
    private const val FAILURE_BACKOFF_MS = 60_000L

    private fun dir(context: Context) = File(context.cacheDir, "slot-art").apply { mkdirs() }

    /** A dump's entries: null when the set doesn't know it (ask elsewhere),
     *  empty when it knows the dump has no art. */
    @Synchronized
    fun entries(context: Context, platform: String, crc: String): List<Entry>? {
        val list = Systems.artKeys(platform).firstNotNullOfOrNull { key ->
            // The Neo Geo's are keyed by set name, in lower case.
            index(context, key)?.let { it.optJSONArray(crc.uppercase()) ?: it.optJSONArray(crc.lowercase()) }
        } ?: return null
        return (0 until list.length()).mapNotNull { i ->
            val e = list.optJSONArray(i) ?: return@mapNotNull null
            val file = e.optString(2)
            if (e.length() < 3 || !filePath.matches(file)) null
            else Entry(e.optString(0), e.optString(1), file)
        }
    }

    /** The label of the ROM hack a Game Boy family file names (HackLabels),
     *  with the hack's key, or null. */
    @Synchronized
    fun hackLabel(context: Context, platform: String, fileName: String): Pair<String, Entry>? {
        if (platform !in setOf(Systems.GB, Systems.GBC, Systems.GBA) || fileName.isBlank()) return null
        val names = index(context, HACK_NAMES) ?: return null
        val systems = index(context, HACK_PLATFORMS) ?: return null
        val titles = HashMap<String, String>()
        for (key in names.keys()) {
            if (HackLabels.fits(platform, systems.optString(key))) titles[key] = names.optString(key)
        }
        val key = HackLabels.keyOf(fileName, titles) ?: return null
        val list = index(context, HACKS)?.optJSONArray(key) ?: return null
        val e = list.optJSONArray(0) ?: return null
        val file = e.optString(2)
        if (e.length() < 3 || !filePath.matches(file)) return null
        return key to Entry(e.optString(0), e.optString(1), file)
    }

    /** A Neo Geo set's own name, else the set it is a clone of as FinalBurn
     *  Neo names clones (the parent's name and a letter or two: mslugh is
     *  mslug's), as the set knows it: the longest such name it lists. */
    @Synchronized
    fun neoGeoSet(context: Context, stem: String): String? {
        val name = stem.lowercase(java.util.Locale.ROOT)
        if (entries(context, Systems.NEOGEO, name) != null) return name
        return parentSet(name) { entries(context, Systems.NEOGEO, it) != null }
    }

    internal fun parentSet(name: String, known: (String) -> Boolean): String? {
        for (end in name.length - 1 downTo maxOf(3, name.length - 3)) {
            val parent = name.substring(0, end)
            if (known(parent)) return parent
        }
        return null
    }

    /** Whether the set's index for a platform could be read (fresh or
     *  cached): a dump it doesn't list then really isn't in the set. */
    @Synchronized
    fun known(context: Context, platform: String): Boolean =
        Systems.artKeys(platform).any { index(context, it) != null }

    /** Indexes and names older than [maxAgeMs] fetched again (RomM's page,
     *  so labels published since show without waiting hours). */
    @Synchronized
    fun refresh(context: Context, maxAgeMs: Long) {
        for (key in platforms) index(context, key, maxAgeMs)
        for (platform in Systems.PLATFORMS) writeNames(context, platform, maxAgeMs)
    }

    /** The explicit refresh is scoped to systems present on the shelf. */
    fun refreshLibrary(context: Context, shelves: Set<String>, shouldContinue: () -> Boolean): Boolean {
        val relevant = shelves.toMutableSet()
        if (Systems.GB in relevant || Systems.GBC in relevant) {
            relevant.add(Systems.GB)
            relevant.add(Systems.GBC)
        }
        fun batch(keys: List<String>, fetch: (String) -> Unit): Boolean {
            for (chunk in keys.chunked(4)) {
                if (!shouldContinue()) return false
                val jobs = chunk.map { key -> refreshWorkers.submit { if (shouldContinue()) fetch(key) } }
                for (job in jobs) job.get()
            }
            return shouldContinue()
        }
        val keys = relevant.flatMap { Systems.artKeys(it) } +
            if (relevant.any { it in setOf(Systems.GB, Systems.GBC, Systems.GBA) }) hackIndexes else emptyList()
        if (!batch(keys.distinct()) { index(context, it, 0L) }) return false
        return batch(relevant.toList()) { writeNames(context, it, 0L) }
    }

    @Synchronized
    fun hasFreshIndexes(context: Context, shelves: List<String>): Boolean {
        val now = System.currentTimeMillis()
        return shelves.none { failedNames.containsKey(it) } && shelves.flatMap { Systems.artKeys(it) }
                .filter { it in platforms }.distinct().all { key ->
            !failedIndexes.containsKey(key) && loaded[key]?.let { now - it.at in 0 until INDEX_FRESH_MS } == true
        }
    }

    private fun index(context: Context, platform: String, fresh: Long = INDEX_FRESH_MS): JSONObject? =
        synchronized(indexLocks.computeIfAbsent(platform) { Any() }) { readIndex(context, platform, fresh) }

    private fun readIndex(context: Context, platform: String, fresh: Long): JSONObject? {
        if (platform !in platforms) return null
        val now = System.currentTimeMillis()
        if (!failedIndexes.containsKey(platform)) loaded[platform]?.takeIf { now - it.at < fresh }?.let { return it.index }
        val cache = File(dir(context), "$platform.json")
        fun cached(): JSONObject? = try {
            if (cache.isFile && cache.length() in 2L..MAX_INDEX.toLong()) JSONObject(cache.readText()) else null
        } catch (_: Exception) { null }
        if (fresh > 0 && failedIndexes[platform]?.let { now - it < FAILURE_BACKOFF_MS } == true) {
            return loaded[platform]?.index ?: cached()
        }
        if (!failedIndexes.containsKey(platform) && cache.isFile && now - cache.lastModified() < fresh) {
            cached()?.let { loaded[platform] = Loaded(cache.lastModified(), it); return it }
        }
        return try {
            val bytes = get("$platform.json", MAX_INDEX) ?: run {
                failedIndexes[platform] = now
                return cached()
            }
            val json = JSONObject(String(bytes, Charsets.UTF_8))
            write(cache, bytes)
            loaded[platform] = Loaded(now, json)
            failedIndexes.remove(platform)
            json
        } catch (error: Exception) {
            Log.w(TAG, "Art set index unavailable: $platform", error)
            // A stale index still names files that exist: they never change.
            failedIndexes[platform] = now
            loaded[platform]?.index ?: cached()?.also { loaded[platform] = Loaded(cache.lastModified(), it) }
        }
    }

    /** The names Cart Studio searches for a platform, written for slot. as
     *  ArtNames/<PLATFORM>.tsv (CRC, tab, name), refreshed every few hours. */
    fun writeNames(context: Context, platform: String, fresh: Long = INDEX_FRESH_MS) =
        synchronized(namesLocks.computeIfAbsent(platform) { Any() }) { refreshNames(context, platform, fresh) }

    private fun refreshNames(context: Context, platform: String, fresh: Long) {
        if (platform !in platforms) return
        val out = File(File(context.filesDir, "ArtNames").apply { mkdirs() }, "$platform.tsv")
        val now = System.currentTimeMillis()
        if (fresh > 0 && failedNames[platform]?.let { now - it < FAILURE_BACKOFF_MS } == true) return
        if (!failedNames.containsKey(platform) && out.isFile && now - out.lastModified() < fresh) return
        try {
            // A shelf's names: its own index's, then its Color carts'.
            val text = StringBuilder()
            for (key in Systems.artKeys(platform)) {
                val bytes = get("$key.names.json", MAX_INDEX) ?: run {
                    failedNames[platform] = now
                    return // Keep the previous complete name catalogue.
                }
                val json = JSONObject(String(bytes, Charsets.UTF_8))
                for (crc in json.keys()) {
                    val name = json.optString(crc).replace('\t', ' ').replace('\n', ' ')
                    if (name.isNotBlank()) text.append(crc).append('\t').append(name).append('\n')
                }
            }
            val bytes = text.toString().toByteArray(Charsets.UTF_8)
            if (!out.isFile || !out.readBytes().contentEquals(bytes)) {
                write(out, bytes)
                PortableArtwork.noteChange()
            } else out.setLastModified(System.currentTimeMillis())
            failedNames.remove(platform)
        } catch (error: Exception) {
            failedNames[platform] = now
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

    /** Whether the set holds a label for a dump: the cart scan for the
     *  Game Boy family, a scan or front render for the home systems. */
    @Synchronized
    fun labelled(context: Context, platform: String, crc: String): Boolean {
        val types = entries(context, platform, crc)?.map { it.type } ?: return false
        return "support-texture" in types ||
            (platform in Systems.HOME_PLATFORMS && "support-2D" in types)
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
            if (path.endsWith(".json")) {
                useCaches = false
                setRequestProperty("Cache-Control", "no-cache")
            }
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
