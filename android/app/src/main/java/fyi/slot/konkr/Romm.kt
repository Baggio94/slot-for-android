package fyi.slot.konkr

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract
import android.system.Os
import android.util.AtomicFile
import android.util.Log
import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder
import java.util.Locale

/**
 * RomM (rommapp/romm), a self-hosted game library: each player connects
 * slot. to their own server, browses its carts by system and downloads the
 * ones they pick into their ROM folder, each system in its own folder.
 *
 * Two ways in, both typed with Android's keyboard:
 *  - a pairing code made in RomM 5 (Profile > Client API Tokens > Pair):
 *    exchanged once for a client token that slot. keeps;
 *  - a username and password, for any version: exchanged for OAuth tokens.
 *    The password is never kept, only the refresh token.
 *
 * What slot. keeps (server, token) lives in the app's private files, never
 * backed up, owner-only, like the RetroAchievements session.
 */
internal object Romm {
    private const val TAG = "SlotRomm"
    private const val MAX_JSON = 32 * 1024 * 1024
    private const val PAGE = 500
    /** Asked first; a viewer account only holds the read ones. */
    // collections.read: the account's favourites, for the page's filter.
    private const val FULL_SCOPES = "me.read roms.read platforms.read collections.read assets.read assets.write " +
        "devices.read devices.write"
    private const val READ_SCOPES = "me.read roms.read platforms.read collections.read"

    class Account(val server: String, val kind: String, val token: String, val user: String)

    sealed interface Result {
        data class Connected(val user: String) : Result
        data class Failed(val message: String) : Result
    }

    /** A system of the server that slot. plays. */
    class Console(val id: Int, val slug: String, val platform: String)

    class Game(val id: Long, val platform: String, val slug: String, val file: String,
               val title: String, val size: Long, val crc: String,
               val regions: String = "", val revision: String = "")

    /** RomM's platform slugs (IGDB's) for the six systems slot. plays. */
    internal fun platformOf(slug: String): String? = when (slug.lowercase(Locale.ROOT)) {
        "gb" -> Systems.GB
        "gbc" -> Systems.GBC
        "gba" -> Systems.GBA
        "nes", "famicom" -> Systems.NES
        "snes", "sfam" -> Systems.SNES
        "genesis", "megadrive", "genesis-slash-megadrive", "sega-mega-drive" -> Systems.MD
        else -> null
    }

    /** ROM folder names for a RomM system, ES-DE's first: an existing one is
     *  used, else the first is created. */
    internal fun folderNames(slug: String, platform: String): List<String> = when (slug.lowercase(Locale.ROOT)) {
        "famicom" -> listOf("famicom", "nes")
        "sfam" -> listOf("sfc", "snes")
        else -> when (platform) {
            Systems.GB -> listOf("gb")
            Systems.GBC -> listOf("gbc")
            Systems.GBA -> listOf("gba")
            Systems.NES -> listOf("nes", "famicom")
            Systems.SNES -> listOf("snes", "sfc", "snesna")
            else -> listOf("megadrive", "genesis", "megadrivejp")
        }
    }

    /** "192.168.1.20:8080" → http://192.168.1.20:8080, "romm.example.com" →
     *  https://romm.example.com; a typed scheme is kept; no trailing / or /api. */
    internal fun normalizeServer(typed: String): String? {
        var text = typed.trim().trimEnd('/')
        if (text.isEmpty() || text.any { it.isWhitespace() }) return null
        if (text.endsWith("/api")) text = text.removeSuffix("/api")
        if (!text.contains("://")) {
            val host = text.substringBefore('/').substringBefore(':').lowercase(Locale.ROOT)
            val local = host == "localhost" || host.endsWith(".local") || host.endsWith(".lan") ||
                host.endsWith(".home") || host.matches(Regex("""\d{1,3}(\.\d{1,3}){3}""")) ||
                !host.contains('.')
            text = (if (local) "http://" else "https://") + text
        }
        val scheme = text.substringBefore("://").lowercase(Locale.ROOT)
        if (scheme != "http" && scheme != "https") return null
        return try {
            val url = URL(text)
            if (url.host.isNullOrBlank()) null else text
        } catch (_: Exception) { null }
    }

    // ---------------------------------------------------------------------
    // The account

    private fun file(context: Context) = File(context.filesDir, "Config/romm.json")

    @Synchronized
    fun account(context: Context): Account? = try {
        val f = file(context)
        if (!f.isFile || f.length() > 16 * 1024) null else {
            val json = JSONObject(f.readText(Charsets.UTF_8))
            val server = json.optString("server")
            val token = json.optString("token")
            if (server.isBlank() || token.isBlank()) null
            else Account(server, json.optString("kind", "client"), token, json.optString("user"))
        }
    } catch (_: Exception) { null }

