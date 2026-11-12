package fyi.slot.konkr

import android.content.Context
import java.io.File
import java.util.Locale

/**
 * A cart's label in slot. for Android's own art set found by its name, when
 * its CRC isn't there (a hack, a translation, another region's dump, a RomM
 * game): the carousel and RomM alike (Siham 2026-11-10). In order:
 *  1. the same name as a dump of the set;
 *  2. the name matched as Cart Studio's matcher does (title, region,
 *     revision, version: Apotris);
 *  3. the same, a missing region settled USA, World, Europe, Japan;
 *  4. a hack named by its game's usual short name (SMW - …, SMB3 - …);
 *  5. a hack whose name starts with a game's (two words at least):
 *     "Castlevania - Aria of Sorrow Reprise" wears Aria of Sorrow's label;
 *  6. last, the same game's label from another region.
 * Only dumps the set has a label for are matched.
 */
internal object ArtNames {
    private const val FRESH_MS = 6L * 60 * 60 * 1000

    private class Names(
        val at: Long,
        val byName: Map<String, String>,
        val catalog: OfficialArtMatcher.Catalog,
        /** Title keys of two words or more, longest first, for prefixes. */
        val keys: List<String>,
    )

    private val loaded = HashMap<String, Names>()
    /** When a platform's names last failed to load: not asked again for a
     *  few minutes, so a pass offline doesn't wait on each cart. */
    private val failed = HashMap<String, Long>()

    /** Hacks' usual short names for their game (No-Intro's name for it). */
    internal val SHORT = mapOf(
        "smw" to "Super Mario World",
        "smw2" to "Super Mario World 2 - Yoshi's Island",
        "smb" to "Super Mario Bros.",
        "smb1" to "Super Mario Bros.",
        "smb2" to "Super Mario Bros. 2",
        "smb3" to "Super Mario Bros. 3",
        "smk" to "Super Mario Kart",
        "sm" to "Super Metroid",
        "mm" to "Mega Man",
        "mm1" to "Mega Man",
        "mm2" to "Mega Man 2",
        "mm3" to "Mega Man 3",
        "mm4" to "Mega Man 4",
        "mm5" to "Mega Man 5",
        "mm6" to "Mega Man 6",
        "loz" to "Legend of Zelda, The",
        "loz2" to "Zelda II - The Adventure of Link",
        "lozalttp" to "Legend of Zelda, The - A Link to the Past",
        "alttp" to "Legend of Zelda, The - A Link to the Past",
        "ffiv" to "Final Fantasy II",
        "ffvi" to "Final Fantasy III",
        "dkc" to "Donkey Kong Country",
        "dkc2" to "Donkey Kong Country 2 - Diddy's Kong Quest",
        "dkc3" to "Donkey Kong Country 3 - Dixie Kong's Double Trouble!",
        "kss" to "Kirby Super Star",
        "sonic 1" to "Sonic The Hedgehog",
        "sonic 2" to "Sonic The Hedgehog 2",
        "sonic 3" to "Sonic The Hedgehog 3",
        "s3k" to "Sonic & Knuckles",
        "sor" to "Streets of Rage",
        "sor2" to "Streets of Rage 2",
        "sor3" to "Streets of Rage 3",
        "fe8" to "Fire Emblem - The Sacred Stones",
        "fe7" to "Fire Emblem",
        "mzm" to "Metroid - Zero Mission",
    )

    /** A file name without its extension (a title stays as it is). */
    internal fun stem(name: String): String {
        val dot = name.lastIndexOf('.')
        if (dot <= 0) return name
        val ext = name.substring(dot + 1)
        return if (ext.length in 1..4 && ext.all { it.isLetterOrDigit() }) name.substring(0, dot) else name
    }

    /** Read again next time (the set's names were just fetched anew). */
    @Synchronized
    fun forget() {
        loaded.clear()
        failed.clear()
    }

    @Synchronized
    private fun names(context: Context, platform: String): Names? {
        val now = System.currentTimeMillis()
        loaded[platform]?.takeIf { now - it.at < FRESH_MS }?.let { return it }
        if (now - (failed[platform] ?: 0L) < 5 * 60_000L) return loaded[platform]
        SlotArtSet.writeNames(context, platform)
        val tsv = File(context.filesDir, "ArtNames/$platform.tsv")
        val all = if (!tsv.isFile) emptyMap() else try {
            tsv.readLines().mapNotNull { line ->
                if (line.indexOf('\t') != 8) null else line.substring(0, 8).uppercase(Locale.ROOT) to line.substring(9)
            }.toMap()
        } catch (_: Exception) { emptyMap() }
        val labelled = all.filterKeys { crc ->
            try { SlotArtSet.labelled(context, platform, crc) } catch (_: Exception) { false }
        }
        // Nothing loaded (no network yet): asked again next time.
        if (labelled.isEmpty()) {
            failed[platform] = now
            return loaded[platform]
        }
        val catalog = OfficialArtMatcher.Catalog.fromNames(platform, labelled)
        val names = Names(now,
            labelled.entries.associate { it.value.lowercase(Locale.ROOT) to it.key },
            catalog,
            catalog.entries.map { it.titleKey }.filter { it.split(' ').size >= 2 }
                .distinct().sortedByDescending { it.length })
        loaded[platform] = names
        return names
    }

    /** The CRC of the set's dump whose label this cart wears, or null. */
    fun crcFor(context: Context, platform: String, name: String): String? {
        val set = names(context, platform) ?: return null
        return match(platform, stem(name), set.byName, set.catalog, set.keys)
    }

    internal fun match(platform: String, title: String, byName: Map<String, String>,
                       catalog: OfficialArtMatcher.Catalog, keys: List<String>): String? {
        byName[title.lowercase(Locale.ROOT)]?.let { return it }
        OfficialArtMatcher.inArtSet(platform, title, catalog, relaxed = false)?.let { return it }
        OfficialArtMatcher.inArtSet(platform, title, catalog, relaxed = true)?.let { return it }
        // The parentheses (region, revision) are kept for the game's name.
        val tags = title.indexOf(" (").let { if (it < 0) "" else title.substring(it) }
        val base = title.substringBefore(" (")
        val short = base.substringBefore(" - ", "").trim().lowercase(Locale.ROOT)
        SHORT[short]?.let { game ->
            OfficialArtMatcher.inArtSet(platform, game + tags, catalog, relaxed = true)?.let { return it }
            OfficialArtMatcher.inArtSet(platform, game, catalog, relaxed = true)?.let { return it }
        }
        val key = OfficialArtMatcher.titleKey(title)
        keys.firstOrNull { key.startsWith("$it ") }?.let { game ->
            OfficialArtMatcher.inArtSet(platform, game + tags, catalog, relaxed = true)?.let { return it }
            OfficialArtMatcher.inArtSet(platform, game, catalog, relaxed = true)?.let { return it }
        }
        return OfficialArtMatcher.inArtSet(platform, title, catalog, relaxed = true, anyRegion = true)
    }
}
