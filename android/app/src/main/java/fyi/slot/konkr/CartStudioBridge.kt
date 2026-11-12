package fyi.slot.konkr

import android.content.Context
import java.io.File

/**
 * Where Cart Studio keeps a cart's shell: Studio/<key>.shell, keyed by the
 * ROM's URI. The native Cart Studio writes it; the shelf reads it back here.
 */
internal object CartStudioBridge {
    fun key(uri: String): String {
        var hash = -3750763034362895579L
        for (byte in uri.toByteArray(Charsets.UTF_8)) {
            hash = (hash xor (byte.toInt() and 0xff).toLong()) * 0x100000001b3L
        }
        return java.lang.Long.toUnsignedString(hash, 16).padStart(16, '0')
    }

    private fun profile(context: Context, uri: String): File =
        File(File(context.filesDir, "Studio"), key(uri) + ".shell")

    fun savedShell(context: Context, uri: String): String? {
        val file = profile(context, uri)
        if (!file.isFile || file.length() !in 1..100) return null
        val raw = file.readText(Charsets.UTF_8).trim()
        return raw.takeIf(::validShell)
    }

    private fun validShell(value: String): Boolean =
        Regex("(auto|notched|rounded|nes|famicom|famicom-namco|famicom-konami|snes|snes-us|" +
            "megadrive|megadrive-jp|megadrive-ea|megadrive-accolade|megadrive-codemasters|megadrive-vr|megadrive-sk) [0-9a-fA-F]{6} (solid|clear|glitter|solid-glitter|clear-glitter)")
            .matches(value)
}