    @Synchronized
    private fun save(context: Context, account: Account) {
        val f = file(context)
        f.parentFile?.mkdirs()
        val atomic = AtomicFile(f)
        val out = atomic.startWrite()
        try {
            out.write(JSONObject()
                .put("server", account.server)
                .put("kind", account.kind)
                .put("token", account.token)
                .put("user", account.user)
                .toString().toByteArray(Charsets.UTF_8))
            atomic.finishWrite(out)
        } catch (error: Exception) {
            atomic.failWrite(out)
            throw error
        }
        try { Os.chmod(f.path, 0b110_000_000) } catch (_: Exception) {}
    }

    @Synchronized
    fun signOut(context: Context) {
        access = null
        file(context).delete()
        libraryFile(context).delete()
    }

    // ---------------------------------------------------------------------
    // HTTP

    private class Reply(val code: Int, val body: ByteArray) {
        fun json(): JSONObject? = try { JSONObject(String(body, Charsets.UTF_8)) } catch (_: Exception) { null }
        fun detail(): String = json()?.optString("detail").orEmpty()
    }

    private fun open(url: String, method: String, auth: String?): HttpURLConnection =
        (URL(url).openConnection() as HttpURLConnection).apply {
            requestMethod = method
            connectTimeout = 10_000
            readTimeout = 30_000
            instanceFollowRedirects = true
            setRequestProperty("User-Agent", "slot. for Android")
            setRequestProperty("Accept", "application/json")
            if (auth != null) setRequestProperty("Authorization", auth)
        }

    private fun read(input: InputStream?, limit: Int): ByteArray {
        if (input == null) return ByteArray(0)
        val out = ByteArrayOutputStream()
        input.use {
            val buffer = ByteArray(32 * 1024)
            while (true) {
                val n = it.read(buffer)
                if (n < 0) break
                if (out.size() + n > limit) throw IOException("RomM reply too large")
                out.write(buffer, 0, n)
            }
        }
        return out.toByteArray()
    }

    private fun call(url: String, method: String = "GET", auth: String? = null,
                     form: Map<String, String>? = null, json: JSONObject? = null): Reply {
        val connection = open(url, method, auth)
        try {
            val body = when {
                form != null -> {
                    connection.setRequestProperty("Content-Type", "application/x-www-form-urlencoded")
                    form.entries.joinToString("&") {
                        URLEncoder.encode(it.key, "UTF-8") + "=" + URLEncoder.encode(it.value, "UTF-8")
                    }.toByteArray(Charsets.UTF_8)
                }
                json != null -> {
                    connection.setRequestProperty("Content-Type", "application/json")
                    json.toString().toByteArray(Charsets.UTF_8)
                }
                else -> null
            }
            if (body != null) {
                connection.doOutput = true
                connection.setFixedLengthStreamingMode(body.size)
                connection.outputStream.use { it.write(body) }
            }
            val code = connection.responseCode
            val stream = if (code in 200..299) connection.inputStream else connection.errorStream
            return Reply(code, read(stream, MAX_JSON))
        } finally {
            connection.disconnect()
        }
    }

    /** What went wrong reaching a server, in a few plain words. */
    private fun unreachable(error: Exception): String = when (error) {
        is java.net.UnknownHostException -> "Server not found. Check the address."
        is java.net.SocketTimeoutException -> "The server didn't answer. Check the address and your Wi-Fi."
        is java.net.ConnectException -> "Couldn't reach the server. Check the address and your Wi-Fi."
        is javax.net.ssl.SSLException -> "Secure connection failed. Try http:// for a server at home."
        else -> "Couldn't reach the server."
    }

    /** The server's RomM version, or null when it isn't a RomM server. */
    private fun version(server: String): String? {
        val reply = call("$server/api/heartbeat")
        if (reply.code != 200) return null
        return reply.json()?.optJSONObject("SYSTEM")?.optString("VERSION")?.takeIf { it.isNotBlank() } ?: "?"
    }

    // ---------------------------------------------------------------------
    // Connecting

    fun pair(context: Context, typedServer: String, code: String): Result {
        val server = normalizeServer(typedServer) ?: return Result.Failed("Enter your RomM server's address.")
        val pasted = code.trim()
        if (pasted.startsWith("rmm_")) return useToken(context, server, pasted)
        val clean = code.replace(" ", "").replace("-", "").uppercase(Locale.ROOT)
        if (clean.isEmpty()) return Result.Failed("Enter the pairing code.")
        return try {
            if (version(server) == null) return Result.Failed("No RomM server at this address.")
            val reply = call("$server/api/client-tokens/exchange", "POST", json = JSONObject().put("code", clean))
            when {
                reply.code == 200 || reply.code == 201 -> {
                    val token = reply.json()?.optString("raw_token").orEmpty()
                    if (!token.startsWith("rmm_")) return Result.Failed("The server's answer wasn't understood.")
                    val user = me(server, "Bearer $token")
                    save(context, Account(server, "client", token, user))
                    access = null
                    Result.Connected(user)
                }
                // Before RomM 5 there is no such route: 404, or 405 behind
                // the token routes.
                reply.code == 405 || (reply.code == 404 && reply.detail() == "Not Found") ->
                    Result.Failed("This RomM has no pairing codes (RomM 5 needed). Use your username and password.")
                reply.code == 404 -> Result.Failed("Wrong or expired code: a code lasts one minute. Make a new one in RomM.")
                reply.code == 403 -> Result.Failed("This RomM account is disabled.")
                else -> Result.Failed("RomM refused the code (HTTP ${reply.code}).")
            }
        } catch (error: Exception) {
            Log.w(TAG, "Pairing failed", error)
            Result.Failed(unreachable(error))
        }
    }

