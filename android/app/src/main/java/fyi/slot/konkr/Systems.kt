package fyi.slot.konkr

import java.util.Locale

/**
 * The systems slot. runs, in one place: which cartridge files belong to
 * which shelf, which libretro cores run them (by their libretro key, as
 * the native side names them) and the RetroArch folder each core saves
 * into. Mirrors slot_store::Platform and slot_store::Core.
 */
internal object Systems {
    /** Shelf platform names, as the native side parses them. */
    const val GBA = "GBA"
    const val GB = "GB"
    const val GBC = "GBC"
    const val NES = "NES"
    const val SNES = "SNES"
    const val MD = "MD"
    const val SMS = "SMS"
    const val GG = "GG"
    const val NGP = "NGP"
    const val WS = "WS"
    const val LYNX = "LYNX"
    const val POKEMINI = "POKEMINI"
    const val PICO8 = "PICO8"

    /** Splore, PICO-8's cart browser: slot.'s own cart on the PICO-8 shelf
     *  (core_selection::SPLORE_URI), not a file in the ROM folder. */
    const val SPLORE_URI = "slot://pico8/Splore.p8"
    /** What the core is handed for Splore; it boots with -splore instead. */
    const val SPLORE_CART = "pico-8 cartridge // http://www.pico-8.com\nversion 41\n__lua__\n-- splore\n"

    /** The systems whose Pokemon games PokeDaisy plays (Core::pokedaisy_system). */
    private val POKEDAISY_SYSTEMS = setOf(GB, GBC, GBA)

    /** One system slot. plays: a row of slot_store::SYSTEMS, field for field
     *  and in the same order (the native tests read these rows and compare,
     *  so keep each one on a single line). */
    class SystemRow(
        /** The shelf key, as the native side parses it. */
        val platform: String,
        /** Cartridge extensions, lower case, without the .zip container. */
        val extensions: List<String>,
        /** ES-DE ROM folder names: an existing one is used, else the first is made. */
        val folders: List<String>,
        /** RomM's platform slugs (IGDB's). */
        val romm: List<String>,
        val retroAchievements: Int,
        val screenScraper: Int,
        /** libretro keys of the board's two cores, the default first. */
        val cores: List<String>,
        /** A home console: its own header, labels and shell. */
        val home: Boolean,
    )

    val TABLE: List<SystemRow> = listOf(
        SystemRow(GBA, listOf("gba"), listOf("gba"), listOf("gba"), 5, 12, listOf("mgba", "gpsp"), home = false),
        SystemRow(GB, listOf("gb", "gbc"), listOf("gb"), listOf("gb"), 4, 9, listOf("mgba", "gambatte"), home = false),
        SystemRow(GBC, listOf("gb", "gbc"), listOf("gbc"), listOf("gbc"), 6, 10, listOf("mgba", "gambatte"), home = false),
        SystemRow(NES, listOf("nes", "fc", "unf", "unif"), listOf("nes", "famicom"), listOf("nes", "famicom"), 7, 3, listOf("fceumm", "nestopia"), home = true),
        SystemRow(SNES, listOf("sfc", "smc"), listOf("snes", "sfc", "snesna"), listOf("snes", "sfam"), 3, 4, listOf("snes9x", "bsnes"), home = true),
        // .bin is shared with other systems: only a SEGA header makes it a Mega Drive cart.
        SystemRow(MD, listOf("md", "gen", "smd", "bin"), listOf("megadrive", "genesis", "megadrivejp"), listOf("genesis", "megadrive", "genesis-slash-megadrive", "sega-mega-drive"), 1, 1, listOf("genesis_plus_gx", "picodrive"), home = true),
        SystemRow(SMS, listOf("sms"), listOf("mastersystem"), listOf("sms", "mastersystem", "master-system"), 11, 2, listOf("genesis_plus_gx", "gearsystem"), home = true),
        SystemRow(GG, listOf("gg"), listOf("gamegear"), listOf("gamegear"), 15, 21, listOf("genesis_plus_gx", "gearsystem"), home = true),
        // One shelf for both Neo Geo Pockets, one for both WonderSwans (Beetle Cygne only).
        SystemRow(NGP, listOf("ngp", "ngc", "ngpc"), listOf("ngp", "ngpc"), listOf("neo-geo-pocket", "neo-geo-pocket-color", "ngp", "ngpc"), 14, 25, listOf("mednafen_ngp", "race"), home = true),
        SystemRow(WS, listOf("ws", "wsc"), listOf("wonderswan", "wonderswancolor"), listOf("wonderswan", "wonderswan-color"), 53, 45, listOf("mednafen_wswan", "mednafen_wswan"), home = true),
        // The Lynx (Beetle Lynx needs lynxboot.img) and the Pokemon mini (PokeMini only).
        SystemRow(LYNX, listOf("lnx", "lyx"), listOf("atarilynx", "lynx"), listOf("lynx", "atari-lynx", "atarilynx"), 13, 28, listOf("handy", "mednafen_lynx"), home = true),
        SystemRow(POKEMINI, listOf("min"), listOf("pokemini"), listOf("pokemon-mini", "pokemini"), 24, 211, listOf("pokemini", "pokemini"), home = true),
        // PICO-8: .p8 text carts and .p8.png pictures of carts (fake-08 only; no RetroAchievements: 0).
        SystemRow(PICO8, listOf("p8", "png"), listOf("pico8", "pico-8"), listOf("pico-8", "pico"), 0, 234, listOf("fake08", "pico8"), home = true),
    )

