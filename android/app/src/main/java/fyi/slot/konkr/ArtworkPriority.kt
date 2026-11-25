package fyi.slot.konkr

import android.content.Context
import org.json.JSONObject
import java.io.File
import java.util.Locale
import kotlin.math.abs

/** Reuse the native shelf's atomically persisted focus, off the render thread. */
internal object ArtworkPriority {
    fun focus(context: Context): String? = try {
        val file = File(context.filesDir, "konkr-shelf-focus.json")
        if (!file.isFile || file.length() !in 1L..16384L) null
        else JSONObject(file.readText()).optString("uri").takeIf { it.isNotBlank() }
    } catch (_: Exception) { null }

    /** Selected cart first, then nearest shelf neighbours in title order.
     * Other shelves keep their relative order; no matching decisions here. */
    fun <T> order(items: List<T>, focus: String?, uri: (T) -> String,
                  platform: (T) -> String, title: (T) -> String): List<T> {
        if (items.size < 2) return items
        val selected = items.firstOrNull { uri(it) == focus }
        val shelf = selected?.let(platform)
            ?: Systems.PLATFORMS.firstOrNull { key -> items.any { platform(it) == key } }
            ?: return items
        val same = items.filter { platform(it) == shelf }
            .sortedWith(compareBy<T> { title(it).lowercase(Locale.ROOT) }.thenBy { uri(it) })
        val at = if (selected == null) 0 else same.indexOfFirst { uri(it) == uri(selected) }.coerceAtLeast(0)
        val near = same.withIndex().sortedWith(compareBy<IndexedValue<T>> { abs(it.index - at) }
            .thenBy { it.index }).map { it.value }
        return near + items.filter { platform(it) != shelf }
    }
}
