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
        SlotLog.i(TAG, "Pending automatic labels: ${remaining.size}; next pass in ${batchDelayMs}ms")
        return true
    }

    /** slot.'s older art set's index (art.slot-cfw.fyi), now only asked
     *  for the carts our own set doesn't hold; null when it can't be had. */
    private fun loadOfficialIndex(context: Context, shouldContinue: () -> Boolean): JSONObject? {
        fun failed(reason: String): JSONObject? {
            SlotLog.w(TAG, "Official art index temporarily unavailable: $reason")
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
            SlotLog.w(TAG, "Official art index request failed", error)
            return failed(error.javaClass.simpleName)
        }
        val index = try { JSONObject(String(indexBytes, Charsets.UTF_8)) }
            catch (error: Exception) {
                SlotLog.w(TAG, "Official art index could not be parsed", error)
                return failed("invalid JSON")
            }
        return index
    }

    /** Four low-priority threads: our own set has no quota, and the carousel
     *  keeps the device's attention. */
    private val workers = java.util.concurrent.Executors.newFixedThreadPool(4) { task ->
        Thread({
            android.os.Process.setThreadPriority(android.os.Process.THREAD_PRIORITY_BACKGROUND)
            task.run()
        }, "slot-gb-labels").apply { isDaemon = true }
    }

    /** A cart's label scan from our own set: by its CRC, else by its name
     *  (ArtNames: a hack, a translation, another region's dump). */
    private data class ArtIdentity(val platform: String, val crc: String)
    private data class OwnArtwork(val identity: ArtIdentity, val bytes: ByteArray)

    private fun artworkTitle(rom: CartStudioCatalog.Rom): String =
        ArtNames.stem(rom.filename.ifBlank { rom.title })

    private fun ownArtwork(context: Context, rom: CartStudioCatalog.Rom, crc: String,
                           shouldContinue: () -> Boolean): OwnArtwork? {
        val tried = HashSet<ArtIdentity>()
        fun scan(platform: String, dump: String): OwnArtwork? {
            if (!shouldContinue()) return null
            val identity = ArtIdentity(platform, dump)
            if (!tried.add(identity)) return null
            return SlotArtSet.scanPng(context, platform, dump)?.let { OwnArtwork(identity, it) }
        }
        scan(rom.platform, crc)?.let { return it }
        val alternate = GbFamilyArtwork.other(rom.platform)
        if (alternate != null) {
            scan(alternate, crc)?.let { return it }
            val title = artworkTitle(rom)
            for ((platform, name) in GbFamilyArtwork.names(rom.platform, title)) {
                if (!shouldContinue()) return null
                ArtNames.exactCrcFor(context, platform, name)?.let { named ->
                    scan(platform, named)?.let { return it }
                }
            }
            // For colour hacks a broad prefix could turn Land 2 into Land 1.
            if (GbFamilyArtwork.originalTitle(title) != null) return null
        }
        if (!shouldContinue()) return null
        return ArtNames.crcFor(context, rom.platform, rom.filename.ifBlank { rom.title })
            ?.let { scan(rom.platform, it) }
    }

    private fun ownScan(context: Context, rom: CartStudioCatalog.Rom, crc: String,
                        shouldContinue: () -> Boolean): ByteArray? =
        ownArtwork(context, rom, crc, shouldContinue)?.bytes

    /** The older art set has one index but separate DATs. Preserve its strict
     *  matching, and only try the other Game Boy family catalogue on a miss. */
    private fun officialMatch(rom: CartStudioCatalog.Rom, crc: CartStudioCatalog.Fingerprint,
                              index: JSONObject, catalog: (String) -> OfficialArtMatcher.Catalog?,
                              shouldContinue: () -> Boolean): Pair<ArtIdentity?, Boolean> {
        val platforms = listOfNotNull(rom.platform, GbFamilyArtwork.other(rom.platform))
        for (platform in platforms) {
            if (!shouldContinue()) return null to false
            if (OfficialArtResolver.artPath(index, platform, crc.hex) != null)
                return ArtIdentity(platform, crc.hex) to false
        }
        var unavailable = false
        val titles = if (platforms.size == 1) listOf(rom.platform to rom.title)
            else GbFamilyArtwork.names(rom.platform, artworkTitle(rom))
        for ((platform, name) in titles) {
            if (!shouldContinue()) return null to unavailable
            val dat = catalog(platform)
            if (dat == null) { unavailable = true; continue }
            val dump = OfficialArtMatcher.fromCatalog(platform, name, crc.head, dat) {
                OfficialArtResolver.artPath(index, platform, it)
            }
            if (dump != null) return ArtIdentity(platform, dump) to false
        }
        return null to unavailable
    }

    /** The carts' label scans from our own set, as PNG: a cart is left out
     *  when it couldn't be asked (the pass asks again), null when the set
     *  doesn't hold one. */
    private fun prefetch(context: Context, roms: List<CartStudioCatalog.Rom>,
                         shouldContinue: () -> Boolean, ownOnly: Boolean): Map<String, java.util.concurrent.Future<Pair<Boolean, ByteArray?>>> {
        return roms.associate { rom ->
            rom.uri to workers.submit<Pair<Boolean, ByteArray?>> {
                if (!shouldContinue()) return@submit false to null
                try {
                    val crc = CartStudioCatalog.cached(context, rom)
                        ?: CartStudioCatalog.identify(context, rom)
                    val shelves = listOfNotNull(rom.platform, GbFamilyArtwork.other(rom.platform))
                    val bytes = if (!ownOnly && rom in ownMisses && SlotArtSet.hasFreshIndexes(context, shelves)) null
                    else ownScan(context, rom, crc.hex, shouldContinue).also {
                        if (ownOnly && it == null && shouldContinue() && SlotArtSet.hasFreshIndexes(context, shelves)) ownMisses.add(rom)
                    }
                    true to bytes
                } catch (error: Exception) {
                    false to null
                }
            }
        }
    }

    /** Games with no official label (hacks, homebrew), remembered between
     *  runs for a week: "2/2 labels, 0 saved" no longer showed at every
     *  start (beta38). After a week, a newer art set may have them. */
    private const val MISS_TTL_MS = PortableArtwork.MISS_TTL_MS
    private val ownMisses = java.util.concurrent.ConcurrentHashMap.newKeySet<CartStudioCatalog.Rom>()
    fun resetForRefresh() { retry.resetAll(); ownMisses.clear() }
    fun clearOwnPass() { ownMisses.clear() }
    private var missesLoaded = false

    private fun missFile(context: Context) = java.io.File(context.filesDir, "AutoLabels/no-official-label.txt")

    private fun rememberedMisses(context: Context, games: List<CartStudioCatalog.Rom>) {
        if (missesLoaded) return
        missesLoaded = true
        val oldest = System.currentTimeMillis() - MISS_TTL_MS
        try {
            val byUri = games.associateBy { it.uri }
            missFile(context).takeIf { it.isFile }?.readLines()?.forEach { line ->
                val at = line.substringBefore('\t').toLongOrNull() ?: return@forEach
                val rom = byUri[line.substringAfter('\t')] ?: return@forEach
                if (GbFamilyArtwork.other(rom.platform) == null && at >= oldest &&
                    PortableArtwork.read(context, "gb-miss", rom.uri) == null) {
                    PortableArtwork.write(context, "gb-miss", rom.uri,
                        PortableArtwork.gbMiss(rom.size, rom.modified, at))
                }
            }
        } catch (_: Exception) {}
    }

    private fun rememberMiss(context: Context, rom: CartStudioCatalog.Rom) {
        PortableArtwork.write(context, GbFamilyArtwork.missKind(rom.platform), rom.uri,
            PortableArtwork.gbMiss(rom.size, rom.modified, System.currentTimeMillis()))
    }

    /** Returns true if more bounded foreground work is pending, including retries. */
    fun fill(context: Context, shouldContinue: () -> Boolean, onSaved: (String) -> Unit,
             ownOnly: Boolean = false): Boolean {
        if (!shouldContinue()) return false
        batchDelayMs = NEXT_BATCH_MS
        // The official GB/GBC/GBA art only: HomeCartLabels fetches the NES,
        // Super Nintendo and Mega Drive labels from ScreenScraper.
        val games = ArtworkPriority.order(CartStudioCatalog.games(context)
            .filter { it.platform !in Systems.HOME_PLATFORMS }, ArtworkPriority.focus(context),
            { it.uri }, { it.platform }, { it.title })
        if (games.isEmpty()) {
            SlotLog.i(TAG, "Deferred: cached ROM library not ready (zero indexed games)")
            return false
        }
        rememberedMisses(context, games)
        val nowWall = System.currentTimeMillis()
        val since = PortableArtwork.refreshEpoch(context)
        retry.expireMisses(nowWall, MISS_TTL_MS)
        val missing = games.filter {
            !CartLabelPaths.file(context.filesDir, it.uri).exists() &&
                !PortableArtwork.validGbMiss(PortableArtwork.read(context, GbFamilyArtwork.missKind(it.platform), it.uri),
                    it.size, it.modified, nowWall, MISS_TTL_MS, since)
        }
        val pending = missing.filter { retry.pending(it.uri) }
        if (pending.isEmpty()) {
            SlotLog.i(TAG, "No pending official art: games=${games.size}, missing=${missing.size}, savedTotal=$savedTotal")
            return false
        }
        var checked = games.size - pending.size
        AutoCartLabelDiagnostics.start(checked, checked + pending.size, savedTotal)

        val now = android.os.SystemClock.elapsedRealtime()
        val due = pending.filter { retry.ready(it.uri, now) }
        if (due.isEmpty()) return !ownOnly && reschedule(pending, shouldContinue)

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
                SlotLog.i(TAG, "Official art index ready: CRC entries=${loaded.length()}")
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
        val pre = HashMap<String, java.util.concurrent.Future<Pair<Boolean, ByteArray?>>>()
        var prefetchedUntil = 0
        try {
        for ((i, rom) in due.withIndex()) {
            if (!shouldContinue() || (!ownOnly && attempted >= MAX_ATTEMPTS)) break
            // Sliding bounded window: submit the next cart as a result is
            // consumed, without waiting for every worker in a batch.
            val end = minOf(due.size, i + 8)
            if (end > prefetchedUntil) {
                pre.putAll(prefetch(context, due.subList(prefetchedUntil, end), shouldContinue, ownOnly))
                prefetchedUntil = end
            }
            val job = pre.remove(rom.uri)
            val target = CartLabelPaths.file(context.filesDir, rom.uri)
            if (target.exists()) {
                job?.cancel(true)
                retry.finished(rom.uri)
                continue
            }
            val early = try { job?.get() } catch (_: Exception) { null }
            if (!shouldContinue()) break
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
                var bytes: ByteArray? = if (early?.first == true) early.second else try {
                    ownScan(context, rom, crc.hex, shouldContinue)
                } catch (error: Exception) {
                    SlotLog.w(TAG, "Own art set lookup failed; trying slot.'s", error)
                    null
                }
                if (bytes == null) {
                    if (ownOnly) { outcome = "DEFERRED"; continue }
                    attempted++
                    val index = officialIndex()
                    if (index == null) {
                        outcome = if (shouldContinue()) "NETWORK_ERROR" else "INTERRUPTED"
                        continue
                    }
                    // Remember null DAT failures too, avoiding repeated timeouts
                    // across this batch even when both GB and GBC are needed.
                    val (resolved, unavailable) = officialMatch(rom, crc, index, { platform ->
                        if (!dats.containsKey(platform))
                            dats[platform] = OfficialDatSource.load(context, platform, shouldContinue)
                        dats[platform]
                    }, shouldContinue)
                    if (!shouldContinue()) {
                        outcome = "INTERRUPTED"
                        break
                    }
                    if (resolved == null && unavailable) {
                        outcome = "DAT_UNAVAILABLE"
                        continue
                    }
                    val path = resolved?.let {
                        OfficialArtResolver.artPath(index, it.platform, it.crc)
                    } ?: continue
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
                SlotLog.w(TAG, "Auto-art deferred for ${rom.title.take(80)}", error)
            } finally {
                val wasPending = retry.pending(rom.uri)
                // A real stored label always wins, including when the user
                // navigated away just after a successful atomic write.
                if (target.exists()) retry.finished(rom.uri)
                else if (!ownOnly && shouldContinue()) {
                    when (outcome) {
                        "NOT_FOUND", "INVALID_ARTWORK" -> {
                            if (SlotArtSet.hasFreshIndexes(context, listOfNotNull(rom.platform, GbFamilyArtwork.other(rom.platform)))) {
                                retry.missed(rom.uri, System.currentTimeMillis())
                                rememberMiss(context, rom)
                            } else retry.transient(rom.uri, android.os.SystemClock.elapsedRealtime())
                        }
                        "EXISTS" -> retry.finished(rom.uri)
                        "DAT_UNAVAILABLE", "NETWORK_ERROR", "ERROR" -> {
                            val scheduled = retry.transient(
                                rom.uri, android.os.SystemClock.elapsedRealtime())
                            SlotLog.i(TAG, "ART_RETRY ${rom.title.take(72)}: " +
                                if (scheduled) "scheduled (attempt ${retry.failedAttempts(rom.uri)}/" +
                                    "${AutoLabelRetryPolicy.MAX_ATTEMPTS})"
                                else "exhausted for this app process")
                        }
                        // Interrupted work never burns a retry allowance.
                    }
                }
                if (wasPending && !retry.pending(rom.uri)) checked++
                AutoCartLabelDiagnostics.result(checked, savedTotal,
                    if (ownOnly && outcome != "SAVED" && outcome != "EXISTS") "DEFERRED" else outcome)
                if (outcome == "NOT_FOUND") notFound++
                SlotLog.i(TAG, "ART_RESULT $outcome [${rom.platform}] ${rom.title.take(96)}")
            }
        }
        } finally { pre.values.forEach { it.cancel(true) } }
        try { CartStudioCatalog.flush(context) }
        catch (error: Exception) { SlotLog.w(TAG, "Could not persist cartridge checksums", error) }

        SlotLog.i(TAG, "Art pass: attempted=$attempted, saved=$saved, fetched=$fetched, " +
            "no_official_match=$notFound, retryable_errors=$errors")
        return !ownOnly && reschedule(pending, shouldContinue)
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
            SlotLog.w(TAG, "slot.'s art set index unavailable for Cart Studio", error); null
        }
        if (GbFamilyArtwork.other(rom.platform) != null) {
            val own = try { ownArtwork(context, rom, crc.hex) { true } }
                catch (error: Exception) {
                    SlotLog.w(TAG, "Own artwork lookup failed for Cart Studio", error); null
                }
            var identity = own?.identity
            var bytes = own?.bytes
            if (bytes == null && index != null) {
                try {
                    val dats = HashMap<String, OfficialArtMatcher.Catalog?>()
                    identity = officialMatch(rom, crc, index, { platform ->
                        if (!dats.containsKey(platform))
                            dats[platform] = OfficialDatSource.load(context, platform) { true }
                        dats[platform]
                    }, { true }).first
                    bytes = identity?.let { hit ->
                        OfficialArtResolver.artPath(index, hit.platform, hit.crc)?.let { path ->
                            val response = proxy.intercept(path)
                            if (response.statusCode == 200) response.data.use { it.readBytes() } else null
                        }
                    }
                } catch (error: Exception) {
                    SlotLog.w(TAG, "GB family artwork deferred for Cart Studio", error)
                }
            }
            val real = bytes?.takeIf { it.size >= 8 && it.copyOfRange(0, 8).contentEquals(pngSignature) }
                ?.let { cropPhysicalCartLabel(it, rom.platform) }
            val logo = try {
                listOfNotNull(identity, ArtIdentity(rom.platform, crc.hex)).distinct()
                    .firstNotNullOfOrNull { hit ->
                        SlotArtSet.entries(context, hit.platform, hit.crc)?.firstOrNull { it.type == "wheel" }
                    }?.let { SlotArtSet.file(context, it) }?.let(SlotArtSet::toPng)
            } catch (error: Exception) {
                SlotLog.w(TAG, "GB family logo lookup failed", error); null
            }
            return real to logo
        }
        fun ownScan(dump: String): ByteArray? = try {
            SlotArtSet.scanPng(context, rom.platform, dump)
        } catch (error: Exception) {
            SlotLog.w(TAG, "Own art set lookup failed for Cart Studio", error); null
        }
        fun theirScan(dump: String): ByteArray? = try {
            index?.let { OfficialArtResolver.artPath(it, rom.platform, dump) }?.let { path ->
                val image = proxy.intercept(path)
                if (image.statusCode == 200) image.data.use { it.readBytes() } else null
            }
        } catch (error: Exception) {
            SlotLog.w(TAG, "slot.'s art set lookup failed for Cart Studio", error); null
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
                SlotLog.w(TAG, "No-Intro name match failed for Cart Studio", error); null
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
            SlotLog.w(TAG, "Logo lookup failed for Cart Studio", error); null
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
            SlotLog.i(TAG, "Unsupported official source dimensions: ${w}x${h} for $platform")
            return null
        }
        val source = BitmapFactory.decodeByteArray(bytes, 0, bytes.size) ?: run {
            SlotLog.i(TAG, "Official source PNG could not be decoded: ${w}x${h}")
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
