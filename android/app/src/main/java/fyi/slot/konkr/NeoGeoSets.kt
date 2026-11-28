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

    private val TRAILING_SET = Regex("""\(([a-z0-9_]+)\)\s*$""")

    /** The set a file names: its name without the extension, lower case;
     *  or, when that is no set, the known set it ends with in brackets:
     *  "Stakes Winner 2 (stakwin2).neo" (Siham 2026-11-28). */
    internal fun setIn(sets: Map<String, Entry>, filename: String): String {
        val stem = filename.substringAfterLast('/').substringBeforeLast('.').trim().lowercase(Locale.ROOT)
        if (stem in sets) return stem
        val named = TRAILING_SET.find(stem) ?: return stem
        val set = named.groupValues[1]
        if (set in sets) return set
        // A set name the dat doesn't know ("Tecmo World Soccer '96
        // (tws96).neo"): the game whose title the name before it has.
        return byTitle(sets, stem.substring(0, named.range.first)) ?: stem
    }

    private val BRACKETS = Regex("""\([^)]*\)|\[[^\]]*\]""")

    /** A title to compare: its first name, no brackets, ", The" first,
     *  letters and digits only. */
    internal fun titleKey(title: String): String {
        var t = title.substringBefore(" / ").replace(BRACKETS, " ").trim().lowercase(Locale.ROOT)
        if (t.endsWith(", the")) t = "the " + t.removeSuffix(", the")
        return t.filter { it.isLetterOrDigit() }
    }

    /** The set whose dat title is `title`, a parent before its clones. */
    internal fun byTitle(sets: Map<String, Entry>, title: String): String? {
        val key = titleKey(title)
        if (key.length < 3) return null
        val found = sets.filter { titleKey(it.value.title) == key }
        return found.entries.sortedWith(compareBy({ it.value.parent.isNotBlank() }, { it.key }))
            .firstOrNull()?.key
    }

    /** The sets made from `set` (its clones), parents' labels' last resort. */
    fun clonesOf(context: Context, set: String): List<String> =
        all(context).filter { it.value.parent == set }.keys.sorted()

    fun setOf(context: Context, filename: String): String = setIn(all(context), filename)

    fun isSet(context: Context, filename: String): Boolean = setOf(context, filename) in all(context)

    /** The set's parent in the dat (a clone's original), or null. */
    fun parentOf(context: Context, set: String): String? =
        all(context)[set]?.parent?.takeIf { it.isNotBlank() && it != set }

    /** The shelf name of a Neo Geo set's file, or "" when the dat has none. */
    fun title(context: Context, filename: String): String =
        all(context)[setOf(context, filename)]?.title?.let(::shelfTitle).orEmpty()

    /** "Samurai Shodown / Samurai Spirits (NGM-045)": its first name. */
    internal fun shelfTitle(title: String): String =
        title.substringBefore(" / ").trim().take(200)
}
