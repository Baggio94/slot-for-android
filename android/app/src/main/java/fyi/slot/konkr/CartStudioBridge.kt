package fyi.slot.konkr

import android.content.Context
import java.io.File

/**
 * Where Cart Studio keeps a cart's shell: Studio/<key>.shell, keyed by the
 * ROM's URI. The native Cart Studio writes it; the shelf reads it back here.
 */
internal object CartStudioBridge {
    fun key(uri: String): String = GameKeys.key(uri)

    private fun profile(context: Context, uri: String): File =
        GameKeys.file(File(context.filesDir, "Studio"), uri, ".shell")

    fun savedShell(context: Context, uri: String): String? {
        val file = profile(context, uri)
        if (!file.isFile || file.length() !in 1..100) return null
        val raw = file.readText(Charsets.UTF_8).trim()
        return raw.takeIf(::validShell)
    }

    private fun validShell(value: String): Boolean =
        Regex("(auto|notched|rounded|nes|famicom|famicom-namco|famicom-konami|snes|snes-us|" +
            "megadrive|megadrive-jp|megadrive-ea|megadrive-accolade|megadrive-codemasters|megadrive-vr|megadrive-sk|" +
            "mastersystem|mastersystem-jp|mastersystem-card|gamegear|gamegear-codemasters|" +
            "ngp|wonderswan|wonderswan-color|lynx|pokemini) [0-9a-fA-F]{6} (solid|clear|glitter|solid-glitter|clear-glitter)")
            .matches(value)
}
