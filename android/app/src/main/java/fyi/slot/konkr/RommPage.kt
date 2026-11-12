package fyi.slot.konkr

import android.net.Uri
import android.text.InputType
import android.util.Log
import android.widget.Toast
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.Executors

/**
 * The Android half of Library > RomM: the connection form (Android's own
 * keyboard), the library fetched from the server, the labels and the
 * downloads. The page itself is drawn by slot. (runtime/romm.rs), which asks
 * through nativePollRommRequest:
 *   open, server, filter, labels<TAB>ids, download<TAB>ids, leave.
 */
internal class RommPage(private val activity: MainActivity, private val romRoot: () -> Uri?) {
    private companion object {
        const val TAG = "SlotRomm"
        const val LAST_SERVER = "romm_last_server"
        const val LAST_MODE_ACCOUNT = "romm_last_mode_account"
        // SELECT's filters, kept between visits.
        const val FILTER_SEARCH = "romm_filter_search"
        const val FILTER_REGION = "romm_filter_region"
        const val FILTER_OFFICIAL = "romm_filter_official"
        const val FILTER_HIDE_SHELF = "romm_filter_hide_shelf"
        const val FILTER_FAVORITES = "romm_filter_favorites"
        val REGIONS = listOf("All", "USA", "Europe", "Japan", "World")
        // nativeRommStatus kinds, as runtime/romm.rs reads them.
        const val LOADING = 0
        const val ERROR = 1
        const val SIGNED_OUT = 2
        const val CLOSE = 3
    }

    private val network = Executors.newSingleThreadExecutor { Thread(it, "slot-romm") }
    /** Downloads on their own: they start at once, even while the library
     *  is asked again. */
    private val downloader = Executors.newSingleThreadExecutor { Thread(it, "slot-romm-downloads") }
    /** A library load under way (one at a time). */
    @Volatile private var loading = false
    /** The list last given to the page (UI thread). */
    private var lastCatalog: String? = null
    private val labeler = Executors.newSingleThreadExecutor { Thread(it, "slot-romm-labels") }
    @Volatile private var games: Map<Long, Romm.Game> = emptyMap()
    /** The account's RomM favourites; null when the server won't say. */
    @Volatile private var favorites: Set<Long>? = null
    /** The labels the page wants now, the ones it shows first. */
    private val wanted = ArrayDeque<Long>()
    private var labelling = false
    /** Games whose header was asked this run (only on the labeller). */
    private val headersRead = HashSet<Long>()
    /** Games waiting for the ROM folder to be chosen again, writable. */
    private var waitingForFolder: List<Long>? = null
    /** Files saved this visit, for the shelf once the page is left. */
    private val downloaded = ArrayList<RomLibrary.SavedFile>()
    /** The page is up: the shelf is refreshed once it is left. */
    @Volatile private var showing = false
    private var form: SlotModalDialog? = null

    fun poll() {
        while (true) {
            val request = activity.nativePollRommRequest() ?: return
            val verb = request.substringBefore('\t')
            val ids = request.substringAfter('\t', "").split(',').mapNotNull { it.trim().toLongOrNull() }
            when (verb) {
                "open" -> open()
                "server" -> server()
                "filter" -> filters()
                "labels" -> labels(ids)
                "download" -> download(ids)
                "leave" -> leave()
            }
        }
    }

    private fun prefs() = activity.getSharedPreferences("slot_romm", android.content.Context.MODE_PRIVATE)

    private fun open() {
        showing = true
        // Labels published since show: on the labeller, before any label.
        labeler.execute { try { Romm.refreshLabels(activity) } catch (_: Exception) {} }
        if (Romm.account(activity) == null) connect(closeOnCancel = true)
        else {
            activity.nativeExternalReturn()
            load()
        }
    }

    private fun leave() {
        showing = false
        synchronized(wanted) { wanted.clear() }
        refreshIfLeft()
    }

    /** Downloads reach their shelves once the page is left: only those
     *  carts are added, no folder rescan. */
    private fun refreshIfLeft() {
        if (showing) return
        val files = synchronized(downloaded) { downloaded.toList().also { downloaded.clear() } }
        if (files.isNotEmpty()) activity.addDownloadedGames(files)
    }

    // -----------------------------------------------------------------
    // Connecting

