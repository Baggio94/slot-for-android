package fyi.slot.konkr

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract
import org.json.JSONArray
import org.json.JSONObject
import java.util.ArrayDeque
import java.util.HashSet
import java.util.Locale

/**
 * Read-only ROM discovery using the Storage Access Framework.
 * Only file names, small cartridge headers and opaque content URIs are read.
 * No commercial games, BIOS files, or writable access are requested or copied.
 */
internal object RomLibrary {
    /** Systems whose cart battery 3.0.0-rc3 learned (entries marked battery_v3). */
    private val BATTERY_V3 = setOf(Systems.WS, Systems.SMS, Systems.GG)
    /**
     * The most carts a shelf holds (library.rs keeps the same bound). 5,000
     * left a 6,000-game folder with an empty shelf (issue #122). Each cart
     * costs about 1 KB of Java heap per copy of the index, and a scan holds
     * a few copies at once: 10,000 stays far below what ran the RG Rotate
     * out of memory with RomM's pages (2026-11-19).
     */
    internal const val MAX_ROMS = 10_000
    /** Files listed before reading: room for ZIPs and .bins that are not
     *  carts (other systems' ZIPs in folders with names slot. doesn't know). */
    private const val MAX_CANDIDATES = MAX_ROMS * 3
    private const val MAX_DIRECTORIES = 5000
    /** The saved index of MAX_ROMS carts, with long non-Latin names. */
    internal const val CACHE_MAX_BYTES = 32 * 1024 * 1024
    private const val MAX_DEPTH = 12

    internal data class ScanResult(val json: String, val count: Int, val truncated: Boolean)
    /** Ignore SAF iteration order, but never hide real additions/removals. */
    internal data class IndexEntry(
        val uri: String, val title: String, val filename: String, val platform: String,
        val code: String, val colorOnly: Boolean, val size: Long, val modified: Long,
        val gbTitle: String = "", val gbCode: String = "",
        val gbJapan: Boolean = false, val gbCgbFlag: Int = -1,
        val homeTitle: String = "", val homeRegion: String = "",
        val homeBattery: Boolean = false, val homeMaker: String = ""
    )
    internal fun equivalentEntries(left: List<IndexEntry>, right: List<IndexEntry>): Boolean {
        if (left.size != right.size) return false
        val a = left.associateBy { it.uri }
        val b = right.associateBy { it.uri }
        return a.size == left.size && b.size == right.size && a == b
    }
    internal fun indexEntries(json: String): List<IndexEntry> {
        val arr = JSONArray(json)
        require(arr.length() <= MAX_ROMS) { "ROM index exceeds bound" }
        return (0 until arr.length()).map { i ->
            val obj = arr.getJSONObject(i)
            IndexEntry(obj.getString("uri"), obj.optString("title"),
                obj.optString("filename"), obj.optString("platform"),
                obj.optString("code"), obj.optBoolean("color_only"),
                obj.optLong("size", -1), obj.optLong("modified", -1),
                obj.optString("gb_title"), obj.optString("gb_code"),
                obj.optBoolean("gb_japan"), obj.optInt("gb_cgb_flag", -1),
                obj.optString("home_title"), obj.optString("home_region"),
                obj.optBoolean("home_battery"), obj.optString("home_maker"))
        }
    }
    internal fun sameIndex(before: String, after: String): Boolean =
        equivalentEntries(indexEntries(before), indexEntries(after))
    private data class Directory(val id: String, val depth: Int, val folder: String)
    /** A cart file listed in the first pass, read in the second. */
    private data class Candidate(val id: String, val name: String, val extension: String,
                                 val size: Long, val modified: Long, val dir: Directory)

    /** Every cartridge of Systems.ROM_EXTENSIONS, and ZIPs holding one. */
    internal fun isSupportedRomExtension(extension: String): Boolean =
        extension.lowercase(Locale.ROOT).let { it == "zip" || it in Systems.ROM_EXTENSIONS }