    /** Every shelf key, in the native side's order. */
    val PLATFORMS: List<String> = TABLE.map { it.platform }

    /** The art set's indexes a shelf reads, in order: the Neo Geo Pocket's
     *  and the WonderSwan's shelves hold their Color carts too, which the
     *  set files apart (NGPC, WSC). */
    fun artKeys(platform: String): List<String> = when (platform) {
        NGP -> listOf(NGP, "NGPC")
        WS -> listOf(WS, "WSC")
        else -> listOf(platform)
    }

    fun row(platform: String?): SystemRow? = TABLE.firstOrNull { it.platform == platform }

    /** Every cartridge extension, without the .zip container. */
    val ROM_EXTENSIONS: Set<String> = TABLE.flatMap { it.extensions }.toSet()

    val HOME_PLATFORMS: Set<String> = TABLE.filter { it.home }.map { it.platform }.toSet()

    private fun lower(ext: String) = ext.lowercase(Locale.ROOT)

    /** The shelf for a cartridge extension. A .gb may still be a GBC cart. */
    fun platformFor(ext: String): String? = when (val e = lower(ext)) {
        "gb" -> GB
        "gbc" -> GBC
        else -> TABLE.firstOrNull { e in it.extensions }?.platform
    }

    fun isHome(ext: String) = platformFor(ext) in HOME_PLATFORMS

    /** The system of a RomM platform slug, or null when slot. doesn't play it. */
    fun platformForRommSlug(slug: String): String? =
        lower(slug).let { s -> TABLE.firstOrNull { s in it.romm }?.platform }

    /** RetroAchievements' console id of a shelf. */
    /** RetroAchievements' console id, null for a system it has none for (PICO-8). */
    fun retroAchievementsId(platform: String?): Int? = row(platform)?.retroAchievements?.takeIf { it > 0 }

    /** ScreenScraper's system id of a shelf. */
    fun screenScraperId(platform: String?): Int? = row(platform)?.screenScraper

    /** libretro key -> RetroArch core name, also its saves/states folder. */
    private val FOLDERS = linkedMapOf(
        "mgba" to "mGBA",
        "gpsp" to "gpSP",
        "gambatte" to "Gambatte",
        "fceumm" to "FCEUmm",
        "nestopia" to "Nestopia",
        "snes9x" to "Snes9x",
        "bsnes" to "bsnes",
        "genesis_plus_gx" to "Genesis Plus GX",
        "picodrive" to "PicoDrive",
        "gearsystem" to "Gearsystem",
        "mednafen_ngp" to "Beetle NeoPop",
        "race" to "RACE",
        "mednafen_wswan" to "Beetle WonderSwan",
        "handy" to "Handy",
        "mednafen_lynx" to "Beetle Lynx",
        "pokemini" to "PokeMini",
        // The third chip of the Pokemon games' GB, GBC and GBA boards.
        "pokedaisy" to "PokeDaisy",
        "fake08" to "fake-08",
        // PICO-8's second chip: the player's own PICO-8 (cores/pico8real).
        "pico8" to "PICO-8",
    )

