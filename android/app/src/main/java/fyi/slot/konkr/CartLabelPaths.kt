package fyi.slot.konkr

import java.io.File

/**
 * A cart's label: Labels/<key>.png, keyed like slot-android's
 * core_selection::key by the game's portable name (GameKeys, slot. 2.5).
 */
internal object CartLabelPaths {
    fun file(filesDir: File, romUri: String): File =
        GameKeys.file(File(filesDir, "Labels"), romUri, ".png")
}
