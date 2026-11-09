package fyi.slot.konkr

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.RectF
import android.util.AtomicFile
import android.util.Log
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import kotlin.math.roundToInt

/**
 * Conservative automatic cartridge label acquisition. Exact ROM CRC first,
 * then conservative No-Intro title + platform + metadata matches from
 * the SAME DAT used in original Cart Studio. No fuzzy artwork guesses.
 * Manual and Studio labels always win.
 *
 * Low priority and bounded: at most 24 new labels per batch, 48 attempts,
 * no work during gameplay or Android background sleep, and no ROM/BIOS writes.
 */
internal object AutoCartLabels {
    private const val TAG = "SlotAutoLabels"
    private const val MAX_INDEX = 12 * 1024 * 1024
    private const val MAX_SOURCE = 6 * 1024 * 1024
    private const val MAX_LABEL = 2 * 1024 * 1024
    // Bigger batches one second apart (Siham 2026-11-04: scraping too slow).
    private const val MAX_ATTEMPTS = 48
    /** Carts whose own labels are fetched side by side. */
    private const val PREFETCH = 12
    private const val NEXT_BATCH_MS = 1_000L
    private val safeArtPath = Regex("[a-zA-Z0-9_./-]+")
    private val pngSignature = byteArrayOf(-119, 80, 78, 71, 13, 10, 26, 10)

    // Only the serial artwork worker mutates this session state. Official
    // negatives never retry; transient failures retry at most four times.
    private val retry = AutoLabelRetryPolicy()
    private const val INDEX_KEY = "@official-art-index"
    private var savedTotal = 0
    @Volatile private var batchDelayMs = NEXT_BATCH_MS

    /** Read only from MainActivity after fill() returns on its single worker. */
    fun nextBatchDelayMs(): Long = batchDelayMs

    private fun reschedule(pending: List<CartStudioCatalog.Rom>,
                           shouldContinue: () -> Boolean): Boolean {
        if (!shouldContinue()) return false
        val remaining = pending.filter { retry.pending(it.uri) }
        if (remaining.isEmpty()) return false
        val now = android.os.SystemClock.elapsedRealtime()
        batchDelayMs = maxOf(NEXT_BATCH_MS,
            retry.nextDelayMs(remaining.map { it.uri }, now))
            .coerceAtMost(180_000L)
        Log.i(TAG, "Pending automatic labels: ${remaining.size}; next pass in ${batchDelayMs}ms")
        return true
    }

    /** slot.'s older art set's index (art.slot-cfw.fyi), now only asked
     *  for the carts our own set doesn't hold; null when it can't be had. */
    private fun loadOfficialIndex(context: Context, shouldContinue: () -> Boolean): JSONObject? {
        fun failed(reason: String): JSONObject? {
            Log.w(TAG, "Official art index temporarily unavailable: $reason")
            return null
        }
        val indexBytes = try {
            val response = CartStudioArtProxy(context).intercept("index.json")
            if (response.statusCode != 200)
                return failed("HTTP ${response.statusCode}")
            response.data.use { input ->
                val output = ByteArrayOutputStream()
                val buffer = ByteArray(16 * 1024)
                while (true) {
                    if (!shouldContinue()) return null
                    val n = input.read(buffer)
                    if (n < 0) break
                    if (output.size() + n > MAX_INDEX) {
                        return failed("index exceeds size limit")
                    }
                    output.write(buffer, 0, n)
                }
                output.toByteArray()
            }
        } catch (error: Exception) {
            Log.w(TAG, "Official art index request failed", error)
            return failed(error.javaClass.simpleName)
        }
        val index = try { JSONObject(String(indexBytes, Charsets.UTF_8)) }
            catch (error: Exception) {
                Log.w(TAG, "Official art index could not be parsed", error)
                return failed("invalid JSON")
            }
        return index
    }

    /** Three low-priority threads: our own set has no quota, and the carousel
     *  keeps the device's attention. */
    private val workers = java.util.concurrent.Executors.newFixedThreadPool(3) { task ->
        Thread({
            android.os.Process.setThreadPriority(android.os.Process.THREAD_PRIORITY_BACKGROUND)
            task.run()
        }, "slot-gb-labels").apply { isDaemon = true }
    }

