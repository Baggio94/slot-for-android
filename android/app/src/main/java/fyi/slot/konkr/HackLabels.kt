package fyi.slot.konkr

import java.text.Normalizer
import java.util.Locale

/**
 * ROM hack labels (tools/home-art/import_hacks.py): a hack has no fixed
 * CRC32, so the art set lists its labels by name in HACKS.json, keyed by a
 * few lower-case words ("radical red"), with HACKS.names.json (key ->
 * title) and HACKS.platforms.json (key -> GB, GBC or GBA). A Game Boy
 * family cart whose CRC32 has no label wears the label of the hack its
 * file name names: the same rule as import_hacks.key_of (3.2.0).
 */
internal object HackLabels {
    /** Words a hack's file name carries that never tell one hack from another. */
    private val NOISE = setOf("pokemon", "version", "hack", "rom", "by", "the", "gba", "gbc", "gb", "usa",
        "europe", "japan", "world", "en", "final", "beta", "patched", "patch")
    private val EXTENSION = Regex("""\.(gba|gbc|gb|zip|7z|bps|ips|ups)$""")
    private val VERSION = Regex("""\bv?\d+(\.\d+)+[a-z]?\b|\bv\d+[a-z]?\b""")
    private val INITIALS = Regex("""\b([a-z])\.(?=[a-z]\b)""")
    private val FIRE_RED = Regex("""\bfire\s*red\b""")
    private val LEAF_GREEN = Regex("""\bleaf\s*green\b""")
    private val WORD = Regex("""[a-z0-9]+""")
    private val LETTERS = Regex("""[a-z]+""")

    private fun ascii(text: String): String =
        Normalizer.normalize(text, Normalizer.Form.NFKD).filter { it.code < 128 }.lowercase(Locale.ROOT)

    /** "Pokémon - Radical Red (v4.1) [Hack].gba" -> [radical, red] */
    fun words(text: String): List<String> {
        var t = ascii(text).replace(EXTENSION, "")
        t = t.replace("'", "").replace("&", " and ").replace("+", " plus ")
        t = t.replace(VERSION, " ").replace(INITIALS, "$1")
        t = t.replace(FIRE_RED, "firered").replace(LEAF_GREEN, "leafgreen")
        return WORD.findAll(t).map { it.value }.filter { it !in NOISE }.toList()
    }

    /** The hack a file is of: the longest key whose words stand together in
     *  its name. A hack whose title starts with Pokémon also needs the word
     *  in the file name, so a game merely called Peach never wears one. */
    fun keyOf(fileName: String, titles: Map<String, String>): String? {
        val have = words(fileName)
        val pokemon = LETTERS.findAll(ascii(fileName)).any { it.value == "pokemon" }
        var best: String? = null
        var bestLength = 0
        for ((key, title) in titles) {
            if (title.startsWith("Pokémon") && !pokemon) continue
            val k = key.split(' ')
            if (k.size <= bestLength) continue
            if ((0..have.size - k.size).any { i -> have.subList(i, i + k.size) == k }) {
                best = key
                bestLength = k.size
            }
        }
        return best
    }

    /** A hack for this system may go on this cart: Game Boy and Game Boy
     *  Color hacks on either, Game Boy Advance hacks on the GBA only. */
    fun fits(cartPlatform: String, hackPlatform: String): Boolean = when (cartPlatform) {
        Systems.GB, Systems.GBC -> hackPlatform == Systems.GB || hackPlatform == Systems.GBC
        else -> cartPlatform == hackPlatform
    }
}