    private fun connect(closeOnCancel: Boolean) {
        if (form?.isShowing() == true) return
        var connected = false
        // Two ways in (Siham 2026-11-10): L1 a pairing code (or a token
        // pasted whole), R1 the RomM account's username and password.
        val pairingText = "Make a code in RomM: Settings, Client API Tokens, Pair."
        val accountText = "Sign in with your RomM account."
        val dialog = SlotModalDialog(activity, "ROMM", pairingText,
            onDismiss = {
                form = null
                activity.nativeExternalReturn()
                if (!connected && closeOnCancel) activity.nativeRommStatus(CLOSE, "")
            })
        form = dialog
        val server = dialog.addInput("SERVER ADDRESS", "e.g. 192.168.1.20:8080 or romm.example.com")
        server.inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI or
            InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
        server.typeface = SlotUi.font(activity)
        prefs().getString(LAST_SERVER, null)?.let { server.setText(it) }
        val code = dialog.addInput("PAIRING CODE", "ABCD-EFGH, or a token rmm_…")
        // No capitals forced: a pasted token keeps its case; a code is
        // written in capitals with its dash as it is typed.
        code.inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD or
            InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
        code.typeface = SlotUi.font(activity)
        code.addTextChangedListener(PairingCodeFormat())
        val user = dialog.addInput("USERNAME", "Your RomM username")
        val password = dialog.addInput("PASSWORD", "Password (not saved)", password = true)
        // Always the pairing code first, R1 for an account (Siham 2026-11-10).
        var account = false
        fun showMode(toAccount: Boolean) {
            account = toAccount
            dialog.setMessage(if (toAccount) accountText else pairingText)
            dialog.setFieldVisible(code, !toAccount)
            dialog.setFieldVisible(user, toAccount)
            dialog.setFieldVisible(password, toAccount)
            dialog.setError("")
        }
        dialog.setShoulders("PAIRING CODE", "ACCOUNT",
            onLeft = { if (account) { showMode(false); dialog.focusFirst() } },
            onRight = { if (!account) { showMode(true); dialog.focusFirst() } })
        showMode(account)
        val connectButton = dialog.addButton("CONNECT") {
            val address = server.text.toString().trim()
            val pairing = code.text.toString().trim()
            val name = user.text.toString().trim()
            val secret = password.text.toString()
            if (Romm.normalizeServer(address) == null) {
                dialog.setError("Enter your RomM server's address.")
                return@addButton
            }
            if (!account && pairing.isEmpty()) {
                dialog.setError("Enter the pairing code.")
                return@addButton
            }
            if (account && (name.isEmpty() || secret.isEmpty())) {
                dialog.setError("Enter your username and password.")
                return@addButton
            }
            password.text.clear()
            prefs().edit().putString(LAST_SERVER, address).putBoolean(LAST_MODE_ACCOUNT, account).apply()
            dialog.setError("Connecting…")
            dialog.setBusy(true)
            val byAccount = account
            network.execute {
                val result = if (!byAccount) Romm.pair(activity, address, pairing)
                    else Romm.signIn(activity, address, name, secret)
                activity.runOnUiThread {
                    if (activity.isDestroyed) return@runOnUiThread
                    when (result) {
                        is Romm.Result.Connected -> {
                            connected = true
                            dialog.dismiss()
                            SlotToast.makeText(activity,
                                if (result.user.isBlank()) "Connected to RomM."
                                else "Connected to RomM as ${result.user}.",
                                Toast.LENGTH_SHORT).show()
                            load()
                        }
                        is Romm.Result.Failed -> {
                            dialog.setBusy(false)
                            dialog.setError(result.message)
                        }
                    }
                }
            }
        }
        // Enter on the last field connects, rather than going round to the
        // address again (Siham 2026-11-10).
        code.imeOptions = android.view.inputmethod.EditorInfo.IME_ACTION_DONE
        val submit = android.widget.TextView.OnEditorActionListener { _, actionId, event ->
            val enter = actionId == android.view.inputmethod.EditorInfo.IME_ACTION_DONE ||
                (event?.keyCode == android.view.KeyEvent.KEYCODE_ENTER &&
                    event.action == android.view.KeyEvent.ACTION_DOWN)
            if (enter) connectButton.post { if (connectButton.isEnabled) connectButton.performClick() }
            false
        }
        code.setOnEditorActionListener(submit)
        password.setOnEditorActionListener(submit)
        // No CANCEL: B goes back (Siham 2026-11-10).
        dialog.show()
    }