    /** The carts' label scans from our own set, as PNG: a cart is left out
     *  when it couldn't be asked (the pass asks again), null when the set
     *  doesn't hold one. */
    private fun prefetch(context: Context, roms: List<CartStudioCatalog.Rom>,
                         shouldContinue: () -> Boolean): Map<String, ByteArray?> {
        val jobs = roms.map { rom ->
            rom.uri to workers.submit<Pair<Boolean, ByteArray?>> {
                if (!shouldContinue()) return@submit false to null
                try {
                    val crc = CartStudioCatalog.cached(context, rom)
                        ?: CartStudioCatalog.identify(context, rom)
                    true to SlotArtSet.scanPng(context, rom.platform, crc.hex)
                } catch (error: Exception) {
                    false to null
                }
            }
        }
        val out = HashMap<String, ByteArray?>()
        for ((uri, job) in jobs) {
            val (asked, bytes) = try { job.get() } catch (_: Exception) { false to null }
            if (asked) out[uri] = bytes
        }
        return out
    }

    /** Returns true if more bounded foreground work is pending, including retries. */
    fun fill(context: Context, shouldContinue: () -> Boolean, onSaved: (String) -> Unit): Boolean {
        if (!shouldContinue()) return false
        batchDelayMs = NEXT_BATCH_MS
        // The official GB/GBC/GBA art only: HomeCartLabels fetches the NES,
        // Super Nintendo and Mega Drive labels from ScreenScraper.
        val games = CartStudioCatalog.games(context).filter { it.platform !in Systems.HOME_PLATFORMS }
        if (games.isEmpty()) {
            Log.i(TAG, "Deferred: cached ROM library not ready (zero indexed games)")
            return false
        }
        val missing = games.filter { !CartLabelPaths.file(context.filesDir, it.uri).exists() }
        val pending = missing.filter { retry.pending(it.uri) }
        if (pending.isEmpty()) {
            Log.i(TAG, "No pending official art: games=${games.size}, missing=${missing.size}, savedTotal=$savedTotal")
            return false
        }
        var checked = games.count { !retry.pending(it.uri) }
        AutoCartLabelDiagnostics.start(checked, checked + pending.size, savedTotal)

        val now = android.os.SystemClock.elapsedRealtime()
        val due = pending.filter { retry.ready(it.uri, now) }
        if (due.isEmpty()) return reschedule(pending, shouldContinue)

        // Our own art set (art.slot-android.app) answers first. slot.'s older
        // set is a fallback: its index is fetched the first time a cart isn't
        // in ours, so an outage there never holds back the labels we have.
        var indexTried = false
        var cachedIndex: JSONObject? = null
        fun officialIndex(): JSONObject? {
            if (indexTried) return cachedIndex
            indexTried = true
            if (!retry.ready(INDEX_KEY, android.os.SystemClock.elapsedRealtime())) return null
            val loaded = loadOfficialIndex(context, shouldContinue)
            cachedIndex = loaded
            if (loaded != null) {
                retry.reset(INDEX_KEY)
                Log.i(TAG, "Official art index ready: CRC entries=${loaded.length()}")
            } else if (shouldContinue()) {
                retry.transient(INDEX_KEY, android.os.SystemClock.elapsedRealtime())
            }
            return loaded
        }

        val art = CartStudioArtProxy(context)
        val dats = HashMap<String, OfficialArtMatcher.Catalog?>()
        var attempted = 0
        var saved = 0
        var fetched = 0
        var notFound = 0
        var errors = 0
        // Only the carts our own set doesn't hold are bounded per pass: it has
        // no quota, so every label it holds comes in one pass.
        val pre = HashMap<String, ByteArray?>()
        var prefetchedUntil = 0
        for ((i, rom) in due.withIndex()) {
            if (!shouldContinue() || attempted >= MAX_ATTEMPTS) break
            val target = CartLabelPaths.file(context.filesDir, rom.uri)
            if (target.exists()) {
                retry.finished(rom.uri)
                continue
            }
            if (i >= prefetchedUntil) {
                // The next few carts' labels from our own set, side by side.
                prefetchedUntil = minOf(due.size, i + PREFETCH)
                pre.clear()
                pre.putAll(prefetch(context, due.subList(i, prefetchedUntil), shouldContinue))
                if (!shouldContinue()) break
            }
            var outcome = "NOT_FOUND"
            try {
                val crc = CartStudioCatalog.cached(context, rom)
                    ?: CartStudioCatalog.identify(context, rom)
                if (!shouldContinue()) {
                    outcome = "INTERRUPTED"
                    break
                }
                // slot. for Android's own art set first (art.slot-android.app),
                // then slot.'s, matched as before.
                var bytes: ByteArray? = if (pre.containsKey(rom.uri)) pre[rom.uri] else try {
                    SlotArtSet.scanPng(context, rom.platform, crc.hex)
                } catch (error: Exception) {
                    Log.w(TAG, "Own art set lookup failed; trying slot.'s", error)
                    null
                }
                if (bytes == null) {
                    attempted++
                    val index = officialIndex()
                    if (index == null) {
                        outcome = if (shouldContinue()) "NETWORK_ERROR" else "INTERRUPTED"
                        continue
                    }
                    val direct = OfficialArtResolver.artPath(index, rom.platform, crc.hex)
                    // Kotlin getOrPut() re-invokes the initializer when it returns
                    // null. Preserve null failures within THIS pass explicitly,
                    // to avoid repeated 9s+12s DAT timeouts on each of 16 carts.
                    if (direct == null && !dats.containsKey(rom.platform)) {
                        dats[rom.platform] = OfficialDatSource.load(
                            context, rom.platform, shouldContinue)
                    }
                    if (!shouldContinue()) {
                        outcome = "INTERRUPTED"
                        break
                    }
                    val dat = if (direct != null) null else dats[rom.platform]
                    if (direct == null && dat == null) {
                        outcome = "DAT_UNAVAILABLE"
                        continue
                    }
                    val resolved = OfficialArtResolver.match(rom, crc, index, dat)
                    val path = resolved?.let {
                        OfficialArtResolver.artPath(index, rom.platform, it)
                    } ?: continue // Verified official no-match: session-terminal.
                    val response = art.intercept(path)
                    if (response.statusCode != 200) {
                        outcome = when (response.statusCode) {
                            404 -> "NOT_FOUND"
                            else -> "NETWORK_ERROR"
                        }
                        errors++
                        continue
                    }
                    outcome = "NETWORK_ERROR" // Partial reads are retryable.
                    bytes = response.data.use { input ->
                        val out = ByteArrayOutputStream()
                        val buffer = ByteArray(16 * 1024)
                        while (true) {
                            if (!shouldContinue()) break
                            val n = input.read(buffer)
                            if (n < 0) break
                            if (out.size() + n > MAX_SOURCE) break
                            out.write(buffer, 0, n)
                        }
                        out.toByteArray()
                    }
                    if (!shouldContinue()) {
                        outcome = "INTERRUPTED"
                        break
                    }
                }
                fetched++
                if (bytes == null || bytes.size !in 8..MAX_SOURCE ||
                    !bytes.copyOfRange(0, 8).contentEquals(pngSignature)) {
                    outcome = "INVALID_ARTWORK"
                    continue
                }
                val label = cropPhysicalCartLabel(bytes, rom.platform)
                if (label == null) {
                    outcome = "INVALID_ARTWORK"
                    continue
                }
                if (label.size > MAX_LABEL) {
                    outcome = "INVALID_ARTWORK"
                    continue
                }
                if (target.exists()) {
                    outcome = "EXISTS"
                    continue
                }
                if (!shouldContinue()) {
                    outcome = "INTERRUPTED"
                    break
                }
                target.parentFile?.mkdirs()
                if (target.exists()) {
                    outcome = "EXISTS"
                    continue
                }
                val atomic = AtomicFile(target)
                val stream = atomic.startWrite()
                try {
                    stream.write(label)
                    atomic.finishWrite(stream)
                } catch (error: Exception) {
                    atomic.failWrite(stream)
                    throw error
                }
                saved++
                savedTotal++
                outcome = "SAVED"
                onSaved(rom.uri)
            } catch (error: Exception) {
                if (outcome != "SAVED") outcome = "ERROR"
                errors++
                Log.w(TAG, "Auto-art deferred for ${rom.title.take(80)}", error)
            } finally {
                val wasPending = retry.pending(rom.uri)
                // A real stored label always wins, including when the user
                // navigated away just after a successful atomic write.
                if (target.exists()) retry.finished(rom.uri)
                else if (shouldContinue()) {
                    when (outcome) {
                        "NOT_FOUND", "INVALID_ARTWORK", "EXISTS" ->
                            retry.finished(rom.uri)
                        "DAT_UNAVAILABLE", "NETWORK_ERROR", "ERROR" -> {
                            val scheduled = retry.transient(
                                rom.uri, android.os.SystemClock.elapsedRealtime())
                            Log.i(TAG, "ART_RETRY ${rom.title.take(72)}: " +
                                if (scheduled) "scheduled (attempt ${retry.failedAttempts(rom.uri)}/" +
                                    "${AutoLabelRetryPolicy.MAX_ATTEMPTS})"
                                else "exhausted for this app process")
                        }
                        // Interrupted work never burns a retry allowance.
                    }
                }
                if (wasPending && !retry.pending(rom.uri)) checked++
                AutoCartLabelDiagnostics.result(checked, savedTotal, outcome)
                if (outcome == "NOT_FOUND") notFound++
                Log.i(TAG, "ART_RESULT $outcome [${rom.platform}] ${rom.title.take(96)}")
            }
        }
        try { CartStudioCatalog.flush(context) }
        catch (error: Exception) { Log.w(TAG, "Could not persist cartridge checksums", error) }

        Log.i(TAG, "Art pass: attempted=$attempted, saved=$saved, fetched=$fetched, " +
            "no_official_match=$notFound, retryable_errors=$errors")
        return reschedule(pending, shouldContinue)
    }

