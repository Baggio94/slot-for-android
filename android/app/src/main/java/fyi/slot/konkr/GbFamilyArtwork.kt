package fyi.slot.konkr

/** Artwork identity only. Never changes the ROM's platform, URI or game key. */
internal object GbFamilyArtwork {
    fun other(platform: String): String? = when (platform) {
        "GB" -> "GBC"
        "GBC" -> "GB"
        else -> null
    }

    private val colourSuffix = Regex(
        """(?:\s*[-:–]\s*|\s+)(?:dx|colou?r(?:ization|isation|ized|ised)?)(?:\s+v\d+(?:\.\d+)*)?$""",
        RegexOption.IGNORE_CASE)

    /** Keep region/revision tags. Only remove an explicit colour-hack suffix;
     *  never truncate a numbered sequel to a shorter game's name. */
    fun originalTitle(title: String): String? {
        val base = title.substringBefore(" (").trim()
        val suffix = colourSuffix.find(base) ?: return null
        val original = base.substring(0, suffix.range.first).trim()
        if (original.isEmpty()) return null
        val canonical = when (OfficialArtMatcher.titleKey(original)) {
            "super mario land 2" -> "Super Mario Land 2 - 6 Golden Coins"
            "metroid ii return of samus", "metroid 2 return of samus" -> "Metroid II - Return of Samus"
            else -> original
        }
        return canonical + title.substring(base.length)
    }

    fun names(platform: String, title: String): List<Pair<String, String>> {
        val alternate = other(platform) ?: return listOf(platform to title)
        val original = originalTitle(title)
        // Exact title on its own system always wins; a colour hack can then
        // wear its original physical GB label on the GBC shell.
        return buildList {
            add(platform to title)
            add(alternate to title)
            if (original != null) {
                add("GB" to original)
                add("GBC" to original)
            }
        }.distinct()
    }

    /** One new lookup for old negative records, without disturbing GBA. */
    fun missKind(platform: String): String =
        if (other(platform) == null) "gb-miss" else "gb-miss-v2"
}