    /** A client API token pasted whole (rmm_…, RomM 5): it doesn't expire
     *  in a minute as a pairing code does. Kept once the server takes it. */
    private fun useToken(context: Context, server: String, token: String): Result {
        if (token.any { it.isWhitespace() } || token.length > 200) return Result.Failed("This token wasn't understood.")
        return try {
            if (version(server) == null) return Result.Failed("No RomM server at this address.")
            val reply = call("$server/api/platforms", auth = "Bearer $token")
            when (reply.code) {
                200 -> {
                    val user = me(server, "Bearer $token")
                    save(context, Account(server, "client", token, user))
                    access = null
                    Result.Connected(user)
                }
                401 -> Result.Failed("RomM refused this token. Copy it again, or make a new one.")
                403 -> Result.Failed("This token can't browse ROMs. Give it the roms.read and platforms.read scopes.")
                else -> Result.Failed("RomM refused the token (HTTP ${reply.code}).")
            }
        } catch (error: Exception) {
            Log.w(TAG, "Token check failed", error)
            Result.Failed(unreachable(error))
        }
    }

    fun signIn(context: Context, typedServer: String, user: String, password: String): Result {
        val server = normalizeServer(typedServer) ?: return Result.Failed("Enter your RomM server's address.")
        if (user.isBlank() || password.isEmpty()) return Result.Failed("Enter your username and password.")
        return try {
            if (version(server) == null) return Result.Failed("No RomM server at this address.")
            var reply = token(server, mapOf("grant_type" to "password", "username" to user,
                "password" to password, "scope" to FULL_SCOPES))
            if (reply.code == 403) {
                // A viewer can't upload saves: browse and download still work.
                reply = token(server, mapOf("grant_type" to "password", "username" to user,
                    "password" to password, "scope" to READ_SCOPES))
            }
            when (reply.code) {
                200 -> {
                    val json = reply.json() ?: return Result.Failed("The server's answer wasn't understood.")
                    val refresh = json.optString("refresh_token")
                    if (refresh.isBlank()) return Result.Failed("The server's answer wasn't understood.")
                    keepAccess(json)
                    save(context, Account(server, "oauth", refresh, user.trim()))
                    Result.Connected(user.trim())
                }
                400, 401 -> Result.Failed("Wrong username or password.")
                403 -> Result.Failed("This RomM account can't browse ROMs.")
                else -> Result.Failed("RomM refused the sign in (HTTP ${reply.code}).")
            }
        } catch (error: Exception) {
            Log.w(TAG, "Sign in failed", error)
            Result.Failed(unreachable(error))
        }
    }

    private fun token(server: String, form: Map<String, String>) =
        call("$server/api/token", "POST", form = form)

    private fun me(server: String, auth: String): String = try {
        val reply = call("$server/api/users/me", auth = auth)
        if (reply.code == 200) reply.json()?.optString("username").orEmpty() else ""
    } catch (_: Exception) { "" }

    /** A password account's access token, held in memory until it expires. */
    private var access: Pair<String, Long>? = null

    private fun keepAccess(json: JSONObject) {
        val token = json.optString("access_token")
        val life = json.optLong("expires", 900L).coerceIn(60L, 86_400L)
        if (token.isNotBlank()) access = token to (System.currentTimeMillis() + (life - 30) * 1000)
    }

    class SignedOut : IOException("Signed out of RomM")

    /** The Authorization header for the kept account; refreshes a password
     *  account's access token when it ran out. SignedOut when the server no
     *  longer takes what slot. kept. */
    @Synchronized
    private fun authorization(context: Context, account: Account): String {
        if (account.kind == "client") return "Bearer " + account.token
        access?.takeIf { it.second > System.currentTimeMillis() }?.let { return "Bearer " + it.first }
        // The refresh token kept now: another call may have used the one
        // read before this one waited (they are used once).
        val kept = account(context) ?: throw SignedOut()
        val reply = token(kept.server, mapOf("grant_type" to "refresh_token", "refresh_token" to kept.token))
        if (reply.code == 400 || reply.code == 401) {
            signOut(context)
            throw SignedOut()
        }
        if (reply.code != 200) throw IOException("RomM token refresh HTTP ${reply.code}")
        val json = reply.json() ?: throw IOException("RomM token refresh unreadable")
        keepAccess(json)
        // Refresh tokens are used once: keep the new one.
        json.optString("refresh_token").takeIf { it.isNotBlank() }?.let {
            save(context, Account(account.server, account.kind, it, account.user))
        }
        return "Bearer " + (access?.first ?: throw IOException("RomM gave no access token"))
    }