    /** Cart Studio's pictures for a Game Boy family cart: the real label as
     *  the automatic labels save it (the own art set first, slot.'s second)
     *  and the art set's logo. Either is null when none is found.
     *  The dump is matched as the automatic labels match it: by its CRC, then
     *  by its name in the No-Intro list when the CRC is unknown (a revision,
     *  a patched or a slightly different dump), and that game's logo is the
     *  one shown (Siham 2026-11-09: Advance Wars had a label but neither). */
    internal fun studioArt(context: Context, rom: CartStudioCatalog.Rom): Pair<ByteArray?, ByteArray?> {
        val crc = CartStudioCatalog.cached(context, rom) ?: CartStudioCatalog.identify(context, rom)
        val proxy = CartStudioArtProxy(context)
        val index: JSONObject? = try {
            val indexed = proxy.intercept("index.json")
            if (indexed.statusCode == 200)
                JSONObject(String(indexed.data.use { it.readBytes() }, Charsets.UTF_8)) else null
        } catch (error: Exception) {
            Log.w(TAG, "slot.'s art set index unavailable for Cart Studio", error); null
        }
        fun ownScan(dump: String): ByteArray? = try {
            SlotArtSet.scanPng(context, rom.platform, dump)
        } catch (error: Exception) {
            Log.w(TAG, "Own art set lookup failed for Cart Studio", error); null
        }
        fun theirScan(dump: String): ByteArray? = try {
            index?.let { OfficialArtResolver.artPath(it, rom.platform, dump) }?.let { path ->
                val image = proxy.intercept(path)
                if (image.statusCode == 200) image.data.use { it.readBytes() } else null
            }
        } catch (error: Exception) {
            Log.w(TAG, "slot.'s art set lookup failed for Cart Studio", error); null
        }
        fun hasArt(dump: String): Boolean = try {
            SlotArtSet.entries(context, rom.platform, dump)?.any { it.type == "support-texture" } == true ||
                index?.let { OfficialArtResolver.artPath(it, rom.platform, dump) } != null
        } catch (_: Exception) { false }
        // The CRC first, then the name, as the shelf's labels.
        var dump = crc.hex
        var scan = ownScan(dump) ?: theirScan(dump)
        if (scan == null) {
            val named = try {
                OfficialDatSource.load(context, rom.platform) { true }?.let { dat ->
                    OfficialArtMatcher.fromCatalog(rom.platform, rom.title, crc.head, dat) { candidate ->
                        candidate.takeIf(::hasArt)
                    }
                }
            } catch (error: Exception) {
                Log.w(TAG, "No-Intro name match failed for Cart Studio", error); null
            }
            if (named != null) {
                dump = named
                scan = ownScan(named) ?: theirScan(named)
            }
        }
        val real = scan?.takeIf { it.size >= 8 && it.copyOfRange(0, 8).contentEquals(pngSignature) }
            ?.let { cropPhysicalCartLabel(it, rom.platform) }
        val logo = try {
            listOf(crc.hex, dump).distinct().firstNotNullOfOrNull { candidate ->
                SlotArtSet.entries(context, rom.platform, candidate)?.firstOrNull { it.type == "wheel" }
            }?.let { SlotArtSet.file(context, it) }?.let(SlotArtSet::toPng)
        } catch (error: Exception) {
            Log.w(TAG, "Logo lookup failed for Cart Studio", error); null
        }
        return real to logo
    }