    /** A pairing code as RomM shows it: ABCD-EFGH, the dash put in once the
     *  first four are typed. A token pasted whole (rmm_…) is left as it is. */
    private class PairingCodeFormat : android.text.TextWatcher {
        private var before = 0
        private var busy = false
        override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {
            before = s?.length ?: 0
        }
        override fun onTextChanged(s: CharSequence?, start: Int, beforeCount: Int, count: Int) {}
        override fun afterTextChanged(s: android.text.Editable?) {
            if (s == null || busy) return
            val text = s.toString()
            val shown = Romm.formatPairingCode(text, grew = text.length > before) ?: return
            if (shown != text) {
                busy = true
                s.replace(0, s.length, shown)
                busy = false
            }
        }
    }

    /** X on the page: the account, to sign out; or the form when signed out. */
    private fun server() {
        val account = Romm.account(activity) ?: return connect(closeOnCancel = false)
        val who = if (account.user.isBlank()) "" else " as ${account.user}"
        var signingOut = false
        val dialog = SlotModalDialog(activity, "ROMM", "Connected to ${account.server}$who.",
            onDismiss = {
                activity.nativeExternalReturn()
                if (signingOut) signedOut()
            })
        dialog.addButton("CLOSE") { dialog.dismiss() }
        dialog.addButton("SIGN OUT") {
            Romm.signOut(activity)
            signingOut = true
            dialog.dismiss()
        }
        dialog.show()
    }

    /** Signed out (by X, or the server no longer takes slot.'s token): the
     *  library is forgotten and the form shows; cancelled, back to Library. */
    private fun signedOut() {
        games = emptyMap()
        lastCatalog = null
        synchronized(wanted) { wanted.clear() }
        activity.nativeRommStatus(SIGNED_OUT, "")
        if (showing) connect(closeOnCancel = true)
    }

    // -----------------------------------------------------------------
    // The library

    /** The library: the one already in hand (or kept from last time) shown
     *  at once, then asked of the server again in the background and shown
     *  again only when it changed. Systems are asked three at a time. */
    private fun load() {
        if (games.isEmpty()) activity.nativeRommStatus(LOADING, "Loading your RomM library…")
        // Given again at each opening: the page may have lost it since.
        lastCatalog = null
        if (loading) return
        loading = true
        network.execute {
            try {
                if (games.isEmpty()) {
                    Romm.cachedLibrary(activity)?.let { (kept, keptFavorites) ->
                        games = kept.associateBy { it.id }
                        favorites = keptFavorites
                    }
                }
                val shown = games.values.toList().takeIf { it.isNotEmpty() }?.also { showCatalog(it) } != null
                val consoles = Romm.consoles(activity)
                val counts = IntArray(consoles.size)
                val pool = Executors.newFixedThreadPool(3) { Thread(it, "slot-romm-library") }
                val all = try {
                    val parts = consoles.mapIndexed { i, console ->
                        pool.submit<List<Romm.Game>> {
                            // Page by page: how far it got, all systems together.
                            Romm.games(activity, console) { count ->
                                val total = synchronized(counts) { counts[i] = count; counts.sum() }
                                // Over the list shown, the page stays as it is.
                                if (!shown) status(LOADING, "Loading your RomM library… " +
                                    String.format(java.util.Locale.US, "%,d", total) +
                                    if (total == 1) " game" else " games")
                            }
                        }
                    }
                    val favoritesAsked = pool.submit<Set<Long>?> { Romm.favorites(activity) }
                    // In the server's order of systems, as before.
                    val list = parts.flatMap { part ->
                        try { part.get() } catch (e: java.util.concurrent.ExecutionException) { throw e.cause ?: e }
                    }
                    favorites = try { favoritesAsked.get() } catch (_: Exception) { null }
                    list
                } finally {
                    pool.shutdownNow()
                }
                games = all.associateBy { it.id }
                Romm.storeLibrary(activity, all, favorites)
                showCatalog(all)
            } catch (_: Romm.SignedOut) {
                activity.runOnUiThread { if (!activity.isDestroyed) signedOut() }
            } catch (error: Exception) {
                Log.w(TAG, "RomM library unavailable", error)
                // Over a library shown, a quiet note; else the page says it.
                status(ERROR, "Couldn't load your RomM library. Check your Wi-Fi.")
            } finally {
                loading = false
            }
        }
    }

    /** The page's list, sent only when it changed since the last one sent
     *  (sending it again redraws every cart). */
    private fun showCatalog(all: List<Romm.Game>) {
        val json = JSONArray()
        for (game in all) json.put(entry(game, Romm.cachedHeader(activity, game)))
        val text = json.toString()
        activity.runOnUiThread {
            if (activity.isDestroyed) return@runOnUiThread
            if (all.isEmpty()) {
                lastCatalog = null
                activity.nativeRommStatus(ERROR,
                    "No Game Boy, NES, Super Nintendo or Mega Drive games on this server.")
            } else if (text != lastCatalog) {
                lastCatalog = text
                activity.nativeRommFilter(filterJson())
                activity.nativeRommCatalog(text)
            }
        }
    }

