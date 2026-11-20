package fyi.slot.konkr

import java.util.Locale

/** The pictures of the Android windows' titles, drawn by slot.'s own text
 *  (crates/slot-ui/examples/export_apps_legend.rs) so they match the native
 *  pages: the first ones by their old names, every other one after its words. */
internal object Headings {
    fun asset(title: String): String = when (title.uppercase(Locale.ROOT)) {
        "APPS" -> "heading-apps.png"
        "RETROACHIEVEMENTS" -> "heading-ra.png"
        "LINK — SHARE CARTRIDGE" -> "heading-link-consent.png"
        "MULTIPLAYER — DIRECT JOIN" -> "heading-direct-join.png"
        else -> "heading-" + title.lowercase(Locale.ROOT)
            .replace(Regex("[^a-z0-9]+"), "-").trim('-') + ".png"
    }
}
