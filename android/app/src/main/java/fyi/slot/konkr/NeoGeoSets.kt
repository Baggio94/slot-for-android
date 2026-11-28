package fyi.slot.konkr

import android.content.Context
import java.util.Locale

/** The Neo Geo sets FinalBurn Neo plays (assets/neogeo-sets.tsv, from its
 *  Neo Geo dat: tools/neogeo-sets/make_sets.py): a ZIP named after one in
 *  an arcade folder (arcade, fbneo, fba, mame) is a Neo Geo cart, and its
 *  shelf name is the dat's title (Siham 2026-11-28). */
internal object NeoGeoSets {
    internal class Entry(val parent: String, val title: String)

    @Volatile private var sets: Map<String, Entry>? = null

    internal fun parse(lines: Sequence<String>): Map<String, Entry> {
        val out = HashMap<String, Entry>(1024)
        for (line in lines) {
            val parts = line.split('\t')
            if (parts.size != 3 || parts[0].isBlank() || parts[2].isBlank()) continue
            out[parts[0].trim().lowercase(Locale.ROOT)] = Entry(parts[1].trim().lowercase(Locale.ROOT), parts[2].trim())
        }
        return out
    }

    private fun all(context: Context): Map<String, Entry> = sets ?: synchronized(this) {
        sets ?: try {
            context.assets.open("neogeo-sets.tsv").bufferedReader().useLines { parse(it) }
        } catch (_: Exception) { emptyMap() }.also { sets = it }
    }

    /** The set a file names: its name without the extension, lower case. */
    internal fun setOf(filename: String): String =
        filename.substringBeforeLast('.').trim().lowercase(Locale.ROOT)

    fun isSet(context: Context, filename: String): Boolean = setOf(filename) in all(context)

    /** The shelf name of a Neo Geo set's file, or "" when the dat has none. */
    fun title(context: Context, filename: String): String =
        all(context)[setOf(filename)]?.title?.let(::shelfTitle).orEmpty()

    /** "Samurai Shodown / Samurai Spirits (NGM-045)": its first name. */
    internal fun shelfTitle(title: String): String =
        title.substringBefore(" / ").trim().take(200)
}