    private fun authorized(context: Context, path: String): Reply {
        val account = account(context) ?: throw SignedOut()
        val reply = call(account.server + path, auth = authorization(context, account))
        if (reply.code == 401) {
            if (account.kind == "client") { signOut(context); throw SignedOut() }
            synchronized(this) { access = null }
            val again = call(account.server + path, auth = authorization(context, account))
            if (again.code == 401) { signOut(context); throw SignedOut() }
            return again
        }
        return reply
    }

    // ---------------------------------------------------------------------
    // The library

    fun consoles(context: Context): List<Console> {
        val reply = authorized(context, "/api/platforms")
        if (reply.code != 200) throw IOException("RomM platforms HTTP ${reply.code}")
        val list = JSONArray(String(reply.body, Charsets.UTF_8))
        return (0 until list.length()).mapNotNull { i ->
            val p = list.optJSONObject(i) ?: return@mapNotNull null
            val slug = p.optString("slug")
            val platform = platformOf(slug) ?: platformOf(p.optString("fs_slug")) ?: return@mapNotNull null
            if (p.optInt("rom_count", 1) == 0) return@mapNotNull null
            Console(p.optInt("id"), slug.ifBlank { p.optString("fs_slug") }, platform)
        }
    }

    /** The pairing code field as typed: capitals, the dash after the first
     *  four (ABCD-EFGH). Null leaves the text alone: a token (rmm_…) or the
     *  start of one. A dash is only added while typing, never on erasing. */
    internal fun formatPairingCode(text: String, grew: Boolean): String? {
        if (text.startsWith("rmm_", ignoreCase = true)) return null
        if (text.isNotEmpty() && "rmm_".startsWith(text.lowercase(Locale.ROOT))) return null
        val chars = text.filter { it.isLetterOrDigit() }.uppercase(Locale.ROOT).take(8)
        return when {
            chars.length > 4 -> chars.take(4) + "-" + chars.drop(4)
            chars.length == 4 && grew -> "$chars-"
            else -> chars
        }
    }

    /** Every game of one system, page by page; [counted] hears how many so
     *  far after each page. */
    fun games(context: Context, system: Console, counted: (Int) -> Unit = {}): List<Game> {
        val games = ArrayList<Game>()
        var offset = 0
        while (true) {
            // platform_ids for RomM 4.4 and later, platform_id before: an
            // unknown parameter is ignored either way.
            val reply = authorized(context, "/api/roms?platform_ids=${system.id}&platform_id=${system.id}" +
                "&limit=$PAGE&offset=$offset&order_by=name&order_dir=asc" +
                "&with_char_index=false&with_filter_values=false&with_rom_id_index=false" +
                "&with_provider_metadata=false")
            if (reply.code != 200) throw IOException("RomM roms HTTP ${reply.code}")
            val text = String(reply.body, Charsets.UTF_8).trim()
            // A page ({items, total}) since RomM 3.x, a plain list before.
            val (items, total) = if (text.startsWith("[")) JSONArray(text) to -1
                else JSONObject(text).let { it.optJSONArray("items") to it.optInt("total", -1) }
            if (items == null) break
            for (i in 0 until items.length()) {
                val rom = items.optJSONObject(i) ?: continue
                if (rom.optBoolean("missing_from_fs", false)) continue
                val file = rom.optString("fs_name")
                if (file.isBlank() || file.contains('/')) continue
                val title = rom.optString("fs_name_no_ext").ifBlank { file.substringBeforeLast('.') }
                games.add(Game(rom.optLong("id"), system.platform, system.slug, file, title.trim(),
                    rom.optLong("fs_size_bytes", -1), crcOf(rom), joined(rom.optJSONArray("regions")),
                    rom.optString("revision").takeIf { it != "null" }.orEmpty().trim()))
            }
            offset += items.length()
            counted(games.size)
            if (text.startsWith("[") || items.length() < PAGE || (total in 0..offset)) break
        }
        return games
    }

    // The library as last loaded, shown at once when the page opens again
    // after a restart while the server is asked again.

    private fun libraryFile(context: Context) = File(context.cacheDir, "romm-library.json")