    val CORE_FOLDERS: Set<String> = FOLDERS.values.toSet()

    /** The folder an in-game save is kept in: its core's, except
     *  PokeDaisy's, which is mGBA's own file (PokeDaisy is mGBA 0.10.5). */
    fun saveFolder(folder: String): String = when (folder) {
        "PokeDaisy" -> "mGBA"
        // The player's own PICO-8 plays fake-08's save of the game (pico8::SaveBridge).
        "PICO-8" -> "fake-08"
        else -> folder
    }

    /** The in-game save's extension a core writes, as in RetroArch: Handy
     *  keeps the Lynx EEPROM in .eeprom, PokeMini the Pokemon mini's in .eep,
     *  Beetle NeoPop and RACE the Neo Geo Pocket's flash in .flash and .ngf. */
    fun saveExtension(folder: String): String = when (folder) {
        "Handy" -> "eeprom"
        "PokeMini" -> "eep"
        "Beetle NeoPop" -> "flash"
        "RACE" -> "ngf"
        // PICO-8's cartdata, the game's own copy (cores/fake08/zz-slot-game-saves.patch).
        "fake-08" -> "p8d.txt"
        else -> "srm"
    }

    private val SAVE_SUFFIXES = listOf(".srm", ".eeprom", ".eep", ".flash", ".ngf", ".p8d.txt")

    /** An in-game save's file name, whatever core wrote it. */
    fun isSaveName(name: String): Boolean =
        SAVE_SUFFIXES.any { name.endsWith(it, ignoreCase = true) }

    /** A save's file name without its save extension. */
    fun saveStem(name: String): String =
        SAVE_SUFFIXES.firstOrNull { name.endsWith(it, ignoreCase = true) }
            ?.let { name.dropLast(it.length) } ?: name

    /** The libretro key of a RetroArch core folder: Slot's private file prefix. */
    fun keyForFolder(folder: String): String =
        FOLDERS.entries.firstOrNull { it.value == folder }?.key ?: folder.lowercase(Locale.ROOT)

    /** The two cores of each shelf: the default first. */
    private fun coresFor(platform: String?): List<String> = row(platform)?.cores ?: emptyList()

    /** The folder of the core that runs this cart: the one the native side
     *  picked when it runs this extension's system, else the system default. */
    fun coreFolder(key: String, ext: String): String {
        val platform = platformFor(ext)
        val cores = coresFor(platform)
        require(cores.isNotEmpty()) { "Unsupported ROM type" }
        // PokeDaisy: the native side only picks it for a Pokemon game.
        val pokeDaisy = if (platform in POKEDAISY_SYSTEMS) listOf("pokedaisy") else emptyList()
        val chosen = key.lowercase(Locale.ROOT).takeIf { it in cores || it in pokeDaisy } ?: cores.first()
        return FOLDERS.getValue(chosen)
    }

    /** A save flush names its core by key; unknown keys keep the old mGBA fallback. */
    fun folderForKey(key: String): String = FOLDERS[key.lowercase(Locale.ROOT)] ?: "mGBA"

    /** A cart's file name: a PNG only as NAME.p8.png (a PICO-8 cart), never
     *  cover art or a screenshot. */
    fun acceptsName(filename: String): Boolean {
        val name = lower(filename)
        return name.substringAfterLast('.', "") != "png" || name.endsWith(".p8.png")
    }