    /** A game as the page reads it (runtime/romm.rs RommEntry). */
    private fun entry(game: Romm.Game, header: Romm.Header?): JSONObject {
        val json = JSONObject()
            .put("id", game.id)
            .put("platform", game.platform)
            .put("title", game.title)
            .put("label", Romm.labelFile(activity, game).path)
            .put("file", game.file)
            .put("size", game.size)
            .put("region", game.regions)
            .put("version", game.revision)
            .put("favorite", favorites?.contains(game.id) == true)
        if (header != null) {
            json.put("code", header.code)
            if (header.gbCgbFlag != null) {
                json.put("gb_title", header.gbTitle)
                    .put("gb_code", header.gbCode)
                    .put("gb_japan", header.gbJapan)
                    .put("gb_cgb_flag", header.gbCgbFlag)
            }
        }
        return json
    }

    // -----------------------------------------------------------------
    // SELECT: the filters

    private fun filterJson(): String {
        val p = prefs()
        return JSONObject()
            .put("search", p.getString(FILTER_SEARCH, "").orEmpty())
            .put("region", p.getString(FILTER_REGION, "").orEmpty())
            .put("official", p.getBoolean(FILTER_OFFICIAL, false))
            .put("hide_shelf", p.getBoolean(FILTER_HIDE_SHELF, false))
            .put("favorites", p.getBoolean(FILTER_FAVORITES, false))
            .toString()
    }

    /** Search (Android's keyboard), Region, Type, the shelf's games and the
     *  RomM favourites; the page shows the result once the form closes. */
    private fun filters() {
        if (form?.isShowing() == true) return
        val p = prefs()
        var region = REGIONS.indexOf(p.getString(FILTER_REGION, "").orEmpty().ifEmpty { "All" }).coerceAtLeast(0)
        var official = p.getBoolean(FILTER_OFFICIAL, false)
        var hideShelf = p.getBoolean(FILTER_HIDE_SHELF, false)
        var onlyFavorites = p.getBoolean(FILTER_FAVORITES, false)
        var cleared = false
        lateinit var search: android.widget.EditText
        // No sentence: every filter fits the KPA without scrolling (Siham 2026-11-10).
        val dialog = SlotModalDialog(activity, "FILTER", "",
            onDismiss = {
                form = null
                val words = if (cleared) "" else search.text.toString().trim()
                prefs().edit()
                    .putString(FILTER_SEARCH, words)
                    .putString(FILTER_REGION, if (cleared || region == 0) "" else REGIONS[region])
                    .putBoolean(FILTER_OFFICIAL, !cleared && official)
                    .putBoolean(FILTER_HIDE_SHELF, !cleared && hideShelf)
                    .putBoolean(FILTER_FAVORITES, !cleared && onlyFavorites)
                    .apply()
                activity.nativeRommFilter(filterJson())
                activity.nativeExternalReturn()
            })
        form = dialog
        search = dialog.addInput("SEARCH", "Type a name, or part of it")
        search.setText(p.getString(FILTER_SEARCH, "").orEmpty())
        dialog.addChoice("REGION", REGIONS, region) { region = it }
        dialog.addChoice("TYPE", listOf("All games", "Official only"), if (official) 1 else 0) { official = it == 1 }
        dialog.addChoice("ON MY SHELF", listOf("Show", "Hide"), if (hideShelf) 1 else 0) { hideShelf = it == 1 }
        dialog.addChoice("FAVORITES", listOf("All games", "RomM favorites only"), if (onlyFavorites) 1 else 0) {
            onlyFavorites = it == 1
            // A token made without collections.read can't see them.
            dialog.setError(if (onlyFavorites && favorites == null)
                "RomM didn't share your favorites. Sign out with X, then sign in again." else "")
        }
        dialog.addButton("CLEAR FILTERS") {
            cleared = true
            dialog.dismiss()
        }
        dialog.setLegend("LEFT RIGHT" to "CHANGE", "B" to "BACK")
        dialog.show()
    }

    private fun status(kind: Int, text: String) = activity.runOnUiThread {
        if (!activity.isDestroyed) activity.nativeRommStatus(kind, text)
    }