    /** Kept for this account only (its server and user). */
    fun storeLibrary(context: Context, games: List<Game>, favorites: Set<Long>?) {
        val account = account(context) ?: return
        val list = JSONArray()
        for (g in games) list.put(JSONArray().put(g.id).put(g.platform).put(g.slug).put(g.file)
            .put(g.title).put(g.size).put(g.crc).put(g.regions).put(g.revision))
        val json = JSONObject().put("server", account.server).put("user", account.user)
            .put("games", list)
        if (favorites != null) json.put("favorites", JSONArray(favorites.toList()))
        val atomic = AtomicFile(libraryFile(context))
        val out = atomic.startWrite()
        try {
            out.write(json.toString().toByteArray(Charsets.UTF_8))
            atomic.finishWrite(out)
        } catch (error: Exception) {
            atomic.failWrite(out)
            Log.w(TAG, "RomM library not kept", error)
        }
    }

    /** The library kept for this account, or null. */
    fun cachedLibrary(context: Context): Pair<List<Game>, Set<Long>?>? = try {
        val account = account(context)
        val f = libraryFile(context)
        if (account == null || !f.isFile || f.length() > MAX_JSON) null else {
            val json = JSONObject(f.readText(Charsets.UTF_8))
            if (json.optString("server") != account.server || json.optString("user") != account.user) null
            else {
                val list = json.getJSONArray("games")
                val games = (0 until list.length()).map { i ->
                    val g = list.getJSONArray(i)
                    Game(g.getLong(0), g.getString(1), g.getString(2), g.getString(3), g.getString(4),
                        g.getLong(5), g.getString(6), g.optString(7), g.optString(8))
                }
                val favorites = json.optJSONArray("favorites")?.let { a ->
                    (0 until a.length()).mapTo(HashSet()) { a.getLong(it) }
                }
                games to favorites
            }
        }
    } catch (error: Exception) {
        Log.w(TAG, "RomM library kept unreadable", error)
        null
    }

    private fun joined(array: JSONArray?): String =
        if (array == null) "" else (0 until array.length()).mapNotNull { array.optString(it).takeIf { s -> s.isNotBlank() } }
            .joinToString(", ")

    /**
     * The games in the account's RomM favourites (its "Favourites"
     * collection), or null when the server won't say (an older RomM, or a
     * pairing token without collections.read).
     */
    fun favorites(context: Context): Set<Long>? {
        val reply = try { authorized(context, "/api/collections") } catch (error: IOException) { return null }
        if (reply.code != 200) return null
        val list = try { JSONArray(String(reply.body, Charsets.UTF_8)) } catch (_: Exception) { return null }
        val ids = HashSet<Long>()
        for (i in 0 until list.length()) {
            val collection = list.optJSONObject(i) ?: continue
            if (!collection.optBoolean("is_favorite", false)) continue
            val roms = collection.optJSONArray("rom_ids") ?: continue
            for (k in 0 until roms.length()) ids.add(roms.optLong(k))
        }
        return ids
    }

    /** The game file's CRC32: the ROM's own, else its single file's. */
    private fun crcOf(rom: JSONObject): String {
        rom.optString("crc_hash").takeIf { it.length == 8 }?.let { return it.uppercase(Locale.ROOT) }
        val files = rom.optJSONArray("files") ?: return ""
        if (files.length() != 1) return ""
        return files.optJSONObject(0)?.optString("crc_hash")?.takeIf { it.length == 8 }
            ?.uppercase(Locale.ROOT).orEmpty()
    }

    // ---------------------------------------------------------------------
    // Labels

    private fun labelDir(context: Context) = File(context.cacheDir, "romm-labels").apply { mkdirs() }

    /** Where a game's label is kept, whether it exists yet or not. */
    fun labelFile(context: Context, game: Game): File =
        File(labelDir(context), "${game.platform}-" + (game.crc.ifBlank { "id${game.id}" }) + ".png")

    private val labelMisses = java.util.Collections.synchronizedSet(HashSet<String>())

    /** RomM's page opened: the art set's lists fetched again when older than
     *  an hour, and the games it had nothing for asked again, so labels
     *  published since show (Siham 2026-11-10). Off the UI thread. */
    fun refreshLabels(context: Context) {
        SlotArtSet.refresh(context, 60L * 60 * 1000)
        ArtNames.forget()
        labelMisses.clear()
    }

    /** slot.'s older art set's index (art.slot-cfw.fyi), as the carousel
     *  falls back on it for the Game Boys; asked again after ten minutes
     *  when it couldn't be had. */
    private var oldIndex: JSONObject? = null
    private var oldIndexTried = 0L

    @Synchronized
    private fun oldIndex(context: Context): JSONObject? {
        oldIndex?.let { return it }
        val now = System.currentTimeMillis()
        if (now - oldIndexTried < 10 * 60_000L) return null
        oldIndexTried = now
        return try {
            val response = CartStudioArtProxy(context).intercept("index.json")
            if (response.statusCode != 200) null
            else JSONObject(String(response.data.use { it.readBytes() }, Charsets.UTF_8))
                .also { oldIndex = it }
        } catch (error: Exception) {
            Log.w(TAG, "slot.'s art set index unavailable for RomM", error)
            null
        }
    }