    /** The smallest file that can be a cartridge: a .p8 text cart can be a
     *  one-line program (its "pico-8 cartridge" header and a few bytes). */
    fun minRomBytes(ext: String): Int =
        if (platformFor(ext) == PICO8) 16 else RomSource.MIN_ROM_BYTES

    /** The shelf title of a cart file: its name without the extension, and
     *  without .p8 for a PICO-8 .p8.png. */
    fun titleOf(filename: String): String {
        val bare = filename.substringBeforeLast('.')
        return if (lower(filename).endsWith(".p8.png")) bare.dropLast(3) else bare
    }

    /** A PNG signature, then IHDR at PICO-8's 160x205 cart picture. */
    fun isPico8Png(header: ByteArray): Boolean {
        val signature = byteArrayOf(0x89.toByte(), 'P'.code.toByte(), 'N'.code.toByte(), 'G'.code.toByte(),
            0x0d, 0x0a, 0x1a, 0x0a)
        if (header.size < 24 || !header.copyOfRange(0, 8).contentEquals(signature)) return false
        if (String(header, 12, 4, Charsets.US_ASCII) != "IHDR") return false
        fun int(at: Int) = (0 until 4).fold(0) { v, i -> (v shl 8) or (header[at + i].toInt() and 0xff) }
        return int(16) == 160 && int(20) == 205
    }

    /** How many leading bytes the scanner needs to identify a cartridge. */
    fun headerBytes(ext: String): Int = when (platformFor(ext)) {
        // Both LoROM and HiROM headers, behind a possible 512-byte copier header.
        SNES -> 0x10200
        MD -> 0x200
        else -> RomSource.MIN_ROM_BYTES
    }

    /** A .bin only counts as a Mega Drive cart with "SEGA" in its header,
     *  a .png as a PICO-8 cart only at a cart picture's 160x205. */
    fun acceptsHeader(ext: String, header: ByteArray): Boolean {
        if (lower(ext) == "png") return isPico8Png(header)
        if (lower(ext) != "bin") return true
        if (header.size < 0x110) return false
        val text = String(header, 0x100, 0x10, Charsets.ISO_8859_1)
        return text.contains("SEGA")
    }

    /** What the shelf draws from a home console header: the internal
     *  title, the region (J, U, E or empty when unknown) and whether the
     *  cart keeps a battery-backed save. A Mega Drive cart also gives its
     *  copyright field ("(C)T-50 1991.JUN" for Electronic Arts), which
     *  picks the publisher's shell. */
    data class HomeHeader(
        val title: String, val region: String, val battery: Boolean, val maker: String = ""
    )

    /** A Game Boy / Game Boy Color cart keeps its save on a battery when
     *  its cartridge type (header 0x147) says RAM+BATTERY (MBC1/2/3/5/7,
     *  MMM01, HuC1/HuC3, TAMA5, Pocket Camera). */
    fun gbBattery(header: ByteArray): Boolean {
        if (header.size <= 0x147) return false
        return (header[0x147].toInt() and 0xff) in setOf(
            0x03, 0x06, 0x09, 0x0d, 0x0f, 0x10, 0x13, 0x1b, 0x1e, 0x22, 0xfc, 0xfd, 0xfe, 0xff)
    }

    /** A Game Boy Advance cart has a battery when it saves to SRAM, or keeps
     *  a real-time clock (Pokémon Ruby, Sapphire, Emerald...): the save
     *  library and clock driver leave their names in the ROM. Flash and
     *  EEPROM carts need none. Reads the whole cart once, from a stream. */
    fun gbaBattery(input: java.io.InputStream): Boolean {
        val marks = listOf("SRAM_V", "SRAM_F_V", "SIIRTC_V").map { it.toByteArray(Charsets.US_ASCII) }
        val keep = marks.maxOf { it.size } - 1
        val buffer = ByteArray(256 * 1024 + keep)
        var carried = 0
        var total = 0L
        while (true) {
            if (Thread.currentThread().isInterrupted) return false
            val n = input.read(buffer, carried, buffer.size - carried)
            if (n <= 0) break
            total += n
            if (total > 64L * 1024 * 1024) return false
            val end = carried + n
            // All three signatures start with S. Scan each byte once.
            for (i in 0 until end) {
                if (buffer[i] != 'S'.code.toByte()) continue
                for (mark in marks) {
                    if (i + mark.size > end) continue
                    var k = 1
                    while (k < mark.size && buffer[i + k] == mark[k]) k++
                    if (k == mark.size) return true
                }
            }
            carried = minOf(keep, end)
            System.arraycopy(buffer, end - carried, buffer, 0, carried)
        }
        return false
    }