    /**
     * Re-scan the selected tree, but avoid re-opening every ROM header when
     * its exact SAF URI, display name, size and last-modified time match our
     * previously persisted library index. Unknown provider metadata still
     * forces a real header read (never trust size=0 or modified=0).
     *
     * Two passes: the folders are listed first (progress reports the carts
     * found, total 0), then each cart is read (progress reports done/total).
     */
    fun scan(context: Context, treeUri: Uri, oldGamesJson: String? = null,
             progress: ((done: Int, total: Int) -> Unit)? = null,
             shouldContinue: () -> Boolean = { true }): ScanResult {
        val started = android.os.SystemClock.elapsedRealtime()
        fun checkActive() {
            if (!shouldContinue()) throw java.util.concurrent.CancellationException("ROM scan superseded")
        }
        val previous = previousEntries(context, treeUri, oldGamesJson)
        var headersRead = 0
        var headersReused = 0
        val resolver = context.contentResolver
        val rootId = DocumentsContract.getTreeDocumentId(treeUri)
        val pending = ArrayDeque<Directory>()
        val seen = HashSet<String>()
        // Search the user-selected SAF tree, including custom layouts, but
        // skip folders that ES-DE names after systems slot. does not run.
        pending.add(Directory(rootId, 0, ""))
        android.util.Log.i("SlotKonkr", "ROM scan scope: selected tree without other systems' folders")
        val games = JSONArray()
        var directories = 0
        var truncated = false
        val columns = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
            DocumentsContract.Document.COLUMN_SIZE,
            DocumentsContract.Document.COLUMN_LAST_MODIFIED
        )