    private fun oldScan(context: Context, platform: String, crc: String): ByteArray? {
        val path = oldIndex(context)?.let { OfficialArtResolver.artPath(it, platform, crc) } ?: return null
        val image = CartStudioArtProxy(context).intercept(path)
        return if (image.statusCode == 200) image.data.use { it.readBytes() } else null
    }

    /** The game's label, from slot. for Android's own art set, then for the
     *  Game Boys slot.'s older one as the carousel (Siham 2026-11-10): by
     *  CRC, then by name as the carousel matches a name (ArtNames). A network
     *  error is tried again twice; a game neither holds isn't asked again
     *  until the page opens again. */
    fun fetchLabel(context: Context, game: Game): Boolean {
        val out = labelFile(context, game)
        if (out.isFile && out.length() > 8) return true
        if (out.path in labelMisses) return false
        var png: ByteArray? = null
        var failed = false
        for (attempt in 0 until 3) {
            if (attempt > 0) Thread.sleep(1_500L * attempt)
            failed = false
            png = try {
                ownLabel(context, game, (listOf(game.crc) + crcsByName(context, game))
                    .filter { it.isNotBlank() }.distinct())
            } catch (error: Exception) {
                Log.w(TAG, "Label lookup failed for ${game.file}", error)
                failed = true
                null
            }
            if (!failed) break
        }
        if (png == null) {
            if (!failed && SlotArtSet.known(context, game.platform)) labelMisses.add(out.path)
            return false
        }
        val atomic = AtomicFile(out)
        val stream = atomic.startWrite()
        return try {
            stream.write(png)
            atomic.finishWrite(stream)
            true
        } catch (error: Exception) {
            atomic.failWrite(stream)
            false
        }
    }

    /** The label from the art set, cut as the shelf's: the Game Boys' from
     *  the cart scan, the home systems' as HomeCartLabels picks theirs. */
    private fun ownLabel(context: Context, game: Game, crcs: List<String>): ByteArray? {
        if (crcs.isEmpty()) return null
        return when (game.platform) {
            Systems.GB, Systems.GBC, Systems.GBA -> (crcs.firstNotNullOfOrNull { crc ->
                SlotArtSet.scanPng(context, game.platform, crc)
            } ?: crcs.firstNotNullOfOrNull { crc -> oldScan(context, game.platform, crc) })
                ?.let { AutoCartLabels.cropPhysicalCartLabel(it, game.platform) }
            else -> {
                val rom = HomeCartLabels.Rom("", game.platform, game.title, game.file,
                    game.size, -1, "", "")
                HomeCartLabels.ownLabel(context, rom, crcs)?.let { (media, bytes) ->
                    HomeCartLabels.labelPng(bytes, media, rom)
                }
            }
        }
    }

    /** The art set's dump named like the game (ArtNames: the carousel's
     *  rules), as a list for the lookup. */
    private fun crcsByName(context: Context, game: Game): List<String> =
        listOfNotNull(ArtNames.crcFor(context, game.platform, game.title))

    // ---------------------------------------------------------------------
    // Headers

    /** A Game Boy family game's header, as RomLibrary reads a local ROM's. */
    class Header(val code: String, val gbTitle: String, val gbCode: String,
                 val gbJapan: Boolean, val gbCgbFlag: Int?)

    private const val HEADER_BYTES = 0x150
    /** A zip's first file is read from its start: enough for the header,
     *  even deflated. Other archives keep the plain shell. */
    private const val ZIP_BYTES = 16 * 1024
    private val packed = setOf("7z", "rar", "gz")
    private val romExtensions = setOf("gb", "gbc", "cgb", "sgb", "dmg", "gba", "agb", "bin")

    private fun headerFile(context: Context, game: Game) =
        File(File(context.cacheDir, "romm-headers").apply { mkdirs() },
            "${game.platform}-" + (game.crc.ifBlank { "id${game.id}" }) + ".bin")

    /** Whether a header can be read for this game: a Game Boy family file,
     *  bare or zipped (not 7z, RAR or gzip). */
    fun wantsHeader(game: Game): Boolean =
        game.platform in setOf(Systems.GB, Systems.GBC, Systems.GBA) &&
            game.file.substringAfterLast('.', "").lowercase(Locale.ROOT) !in packed

    /** The header kept from an earlier read, or null. */
    fun cachedHeader(context: Context, game: Game): Header? {
        if (!wantsHeader(game)) return null
        val f = headerFile(context, game)
        return if (f.isFile && f.length() == HEADER_BYTES.toLong()) try { parseHeader(game.platform, f.readBytes()) }
            catch (_: Exception) { null } else null
    }