    fun homeHeader(ext: String, header: ByteArray, filename: String): HomeHeader {
        val fromName = regionFromName(filename)
        return when (platformFor(ext)) {
            NES -> nesHeader(header, fromName)
            SNES -> snesHeader(header, fromName)
            MD -> mdHeader(ext, header, fromName)
            // Their headers say nothing of a battery: the games known to
            // keep their save on one (a .srm seen later shows the others).
            SMS, GG -> HomeHeader("", fromName, segaBattery(filename))
            else -> HomeHeader("", fromName, false)
        }
    }

    /** Master System and Game Gear games whose cartridge keeps its save on a
     *  battery (by No-Intro title). Others show it once a save file exists. */
    private val SEGA_BATTERY = listOf(
        "phantasy star", "ys - the vanished omens", "ys (japan)", "golden axe warrior",
        "ultima iv", "miracle warriors", "penguin land", "monopoly",
        "shining force", "defenders of oasis", "sylvan tale", "tails adventure",
        "madou monogatari", "lunar - sanposuru gakuen", "lunar - walking school",
    )
    internal fun segaBattery(filename: String): Boolean {
        val title = filename.substringAfterLast('/').lowercase(Locale.ROOT)
        return SEGA_BATTERY.any { title.startsWith(it) }
    }

    /** A WonderSwan cart keeps its save on a battery when its footer (the
     *  ROM's last 10 bytes) names SRAM (types 1 to 5); EEPROM needs none.
     *  Reads the cart once, from a stream, keeping only its tail. */
    fun wsBattery(input: java.io.InputStream): Boolean {
        val buffer = ByteArray(64 * 1024)
        val tail = ByteArray(10)
        var have = 0
        var total = 0L
        while (true) {
            if (Thread.currentThread().isInterrupted) return false
            val n = input.read(buffer)
            if (n <= 0) break
            total += n
            if (total > 64L * 1024 * 1024) return false
            if (n >= 10) {
                System.arraycopy(buffer, n - 10, tail, 0, 10); have = 10
            } else {
                val keep = minOf(have, 10 - n)
                System.arraycopy(tail, have - keep, tail, 0, keep)
                System.arraycopy(buffer, 0, tail, keep, n)
                have = keep + n
            }
        }
        if (have < 10) return false
        return (tail[5].toInt() and 0xff) in 0x01..0x05
    }

    /** No-Intro / GoodTools tags: (Japan), (USA), (Europe), (J), (U), (E)... */
    internal fun regionFromName(filename: String): String {
        val tags = Regex("""\(([^)]*)\)""").findAll(filename)
            .flatMap { it.groupValues[1].split(',').asSequence() }
            .map { it.trim().lowercase(Locale.ROOT) }
            .toList()
        for (tag in tags) {
            when (tag) {
                "japan", "j", "jp", "jpn" -> return "J"
                "usa", "u", "us", "world", "w", "canada" -> return "U"
                "europe", "e", "eu", "pal", "france", "germany", "spain", "italy",
                "uk", "australia", "sweden", "netherlands", "f", "g" -> return "E"
            }
        }
        return ""
    }

    private fun nesHeader(header: ByteArray, region: String): HomeHeader {
        val ines = header.size >= 16 && header[0] == 'N'.code.toByte() &&
            header[1] == 'E'.code.toByte() && header[2] == 'S'.code.toByte() &&
            header[3].toInt() == 0x1a
        // Byte 6 bit 1: battery-backed PRG RAM.
        val battery = ines && (header[6].toInt() and 0x02) != 0
        return HomeHeader("", region, battery)
    }

