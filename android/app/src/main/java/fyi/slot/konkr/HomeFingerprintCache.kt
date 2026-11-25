package fyi.slot.konkr

import android.content.Context
import android.util.AtomicFile
import org.json.JSONObject
import java.io.File

/** Local cache of full and headerless CRCs. Unreliable SAF metadata is never cached. */
internal object HomeFingerprintCache {
    private const val FILE = "home-crc-cache-v1.json"
    private val hex = Regex("[0-9A-F]{8}")
    private var path: String? = null
    private var records = JSONObject()
    private var dirty = 0

    private fun load(context: Context) {
        val file = File(context.filesDir, FILE)
        if (path == file.absolutePath) return
        records = try {
            if (file.length() in 1..8_000_000L) JSONObject(file.readText()) else JSONObject()
        } catch (_: Exception) { JSONObject() }
        path = file.absolutePath
        dirty = 0
    }

    fun get(context: Context, uri: String, size: Long, modified: Long,
            read: () -> Pair<String, String?>): Pair<String, String?> {
        if (size <= 0 || modified <= 0) return read()
        synchronized(this) {
            load(context)
            val old = records.optJSONObject(uri)
            if (old != null && old.optLong("size") == size && old.optLong("modified") == modified) {
                val whole = old.optString("whole")
                val bare = old.optString("bare").takeIf { it.isNotEmpty() }
                if (hex.matches(whole) && (bare == null || hex.matches(bare))) return whole to bare
            }
        }
        val result = read()
        synchronized(this) {
            load(context)
            if (records.length() >= 12_000 && !records.has(uri)) records = JSONObject()
            records.put(uri, JSONObject().put("size", size).put("modified", modified)
                .put("whole", result.first).put("bare", result.second.orEmpty()))
            dirty++
            if (dirty >= 32) try { flush(context) } catch (_: Exception) { /* next pass retries */ }
        }
        return result
    }

    @Synchronized fun flush(context: Context) {
        load(context)
        if (dirty == 0) return
        val file = AtomicFile(File(context.filesDir, FILE))
        val out = file.startWrite()
        try {
            out.write(records.toString().toByteArray(Charsets.UTF_8))
            file.finishWrite(out)
            dirty = 0
        } catch (error: Exception) { file.failWrite(out); throw error }
    }
}