    /** The game's first bytes, read from the server (a range request: a few
     *  hundred bytes, whatever the file's size), kept for next time. */
    fun fetchHeader(context: Context, game: Game): Header? {
        if (!wantsHeader(game)) return null
        cachedHeader(context, game)?.let { return it }
        val account = account(context) ?: throw SignedOut()
        val path = "/api/roms/${game.id}/content/" + URLEncoder.encode(game.file, "UTF-8").replace("+", "%20")
        val zipped = game.file.substringAfterLast('.', "").lowercase(Locale.ROOT) == "zip"
        val wanted = if (zipped) ZIP_BYTES else HEADER_BYTES
        fun read(auth: String): Pair<Int, ByteArray> {
            val connection = open(account.server + path, "GET", auth)
            connection.setRequestProperty("Accept", "*/*")
            connection.setRequestProperty("Range", "bytes=0-${wanted - 1}")
            try {
                val code = connection.responseCode
                if (code != 200 && code != 206) return code to ByteArray(0)
                // A server that ignores the range sends the whole file: only
                // its first bytes are read.
                val out = ByteArray(wanted)
                var got = 0
                connection.inputStream.use { input ->
                    while (got < wanted) {
                        val n = input.read(out, got, wanted - got)
                        if (n < 0) break
                        got += n
                    }
                }
                return code to out.copyOf(got)
            } finally {
                connection.disconnect()
            }
        }
        val first = read(authorization(context, account))
        val fetched = if (first.first != 401) first.second else {
            if (account.kind == "client") { signOut(context); throw SignedOut() }
            synchronized(this) { access = null }
            val again = read(authorization(context, account))
            if (again.first == 401) { signOut(context); throw SignedOut() }
            again.second
        }
        val bytes = (if (zipped) zipHead(fetched) else fetched) ?: return null
        if (bytes.size < HEADER_BYTES) return null
        val header = parseHeader(game.platform, bytes) ?: return null
        try {
            val atomic = AtomicFile(headerFile(context, game))
            val stream = atomic.startWrite()
            try { stream.write(bytes); atomic.finishWrite(stream) }
            catch (error: Exception) { atomic.failWrite(stream) }
        } catch (_: Exception) {}
        return header
    }

    /** The first HEADER_BYTES of a zip's first file, from the zip's first
     *  bytes: its local header, then the file stored or deflated. Null when
     *  that file isn't a Game Boy family ROM or too little was read. */
    internal fun zipHead(zip: ByteArray): ByteArray? {
        fun u16(at: Int) = (zip[at].toInt() and 0xff) or ((zip[at + 1].toInt() and 0xff) shl 8)
        if (zip.size < 30 || u16(0) != 0x4b50 || u16(2) != 0x0403) return null
        val flags = u16(6)
        val method = u16(8)
        val nameLength = u16(26)
        val extraLength = u16(28)
        val start = 30 + nameLength + extraLength
        if ((flags and 1) != 0 || start >= zip.size) return null // encrypted
        val name = String(zip, 30, nameLength, Charsets.UTF_8)
        if (name.substringAfterLast('.', "").lowercase(Locale.ROOT) !in romExtensions) return null
        return when (method) {
            0 -> zip.copyOfRange(start, minOf(zip.size, start + HEADER_BYTES))
            8 -> {
                val inflater = java.util.zip.Inflater(true)
                try {
                    inflater.setInput(zip, start, zip.size - start)
                    val out = ByteArray(HEADER_BYTES)
                    var got = 0
                    while (got < HEADER_BYTES && !inflater.finished() && !inflater.needsInput()) {
                        val n = inflater.inflate(out, got, HEADER_BYTES - got)
                        if (n == 0) break
                        got += n
                    }
                    out.copyOf(got)
                } catch (_: java.util.zip.DataFormatException) {
                    null
                } finally {
                    inflater.end()
                }
            }
            else -> null
        }
    }

    /** The fields slot.'s automatic shells read, as RomLibrary reads them. */
    internal fun parseHeader(platform: String, header: ByteArray): Header? {
        fun ascii(from: Int, length: Int) = String(header, from, length, Charsets.US_ASCII)
        fun plain(text: String) = text.takeIf { it.all { c -> c in 'A'..'Z' || c in '0'..'9' } } ?: ""
        return when (platform) {
            Systems.GBA -> if (header.size < 0xb0) null
                else Header(plain(ascii(0xac, 4)), "", "", false, null)
            Systems.GB, Systems.GBC -> if (header.size < 0x14b) null
                else Header("", ascii(0x134, 11).substringBefore('\u0000').trim(), plain(ascii(0x13f, 4)),
                    header[0x14a].toInt() == 0, header[0x143].toInt() and 0xff)
            else -> null
        }
    }

    // ---------------------------------------------------------------------
    // Downloads