    private fun snesHeader(header: ByteArray, nameRegion: String): HomeHeader {
        // LoROM then HiROM, each also behind a 512-byte copier header: a
        // valid checksum wins, then a plausible homebrew header.
        val bases = listOf(0x7fc0, 0xffc0, 0x81c0, 0x101c0)
        val base = bases.firstOrNull { checksummed(header, it) }
            ?: bases.firstOrNull { plausible(header, it) }
            ?: return HomeHeader("", nameRegion, false)
        val title = String(header, base, 21, Charsets.ISO_8859_1).trim { it <= ' ' }
        val type = header[base + 0x16].toInt() and 0xff
        // ROM+RAM+battery: low nibble 2, 5 or 6 (also with coprocessors), 9/A (RTC).
        val battery = (type and 0x0f) in setOf(0x02, 0x05, 0x06, 0x09, 0x0a) &&
            (header[base + 0x18].toInt() and 0xff) in 1..0x0c
        val region = when (header[base + 0x19].toInt() and 0xff) {
            0x00 -> "J"
            0x01, 0x0d -> "U"
            in 0x02..0x0c, in 0x10..0x14 -> "E"
            else -> ""
        }
        return HomeHeader(title, nameRegion.ifEmpty { region }, battery)
    }

    private fun checksummed(header: ByteArray, base: Int): Boolean {
        if (base + 0x20 > header.size) return false
        val checksum = (header[base + 0x1e].toInt() and 0xff) or
            ((header[base + 0x1f].toInt() and 0xff) shl 8)
        val complement = (header[base + 0x1c].toInt() and 0xff) or
            ((header[base + 0x1d].toInt() and 0xff) shl 8)
        return checksum xor complement == 0xffff && plausible(header, base)
    }

    // Homebrew often skips the checksum: a printable title and a sane map mode.
    private fun plausible(header: ByteArray, base: Int): Boolean {
        if (base + 0x20 > header.size) return false
        val title = (0 until 21).all { (header[base + it].toInt() and 0xff) in 0x20..0x7e }
        val map = header[base + 0x15].toInt() and 0xff
        return title && (map and 0xe0) == 0x20
    }

    private fun mdHeader(ext: String, header: ByteArray, nameRegion: String): HomeHeader {
        // .smd dumps are interleaved behind a 512-byte header: the region
        // and save fields are not readable as is, so only the name decides.
        if (lower(ext) == "smd" || header.size < 0x200) return HomeHeader("", nameRegion, false)
        val title = String(header, 0x150, 0x30, Charsets.ISO_8859_1).trim { it <= ' ' }
            .ifEmpty { String(header, 0x120, 0x30, Charsets.ISO_8859_1).trim { it <= ' ' } }
        // "RA" then a type byte with bit 6 set: battery-backed SRAM.
        val battery = header[0x1b0] == 'R'.code.toByte() && header[0x1b1] == 'A'.code.toByte() &&
            (header[0x1b2].toInt() and 0x40) != 0
        val codes = String(header, 0x1f0, 3, Charsets.ISO_8859_1).trim().uppercase(Locale.ROOT)
        val region = when {
            'U' in codes -> "U"
            'E' in codes -> "E"
            'J' in codes -> "J"
            // Newer carts write one hex digit: bit 4 USA, bit 8 Europe, bit 1 Japan.
            codes.length == 1 && hexRegion(codes[0], 4) -> "U"
            codes.length == 1 && hexRegion(codes[0], 8) -> "E"
            codes.length == 1 && hexRegion(codes[0], 1) -> "J"
            else -> ""
        }
        val maker = String(header, 0x110, 0x10, Charsets.ISO_8859_1).trim { it <= ' ' }
        return HomeHeader(title, nameRegion.ifEmpty { region }, battery, maker)
    }

    private fun hexRegion(c: Char, bit: Int): Boolean {
        if (c !in '0'..'9' && c !in 'A'..'F') return false
        return (c.digitToInt(16) and bit) != 0
    }
}