    // -----------------------------------------------------------------
    // Labels

    private fun labels(ids: List<Long>) {
        synchronized(wanted) {
            wanted.clear()
            wanted.addAll(ids)
            if (labelling) return
            labelling = true
        }
        labeler.execute {
            while (true) {
                val id = synchronized(wanted) {
                    wanted.removeFirstOrNull() ?: run { labelling = false; null }
                } ?: break
                val game = games[id] ?: continue
                // The Game Boys' shell from their header: a few hundred
                // bytes from the server, read once.
                if (Romm.wantsHeader(game) && game.id !in headersRead) {
                    headersRead.add(game.id)
                    val fresh = Romm.cachedHeader(activity, game) == null
                    val header = try { if (fresh) Romm.fetchHeader(activity, game) else null }
                        catch (error: Exception) {
                            Log.w(TAG, "Header unavailable: ${game.file}", error)
                            if (error !is Romm.SignedOut) headersRead.remove(game.id)
                            null
                        }
                    if (header != null) {
                        val text = entry(game, header).toString()
                        activity.runOnUiThread { if (!activity.isDestroyed) activity.nativeRommHeader(text) }
                    }
                }
                if (Romm.labelFile(activity, game).isFile) continue
                if (Romm.fetchLabel(activity, game)) {
                    activity.runOnUiThread { if (!activity.isDestroyed) activity.nativeRommLabel(id) }
                }
            }
        }
    }

    // -----------------------------------------------------------------
    // Downloads

    /** The ROM folder was chosen: the games waiting for it download now.
     *  True when some were waiting. */
    fun folderChosen(): Boolean {
        val ids = waitingForFolder ?: return false
        waitingForFolder = null
        if (showing) download(ids)
        return true
    }

    /** The folder picker was closed without a choice. */
    fun folderCancelled() {
        waitingForFolder = null
    }

    private fun download(ids: List<Long>) {
        val chosen = ids.mapNotNull { games[it] }
        if (chosen.isEmpty()) {
            activity.nativeRommProgress(1, 1, -1)
            return
        }
        val tree = romRoot()
        if (tree == null) {
            // No ROM folder, or one slot. may only read (chosen before 2.3):
            // chosen again, writable, the download starts by itself.
            activity.nativeRommProgress(chosen.size, chosen.size, -1)
            waitingForFolder = ids
            SlotToast.makeText(activity,
                if (activity.hasRomFolder()) "Choose your ROM folder again to save games."
                else "Choose your ROM folder to save games.",
                Toast.LENGTH_LONG).show()
            activity.chooseRomFolderForRomm()
            return
        }
        activity.nativeRommProgress(0, chosen.size, 0)
        downloader.execute {
            var saved = 0
            var failed = 0
            var out = false
            for ((i, game) in chosen.withIndex()) {
                val ok = try {
                    val file = Romm.download(activity, tree, game) { done, total ->
                        val percent = if (total > 0) (done * 100 / total).toInt().coerceIn(0, 100) else -1
                        activity.runOnUiThread {
                            if (!activity.isDestroyed) activity.nativeRommProgress(i, chosen.size, percent)
                        }
                    }
                    // The label the page shows, for the cart at once.
                    val label = Romm.labelFile(activity, game).takeIf { it.isFile && it.length() > 8 }
                    synchronized(downloaded) {
                        downloaded += file.copy(label = label, labelPlatform = game.platform)
                    }
                    true
                } catch (_: Romm.SignedOut) {
                    out = true
                    activity.runOnUiThread { if (!activity.isDestroyed) signedOut() }
                    break
                } catch (error: Exception) {
                    Log.w(TAG, "Download failed: ${game.file}", error)
                    false
                }
                if (ok) saved++ else failed++
                activity.runOnUiThread {
                    if (!activity.isDestroyed) activity.nativeRommDownloaded(game.id, ok)
                }
            }
            activity.runOnUiThread {
                if (activity.isDestroyed) return@runOnUiThread
                activity.nativeRommProgress(chosen.size, chosen.size, -1)
                if (out && saved == 0) return@runOnUiThread
                val text = when {
                    failed == 0 && saved == 1 -> "1 game downloaded."
                    failed == 0 -> "$saved games downloaded."
                    saved == 0 -> "Download failed. Check your Wi-Fi and the ROM folder."
                    else -> "$saved downloaded, $failed failed."
                }
                SlotToast.makeText(activity, text, Toast.LENGTH_SHORT).show()
                refreshIfLeft()
            }
        }
    }
}