    /** The folder for a game's system in the ROM folder: an existing one by
     *  any of its usual names, else made with ES-DE's. The ROM folder itself
     *  when it is that system's folder. With its name as a scan sees it
     *  ("" for the ROM folder itself). */
    private fun systemFolder(context: Context, tree: Uri, game: Game): Pair<Uri, String> {
        val resolver = context.contentResolver
        val rootId = DocumentsContract.getTreeDocumentId(tree)
        val root = DocumentsContract.buildDocumentUriUsingTree(tree, rootId)
        val wanted = folderNames(game.slug, game.platform)
        resolver.query(root, arrayOf(DocumentsContract.Document.COLUMN_DISPLAY_NAME), null, null, null)?.use {
            if (it.moveToFirst() && (it.getString(0) ?: "").lowercase(Locale.ROOT) in wanted) return root to ""
        }
        val children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, rootId)
        val found = HashMap<String, String>()
        resolver.query(children, arrayOf(DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME, DocumentsContract.Document.COLUMN_MIME_TYPE),
            null, null, null)?.use {
            while (it.moveToNext()) {
                if (it.getString(2) != DocumentsContract.Document.MIME_TYPE_DIR) continue
                found[it.getString(1).lowercase(Locale.ROOT)] = it.getString(0)
            }
        }
        wanted.firstOrNull { it in found }?.let {
            return DocumentsContract.buildDocumentUriUsingTree(tree, found.getValue(it)) to it
        }
        return (DocumentsContract.createDocument(resolver, root, DocumentsContract.Document.MIME_TYPE_DIR, wanted.first())
            ?: throw IOException("Couldn't make the ${wanted.first()} folder")) to wanted.first()
    }

    private fun childNamed(context: Context, tree: Uri, folder: Uri, name: String): Uri? {
        val id = DocumentsContract.getDocumentId(folder)
        val children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, id)
        context.contentResolver.query(children, arrayOf(DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME), null, null, null)?.use {
            while (it.moveToNext()) {
                if (it.getString(1).equals(name, ignoreCase = true)) {
                    return DocumentsContract.buildDocumentUriUsingTree(tree, it.getString(0))
                }
            }
        }
        return null
    }

    /** Downloads one game into its system's folder. Written as "<name>.part"
     *  and renamed when whole, so a cut download never shows on the shelf.
     *  The game's file, there already or just saved, for the shelf. */
    fun download(context: Context, tree: Uri, game: Game, progress: (Long, Long) -> Unit): RomLibrary.SavedFile {
        val account = account(context) ?: throw SignedOut()
        val (folder, folderName) = systemFolder(context, tree, game)
        childNamed(context, tree, folder, game.file)?.let { return RomLibrary.SavedFile(it, folderName) }
        childNamed(context, tree, folder, game.file + ".part")?.let {
            DocumentsContract.deleteDocument(context.contentResolver, it)
        }
        val path = "/api/roms/${game.id}/content/" + URLEncoder.encode(game.file, "UTF-8").replace("+", "%20")
        var connection = open(account.server + path, "GET", authorization(context, account))
        connection.readTimeout = 60_000
        try {
            var code = connection.responseCode
            if (code == 401 && account.kind == "oauth") {
                connection.disconnect()
                synchronized(this) { access = null }
                connection = open(account.server + path, "GET", authorization(context, account))
                code = connection.responseCode
            }
            if (code == 401) { signOut(context); throw SignedOut() }
            if (code != 200) throw IOException("RomM download HTTP $code")
            val total = connection.contentLengthLong.takeIf { it > 0 } ?: game.size
            val part = DocumentsContract.createDocument(context.contentResolver, folder,
                "application/octet-stream", game.file + ".part") ?: throw IOException("Couldn't write the ROM")
            var whole = false
            try {
                (context.contentResolver.openOutputStream(part, "w") ?: throw IOException("Couldn't write the ROM")).use { out ->
                    copy(connection.inputStream, out, total, progress)
                }
                whole = true
            } finally {
                if (!whole) try { DocumentsContract.deleteDocument(context.contentResolver, part) } catch (_: Exception) {}
            }
            val file = DocumentsContract.renameDocument(context.contentResolver, part, game.file)
                ?: throw IOException("Couldn't name the ROM")
            return RomLibrary.SavedFile(file, folderName)
        } finally {
            connection.disconnect()
        }
    }

    private fun copy(input: InputStream, out: OutputStream, total: Long, progress: (Long, Long) -> Unit) {
        input.use {
            val buffer = ByteArray(64 * 1024)
            var done = 0L
            var told = 0L
            while (true) {
                val n = it.read(buffer)
                if (n < 0) break
                out.write(buffer, 0, n)
                done += n
                if (done - told >= 256 * 1024) { told = done; progress(done, total) }
            }
            progress(done, total)
        }
    }
}
