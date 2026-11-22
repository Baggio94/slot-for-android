package fyi.slot.konkr

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract
import android.system.Os
import android.util.AtomicFile
import android.util.JsonReader
import android.util.JsonToken
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
    /** An in-game save: a few hundred KB at most (GBA 128 KB, N64 none). */
    private const val MAX_SAVE = 4 * 1024 * 1024
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
    internal fun platformOf(slug: String): String? = Systems.platformForRommSlug(slug)

    /** ROM folder names for a RomM system, ES-DE's first: an existing one is
     *  used, else the first is created. */
    internal fun folderNames(slug: String, platform: String): List<String> = when (slug.lowercase(Locale.ROOT)) {
        "famicom" -> listOf("famicom", "nes")
        "sfam" -> listOf("sfc", "snes")
        // One shelf, two consoles: a Color game goes to the Color's folder.
        "neo-geo-pocket-color", "ngpc" -> listOf("ngpc", "ngp")
        "wonderswan-color" -> listOf("wonderswancolor", "wonderswan")
        else -> (Systems.row(platform) ?: Systems.row(Systems.MD))!!.folders
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

    /** When the requests of this thread must be done by (System.nanoTime),
     *  for a launch that never waits long on the server (beta37). */
    private val deadline = ThreadLocal<Long?>()

    /** Runs [block] with its requests cut short after [millis]: a server
     *  that doesn't answer (RomM at home, the handheld elsewhere) costs the
     *  launch at most that, not 10 to 40 seconds. */
    internal fun <T> within(millis: Long, block: () -> T): T {
        val before = deadline.get()
        deadline.set(System.nanoTime() + millis * 1_000_000)
        try {
            return block()
        } finally {
            deadline.set(before)
        }
    }

    /** A timeout for the next step: [usual], or what's left of the time. */
    private fun timeLeft(usual: Int): Int {
        val by = deadline.get() ?: return usual
        val left = (by - System.nanoTime()) / 1_000_000
        if (left <= 0) throw java.net.SocketTimeoutException("RomM didn't answer in time")
        return minOf(usual.toLong(), left).toInt()
    }

    internal fun open(url: String, method: String, auth: String?): HttpURLConnection =
        (URL(url).openConnection() as HttpURLConnection).apply {
            requestMethod = method
            connectTimeout = timeLeft(10_000)
            readTimeout = timeLeft(30_000)
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
                timeLeft(1)
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
            SlotLog.w(TAG, "Pairing failed", error)
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
            SlotLog.w(TAG, "Token check failed", error)
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
            SlotLog.w(TAG, "Sign in failed", error)
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
     *  far after each page. Each page is read as it arrives, keeping only
     *  what slot. uses: a page held whole (up to 15 MB of JSON, three
     *  systems at once) ran the RG Rotate out of memory (beta42, 2026-11-19). */
    fun games(context: Context, system: Console, counted: (Int) -> Unit = {}): List<Game> {
        val games = ArrayList<Game>()
        var offset = 0
        while (true) {
            // platform_ids for RomM 4.4 and later, platform_id before: an
            // unknown parameter is ignored either way.
            val page = authorizedJson(context, "/api/roms?platform_ids=${system.id}&platform_id=${system.id}" +
                "&limit=$PAGE&offset=$offset&order_by=name&order_dir=asc" +
                "&with_char_index=false&with_filter_values=false&with_rom_id_index=false" +
                "&with_provider_metadata=false") { reader -> readGamesPage(reader, system, games) }
            offset += page.items
            counted(games.size)
            if (page.plainList || page.items < PAGE || (page.total in 0..offset)) break
        }
        return games
    }

    private class GamesPage(val items: Int, val total: Int, val plainList: Boolean)

    /** One page of /api/roms: {items, total} since RomM 3.x, a plain list
     *  before. Games go into [into]. */
    private fun readGamesPage(reader: JsonReader, system: Console, into: MutableList<Game>): GamesPage {
        var items = 0
        var total = -1
        fun list() {
            reader.beginArray()
            while (reader.hasNext()) {
                items++
                readGame(reader, system)?.let { into.add(it) }
            }
            reader.endArray()
        }
        if (reader.peek() == JsonToken.BEGIN_ARRAY) {
            list()
            return GamesPage(items, -1, true)
        }
        reader.beginObject()
        while (reader.hasNext()) {
            when (reader.nextName()) {
                "items" -> if (reader.peek() == JsonToken.BEGIN_ARRAY) list() else reader.skipValue()
                "total" -> total = if (reader.peek() == JsonToken.NUMBER) reader.nextInt() else { reader.skipValue(); -1 }
                else -> reader.skipValue()
            }
        }
        reader.endObject()
        return GamesPage(items, total, false)
    }

    /** One game of a page, with only the fields slot. keeps; null when it
     *  is missing from the server's disk or has no plain file name. */
    private fun readGame(reader: JsonReader, system: Console): Game? {
        if (reader.peek() != JsonToken.BEGIN_OBJECT) { reader.skipValue(); return null }
        var id = 0L
        var missing = false
        var file = ""
        var stem = ""
        var size = -1L
        var crc = ""
        val fileCrcs = ArrayList<String>()
        var fileCount = 0
        val regions = ArrayList<String>()
        var revision = ""
        fun string(): String = when (reader.peek()) {
            JsonToken.STRING, JsonToken.NUMBER -> reader.nextString()
            JsonToken.BOOLEAN -> reader.nextBoolean().toString()
            JsonToken.NULL -> { reader.nextNull(); "null" }
            else -> { reader.skipValue(); "" }
        }
        reader.beginObject()
        while (reader.hasNext()) {
            when (reader.nextName()) {
                "id" -> id = string().toLongOrNull() ?: 0L
                "missing_from_fs" -> missing = if (reader.peek() == JsonToken.BOOLEAN) reader.nextBoolean() else { reader.skipValue(); false }
                "fs_name" -> file = string()
                "fs_name_no_ext" -> stem = string()
                "fs_size_bytes" -> size = string().toLongOrNull() ?: -1L
                "crc_hash" -> crc = string()
                "revision" -> revision = string()
                "regions" -> if (reader.peek() == JsonToken.BEGIN_ARRAY) {
                    reader.beginArray()
                    while (reader.hasNext()) string().takeIf { it.isNotBlank() && it != "null" }?.let { regions.add(it) }
                    reader.endArray()
                } else reader.skipValue()
                "files" -> if (reader.peek() == JsonToken.BEGIN_ARRAY) {
                    reader.beginArray()
                    while (reader.hasNext()) {
                        fileCount++
                        if (reader.peek() != JsonToken.BEGIN_OBJECT) { reader.skipValue(); continue }
                        reader.beginObject()
                        while (reader.hasNext()) {
                            if (reader.nextName() == "crc_hash") fileCrcs.add(string()) else reader.skipValue()
                        }
                        reader.endObject()
                    }
                    reader.endArray()
                } else reader.skipValue()
                else -> reader.skipValue()
            }
        }
        reader.endObject()
        if (missing) return null
        if (file.isBlank() || file == "null" || file.contains('/')) return null
        val title = stem.takeIf { it.isNotBlank() && it != "null" } ?: file.substringBeforeLast('.')
        val crcHash = crc.takeIf { it.length == 8 }
            ?: fileCrcs.singleOrNull()?.takeIf { fileCount == 1 && it.length == 8 }
        return Game(id, system.platform, system.slug, file, title.trim(), size,
            crcHash?.uppercase(Locale.ROOT).orEmpty(), regions.joinToString(", "),
            revision.takeIf { it != "null" }.orEmpty().trim())
    }

    /** An authorized GET whose JSON reply is read as it arrives by [parse]
     *  (no copy of it held whole). */
    private fun <T> authorizedJson(context: Context, path: String, parse: (JsonReader) -> T): T {
        val account = account(context) ?: throw SignedOut()
        fun attempt(): Pair<Int, T?> {
            val connection = open(account.server + path, "GET", authorization(context, account))
            try {
                val code = connection.responseCode
                if (code != 200) {
                    runCatching { connection.errorStream?.close() }
                    return code to null
                }
                return code to JsonReader(connection.inputStream.bufferedReader(Charsets.UTF_8)).use { reader ->
                    try { parse(reader) } catch (error: IllegalStateException) { throw IOException("RomM reply unreadable", error) }
                }
            } finally {
                connection.disconnect()
            }
        }
        var (code, result) = attempt()
        if (code == 401) {
            if (account.kind == "client") { signOut(context); throw SignedOut() }
            synchronized(this) { access = null }
            val again = attempt()
            code = again.first
            result = again.second
            if (code == 401) { signOut(context); throw SignedOut() }
        }
        if (code != 200 || result == null) throw IOException("RomM roms HTTP $code")
        return result
    }

    // ---------------------------------------------------------------------
    // Saves (slot. 2.5, Sync Saves with RomM): a game's in-game save kept on
    // the server under its RomM game, with the core's name as "emulator",
    // as other RetroArch-core clients do. Plain /api/saves calls, which RomM
    // has had since 3.x: no device registration needed.

    /** One save or state the server holds for a game. [nameNoTags]: its
     *  name without the date tags RomM adds to a slot's versions. */
    class ServerSave(val id: Long, val fileName: String, val updatedAt: String,
                     val emulator: String, val downloadPath: String, val nameNoTags: String = fileName)

    /** The account can be asked about saves: signed in. */
    fun signedIn(context: Context): Boolean = account(context) != null

    /** The RomM game a cart is: same system, same file name without its
     *  extension, from the library kept, else by asking the server. */
    /** A file name as Android's storage writes it: `: ? * " < > | \` and
     *  control characters become `_` (FileUtils.buildValidFatFilename), so
     *  `Zelda: Minish Cap.gba` is `Zelda_ Minish Cap.gba` on the handheld. */
    internal fun storedName(name: String): String = buildString {
        for (c in name) append(if (c.code < 0x20 || c.code == 0x7f || c in "\"*/:<>?\\|") '_' else c)
    }

    /** The same game's name, on the server and on the handheld. */
    internal fun sameName(server: String, handheld: String): Boolean =
        storedName(server).lowercase(Locale.ROOT) == storedName(handheld).lowercase(Locale.ROOT)

    fun romIdFor(context: Context, platform: String, stem: String): Long? {
        cachedLibrary(context)?.first?.firstOrNull {
            it.platform == platform && sameName(it.file.substringBeforeLast('.'), stem)
        }?.let { return it.id }
        val console = consoles(context).firstOrNull { it.platform == platform } ?: return null
        val reply = authorized(context, "/api/roms?platform_ids=${console.id}&platform_id=${console.id}" +
            "&search_term=" + URLEncoder.encode(stem, "UTF-8") + "&limit=50" +
            "&with_char_index=false&with_filter_values=false&with_rom_id_index=false" +
            "&with_provider_metadata=false")
        if (reply.code != 200) throw IOException("RomM search HTTP ${reply.code}")
        val text = String(reply.body, Charsets.UTF_8).trim()
        val items = if (text.startsWith("[")) JSONArray(text) else JSONObject(text).optJSONArray("items")
            ?: return null
        for (i in 0 until items.length()) {
            val rom = items.optJSONObject(i) ?: continue
            val name = rom.optString("fs_name_no_ext").ifBlank { rom.optString("fs_name").substringBeforeLast('.') }
            if (sameName(name, stem)) return rom.optLong("id")
        }
        return null
    }

    /** The saves the server holds for a game. */
    fun saves(context: Context, romId: Long): List<ServerSave> = assets(context, "saves", romId)

    /** A save's bytes: /api/saves/{id}/content, or its download path on
     *  an older RomM. */
    fun downloadSave(context: Context, save: ServerSave): ByteArray = downloadAsset(context, "saves", save)

    /** Deletes saves from the server (POST /api/saves/delete, RomM 3 and
     *  later): a save the player deleted everywhere (SaveDeletes). */
    fun deleteSaves(context: Context, ids: List<Long>) = deleteAssets(context, "saves", ids)

    /** Sends a game's save, replacing the one of the same name and emulator. */
    fun uploadSave(context: Context, romId: Long, emulator: String, fileName: String,
                   bytes: ByteArray): ServerSave =
        uploadAsset(context, "saves", "saveFile", romId, emulator, fileName, bytes, null)

    // ---------------------------------------------------------------------
    // States (slot. 3.0, Sync States with RomM, beta): a game's resume state
    // kept on the server like its save, under the core's name as "emulator"
    // and RetroArch's file name (<rom>.state.auto), with a picture of the
    // game. /api/states mirrors /api/saves.

    fun states(context: Context, romId: Long): List<ServerSave> = assets(context, "states", romId)

    fun downloadState(context: Context, state: ServerSave): ByteArray = downloadAsset(context, "states", state)

    fun deleteStates(context: Context, ids: List<Long>) = deleteAssets(context, "states", ids)

    /** Sends a game's resume state, replacing the one of the same name and
     *  emulator, with [screenshot] (a PNG) when there is one. */
    fun uploadState(context: Context, romId: Long, emulator: String, fileName: String,
                    bytes: ByteArray, screenshot: ByteArray?): ServerSave =
        uploadAsset(context, "states", "stateFile", romId, emulator, fileName, bytes, screenshot)

    private fun assets(context: Context, kind: String, romId: Long): List<ServerSave> {
        val reply = authorized(context, "/api/$kind?rom_id=$romId")
        if (reply.code != 200) throw IOException("RomM $kind HTTP ${reply.code}")
        val list = JSONArray(String(reply.body, Charsets.UTF_8))
        return (0 until list.length()).mapNotNull { i ->
            val asset = list.optJSONObject(i) ?: return@mapNotNull null
            if (asset.optLong("rom_id", romId) != romId) return@mapNotNull null
            serverAsset(asset)
        }
    }

    private fun serverAsset(asset: JSONObject): ServerSave {
        val name = asset.optString("file_name")
        return ServerSave(asset.optLong("id"), name, asset.optString("updated_at"),
            asset.optString("emulator").takeIf { it != "null" }.orEmpty(),
            asset.optString("download_path").takeIf { it != "null" }.orEmpty(),
            asset.optString("file_name_no_tags").takeIf { it.isNotBlank() && it != "null" } ?: name)
    }

    private fun downloadAsset(context: Context, kind: String, asset: ServerSave): ByteArray {
        val what = kind.removeSuffix("s")
        val reply = authorizedRaw(context, "/api/$kind/${asset.id}/content")
        if (reply.code == 200) return reply.body
        if (reply.code == 404 && asset.downloadPath.isNotBlank()) {
            val path = if (asset.downloadPath.startsWith("/")) asset.downloadPath else "/" + asset.downloadPath
            val old = authorizedRaw(context, path)
            if (old.code == 200) return old.body
            throw IOException("RomM $what download HTTP ${old.code}")
        }
        throw IOException("RomM $what download HTTP ${reply.code}")
    }

    private fun deleteAssets(context: Context, kind: String, ids: List<Long>) {
        if (ids.isEmpty()) return
        val account = account(context) ?: throw SignedOut()
        val body = JSONObject().put(kind, JSONArray(ids))
        var reply = call(account.server + "/api/$kind/delete", "POST", authorization(context, account), json = body)
        if (reply.code == 401 && account.kind != "client") {
            synchronized(this) { access = null }
            reply = call(account.server + "/api/$kind/delete", "POST", authorization(context, account), json = body)
        }
        if (reply.code == 401) { signOut(context); throw SignedOut() }
        if (reply.code !in 200..299) throw IOException("RomM ${kind.removeSuffix("s")} delete HTTP ${reply.code}")
    }

    private fun uploadAsset(context: Context, kind: String, field: String, romId: Long, emulator: String,
                            fileName: String, bytes: ByteArray, screenshot: ByteArray?): ServerSave {
        val what = kind.removeSuffix("s")
        val account = account(context) ?: throw SignedOut()
        val url = account.server + "/api/$kind?rom_id=$romId&emulator=" + URLEncoder.encode(emulator, "UTF-8")
        fun send(auth: String): Reply {
            val boundary = "slot" + System.nanoTime()
            fun part(name: String, file: String, type: String) = ("--$boundary\r\nContent-Disposition: form-data; " +
                "name=\"$name\"; filename=\"" + file.replace("\"", "") + "\"\r\nContent-Type: $type\r\n\r\n")
                .toByteArray(Charsets.UTF_8)
            val head = part(field, fileName, "application/octet-stream")
            val picture = screenshot?.takeIf { it.isNotEmpty() }?.let {
                "\r\n".toByteArray(Charsets.UTF_8) + part("screenshotFile", fileName + ".png", "image/png") + it
            } ?: ByteArray(0)
            val tail = "\r\n--$boundary--\r\n".toByteArray(Charsets.UTF_8)
            val connection = open(url, "POST", auth)
            try {
                connection.setRequestProperty("Content-Type", "multipart/form-data; boundary=$boundary")
                connection.doOutput = true
                connection.setFixedLengthStreamingMode(head.size + bytes.size + picture.size + tail.size)
                connection.outputStream.use { it.write(head); it.write(bytes); it.write(picture); it.write(tail) }
                val code = connection.responseCode
                val stream = if (code in 200..299) connection.inputStream else connection.errorStream
                return Reply(code, read(stream, MAX_JSON))
            } finally {
                connection.disconnect()
            }
        }
        var reply = send(authorization(context, account))
        if (reply.code == 401 && account.kind != "client") {
            synchronized(this) { access = null }
            reply = send(authorization(context, account))
        }
        if (reply.code == 401) { signOut(context); throw SignedOut() }
        if (reply.code == 403) throw IOException("This RomM account can't keep ${kind}")
        if (reply.code == 404 && kind == "states") throw IOException("This RomM server can't keep states")
        if (reply.code !in 200..299) throw IOException("RomM $what upload HTTP ${reply.code}")
        return serverAsset(reply.json() ?: throw IOException("RomM $what upload unreadable"))
    }

    /** The firmware the player keeps on RomM for one platform; none when
     *  the server or this account doesn't offer it. */
    private fun firmware(context: Context, platformId: Int): List<Pair<Long, String>> {
        val reply = authorized(context, "/api/firmware?platform_id=$platformId")
        if (reply.code != 200) return emptyList()
        val list = try { JSONArray(String(reply.body, Charsets.UTF_8)) } catch (_: Exception) { return emptyList() }
        return (0 until list.length()).mapNotNull { i ->
            val f = list.optJSONObject(i) ?: return@mapNotNull null
            if (f.optBoolean("missing_from_fs")) return@mapNotNull null
            val name = f.optString("file_name").takeIf { it.isNotBlank() } ?: return@mapNotNull null
            f.optLong("id") to name
        }
    }

    /** Fetches the BIOS named in `wanted` (gba_bios.bin…) from the player's
     *  platforms on RomM (GB, GBC, GBA, Lynx, Game Gear, Master System, Mega
     *  Drive, Pokemon mini); returns those slot. kept. */
    fun fetchBios(context: Context, wanted: Set<String>): List<String> {
        val left = wanted.toMutableSet()
        val added = mutableListOf<String>()
        val systems = setOf(Systems.GB, Systems.GBC, Systems.GBA, Systems.LYNX, Systems.GG,
            Systems.SMS, Systems.MD, Systems.POKEMINI)
        for (console in consoles(context).filter { it.platform in systems }) {
            if (left.isEmpty()) break
            for ((id, file) in firmware(context, console.id)) {
                val name = file.lowercase(Locale.ROOT)
                if (name !in left) continue
                val reply = authorizedRaw(context, "/api/firmware/$id/content/" +
                    URLEncoder.encode(file, "UTF-8").replace("+", "%20"))
                if (reply.code != 200) continue
                if (BiosLibrary.addIfMissing(context, name, reply.body)) {
                    added += name
                    left -= name
                }
            }
        }
        return added
    }

    /** As `authorized`, for a file rather than JSON. */
    private fun authorizedRaw(context: Context, path: String): Reply {
        val account = account(context) ?: throw SignedOut()
        fun get(auth: String): Reply {
            val connection = open(account.server + path, "GET", auth)
            connection.setRequestProperty("Accept", "*/*")
            try {
                val code = connection.responseCode
                val stream = if (code in 200..299) connection.inputStream else connection.errorStream
                return Reply(code, read(stream, MAX_SAVE))
            } finally {
                connection.disconnect()
            }
        }
        var reply = get(authorization(context, account))
        if (reply.code == 401 && account.kind != "client") {
            synchronized(this) { access = null }
            reply = get(authorization(context, account))
        }
        if (reply.code == 401) { signOut(context); throw SignedOut() }
        return reply
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
            SlotLog.w(TAG, "RomM library not kept", error)
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
        SlotLog.w(TAG, "RomM library kept unreadable", error)
        null
    }

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
            SlotLog.w(TAG, "slot.'s art set index unavailable for RomM", error)
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
                SlotLog.w(TAG, "Label lookup failed for ${game.file}", error)
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
        // Already there, under its name or the one Android gave it.
        val stored = storedName(game.file)
        (childNamed(context, tree, folder, game.file)
            ?: stored.takeIf { it != game.file }?.let { childNamed(context, tree, folder, it) })
            ?.let { return RomLibrary.SavedFile(it, folderName) }
        for (part in setOf(game.file + ".part", stored + ".part")) {
            childNamed(context, tree, folder, part)?.let {
                DocumentsContract.deleteDocument(context.contentResolver, it)
            }
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