        val found = ArrayList<Candidate>()
        while (pending.isNotEmpty()) {
            checkActive()
            if (directories >= MAX_DIRECTORIES || found.size >= MAX_CANDIDATES) {
                truncated = true
                break
            }
            val dir = pending.removeFirst()
            if (!seen.add(dir.id)) continue
            directories++
            val children = DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, dir.id)
            // Null means provider unavailable, not an empty ROM folder.
            val listing = resolver.query(children, columns, null, null, null)
                ?: throw java.io.IOException("SAF folder listing unavailable")
            listing.use { cursor ->
                val idIndex = cursor.getColumnIndexOrThrow(columns[0])
                val nameIndex = cursor.getColumnIndexOrThrow(columns[1])
                val mimeIndex = cursor.getColumnIndexOrThrow(columns[2])
                // Cloud/document providers may omit these optional columns.
                val sizeIndex = cursor.getColumnIndex(columns[3])
                val modifiedIndex = cursor.getColumnIndex(columns[4])
                while (cursor.moveToNext()) {
                    val id = cursor.getString(idIndex) ?: continue
                    val name = cursor.getString(nameIndex) ?: continue
                    val mime = cursor.getString(mimeIndex) ?: continue
                    if (name.startsWith(".")) continue
                    if (mime == DocumentsContract.Document.MIME_TYPE_DIR) {
                        if (dir.depth < MAX_DEPTH && !ignoredDirectory(name)) {
                            pending.add(Directory(id, dir.depth + 1, name))
                        }
                        continue
                    }

                    val extension = name.substringAfterLast('.', "").lowercase(Locale.ROOT)
                    if (!isSupportedRomExtension(extension)) continue
                    if (found.size >= MAX_CANDIDATES) {
                        truncated = true
                        break
                    }
                    val size = if (sizeIndex >= 0 && !cursor.isNull(sizeIndex))
                        cursor.getLong(sizeIndex) else -1L
                    val modified = if (modifiedIndex >= 0 && !cursor.isNull(modifiedIndex))
                        cursor.getLong(modifiedIndex) else -1L
                    found.add(Candidate(id, name, extension, size, modified, dir))
                }
            }
            progress?.invoke(found.size, 0)
        }
        val listedAt = android.os.SystemClock.elapsedRealtime()
        // Bound both open SAF streams and in-flight results. Keep listing
        // order deterministic, even when workers finish out of order.
        val readers = java.util.concurrent.Executors.newFixedThreadPool(RomReads.headerReaders(treeUri.toString())) { task ->
            Thread(task, "slot-rom-header").apply { isDaemon = true }
        }
        try {
            var index = 0
            while (index < found.size) {
                checkActive()
                if (games.length() >= MAX_ROMS) { truncated = true; break }
                val end = minOf(found.size, index + 8)
                val jobs = (index until end).map { i ->
                    readers.submit<ReadEntry?> {
                        checkActive()
                        val (id, name, extension, size, modified, dir) = found[i]
                        val uri = DocumentsContract.buildDocumentUriUsingTree(treeUri, id)
                        readEntry(context, uri, name, extension, size, modified,
                            dir.folder, previous[uri.toString()])
                    }
                }
                for (job in jobs) {
                    checkActive()
                    if (games.length() >= MAX_ROMS) { truncated = true; break }
                    // A worker that saw the scan superseded reports it as a
                    // superseded scan, not as a failed one.
                    val read = try { job.get() } catch (error: java.util.concurrent.ExecutionException) {
                        throw (error.cause as? java.util.concurrent.CancellationException) ?: error
                    }
                    index++
                    progress?.invoke(index, found.size)
                    if (read != null) {
                        if (read.reused) headersReused++ else headersRead++
                        games.put(read.game)
                    }
                }
            }
        } finally { readers.shutdownNow() }
        val readAt = android.os.SystemClock.elapsedRealtime()
        progress?.invoke(found.size, found.size)
        // A partial read never removes a cart: the carts of the last index
        // this scan didn't reach stay, after the ones it found (issue #122:
        // a fresh install used to keep its empty shelf instead).
        var kept = 0
        if (truncated) {
            val listed = HashSet<String>(games.length() * 2)
            for (i in 0 until games.length()) listed.add(games.getJSONObject(i).optString("uri"))
            for ((uri, old) in previous) {
                if (games.length() >= MAX_ROMS) break
                if (uri in listed) continue
                games.put(old)
                kept++
            }
        }
        android.util.Log.i("SlotKonkr", "ROM scan: $directories folders, ${games.length()} carts, " +
            "headers read=$headersRead, reused=$headersReused" +
            (if (truncated) ", partial (limit reached; $kept earlier carts kept)" else ""))
        val json = games.toString()
        android.util.Log.i("SlotKonkr", "ROM scan timings: listing/cache=${listedAt-started}ms, headers=${readAt-listedAt}ms, index=${android.os.SystemClock.elapsedRealtime()-readAt}ms")
        return ScanResult(json, games.length(), truncated)
    }

    /** The last index's carts by URI, in its order. In its own function so
     *  the index's text is gone before the scan builds the new one (memory:
     *  issue #122 raised the bound). A bad cache only costs speed. */
    private fun previousEntries(context: Context, treeUri: Uri,
                                oldGamesJson: String?): LinkedHashMap<String, JSONObject> {
        val previous = LinkedHashMap<String, JSONObject>()
        val cachedJson = oldGamesJson ?: try {
            val cache = java.io.File(context.filesDir, "rom-library-cache-v1.json")
            if (cache.length() > CACHE_MAX_BYTES) null
            else JSONObject(cache.readText(Charsets.UTF_8)).let { meta ->
                if (meta.optInt("schema") == 1 &&
                    meta.optString("root") == treeUri.toString())
                    meta.optJSONArray("games")?.toString()
                else null
            }
        } catch (_: Exception) { null }
        if (cachedJson != null) {
            try {
                val cached = JSONArray(cachedJson)
                for (index in 0 until cached.length().coerceAtMost(MAX_ROMS)) {
                    val entry = cached.optJSONObject(index) ?: continue
                    val uri = entry.optString("uri")
                    if (uri.startsWith("content://")) previous[uri] = entry
                }
            } catch (_: Exception) {
                // Bad cache affects speed only; discovery still reads headers.
            }
        }
        return previous
    }

    /** One cart as the index keeps it, and whether its header came from
     *  the old index. Null when the file is not a cart slot. runs. */
    private class ReadEntry(val game: JSONObject, val reused: Boolean)

    private fun readEntry(context: Context, fileUri: Uri, name: String, extension: String,
                          size: Long, modified: Long, folder: String,
                          old: JSONObject?): ReadEntry? {
        val resolver = context.contentResolver
        // Keep the shelf title identical to the RetroArch save basename.
        val title = name.substringBeforeLast('.').trim().take(200)
        if (title.isEmpty()) return null
        val uriText = fileUri.toString()
        val reuse = old != null && canReuseHeader(
            old.optString("title"), old.optLong("size", -1),
            old.optLong("modified", -1), title, size, modified) &&
            // Old indexes lack the GB header metadata used by
            // Cart Studio. Read only 0x150 bytes once to migrate.
            // Older ones also lack whether a GB/GBA cart has a
            // battery (2026-11): read once more to learn it.
            (old.has("home_battery") && (old.optString("platform") == "GBA" ||
                old.has("gb_cgb_flag") || old.has("home_maker"))) &&
            // 3.0.0-rc3 learns the WonderSwan's, Master System's and Game
            // Gear's batteries: their older entries are read once more.
            (old.optString("platform") !in BATTERY_V3 || old.has("battery_v3")) &&
            (!Systems.isHome(if (extension == "zip") old.optString("rom_ext") else extension) ||
                old.optBoolean("header_v2"))
        // ZIP archives remain a single shelf cartridge identified
        // by the ZIP's SAF URI and filename. Resolve the embedded
        // ROM header/extension only on cache misses.
        val zipped = extension == "zip"
        val zipHeader = if (zipped && !reuse) {
            try {
                resolver.openInputStream(fileUri)?.use { input ->
                    RomSource.inspectWithBattery(input, name)
                }
            } catch (error: Exception) {
                android.util.Log.w("SlotKonkr",
                    "Skipping unsupported/corrupt cartridge ZIP: " +
                        name.take(80), error)
                null
            }
        } else null
        if (zipped && !reuse && zipHeader == null) return null
        val embeddedExtension = zipHeader?.extension
            ?: if (zipped) old?.optString("rom_ext")?.takeIf { it.isNotEmpty() }
                ?: when (old?.optString("platform")) {
                "GBA" -> "gba"
                "GBC" -> "gbc"
                else -> "gb"
            } else extension
        val gba = embeddedExtension == "gba"
        val home = Systems.isHome(embeddedExtension)
        var homeHeader = Systems.HomeHeader("", "", false)
        var code: String
        var colorOnly: Boolean
        var platform: String
        var gbTitle = ""
        var gbCode = ""
        var gbJapan = false
        var gbCgbFlag = -1
        if (reuse && old != null) {
            code = old.optString("code")
            colorOnly = old.optBoolean("color_only")
            platform = old.optString("platform")
            gbTitle = old.optString("gb_title")
            gbCode = old.optString("gb_code")
            gbJapan = old.optBoolean("gb_japan")
            gbCgbFlag = old.optInt("gb_cgb_flag", -1)
            homeHeader = Systems.HomeHeader(old.optString("home_title"),
                old.optString("home_region"), old.optBoolean("home_battery"),
                old.optString("home_maker"))
        } else {
            val header = zipHeader?.bytes
                ?: cartridgeHeader(context, fileUri, Systems.headerBytes(embeddedExtension))
            // A .bin is a Mega Drive cart only with a SEGA header.
            if (!Systems.acceptsHeader(embeddedExtension, header)) return null
            if (home) homeHeader = Systems.homeHeader(embeddedExtension, header, name)
            // The WonderSwan's save type is in the footer, at the ROM's end.
            if (Systems.platformFor(embeddedExtension) == Systems.WS) {
                homeHeader = homeHeader.copy(battery = zipHeader?.battery ?: wsBattery(context, fileUri, name))
            } else if (gba) {
                homeHeader = Systems.HomeHeader("", "", zipHeader?.battery ?: try {
                    resolver.openInputStream(fileUri)?.use { input ->
                        RomSource.useRom(input, name) { rom, _ -> Systems.gbaBattery(rom) }
                    } ?: false
                } catch (_: Exception) { false })
            } else if (!home) homeHeader = Systems.HomeHeader("", "", Systems.gbBattery(header))
            // Use the exact Nintendo GB header fields read by
            // Cart Studio's automatic shell selector. Never infer
            // colours from the ROM filename or download artwork.
            if (!gba && !home && header.size >= 0x14b) {
                gbTitle = String(header, 0x134, 11, Charsets.US_ASCII)
                    .substringBefore('\u0000').trim()
                gbCode = String(header, 0x13f, 4, Charsets.US_ASCII)
                    .takeIf { value -> value.all { it in 'A'..'Z' || it in '0'..'9' } }
                    ?: ""
                gbJapan = header[0x14a].toInt() == 0
                gbCgbFlag = header[0x143].toInt() and 0xff
            }
            code = if (gba && header.size >= 0xb0) {
                String(header, 0xac, 4, Charsets.US_ASCII)
                    .takeIf { it.all { c -> c in 'A'..'Z' || c in '0'..'9' } }
                    ?: ""
            } else ""
            colorOnly = !gba && !home && (embeddedExtension == "gbc" ||
                (header.size > 0x143 && (header[0x143].toInt() and 0xff) == 0xc0))
            val container = folder.lowercase(Locale.ROOT)
            val gbColor = !gba && !home && (embeddedExtension == "gbc" || colorOnly ||
                container == "gbc" || container == "game boy color")
            platform = when {
                home -> Systems.platformFor(embeddedExtension)!!
                gba -> "GBA"
                gbColor -> "GBC"
                else -> "GB"
            }
        }
        val game = JSONObject()
            .put("title", title)
            .put("filename", name)
            .put("platform", platform)
            .put("uri", uriText)
            .put("code", code)
            .put("color_only", colorOnly)
            .put("size", size)
            .put("modified", modified)
        if (home) {
            // The home console shells, labels and battery read
            // these; rom_ext lets a cached ZIP keep its system.
            game.put("rom_ext", embeddedExtension)
                .put("home_title", homeHeader.title)
                .put("home_region", homeHeader.region)
                .put("home_battery", homeHeader.battery)
                .put("home_maker", homeHeader.maker)
                .put("battery_v3", true)
                .put("header_v2", true)
        } else {
            // GB/GBC/GBA: the board shows a battery only for a
            // cart that keeps its save on one.
            game.put("home_battery", homeHeader.battery)
        }
        if (!home && !gba) {
            game.put("gb_title", gbTitle)
                .put("gb_code", gbCode)
                .put("gb_japan", gbJapan)
                .put("gb_cgb_flag", gbCgbFlag)
        }
        return ReadEntry(game, reuse)
    }

    /** A file slot. just saved in the ROM folder (RomM): its folder's name
     *  ("" for the ROM folder itself), as a scan would see it. */
    internal data class SavedFile(val uri: Uri, val folder: String,
                                  val label: java.io.File? = null, val labelPlatform: String = "")

    /** The URI a scan gives this file, whatever form its provider gave. */
    fun indexUri(treeUri: Uri, file: Uri): Uri =
        DocumentsContract.buildDocumentUriUsingTree(treeUri, DocumentsContract.getDocumentId(file))

    /**
     * The index with files slot. just saved added to it, read the way a scan
     * reads them, so the next scan finds the same index. No folder listing.
     * Null when it can't be done safely (no index, a file unreadable or not a
     * cart): the caller scans the folder instead.
     */
    fun withSavedFiles(context: Context, treeUri: Uri, gamesJson: String,
                       files: List<SavedFile>): String? {
        val resolver = context.contentResolver
        val columns = arrayOf(
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_SIZE,
            DocumentsContract.Document.COLUMN_LAST_MODIFIED
        )
        val added = ArrayList<JSONObject>()
        // RomM's games arrive one by one, each add naming them all: those
        // already in the index aren't read again (each read was the whole
        // file, RetroPup's 18 games took seconds per add).
        val known = JSONArray(gamesJson).let { games ->
            (0 until games.length()).mapNotNullTo(HashSet()) { games.optJSONObject(it)?.optString("uri") }
        }
        for (file in files) {
            val fileUri = indexUri(treeUri, file.uri)
            if (fileUri.toString() in known) continue
            val (name, size, modified) = resolver.query(fileUri, columns, null, null, null)?.use { c ->
                if (!c.moveToFirst()) return null
                val name = c.getString(c.getColumnIndexOrThrow(columns[0])) ?: return null
                // Providers may leave these out, as in a scan.
                val sizeIndex = c.getColumnIndex(columns[1])
                val modifiedIndex = c.getColumnIndex(columns[2])
                Triple(name,
                    if (sizeIndex >= 0 && !c.isNull(sizeIndex)) c.getLong(sizeIndex) else -1L,
                    if (modifiedIndex >= 0 && !c.isNull(modifiedIndex)) c.getLong(modifiedIndex) else -1L)
            } ?: return null
            if (name.startsWith(".")) return null
            val extension = name.substringAfterLast('.', "").lowercase(Locale.ROOT)
            if (!isSupportedRomExtension(extension)) return null
            added += readEntry(context, fileUri, name, extension, size, modified,
                file.folder, null)?.game ?: return null
        }
        return mergeGames(gamesJson, added)
    }

    /** The index with these carts in it, each replacing the one at its URI.
     *  Null past the shelf's bound. */
    internal fun mergeGames(gamesJson: String, added: List<JSONObject>): String? {
        val old = JSONArray(gamesJson)
        val replaced = added.mapTo(HashSet()) { it.optString("uri") }
        val games = JSONArray()
        for (i in 0 until old.length()) {
            val game = old.optJSONObject(i) ?: continue
            if (game.optString("uri") !in replaced) games.put(game)
        }
        for (game in added) games.put(game)
        return if (games.length() > MAX_ROMS) null else games.toString()
    }

    /** Only stable nonzero provider metadata permits skipping a header read. */
    internal fun canReuseHeader(oldTitle: String?, oldSize: Long, oldModified: Long,
                                title: String, size: Long, modified: Long): Boolean =
        oldTitle == title && size > 0 && modified > 0 &&
            oldSize == size && oldModified == modified

    private fun ignoredDirectory(name: String): Boolean {
        val lower = name.lowercase(Locale.ROOT)
        return lower in IGNORED_DIRECTORIES || lower in OTHER_SYSTEM_DIRECTORIES
    }

    internal val IGNORED_DIRECTORIES = setOf(
        "bios", "saves", "savestates", "states", "screenshots", "videos",
        "themes", "media", "images", "artwork", "covers", "system volume information",
        // ES-DE's scraped media, gamelists and its own folder: never ROMs.
        "downloaded_media", "gamelists", "manuals", "es-de"
    )

    /**
     * ES-DE ROM folder names of systems slot. does not run (the ES-DE
     * convention most handheld setups follow). Their folders can hold
     * thousands of files, and listing them through SAF is most of a scan's
     * time. Our own systems' folders (gb, gbc, gba, sgb, nes, famicom, snes,
     * sfc, snesna, megadrive, megadrivejp, genesis, mastersystem, gamegear, ngp, ngpc,
     * wonderswan, wonderswancolor) and any folder with an
     * unknown name are still searched, so custom layouts keep working.
     */
    internal val OTHER_SYSTEM_DIRECTORIES = setOf(
        "3do", "adam", "ags", "amiga", "amiga1200", "amiga600", "amigacd32", "amstradcpc",
        "android", "apple2", "apple2gs", "arcade", "arcadia", "archimedes", "arduboy",
        "astrocde", "atari2600", "atari5200", "atari7800", "atari800", "atarijaguar",
        "atarijaguarcd", "atarist", "atarixe", "atomiswave", "bbcmicro", "c64",
        "cdimono1", "cdtv", "chailove", "channelf", "coco", "colecovision", "consolearcade",
        "cps", "cps1", "cps2", "cps3", "crvision", "daphne", "desktop", "doom", "dos",
        "dragon32", "dreamcast", "easyrpg", "electron", "emulators", "epic", "fba", "fbneo",
        "fds", "flash", "fm7", "fmtowns", "fpinball", "gamate", "gameandwatch", "gamecom",
        "gc", "gmaster", "gx4000", "intellivision", "j2me", "kodi", "laserdisc",
        "lcdgames", "lowresnx", "lutro", "macintosh", "mame", "mame-advmame",
        "megacd", "megacdjp", "megaduck", "mess", "model2", "model3", "moto", "msx", "msx1",
        "msx2", "msxturbor", "mugen", "multivision", "n3ds", "n64", "n64dd", "naomi",
        "naomi2", "naomigd", "nds", "neogeo", "neogeocd", "neogeocdjp", "ngage",
        "odyssey2", "openbor", "oric", "palm", "pc", "pc88", "pc98", "pcarcade",
        "pcengine", "pcenginecd", "pcfx", "pico8", "plus4", "ports", "ps2", "ps3",
        "ps4", "psp", "psvita", "psx", "pv1000", "quake", "samcoupe", "saturn", "saturnjp",
        "scummvm", "scv", "sega32x", "sega32xjp", "sega32xna", "segacd", "sg-1000",
        "solarus", "spectravideo", "steam", "stv", "supergrafx", "supervision", "supracan",
        "switch", "symbian", "tanodragon", "tg16", "tg-cd", "ti99", "tic80", "to8", "trs-80",
        "uzebox", "vectrex", "vic20", "videopac", "virtualboy", "vpinball", "vsmile",
        "wasm4", "wii", "wiiu", "windows", "windows3x", "windows9x",
        "x1", "x68000", "xbox", "xbox360", "zmachine", "zx81", "zxnext",
        "zxspectrum"
    )

    private fun wsBattery(context: Context, fileUri: Uri, name: String): Boolean {
        // Local SAF documents are usually seekable. A pipe/cloud provider
        // may not be: retain the bounded stream fallback.
        try {
            context.contentResolver.openFileDescriptor(fileUri, "r")?.use { fd ->
                val length = fd.statSize
                if (length in 10..RomSource.MAX_ROM_BYTES) {
                    java.io.FileInputStream(fd.fileDescriptor).use { input ->
                        input.channel.position(length - 10)
                        return Systems.wsBattery(input)
                    }
                }
            }
        } catch (_: Exception) { }
        return try {
            context.contentResolver.openInputStream(fileUri)?.use { input ->
                RomSource.useRom(input, name) { rom, _ -> Systems.wsBattery(rom) }
            } ?: false
        } catch (_: Exception) { false }
    }

    private fun cartridgeHeader(context: Context, fileUri: Uri, length: Int): ByteArray {
        return try {
            context.contentResolver.openInputStream(fileUri)?.use { stream ->
                val header = ByteArray(length)
                var n = 0
                while (n < header.size) {
                    val bytes = stream.read(header, n, header.size - n)
                    if (bytes <= 0) break
                    n += bytes
                }
                header.copyOf(n)
            } ?: byteArrayOf()
        } catch (_: Exception) {
            byteArrayOf()
        }
    }
}