    internal fun validArtworkPath(path: String, platform: String): Boolean =
        path.isNotBlank() && path.length <= 150 &&
            safeArtPath.matches(path) && !path.startsWith("/") &&
            !path.split('/').contains("..") && path.endsWith(".png") &&
            path.startsWith("$platform/")

    /**
     * Mirror the original Cart Studio Label.full: center-cover the *complete*
     * official support-texture onto the platform's label well, and composite
     * transparency over black. The source is already the label media: the
     * earlier hard-coded "physical cartridge scan" aspect checks and 137..865
     * percent crop rejected valid images and cut off real label artwork.
     *
     * Saved at up to 3x the label well (3x panels) within the bounded decode
     * budget: no custom/Studio label or ROM bytes are changed.
     */
    internal fun cropPhysicalCartLabel(bytes: ByteArray, platform: String): ByteArray? {
        val opts = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, opts)
        val w = opts.outWidth
        val h = opts.outHeight
        if (w !in 64..4096 || h !in 64..4096 ||
            w.toLong() * h.toLong() > 12_000_000L) {
            Log.i(TAG, "Unsupported official source dimensions: ${w}x${h} for $platform")
            return null
        }
        val source = BitmapFactory.decodeByteArray(bytes, 0, bytes.size) ?: run {
            Log.i(TAG, "Official source PNG could not be decoded: ${w}x${h}")
            return null
        }
        return try {
            val (lw, lh) = when (platform) {
                "GBA" -> 264 to 136
                "GB" -> 220 to 195
                "GBC" -> 223 to 199
                else -> return null
            }
            // Kept at up to three times the label's 1x size, so it stays
            // sharp on 3x panels; never enlarged past the source's own pixels.
            val k = minOf(w.toFloat() / lw, h.toFloat() / lh).coerceIn(1f, 3f)
            val tw = (lw * k).roundToInt()
            val th = (lh * k).roundToInt()
            val coverScale = maxOf(tw.toFloat() / w, th.toFloat() / h)
            val dw = w * coverScale
            val dh = h * coverScale
            val left = (tw - dw) / 2f
            val top = (th - dh) / 2f
            val result = Bitmap.createBitmap(tw, th, Bitmap.Config.ARGB_8888)
            try {
                val canvas = Canvas(result)
                canvas.drawColor(Color.BLACK)
                canvas.drawBitmap(source, null, RectF(left, top, left + dw, top + dh),
                    Paint(Paint.FILTER_BITMAP_FLAG))
                val out = ByteArrayOutputStream()
                if (!result.compress(Bitmap.CompressFormat.PNG, 100, out)) null
                else out.toByteArray()
            } finally {
                result.recycle()
            }
        } finally {
            source.recycle()
        }
    }
}
