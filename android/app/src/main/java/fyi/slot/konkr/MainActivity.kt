package fyi.slot.konkr

import android.app.Activity
import android.content.ContentValues
import android.content.Context
import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioTrack
import android.media.AudioTimestamp
import android.media.PlaybackParams
import android.os.Environment
import android.os.Handler
import android.os.Looper
import android.os.Build
import android.os.SystemClock
import android.os.BatteryManager
import android.os.VibrationEffect
import android.os.Vibrator
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.content.IntentFilter
import android.provider.MediaStore
import android.util.AtomicFile
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import android.content.Intent
import android.content.pm.ActivityInfo
import android.content.ActivityNotFoundException
import android.graphics.Color
import android.graphics.Bitmap
import android.widget.ImageView
import java.nio.ByteBuffer
import java.nio.ByteOrder
import android.graphics.BitmapFactory
import java.io.ByteArrayOutputStream
import android.view.Gravity
import android.widget.FrameLayout
import android.widget.TextView
import android.widget.Toast
import android.widget.EditText
import android.net.Uri
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.wifi.WifiManager
import java.net.Inet4Address
import org.json.JSONObject
import org.json.JSONArray
import java.util.concurrent.ConcurrentHashMap
import android.provider.DocumentsContract
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicInteger
import android.opengl.GLES20
import android.opengl.GLSurfaceView
import android.view.Choreographer
import android.os.Bundle
import android.util.Log
import android.hardware.input.InputManager
import android.view.InputDevice
import android.view.InputEvent
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.View
import android.view.WindowManager
import javax.microedition.khronos.egl.EGLConfig
import javax.microedition.khronos.opengles.GL10

/** Android owns EGL, lifecycle and physical input; Rust owns Slot rendering. */
class MainActivity : Activity() {
    // dp/sp follow the screen height so Android screens keep the KONKR's look.
    override fun attachBaseContext(newBase: Context) {
        super.attachBaseContext(newBase)
        SlotUi.panelDensityOverride(newBase)?.let(::applyOverrideConfiguration)
    }

    companion object {
        init { System.loadLibrary("slot_android") }
        private const val TAG = "SlotKonkr"
        // Same bits as slot_retro::sensors.
        private const val SENSOR_ACCELEROMETER = 1
        private const val SENSOR_GYROSCOPE = 2
        private const val SENSOR_LIGHT = 4
        private const val FOLDER_REQUEST = 4701
        private const val BIOS_REQUEST = 4702
        private const val SAVE_REQUEST = 4703
        private const val STATE_REQUEST = 4704
        private const val THEME_REQUEST = 4705
        private const val WALLPAPER_REQUEST = 4706
        private val WALLPAPER_TARGET_NAME = Regex("[A-Za-z]{1,16}\\.png")
        private const val LABEL_REQUEST = 4707
        private const val STUDIO_PICTURE_REQUEST = 4712
        private const val PREFS = "slot_konkr_library"
        internal const val PREFS_NAME = PREFS
        private const val ROM_ROOT = "rom_root_uri"
        /** RomM's games reach the shelf in groups this far apart. */
        private const val SILENT_PUBLISH_MS = 2_500L
        /** Labels handed to the shelf per UI pulse (it draws one a frame). */
        private const val AUTO_LABELS_PER_PULSE = 32
        private const val BIOS_ROOT = "bios_root_uri"
        private const val SAVE_ROOT = "saves_root_uri"
        private const val SHARE_GBA_SAVES = "share_gba_saves"
        private const val STATE_ROOT = "states_root_uri"
        private const val ROM_CACHE_NAME = "rom-library-cache-v1.json"
        private const val ROM_CACHE_SCHEMA = 1
        private const val ROM_CACHE_MAX_BYTES = RomLibrary.CACHE_MAX_BYTES
        private val BUTTONS = setOf(
            KeyEvent.KEYCODE_DPAD_LEFT, KeyEvent.KEYCODE_DPAD_RIGHT,
            KeyEvent.KEYCODE_DPAD_UP, KeyEvent.KEYCODE_DPAD_DOWN,
            KeyEvent.KEYCODE_BUTTON_L1, KeyEvent.KEYCODE_BUTTON_R1,
            KeyEvent.KEYCODE_BUTTON_L2, KeyEvent.KEYCODE_BUTTON_R2,
            KeyEvent.KEYCODE_BUTTON_A, KeyEvent.KEYCODE_BUTTON_B,
            KeyEvent.KEYCODE_BUTTON_X, KeyEvent.KEYCODE_BUTTON_Y,
            KeyEvent.KEYCODE_BUTTON_START, KeyEvent.KEYCODE_BUTTON_SELECT,
            KeyEvent.KEYCODE_BUTTON_MODE,
            // Stick clicks: Boktai's sun, like mGBA's R3 / L3.
            KeyEvent.KEYCODE_BUTTON_THUMBL, KeyEvent.KEYCODE_BUTTON_THUMBR
        )
        private val DPAD_KEYS = setOf(
            KeyEvent.KEYCODE_DPAD_LEFT, KeyEvent.KEYCODE_DPAD_RIGHT,
            KeyEvent.KEYCODE_DPAD_UP, KeyEvent.KEYCODE_DPAD_DOWN
        )
    }

    external fun nativeSurfaceCreated(): Boolean
    external fun nativeSurfaceChanged(width: Int, height: Int)
    external fun nativeSetPanelSize(width: Int, height: Int)
    external fun nativeSetDevice(name: String, model: String)
    external fun nativeDrawFrame(vsyncFrameNs: Long)
    // Beta9 optional actual display present timestamp via EGL. Read-only.
    external fun nativeEglPresentationReset()
    external fun nativeEglPresentationTick(pressedNs: Long)
    external fun nativeEglPresentationReport(): String
    external fun nativeKey(code: Int, pressed: Boolean)
    external fun nativeTouch(action: Int, x: Float, y: Float, timeMs: Long)
    external fun nativeUpdateProgress(percent: Int)
    external fun nativeSetSlotFolder(place: Int)
    /** Library's slot. folder row while the first copy runs: 0-100, -1 done. */
    external fun nativeSetSlotFolderCopy(percent: Int)
    external fun nativeSetTaskPill(spec: String)
    external fun nativeSetHasTouchscreen(has: Boolean)
    /** Wi-Fi and Bluetooth's width on the status line, in canvas pixels. */
    external fun nativeSetStatusIconsWidth(width: Int)
    external fun nativeLaunchAsked(ms: Long)
    external fun nativeFrameWatchFlush()
    external fun nativeJournal(): String
    external fun nativeReloadAll()
    external fun nativeSetRommSync(state: Int)
    external fun nativeSetRommStates(state: Int)
    external fun nativeSetRomsMissing(missing: Boolean)
    external fun nativeExternalClose()
    external fun nativeExternalBegin()
    /** A key of a controller: Settings > Controls picks its port and button. */
    external fun nativePadKey(device: Int, code: Int, pressed: Boolean, stick: Boolean)
    /** Every controller connected now, as [{"id","name","fp"}]. */
    external fun nativeControllers(json: String)
    external fun nativeResetInput()
    external fun nativeExternalReturn()
    external fun nativePerformanceStats(): String?
    external fun nativeEmulationDiagnostics(): String?
    external fun nativeShelfIndicatorFlags(): Int
    external fun nativeSetShelfStatus(code: Int)
    external fun nativeSetScanProgress(done: Int, total: Int)
    external fun nativeSetLibrary(json: String): Int
    external fun nativeConfigure(dataDir: String, libraryDir: String,
        isRetroidPocketClassic: Boolean)
    external fun nativePollLaunchUri(): String?
    external fun nativeGameReady(uri: String, path: String)
    external fun nativeGameError(uri: String, message: String)
    external fun nativeExitGame()
    external fun nativeSuspend()
    external fun nativePresentationResume()
    external fun nativeMenuPalette(): Long
    external fun nativeMenuDarkInk(): Int
    external fun nativeIsPlaying(): Boolean
    external fun nativeGameUnpaused(): Boolean
    external fun nativeSlotButtonReleased(held: Boolean)
    external fun nativeLidCover(closed: Boolean)
    /** The lid, with no cover animation (opened straight out of sleep). */
    external fun nativeLidCoverNow(closed: Boolean)
    external fun nativeLidStep()
    /** Settings > Handheld (RG Rotate): [dim after s, sleep after s (0 never),
     *  black cover with no game (1), animations (1)]. */
    external fun nativeRotateDelays(): IntArray
    external fun nativeRotateSleepState(state: Int)
    external fun nativeCarouselQuiet(): Boolean
    external fun nativeWarmRomCandidate(): String?
    external fun nativeCanRefreshLibrary(): Boolean
    external fun nativeAudioClockCounters(): LongArray
    external fun nativeAudioSampleRate(): Int
    // RETROARCH pacing: panel refresh (mHz) and AudioTrack fill for rate control.
    external fun nativeSetDisplayRefresh(millihz: Int)
    external fun nativeSetAudioOutputDepth(queuedFrames: Int, capacityFrames: Int)
    external fun nativeAudioQueuedShorts(): Int
    external fun nativeSetMgbaLinkRecoveryEnabled(enabled: Boolean)
    external fun nativeDisplaySyncEnabled(): Boolean
    // Frame Pacing NATIVE: the game wants the swap-paced loop (RetroArch's own).
    external fun nativeRaSwapLoop(): Boolean
    external fun nativeAudioSpeedPermille(): Int
    external fun nativeFastAudioSupported(supported: Boolean)
    external fun nativeReadAudioInto(samples: ShortArray): Int
    external fun nativePollMessage(): String?
    external fun nativePollRaNotice(): String?
    external fun nativeRaStatus(): String?
    external fun nativeRaNetworkAvailable()
    external fun nativeRaAccountConfigured()
    external fun nativeRaPrefetchHash(hash: String, console: Int)
    external fun nativeRaFlushReady(): Boolean
    external fun nativeRaStopReady(): Boolean
    external fun nativeSystemStatus(clock: String, batteryPercent: Int, charging: Boolean)
    external fun nativeSystemDate(date: String)
    external fun nativePollUiAction(): Int
    external fun nativeTakeScreenshot(): ByteArray?
    external fun nativeUpdateStatus(status: Int, installed: String, available: String, automatic: Boolean)
    external fun nativeReloadVisualAssets()
    external fun nativePollCartLabelUri(): String?
    external fun nativeWallpaperTarget(): String?
    external fun nativeReloadCartLabel(uri: String)
    external fun nativeStudioArt(uri: String, real: ByteArray?, logo: ByteArray?, kind: Int, name: String)
    // Library > RomM (RommPage.kt and runtime/romm.rs).
    external fun nativePollRommRequest(): String?
    external fun nativeRommCatalog(json: String)
    external fun nativeRommStatus(kind: Int, text: String)
    external fun nativeRommLabel(id: Long)
    external fun nativeRommHeader(json: String)
    external fun nativeRommProgress(done: Int, total: Int, percent: Int)
    external fun nativeRommDownloaded(id: Long, ok: Boolean)
    external fun nativeRommFilter(json: String)
    external fun nativePollDeleteRequest(): String?
    external fun nativeGameDeleted(uri: String, ok: Boolean)
    external fun nativePollCartSfx(): Int
    external fun nativePollRumbleStrength(): Int
    external fun nativeSensorsAvailable(mask: Int)
    external fun nativeSensorsWanted(): Int
    external fun nativeSensorEvent(sensor: Int, x: Float, y: Float, z: Float, timestampNs: Long)
    external fun nativeCoreForUri(uri: String): String
    external fun nativePollSaveFlush(): String?
    external fun nativeSetShareSaves(enabled: Boolean)
    external fun nativeSetNextResumeMode(mode: Int)
    // The renderer requests LAN only when Host/Join is selected. It never
    // touches WifiManager, Android UI or permissions from the GL thread.
    external fun nativeLinkPeerRomDecision(choice: Int)
    external fun nativeLinkPeerLabel(): String?
    // Internet Multiplayer: the room-code message to hand to the share sheet.
    external fun nativeLinkShareText(): String?
    external fun nativeLinkNetworkReady(ipv4: String)
    external fun nativeLinkNetworkError(reason: String)
    external fun nativeLinkNeedsMulticast(): Boolean
    external fun nativeLinkActive(): Boolean

    private var pendingCartLabelUri: String? = null
    private lateinit var cartSounds: CartSounds
    private val scanner = Executors.newSingleThreadExecutor()
    /** Folder scans asked and not finished. */
    private val scansRunning = AtomicInteger()
    /** RomM's downloads that came during a folder scan: added once it ends,
     *  rather than a new scan for each game (each cancelled the last, and
     *  none reached the shelf before the end: ROM folder chosen from RomM). */
    private var addAfterScan: List<RomLibrary.SavedFile>? = null
    private data class PendingLibraryIndex(
        val root: Uri, val json: String, val prepared: String,
        val serial: Int, val quiet: Boolean,
        /** RomM's games, one by one: DOWNLOADING n/m says it already, so no
         *  UPDATING LIBRARY each time, and the label search waits for the
         *  batch's end (it flashed 29 times: Siham 2026-11-16). */
        val silent: Boolean = false,
        /** The ROM folder was over slot.'s bounds: some games were skipped. */
        val partial: Boolean = false,
        val refreshArtwork: Boolean = false
    )
    private var pendingLibraryIndex: PendingLibraryIndex? = null
    /** The index last handed to the shelf, and when RomM's last one went:
     *  an identical one is never sent again, and RomM's games go in groups
     *  every few seconds so their faces get drawn (2.4.1-beta15 log). */
    private var lastPublishedIndex: Pair<Int, Int>? = null
    private var lastSilentPublishAt = 0L
    private var silentHeldLogged = false
    private lateinit var taskIndicators: SlotShelfTaskIndicators
    private lateinit var libraryRefreshIndicator: SlotLibraryRefreshIndicator
    private lateinit var autoLabelIndicator: SlotAutoLabelIndicator
    private val raLoginWorker = Executors.newSingleThreadExecutor()
    private val screenshotWorker = Executors.newSingleThreadExecutor()
    private val raSmartWorker = Executors.newSingleThreadExecutor { runnable ->
        Thread(runnable, "slot-ra-smart-cache").apply { priority = Thread.MIN_PRIORITY }
    }
    private val raSmartRunning = AtomicBoolean(false)
    private val gameLoader = Executors.newSingleThreadExecutor()
    /** RomM's saves go up here, not on gameLoader: the next cart's insertion
     *  never waits on the server (beta37). */
    private val rommSync = Executors.newSingleThreadExecutor { task ->
        Thread(task, "slot-romm-saves").apply { priority = Thread.MIN_PRIORITY }
    }
    /** Per game: RomM work queued or running ([rommLater]); while some is,
     *  its launch doesn't ask the server, the handheld's save being newer. */
    private val rommBusy = java.util.concurrent.ConcurrentHashMap<String, Int>()
    /** Per game: an upload queued and not started, so 30-second saves don't
     *  pile up (the one queued sends the newest save). */
    private val rommQueued = java.util.concurrent.ConcurrentHashMap.newKeySet<String>()

    private fun rommKey(target: RetroArchStorage.Target) = target.romIdentity + "." + target.core

    /** Whether a touchscreen is among Android's input devices (beta41):
     *  Controls shows Touchscreen only then. Unknown counts as one. */
    private fun reportTouchscreen() {
        val has = try {
            InputDevice.getDeviceIds().any { id ->
                val device = InputDevice.getDevice(id) ?: return@any false
                !device.isVirtual && device.supportsSource(InputDevice.SOURCE_TOUCHSCREEN)
            }
        } catch (error: Exception) {
            SlotLog.w(TAG, "Touchscreen unknown; Controls keeps it", error)
            true
        }
        try { nativeSetHasTouchscreen(has) } catch (_: UnsatisfiedLinkError) {}
    }

    /** The task pill's tasks (beta39), sent to the engine when they change. */
    private fun publishTaskPill() {
        if (!::taskIndicators.isInitialized) return
        val tasks = ArrayList<TaskPill.Task>()
        slotFolder.copyPercent()?.let { tasks.add(TaskPill.Task(TaskPill.COPY, "COPYING $it%", it / 100f)) }
        slotFolder.syncProgress()?.let { (done, total) ->
            // Files coming in from the folder, counted as they arrive; a
            // Refresh Library's check shows even when none come in.
            tasks.add(if (total > 0) TaskPill.Task(TaskPill.SYNC, "SYNCING $done/$total", done.toFloat() / total)
                else TaskPill.Task(TaskPill.SYNC, "SYNCING", -1f))
        }
        taskIndicators.downloads.pillWords?.let {
            tasks.add(TaskPill.Task(TaskPill.DOWNLOADS, it, taskIndicators.downloads.pillProgress))
        }
        if (rommBusy.isNotEmpty()) tasks.add(TaskPill.Task(TaskPill.ROMM_SAVE, "ROMM SAVE", -1f))
        taskIndicators.library.pillWords?.let {
            tasks.add(TaskPill.Task(TaskPill.SCAN, it, taskIndicators.library.pillProgress))
        }
        val labelWords = taskIndicators.labels.pillWords
        if (labelWords != null) tasks.add(TaskPill.Task(TaskPill.LABELS, labelWords, taskIndicators.labels.pillProgress))
        else if (manualArtworkRefresh.get() && autoLabelsRunning.get()) {
            tasks.add(TaskPill.Task(TaskPill.LABELS, "LABELS", -1f))
        }
        TaskPill.spec(tasks, nativeIsPlaying())?.let { nativeSetTaskPill(it) }
    }

    private fun rommLater(target: RetroArchStorage.Target, upload: Boolean, work: () -> String?) {
        val key = rommKey(target)
        if (upload && !rommQueued.add(key)) return
        rommBusy.merge(key, 1) { a, b -> a + b }
        try {
            rommSync.execute {
                if (upload) rommQueued.remove(key)
                try {
                    work()?.let { note ->
                        runOnUiThread {
                            if (!isDestroyed) SlotToast.makeText(this@MainActivity, note, Toast.LENGTH_SHORT).show()
                        }
                    }
                } catch (error: Exception) {
                    SlotLog.w(TAG, "RomM save work skipped", error)
                } finally {
                    rommBusy.computeIfPresent(key) { _, n -> if (n > 1) n - 1 else null }
                }
            }
        } catch (error: Exception) {
            if (upload) rommQueued.remove(key)
            rommBusy.computeIfPresent(key) { _, n -> if (n > 1) n - 1 else null }
            SlotLog.w(TAG, "RomM save work not queued", error)
        }
    }
    // Optional I/O must never delay a real ROM launch or pending save exports.
    private val romWarmWorker = Executors.newSingleThreadExecutor { task ->
        Thread(task, "slot-kpa-rom-warm").apply { priority = Thread.MIN_PRIORITY }
    }
    private val warmedCandidate = java.util.concurrent.atomic.AtomicReference<String?>(null)
    /** Cart Studio's art requests: one cart's real label and logo at a time. */
    private val studioArtWorker = Executors.newSingleThreadExecutor()
    private val autoLabelsWorker = Executors.newSingleThreadExecutor { task ->
        Thread(task, "slot-kpa-auto-labels").apply { priority = Thread.MIN_PRIORITY }
    }
    private val autoLabelsRunning = AtomicBoolean(false)
    private val autoLabelsRestartRequested = AtomicBoolean(false)
    private val artworkRefreshRequested = AtomicBoolean(false)
    private val manualArtworkRefresh = AtomicBoolean(false)
    private val manualRefreshSerial = AtomicInteger(0)
    private val manualBiosFailed = AtomicBoolean(false)
    private val manualCatalogueFailed = AtomicBoolean(false)
    private val manualOfflineNotice = AtomicBoolean(false)
    private val autoLabelsNeedSync = AtomicBoolean(false)
    private val artworkCheckpointRunning = AtomicBoolean(false)
    private var labelsSinceCheckpoint = 0
    /** Labels saved before this run of passes, for its "N LABELS ADDED". */
    @Volatile private var autoLabelsChainBase = -1
    private var autoLabelsOwnPassDone = false
    /** Only retries left (a server that timed out): the visible pass ends,
     *  they go on unseen (it stayed on LABELS 12/13: Siham 2026-11-17). */
    @Volatile private var autoLabelsQuiet = false
    /** The network was validated at its last change (onCapabilitiesChanged). */
    @Volatile private var labelsNetworkValidated = false
    @Volatile private var autoLabelsLastProgress = -1L
    /** Last pulse's game state: leaving a game resumes the label passes. */
    private var autoLabelsWasPlaying = false
    // Paint fresh labels only when all moving carts and pending GPU uploads
    // have settled. The renderer's manual reload path is synchronous.
    private val autoLabelUpdates = java.util.concurrent.ConcurrentLinkedQueue<String>()
    /** RomM's page labels for the games just downloaded, by the URI a scan
     *  gives them, kept until each is on its cart, whether the games reach
     *  the shelf by an add or by a folder scan (Siham 2026-11-16). */
    private val rommLabels = java.util.concurrent.ConcurrentHashMap<String, Pair<File, String>>()
    private val activeTargets = ConcurrentHashMap<String, RetroArchStorage.Target>()
    private val uiHandler = Handler(Looper.getMainLooper())
    private val romm: RommPage by lazy {
        RommPage(this) {
            // The ROM folder, when slot. may still write into it.
            getSharedPreferences(PREFS, MODE_PRIVATE).getString(ROM_ROOT, null)?.let(Uri::parse)
                ?.takeIf { uri -> contentResolver.persistedUriPermissions.any { it.uri == uri && it.isWritePermission } }
        }
    }

    /** Games came from RomM: the shelf learns them once the page is left,
     *  quietly (no scan bar, no toast: RomM said how many it saved). */
    internal fun refreshLibraryAfterDownloads() {
        val saved = getSharedPreferences(PREFS, MODE_PRIVATE).getString(ROM_ROOT, null) ?: return
        val uri = Uri.parse(saved)
        if (contentResolver.persistedUriPermissions.any { it.uri == uri && it.isReadPermission }) {
            scanFolder(uri, quiet = true, shown = true)
        }
    }

    /** Games RomM saved: only they are read and added to their shelves,
     *  the rest of the index stays as it is (no folder rescan). A scan on
     *  its way knows more than the saved index: then the folder is read
     *  again, as before, and so it is whenever the files can't be read. */
    internal fun addDownloadedGames(files: List<RomLibrary.SavedFile>) {
        val saved = getSharedPreferences(PREFS, MODE_PRIVATE).getString(ROM_ROOT, null) ?: return
        val root = Uri.parse(saved)
        if (contentResolver.persistedUriPermissions.none { it.uri == root && it.isReadPermission }) return
        for (file in files) {
            val label = file.label ?: continue
            try { rommLabels[RomLibrary.indexUri(root, file.uri).toString()] = label to file.labelPlatform }
            catch (_: Exception) { /* The label search finds it instead. */ }
        }
        if (scansRunning.get() > 0) {
            // The list names every game of the batch: the latest is enough.
            addAfterScan = files
            return
        }
        // Games arrive one by one (RommPage): the shelf not shown yet is the
        // base of the next, rather than a scan of the whole folder each time.
        val waiting = pendingLibraryIndex?.takeIf { it.root == root }?.json
        // An add never cancels the one before it (only a folder scan does):
        // with games arriving faster than an add took, each cancelled the
        // last and none reached the shelf before the end (2.4.1-beta8 log).
        val serial = scanSerial.get()
        scanner.execute {
            if (scanSerial.get() != serial) return@execute
            val previous = waiting ?: cachedLibraryGames(root)
            val json = try {
                previous?.let { RomLibrary.withSavedFiles(this, root, it, files) }
            } catch (error: Exception) {
                SlotLog.w(TAG, "Downloaded games not read; scanning the folder", error)
                null
            }
            if (previous == null || json == null) {
                runOnUiThread {
                    if (!isDestroyed && scanSerial.get() == serial) scanFolder(root, quiet = true, shown = true)
                }
                return@execute
            }
            // The shelf waiting to be shown is replaced by this one: shown even unchanged.
            if (waiting == null && RomLibrary.sameIndex(previous, json)) return@execute
            giveRommLabels(json)
            val prepared = json
            SlotLog.i(TAG, "RomM: ${files.size} game(s) added without a folder scan")
            runOnUiThread {
                if (isDestroyed || scanSerial.get() != serial) return@runOnUiThread
                pendingLibraryIndex = PendingLibraryIndex(root, json, prepared, serial, quiet = true, silent = true)
                publishPendingLibraryIfIdle()
            }
        }
    }

    /** A downloaded cart wears the label RomM's page showed for it at once,
     *  rather than waiting for the label search: the same art set, cut as
     *  the shelf's. Only when it has no label yet and its system is the one
     *  the label was cut for. On the scanner thread, before the shelf; a
     *  cart already drawn gets its new face too. */
    private fun giveRommLabels(json: String) {
        if (rommLabels.isEmpty()) return
        val platforms = try {
            RomLibrary.indexEntries(json).associate { it.uri to it.platform }
        } catch (_: Exception) { return }
        for ((uri, given) in rommLabels.entries.toList()) {
            // Not in this index yet: kept for the next one.
            val platform = platforms[uri] ?: continue
            rommLabels.remove(uri)
            val (label, labelPlatform) = given
            try {
                if (platform != labelPlatform) continue
                val target = CartLabelPaths.file(filesDir, uri)
                if (target.exists() || !label.isFile) continue
                target.parentFile?.mkdirs()
                val atomic = AtomicFile(target)
                val out = atomic.startWrite()
                try {
                    label.inputStream().use { it.copyTo(out) }
                    atomic.finishWrite(out)
                } catch (error: Exception) {
                    atomic.failWrite(out)
                    throw error
                }
                autoLabelUpdates.add(uri)
            } catch (error: Exception) {
                SlotLog.w(TAG, "RomM label not given to its cart; the label search will", error)
            }
        }
    }

    /** L2 + R2 held on a cart (Siham 2026-11-10): its ROM file goes, its
     *  saves and save states stay. The shelf already took it off. */
    private fun deleteRequestedGames() {
        while (true) {
            val uri = nativePollDeleteRequest() ?: return
            scanner.execute { deleteGame(uri) }
        }
    }

    private fun deleteGame(uri: String) {
        val ok = try {
            DocumentsContract.deleteDocument(contentResolver, Uri.parse(uri))
        } catch (error: Exception) {
            SlotLog.w(TAG, "Couldn't delete a ROM", error)
            false
        }
        if (ok) {
            // The cached index forgets it too: no rescan.
            val root = getSharedPreferences(PREFS, MODE_PRIVATE).getString(ROM_ROOT, null)?.let(Uri::parse)
            if (root != null) try {
                val games = cachedLibraryGames(root)?.let { JSONArray(it) }
                if (games != null) {
                    val kept = JSONArray()
                    for (i in 0 until games.length()) {
                        val game = games.optJSONObject(i) ?: continue
                        if (game.optString("uri") != uri) kept.put(game)
                    }
                    storeCachedLibrary(root, kept.toString())
                }
            } catch (error: Exception) {
                SlotLog.w(TAG, "ROM cache not updated after a delete", error)
            }
        }
        runOnUiThread {
            if (isDestroyed) return@runOnUiThread
            nativeGameDeleted(uri, ok)
            if (ok) {
                shelfCarts = (shelfCarts - 1).coerceAtLeast(0)
                SlotToast.makeText(this, "Game deleted. Saves kept.", Toast.LENGTH_SHORT).show()
            } else {
                // Read-only folder (chosen before 2.3) or the file is gone:
                // the shelf is read again, the cart comes back if it's there.
                SlotToast.makeText(this, "Couldn't delete this game. Choose your ROM folder again.",
                    Toast.LENGTH_LONG).show()
                refreshLibraryAfterDownloads()
            }
        }
    }

    /** Whether a ROM folder was ever chosen (writable or not). */
    internal fun hasRomFolder(): Boolean =
        getSharedPreferences(PREFS, MODE_PRIVATE).getString(ROM_ROOT, null) != null

    /** RomM needs the ROM folder writable: the picker, on the folder chosen
     *  before when Android allows it. */
    internal fun chooseRomFolderForRomm() {
        // The page stays behind Android's picker, as Library's rows do
        // (it closed, and the games came with no page to follow them).
        nativeExternalBegin()
        openRomFolderPicker()
    }

    private val slotUpdater: SlotUpdater by lazy {
        SlotUpdater(this) { status, version ->
            if (!isDestroyed) {
                val automatic = getSharedPreferences("slot-updater-v1", MODE_PRIVATE)
                    .getBoolean("enabled", true)
                nativeUpdateStatus(status,
                    packageManager.getPackageInfo(packageName, 0).versionName.orEmpty(),
                    version, automatic)
            }
        }.apply {
            onProgress = { percent -> if (!isDestroyed) nativeUpdateProgress(percent) }
        }
    }
    /** The slot. folder (2.5): the mirror on the card that travels. */
    internal val slotFolder: SlotFolder by lazy { SlotFolder(this) }
    /** Copies this handheld's saves to the slot. folder a little after
     *  the last save, not after each one. */
    private val slotFolderPush = Runnable { slotFolder.syncSaves() }

    /** Folders slot. already uses for something else (scenario A7). */
    internal fun otherFolders(): Set<String> {
        val prefs = getSharedPreferences(PREFS, MODE_PRIVATE)
        return listOf(ROM_ROOT, BIOS_ROOT, SAVE_ROOT, STATE_ROOT)
            .mapNotNull { prefs.getString(it, null) }.toSet()
    }

    /** The ROM folder is on an SD card that isn't in this handheld now
     *  (taken out, or in another one): the shelf greys its carts. Only the
     *  mounted volumes are looked at, so this is cheap enough to poll. */
    internal fun romCardMissing(): Boolean = try {
        val root = getSharedPreferences(PREFS, MODE_PRIVATE).getString(ROM_ROOT, null)?.let(Uri::parse)
        val volume = root?.takeIf { it.authority == "com.android.externalstorage.documents" }
            ?.let { android.provider.DocumentsContract.getTreeDocumentId(it).substringBefore(':', "") }
        if (volume == null || volume.isEmpty() || volume == "primary") false
        else getSystemService(android.os.storage.StorageManager::class.java).storageVolumes.none {
            it.uuid.equals(volume, ignoreCase = true) && it.state == android.os.Environment.MEDIA_MOUNTED
        }
    } catch (_: Exception) { false }

    /** The ROM folder's place on its storage (`Roms`), kept in slot.json. */
    internal fun romFolderPath(): String? = try {
        getSharedPreferences(PREFS, MODE_PRIVATE).getString(ROM_ROOT, null)?.let {
            android.provider.DocumentsContract.getTreeDocumentId(Uri.parse(it)).substringAfter(':', "")
                .takeIf { path -> path.isNotEmpty() }
        }
    } catch (_: Exception) { null }

    /** The BIOS folder's path on its storage, for slot.json (3.0.1-beta12). */
    internal fun biosFolderPath(): String? = try {
        getSharedPreferences(PREFS, MODE_PRIVATE).getString(BIOS_ROOT, null)?.let {
            android.provider.DocumentsContract.getTreeDocumentId(Uri.parse(it)).substringAfter(':', "")
                .takeIf { path -> path.isNotEmpty() }
        }
    } catch (_: Exception) { null }

    /** A slot folder's BIOS folder, asked for once its ROM folder is chosen. */
    private var biosAfterRoms: String? = null

    /** Scenario D1: a slot folder from another handheld names its ROM
     *  folder; with none chosen here, the picker opens right on it, then on
     *  its BIOS folder when it names one too (3.0.1-beta12). */
    internal fun suggestRomFolder(path: String?, bios: String?) {
        val prefs = getSharedPreferences(PREFS, MODE_PRIVATE)
        val volume = slotFolder.volume() ?: return
        biosAfterRoms = bios?.takeIf { !prefs.contains(BIOS_ROOT) }
        if (path == null || prefs.getString(ROM_ROOT, null) != null) {
            suggestBiosFolder()
            return
        }
        SlotToast.makeText(this, "Now choose your ROM folder.", Toast.LENGTH_LONG).show()
        openRomFolderPicker(android.provider.DocumentsContract.buildDocumentUri(
            "com.android.externalstorage.documents", "$volume:$path"))
    }

    /** The slot folder's BIOS folder, the picker opened right on it. A new
     *  installation is never asked: BIOS are added in Library when wanted
     *  (Siham 2026-11-25). */
    private fun suggestBiosFolder() {
        val path = biosAfterRoms ?: return
        biosAfterRoms = null
        if (getSharedPreferences(PREFS, MODE_PRIVATE).contains(BIOS_ROOT) || isDestroyed || nativeIsPlaying()) return
        val volume = slotFolder.volume() ?: return
        SlotToast.makeText(this, "Now choose your BIOS folder.", Toast.LENGTH_LONG).show()
        nativeExternalBegin()
        openBiosFolderPicker(android.provider.DocumentsContract.buildDocumentUri(
            "com.android.externalstorage.documents", "$volume:$path"))
    }

    /** Install was pressed before Android let slot. install updates: the
     *  download starts once the player comes back with it allowed. */
    private var updateAfterPermission = false

    /** Asks Android's "install unknown apps" for slot. first, so the
     *  installer follows the download at once (Siham 2026-11-15). */
    private fun installUpdate() {
        if (nativeIsPlaying()) return
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O &&
            !packageManager.canRequestPackageInstalls()) {
            try {
                updateAfterPermission = true
                SlotToast.makeText(this, "Allow slot. to install updates, then come back.",
                    Toast.LENGTH_LONG).show()
                // About stays behind Android's page (it closed on the way
                // there and the player came back to the shelf, 2.4.1).
                nativeExternalBegin()
                returnFromSlotChild = true
                startActivity(Intent(android.provider.Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                    Uri.parse("package:$packageName")))
                return
            } catch (error: Exception) {
                updateAfterPermission = false
                returnFromSlotChild = false
                nativeExternalReturn()
                SlotLog.w(TAG, "Cannot open the install permission; downloading first", error)
            }
        }
        slotUpdater.downloadAndInstall { apk ->
            if (!isDestroyed && resumed && !nativeIsPlaying()) {
                try {
                    slotUpdater.startInstaller(apk)
                } catch (error: Exception) {
                    SlotLog.w(TAG, "User-confirmed installer not available", error)
                    SlotToast.makeText(this@MainActivity,
                        "Allow updates from slot. in Android settings.",
                        Toast.LENGTH_LONG).show()
                }
            }
        }
    }
    private val audioRunning = AtomicBoolean(false)
    // Each AudioTrack worker owns a unique generation; a stopped or crashed
    // worker must never resume because a newer game has set audioRunning=true.
    private val audioWorkerGeneration = AtomicInteger(0)
    private val audioRetryNotBeforeMs = java.util.concurrent.atomic.AtomicLong(0L)
    // A single pending wakeup is enough. GLSurfaceView never waits for audio.
    private var lastEmulationReportPollAt = 0L
    private var lastLibraryStatePoll = 0L
    @Volatile private var diagnosticAudioUnderruns = 0
    // Prevent an empty-read 8ms sleep from missing a freshly produced frame.
    private val audioFrameSignal = java.util.concurrent.Semaphore(0)
    // Invoked on GLSurfaceView's GL thread after a real core frame has
    // queued PCM but BEFORE Run-Ahead's speculative frames and GLES
    // presentation, so the audio worker can write during them.
    private val earlyPcmWakes = AtomicInteger(0)
    @Suppress("unused")
    private fun onNativePcmReady() {
        if (audioRunning.get() && audioFrameSignal.availablePermits() == 0) {
            audioFrameSignal.release()
            earlyPcmWakes.incrementAndGet()
        }
    }
    private val vibrator by lazy { getSystemService(VIBRATOR_SERVICE) as? Vibrator }
    private var lastRumbleAt = 0L
    private var lastRumbleAmplitude = 0
    private var rumbleUntil = 0L
    private val rumblePulse = object : Runnable {
        override fun run() {
            if (!resumed || isDestroyed) { stopRumble(); return }
            pollRumble()
            uiHandler.postDelayed(this, if (nativeGameUnpaused()) 16L else 100L)
        }
    }
    // Tilt, gyro and solar cartridges: the core switches sensors on and
    // Android listens only while it wants them, like RetroArch.
    private val sensorManager by lazy { getSystemService(SENSOR_SERVICE) as? SensorManager }
    private val sensorTypes = listOf(
        SENSOR_ACCELEROMETER to Sensor.TYPE_ACCELEROMETER,
        SENSOR_GYROSCOPE to Sensor.TYPE_GYROSCOPE,
        SENSOR_LIGHT to Sensor.TYPE_LIGHT,
    )
    private var sensorsListening = 0
    private val sensorListener = object : SensorEventListener {
        override fun onSensorChanged(event: SensorEvent) {
            val sensor = sensorTypes.firstOrNull { it.second == event.sensor.type }?.first ?: return
            val v = event.values
            // Differentiate motion using the sample time, not the UI thread's
            // delivery time: Android can deliver several readings together.
            nativeSensorEvent(sensor, v[0], v.getOrElse(1) { 0f }, v.getOrElse(2) { 0f }, event.timestamp)
        }
        override fun onAccuracyChanged(sensor: Sensor, accuracy: Int) {}
    }
    @Volatile private var resumed = false
    /** adb's shelf bench (SlotBench): never during a game. */
    private val bench by lazy { SlotBench(this) { resumed && !nativeIsPlaying() } }
    private val raSmartForeground = java.lang.Object()
    private var carouselDozing = false
    private var lastCarouselHeartbeat = 0L
    // A key may still be queued for the GL thread while the shelf reports
    // quiet: keep rendering every frame until the engine has consumed it.
    private var carouselAwakeUntil = 0L
    private var returnFromSlotChild = false
    private lateinit var performanceHud: TextView
    private lateinit var shelfStatus: SlotStatusIndicators
    private var lastPerfPoll = 0L
    @Volatile private var displaySyncApplied = false
    // Frame Pacing NATIVE: render continuously and let the blocking buffer
    // swap pace the core, as RetroArch does.
    @Volatile private var raSwapLoopApplied = false
    // Last D-pad hat position (-1, 0, 1) of each controller reporting one.
    private val hats = HashMap<Int, IntArray>()
    // Controllers last sent to slot. (nativeControllers), by device id.
    private val knownControllers = HashSet<Int>()
    private val controllerListener = object : InputManager.InputDeviceListener {
        override fun onInputDeviceAdded(deviceId: Int) = pushControllers()
        override fun onInputDeviceRemoved(deviceId: Int) = pushControllers()
        override fun onInputDeviceChanged(deviceId: Int) = pushControllers()
    }
    private var stickForwarded = false
    @Volatile private var latestVsyncFrameNs = 0L
    // Beta10 read-only Choreographer delivery telemetry (UI thread).
    // These are UI callbacks, never confirmed display-present timestamps.
    @Volatile private var latestVsyncSequence = 0L
    private var previousUiVsyncFrameNs = 0L
    private val uiVsyncCallbacks = java.util.concurrent.atomic.AtomicLong(0L)
    private val uiVsyncOver25ms = java.util.concurrent.atomic.AtomicLong(0L)
    private val uiVsyncOver40ms = java.util.concurrent.atomic.AtomicLong(0L)
    // RA cached game details can be ready before Slot's seating/CRT sequence
    // has set PLAYING=true. Hold that notice, then reveal it on actual gameplay.
    // Distinguish that legitimate startup from a stale notice after eject.
    private var raLaunchPending = false
    private var raLaunchRequestedAt = 0L
    private var raWasPlaying = false
    private var raPendingGameNotice: RaAchievementOverlay.Notice? = null

    // Only attached while Frame Pacing RESPONSIVE runs a solo game: NATIVE
    // and the shelf render continuously.
    private val vsyncCallback = object : Choreographer.FrameCallback {
        override fun doFrame(frameTimeNanos: Long) {
            if (!displaySyncApplied || raSwapLoopApplied || !resumed || isDestroyed) return
            recordVsync(frameTimeNanos)
            view.requestRender()
            Choreographer.getInstance().postFrameCallback(this)
        }
    }
    private fun recordVsync(frameTimeNanos: Long) {
        // One fixed-time record per scheduler VSync; avoid allocations,
        // network calls, EGL queries and render-policy changes.
        val previous = previousUiVsyncFrameNs
        if (previous > 0L && frameTimeNanos > previous) {
            val uiGap = frameTimeNanos - previous
            if (uiGap > 25_000_000L) uiVsyncOver25ms.incrementAndGet()
            if (uiGap > 40_000_000L) uiVsyncOver40ms.incrementAndGet()
        }
        previousUiVsyncFrameNs = frameTimeNanos
        latestVsyncFrameNs = frameTimeNanos
        latestVsyncSequence++
        uiVsyncCallbacks.incrementAndGet()
    }
    // RETROARCH pacing clocks the core from the panel, so it needs the
    // current refresh rate (it can change with the display mode).
    private fun publishDisplayRefresh() {
        val hz = display?.refreshRate ?: 0f
        nativeSetDisplayRefresh(if (hz > 0f) (hz * 1000f).toInt() else 0)
    }
    private fun updateDisplaySyncMode() {
        if (!::view.isInitialized) return
        val enabled = resumed && nativeIsPlaying() && nativeDisplaySyncEnabled()
        val swapLoop = enabled && nativeRaSwapLoop()
        if (enabled == displaySyncApplied && swapLoop == raSwapLoopApplied) return
        if (enabled) publishDisplayRefresh()
        // Diagnostic-only event: a switch from continuous GL rendering to
        // Choreographer during the first 100ms of gameplay could explain
        // an isolated startup hitch. Do not change frame scheduling here.
        SlotLog.i(TAG, "Game display pacing transition: " +
            (if (swapLoop) "Swap-paced" else if (enabled) "Choreographer" else "Continuous") +
            ", playing=${nativeIsPlaying()}")
        displaySyncApplied = enabled
        raSwapLoopApplied = swapLoop
        if (enabled && !swapLoop) {
            view.renderMode = GLSurfaceView.RENDERMODE_WHEN_DIRTY
            Choreographer.getInstance().removeFrameCallback(vsyncCallback)
            Choreographer.getInstance().postFrameCallback(vsyncCallback)
        } else {
            Choreographer.getInstance().removeFrameCallback(vsyncCallback)
            previousUiVsyncFrameNs = 0L
            view.renderMode = GLSurfaceView.RENDERMODE_CONTINUOUSLY
        }
    }

    // The engine still runs every frame while animating, scrolling, playing,
    // or preparing cartridge art. A settled shelf is refreshed at 4 Hz for
    // clocks, library scans and asynchronous UI changes.
    /** Keeps the shelf drawing every frame for a while (RG Rotate lid). */
    internal fun wakeCarouselFor(ms: Long) {
        // Kept even before onResume (the lid opening out of sleep): the
        // cover animation then runs at full rate from its first frame.
        carouselAwakeUntil = maxOf(carouselAwakeUntil, SystemClock.uptimeMillis() + ms)
        wakeCarouselRender()
    }

    private fun wakeCarouselRender() {
        if (!::view.isInitialized || !resumed) return
        carouselAwakeUntil = maxOf(carouselAwakeUntil, SystemClock.uptimeMillis() + 500L)
        if (carouselDozing) {
            carouselDozing = false
            view.renderMode = GLSurfaceView.RENDERMODE_CONTINUOUSLY
        }
        view.requestRender()
    }

    private fun updateCarouselPacing() {
        if (!::view.isInitialized || !resumed) return
        val canDoze = !displaySyncApplied && !nativeIsPlaying() && nativeCarouselQuiet()
            && SystemClock.uptimeMillis() >= carouselAwakeUntil
        if (!canDoze) {
            if (carouselDozing) wakeCarouselRender()
            return
        }
        if (!carouselDozing) {
            carouselDozing = true
            lastCarouselHeartbeat = SystemClock.uptimeMillis()
            view.renderMode = GLSurfaceView.RENDERMODE_WHEN_DIRTY
        }
        val now = SystemClock.uptimeMillis()
        if (now - lastCarouselHeartbeat >= 250L) {
            lastCarouselHeartbeat = now
            view.requestRender()
        }
    }

    private fun publishAutoLabelsWhenIdle() {
        // Straight to the shelf, which draws them one per frame once it's
        // still: waiting here for a quiet carousel left labels off the shelf
        // until slot. restarted, for players who keep browsing or editing
        // carts (RetroPup's video, 2026-11-16).
        if (autoLabelUpdates.isEmpty()) return
        repeat(AUTO_LABELS_PER_PULSE) {
            val uri = autoLabelUpdates.poll() ?: return@repeat
            nativeReloadCartLabel(uri)
        }
        wakeCarouselRender()
    }

    private fun updateRomWarmCandidate() {
        val next = if (nativeIsPlaying()) null else nativeWarmRomCandidate()
        if (warmedCandidate.getAndSet(next) == next || next == null) return
        try {
            romWarmWorker.execute {
                RomWarmCache.prepare(applicationContext, next,
                    shouldContinue = { resumed && !nativeIsPlaying() && warmedCandidate.get() == next },
                    superseded = { warmedCandidate.get().let { it != null && it != next } })
            }
        } catch (_: java.util.concurrent.RejectedExecutionException) { /* shutting down */ }
    }

    private var audioThread: Thread? = null
    private var raConnectivity: ConnectivityManager? = null
    private var activeRaPanel: Pair<RaPanelDialog, String>? = null
    private fun refreshRaConnectivityOnScreen() {
        // ConnectivityManager may call us from a binder thread: touch views
        // only on main, and reuse the already scanned account snapshot.
        runOnUiThread {
            val (panel, user) = activeRaPanel ?: return@runOnUiThread
            if (panel.isShowing()) {
                RaDiagnostics.lastSnapshot(user)?.let { snapshot ->
                    panel.update(retroAchievementsStatus(), hasValidatedInternet(), snapshot)
                }
            }
        }
    }
    private var linkMulticastLock: WifiManager.MulticastLock? = null

    /** The LAN need not have Internet: the two handhelds only need the same Wi-Fi.
     *  Resolve the actual interface's IPv4 address, never use the default
     *  network's mobile/VPN address or a hardcoded 10.42.0.1 Linux radio. */
    private fun linkWifiAddress(): String? {
        val cm = getSystemService(CONNECTIVITY_SERVICE) as? ConnectivityManager ?: return null
        for (network in cm.allNetworks) {
            val capabilities = cm.getNetworkCapabilities(network) ?: continue
            if (!capabilities.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) ||
                capabilities.hasTransport(NetworkCapabilities.TRANSPORT_VPN)) continue
            val links = cm.getLinkProperties(network) ?: continue
            val interfaceName = links.interfaceName ?: continue
            val iface = try {
                java.net.NetworkInterface.getByName(interfaceName)
            } catch (error: java.net.SocketException) {
                SlotLog.w(TAG, "Link Wi-Fi could not inspect interface $interfaceName", error)
                null
            } ?: continue
            val addresses = try {
                if (!iface.isUp) continue
                val values = mutableSetOf<String>()
                val registered = iface.inetAddresses
                while (registered.hasMoreElements()) {
                    val address = registered.nextElement()
                    if (address is Inet4Address) values.add(address.hostAddress)
                }
                values
            } catch (error: java.net.SocketException) {
                SlotLog.w(TAG, "Link Wi-Fi interface $interfaceName unavailable", error)
                continue
            }
            for (address in links.linkAddresses) {
                val ip = address.address
                if (ip is Inet4Address && !ip.isLoopbackAddress &&
                    !ip.isLinkLocalAddress && !ip.isMulticastAddress &&
                    !ip.isAnyLocalAddress && addresses.contains(ip.hostAddress)) {
                    SlotLog.i(TAG, "Link Wi-Fi selected: interface=$interfaceName ipv4=${ip.hostAddress}")
                    return ip.hostAddress
                }
            }
            SlotLog.w(TAG, "Link Wi-Fi skipped stale address on interface=$interfaceName")
        }
        SlotLog.w(TAG, "Link Wi-Fi could not find a live interface with a matching IPv4 address")
        return null
    }

    private fun releaseLinkMulticast() {
        val current = linkMulticastLock
        linkMulticastLock = null
        try { if (current?.isHeld == true) current.release() }
        catch (error: RuntimeException) { SlotLog.w(TAG, "Link multicast release failed", error) }
        releaseLinkLowLatency()
    }

    private var linkLowLatencyLock: WifiManager.WifiLock? = null

    /** Wi-Fi power save holds packets for the next beacon (~100 ms), which
     *  stalls a lockstep link and pushes its input delay up. Keep the radio
     *  awake only while a Link session plays. */
    private fun holdLinkLowLatency(active: Boolean) {
        if (!active) { releaseLinkLowLatency(); return }
        if (linkLowLatencyLock != null) return
        try {
            val manager = applicationContext.getSystemService(WIFI_SERVICE) as WifiManager
            val lock = manager.createWifiLock(
                WifiManager.WIFI_MODE_FULL_LOW_LATENCY, "slot-konkr-link")
            lock.setReferenceCounted(false)
            lock.acquire()
            linkLowLatencyLock = lock
            SlotLog.i(TAG, "Link Wi-Fi low latency on")
        } catch (error: RuntimeException) {
            SlotLog.w(TAG, "Link Wi-Fi low latency unavailable", error)
        }
    }

    private fun releaseLinkLowLatency() {
        val current = linkLowLatencyLock ?: return
        linkLowLatencyLock = null
        try { if (current.isHeld) current.release() }
        catch (error: RuntimeException) { SlotLog.w(TAG, "Link Wi-Fi lock release failed", error) }
    }

    /** The peer has proven possession of an installation-bound Ed25519 key
     * before this UI appears. A14 never trusts IPs or legacy lab flags.
     * Keys are remembered only by Rust after a verified ROM exchange.
     */
    /** Internet host: hand the room code to WhatsApp, SMS or anything else.
     *  The code is also copied, so it can be pasted by hand. The room stays
     *  open while slot. is in the background (see Rust Input::Suspend). */
    private fun shareLinkCode() {
        val text = nativeLinkShareText() ?: return
        try {
            val clipboard = getSystemService(Context.CLIPBOARD_SERVICE)
                as android.content.ClipboardManager
            clipboard.setPrimaryClip(android.content.ClipData.newPlainText("slot. room code", text))
        } catch (error: Exception) {
            SlotLog.w(TAG, "Cannot copy the room code", error)
        }
        try {
            val send = Intent(Intent.ACTION_SEND).apply {
                type = "text/plain"
                putExtra(Intent.EXTRA_TEXT, text)
            }
            startActivity(Intent.createChooser(send, "Send the room code"))
        } catch (error: Exception) {
            SlotLog.w(TAG, "No app can share the room code", error)
            SlotToast.makeText(this, "Code copied. Paste it in a message to your friend.",
                Toast.LENGTH_LONG).show()
        }
    }

    private fun showPeerCartridgeConsent() {
        if (!resumed || isDestroyed || isFinishing) {
            nativeLinkPeerRomDecision(0)
            return
        }
        val answered = AtomicBoolean(false)
        fun reply(choice: Int) {
            if (answered.compareAndSet(false, true)) nativeLinkPeerRomDecision(choice)
        }
        try {
            val peer = nativeLinkPeerLabel().orEmpty().take(36)
                .ifBlank { "UNRECOGNIZED DEVICE" }
            val dialog = SlotModalDialog(
                this,
                "LINK — SHARE CARTRIDGE",
                "Share this cartridge with $peer over Wi-Fi? " +
                    "Only allow a console you trust.",
                onDismiss = { reply(0) }
            )
            dialog.addButton("CANCEL") { reply(0); dialog.dismiss() }
            dialog.addButton("ALLOW ONCE") { reply(1); dialog.dismiss() }
            dialog.addButton("ALWAYS ALLOW") { reply(2); dialog.dismiss() }
            dialog.show()
        } catch (error: Exception) {
            SlotLog.w(TAG, "Cannot show Slot peer permission", error)
            reply(0)
        }
    }

    private fun requestLinkWifi() {
        if (!resumed) {
            nativeLinkNetworkError("Return to slot. before starting Multiplayer.")
            return
        }
        val address = linkWifiAddress()
        if (address == null) {
            nativeLinkNetworkError("Connect both consoles to the same Wi-Fi.")
            return
        }
        try {
            releaseLinkMulticast()
            val manager = applicationContext.getSystemService(WIFI_SERVICE) as WifiManager
            val lock = manager.createMulticastLock("slot-konkr-multiplayer")
            lock.setReferenceCounted(false)
            lock.acquire()
            linkMulticastLock = lock
            // Lock must be held before the Rust discovery worker starts.
            nativeLinkNetworkReady(address)
        } catch (error: Exception) {
            releaseLinkMulticast()
            SlotLog.w(TAG, "Unable to acquire LAN discovery lock", error)
            nativeLinkNetworkError("Couldn't search for other consoles on this Wi-Fi.")
        }
    }
    private val raConnectivityCallback = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) {
            // Signals only that a network appeared. Rust still verifies RA HTTPS.
            if (!isDestroyed) {
                nativeRaNetworkAvailable()
                startRaSmartCache()
                startAutoCartLabels()
                refreshRaConnectivityOnScreen()
            }
        }
        override fun onCapabilitiesChanged(network: Network, caps: NetworkCapabilities) {
            if (!isDestroyed) {
                val validated = caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
                if (validated) startRaSmartCache()
                // Android repeats this callback (signal, link speed...): a
                // label pass only when the network has just been validated,
                // not one every few seconds (beta41).
                if (validated && !labelsNetworkValidated) startAutoCartLabels()
                labelsNetworkValidated = validated
                refreshRaConnectivityOnScreen()
            }
        }
        override fun onLost(network: Network) {
            labelsNetworkValidated = false
            if (!isDestroyed) refreshRaConnectivityOnScreen()
        }
    }
    private val pulse = object : Runnable {
        override fun run() {
            // No 100ms UI supervision after the Activity loses foreground.
            if (!resumed || isDestroyed) return
            // Changed ROM indexes are applied only when the GL engine is
            // back on its idle carousel, never during a game or its launch.
            publishPendingLibraryIfIdle()
            pollSystemStatus()
            if (::shelfStatus.isInitialized) {
                val flags = nativeShelfIndicatorFlags()
                shelfStatus.refresh(flags)
                if (::taskIndicators.isInitialized) {
                    taskIndicators.setLightTheme(flags and 16 != 0)
                }
                // Library feedback and START are rendered inside Slot's
                // GL carousel, not overlaid by an Android TextView.
            }
            updateSensors()
            updateDisplaySyncMode()
            updateCarouselPacing()
            publishAutoLabelsWhenIdle()
            val labelsPlaying = nativeIsPlaying()
            if (autoLabelsWasPlaying && !labelsPlaying) {
                // Back on the carousel after a game: carry on where the
                // label passes stopped (Siham 2026-11-04).
                startAutoCartLabels()
            }
            autoLabelsWasPlaying = labelsPlaying
            if (::taskIndicators.isInitialized) {
                // RomM's downloads go on with its page left: top right.
                val downloads = if (labelsPlaying) null else romm.downloadProgress()
                // An empty shelf shows the downloads in its middle, as the
                // first scan, not NO GAMES FOUND (Siham 2026-11-16).
                val onEmpty = downloads != null && shelfCarts == 0 && scansRunning.get() == 0
                if (downloads == null || onEmpty) taskIndicators.downloads.hide()
                else taskIndicators.downloads.showDownload(downloads[0], downloads[1])
                if (onEmpty && downloads != null) {
                    if (!downloadStatusShown) {
                        downloadStatusShown = true
                        downloadCountShown = -1
                        nativeSetShelfStatus(shelfDownloading)
                    }
                    if (downloads[0] != downloadCountShown) {
                        downloadCountShown = downloads[0]
                        nativeSetScanProgress(downloads[0], downloads[1])
                    }
                } else if (downloadStatusShown) {
                    downloadStatusShown = false
                    // Unless a scan or a published shelf has said otherwise.
                    if (scansRunning.get() == 0) nativeSetShelfStatus(if (shelfCarts == 0) shelfEmpty else shelfNone)
                }
            }
            if (::autoLabelIndicator.isInitialized) {
                if (nativeIsPlaying()) {
                    autoLabelIndicator.hide()
                } else if (autoLabelsRunning.get() && !autoLabelsQuiet) {
                    // The scanner only writes one immutable, lock-free
                    // snapshot. No Handler post or UI work per cartridge.
                    val progress = AutoCartLabelDiagnostics.snapshot()
                    if (progress.total > 0 && AutoCartLabelDiagnostics.passLooked) {
                        autoLabelIndicator.progress(
                            progress.checked, progress.total, progress.saved)
                    }
                } else {
                    autoLabelIndicator.tick()
                }
            }
            publishTaskPill()
            updateRomWarmCandidate()
            val playing = resumed && nativeIsPlaying() && !(::lidCover.isInitialized && lidCover.closed)
            if (playing) {
                if (!audioRunning.get()) startAudio()
            } else if (audioRunning.get()) {
                stopAudio()
            }
            keepScreenOn(playing && nativeGameUnpaused())
            drainPendingSaveFlush()
            val request = nativePollLaunchUri()
            if (request != null) {
                raLaunchPending = true
                raLaunchRequestedAt = SystemClock.elapsedRealtime()
                raPendingGameNotice = null
                loadGameFromSaf(request)
            }
            romm.poll()
            deleteRequestedGames()
            SlotUi.updateMenuPalette(nativeMenuPalette(), nativeMenuDarkInk())
            when (nativePollUiAction()) {
                1 -> openRomFolderPicker()
                2 -> refreshLibrary()
                3 -> {
                    try {
                        // Dialog.show() returns before Android's first
                        // composited frame. The RA dialog's dismiss callback,
                        // not this call, owns closing the native Slot Menu.
                        openRetroAchievementsAccount()
                    } catch (error: Exception) {
                        nativeExternalReturn()
                        SlotLog.e(TAG, "Unable to open RetroAchievements", error)
                        SlotToast.makeText(this@MainActivity,
                            "Couldn't open RetroAchievements.", Toast.LENGTH_SHORT).show()
                    }
                }
                4 -> openBiosFolderPicker()
                5 -> openSharedFolderPicker(SAVE_REQUEST)
                6 -> openSharedFolderPicker(STATE_REQUEST)
                7 -> openVisualPicker(THEME_REQUEST)
                8 -> openVisualPicker(WALLPAPER_REQUEST)
                9 -> resetVisualFile(THEME_REQUEST)
                10 -> resetVisualFile(WALLPAPER_REQUEST)
                11 -> openCartLabelPicker()
                12 -> removeSelectedCartLabel()
                60 -> if (::lidCover.isInitialized) lidCover.requestAdmin()
                61 -> if (::lidCover.isInitialized) lidCover.confirmRemoveAdmin()
                62 -> if (::lidCover.isInitialized) lidCover.openLockSettings()
                30 -> requestStudioArt()
                32 -> requestStudioGameArt(1)
                33 -> requestStudioPicture()
                15 -> {
                    try {
                        returnFromSlotChild = true
                        startActivity(Intent(this@MainActivity, AppsActivity::class.java))
                        SlotTransitions.open(this@MainActivity)
                    } catch (error: Exception) {
                        returnFromSlotChild = false
                        nativeExternalReturn()
                        SlotLog.w(TAG, "Cannot open Apps", error)
                        SlotToast.makeText(this@MainActivity, "Couldn't open Apps.",
                            Toast.LENGTH_SHORT).show()
                    }
                }
                16 -> requestLinkWifi()
                17 -> toggleShareGbaSaves()
                70 -> slotFolder.choose()
                71 -> {
                    if (!RommSaves.toggle(this@MainActivity)) {
                        SlotToast.makeText(this@MainActivity, "Connect to RomM first, in Library > RomM Server.",
                            Toast.LENGTH_SHORT).show()
                    }
                    nativeSetRommSync(RommSaves.state(this@MainActivity))
                    nativeSetRommStates(RommStates.state(this@MainActivity))
                }
                72 -> {
                    RommStates.toggle(this@MainActivity)?.let { note ->
                        SlotToast.makeText(this@MainActivity, note, Toast.LENGTH_SHORT).show()
                    }
                    nativeSetRommStates(RommStates.state(this@MainActivity))
                }
                18 -> showPeerCartridgeConsent()
                23 -> shareLinkCode()
                40 -> saveScreenshot()
                80 -> exportLogs()
                19 -> slotUpdater.check(true)
                20 -> slotUpdater.openNotes()
                21 -> installUpdate()
                22 -> {
                    val enabled = slotUpdater.toggle()
                    slotUpdater.refreshVisibleStatus()
                    if (enabled) slotUpdater.check(false)
                }
            }
            // Multicast is required during discovery, not for ongoing TCP
            // emulation. Release on success, cancellation and timeout.
            if (linkMulticastLock != null && !nativeLinkNeedsMulticast()) {
                releaseLinkMulticast()
            }
            if (resumed) holdLinkLowLatency(nativeLinkActive())
            val nowPerf = android.os.SystemClock.uptimeMillis()
            if (nowPerf - lastLibraryStatePoll >= 2_000L) {
                // Library's values: RomM's sync and the slot. folder's place.
                lastLibraryStatePoll = nowPerf
                nativeSetRommSync(RommSaves.state(this@MainActivity))
                nativeSetRommStates(RommStates.state(this@MainActivity))
                nativeSetSlotFolder(slotFolder.place())
                nativeSetRomsMissing(romCardMissing())
                slotFolder.cardCheck(resumed && nativeCanRefreshLibrary())
            }
            if (resumed && nowPerf - lastEmulationReportPollAt >= 1_000L) {
                lastEmulationReportPollAt = nowPerf
                val playing = nativeIsPlaying()
                EmulationDiagnostics.report(playing,
                    if (playing) nativeEmulationDiagnostics() else null,
                    if (playing) nativeAudioQueuedShorts() else 0,
                    if (playing) nativeAudioSampleRate() else 0,
                    diagnosticAudioUnderruns)
            }
            if (resumed && nowPerf - lastPerfPoll >= 250) {
                lastPerfPoll = nowPerf
                val value = nativePerformanceStats()
                performanceHud.visibility = if (value.isNullOrBlank()) View.GONE else View.VISIBLE
                if (!value.isNullOrBlank() && performanceHud.text.toString() != value) {
                    performanceHud.text = value
                }
            }
            nativePollMessage()?.let { message ->
                // Keep the complete diagnostic in Logcat. Actionable failures
                // receive a longer on-screen warning rather than vanishing
                // after the same interval as a simple confirmation.
                SlotLog.w(TAG, message)
                val concise = SlotToast.userMessage(message)
                val needsAction = listOf(
                    "can't", "cannot", "couldn't", "failed", "unavailable",
                    "paused", "mismatch", "needs", "check", "retry"
                ).any { concise.contains(it, ignoreCase = true) }
                SlotToast.makeText(this@MainActivity, concise,
                    if (needsAction) Toast.LENGTH_LONG else Toast.LENGTH_SHORT).show()
            }
            // RetroArch-like achievement widget: cached official badge and
            // original Slot label font. Foreground-only, in our Activity frame,
            // never a system Toast that Android may place at screen bottom.
            if (resumed) {
                val gameActive = nativeIsPlaying()
                // On a real game start, flush any cached early RA status.
                // Only a PLAYING -> stopped edge means a game was ejected.
                // Earlier Beta19 incorrectly treated loading as ejected.
                if (gameActive && !raWasPlaying) {
                    raLaunchPending = false
                    raPendingGameNotice?.let { raAchievementOverlay.push(it) }
                    raPendingGameNotice = null
                } else if (!gameActive && raWasPlaying) {
                    raLaunchPending = false
                    raPendingGameNotice = null
                    raAchievementOverlay.dismissGameLaunchNotices()
                } else if (!gameActive && raLaunchPending &&
                    SystemClock.elapsedRealtime() - raLaunchRequestedAt > 12_000L) {
                    // Failed or cancelled launches must not preserve a stale
                    // pre-launch notice indefinitely.
                    raLaunchPending = false
                    raPendingGameNotice = null
                }
                raWasPlaying = gameActive
                repeat(4) {
                    val raw = nativePollRaNotice() ?: return@repeat
                    try {
                        val notice = JSONObject(raw)
                        val title = notice.optString("title", "").take(100)
                        val detail = notice.optString("detail", "").take(160)
                        if (title.isNotBlank()) {
                            val earned = notice.optBoolean("earned", false)
                            // This line is only for a recognized game's launch
                            // notification. Never decorate unlock/error notices.
                            val isGameReady = !earned &&
                                detail.startsWith("You have ") &&
                                detail.contains(" achievements unlocked")
                            val uiNotice = RaAchievementOverlay.Notice(
                                title = title,
                                detail = detail,
                                earned = earned,
                                badge = notice.optString("badge", "").takeIf { it.isNotBlank() },
                                onlineMode = if (isGameReady) hasValidatedInternet() else null
                            )
                            if (isGameReady && !gameActive) {
                                // During the cartridge seating/CRT animation,
                                // native RA's cached game can already be ready.
                                // Don't discard that valid launch notification.
                                // But never display an old status after ejection.
                                if (raLaunchPending) raPendingGameNotice = uiNotice
                            } else {
                                raAchievementOverlay.push(uiNotice)
                            }
                        }
                    } catch (_: Exception) {
                        SlotLog.w(TAG, "Ignoring malformed RetroAchievements notice")
                    }
                }
                // Ejection cleanup is edge-triggered above, not performed
                // every idle/loading poll (which suppressed RA in Beta19).
            }
            if (!isDestroyed && resumed) uiHandler.postDelayed(this, 100L)
        }
    }
    // Runs on the main thread, queueing real storage I/O on gameLoader.
    // Called at the 100ms foreground pulse AND after the final suspend save,
    // so stopping the UI timer cannot strand an export until next resume.
    private fun drainPendingSaveFlush() {
        // Flush requests before accepting a new ROM launch to preserve
        // save-before-load ordering on the single storage worker.
        var completed = nativePollSaveFlush()
        while (completed != null) {
            try {
                val payload = JSONObject(completed)
                val uri = payload.getString("uri")
                val core = Systems.folderForKey(payload.getString("core"))
                val target = activeTargets[uri]
                if (target != null && target.core == core) {
                    gameLoader.execute {
                        val stamp = payload.optString("manual_stamp", "")
                        val deleted = payload.optString("deleted_stamp", "")
                        val failures = if (stamp.isNotEmpty()) {
                            RetroArchStorage.exportManualState(this@MainActivity, target, stamp)
                        } else if (deleted.isNotEmpty()) {
                            RetroArchStorage.deleteManualState(this@MainActivity, target, deleted)
                        } else {
                            runOnUiThread {
                                // The slot. folder catches up shortly after.
                                uiHandler.removeCallbacks(slotFolderPush)
                                uiHandler.postDelayed(slotFolderPush, 5_000L)
                            }
                            RetroArchStorage.exportAfterSave(this@MainActivity, target).also {
                                // Then to RomM, when Sync Saves with RomM is on:
                                // on its own queue, the next launch doesn't wait.
                                // The resume state follows its save (Sync States, 3.0).
                                if (RommSaves.enabled(this@MainActivity)) rommLater(target, upload = true) {
                                    var saveUploaded = false
                                    var stateUploaded = false
                                    val save = RommSaves.afterSave(this@MainActivity, target) { saveUploaded = true }
                                    val state = RommStates.afterSave(this@MainActivity, target) { stateUploaded = true }
                                    // Both transfers use this one worker. Report them only after
                                    // both finished; an unchanged or deferred file never counts.
                                    TaskPill.rommSyncWords(saveUploaded, stateUploaded)?.let { TaskPill.confirm(it) }
                                    save ?: state
                                }
                            }
                        }
                        if (failures.isNotEmpty()) {
                            runOnUiThread {
                                if (!isDestroyed) {
                                    SlotLog.w(TAG, "Save export warnings: " + failures.joinToString(" | "))
                                    val onlyMirrors = failures.all {
                                        it.startsWith("Shared GBA save sync deferred")
                                    }
                                    val notice = if (onlyMirrors)
                                        "Game saved. The copy for the other core will be made later."
                                    else exportNotice(failures)
                                    SlotToast.makeText(this@MainActivity, notice, Toast.LENGTH_LONG).show()
                                }
                            }
                        }
                    }
                }
            } catch (error: Exception) {
                SlotLog.e(TAG, "Invalid save sync notification", error)
            }
            completed = nativePollSaveFlush()
        }
    }

    private val scanSerial = AtomicInteger()
    private var lastStatusPoll: Long = -15000L
    // Fixed native shelf states mirror SHELF_STATUS_TEXT in runtime.rs.
    private val shelfNone = 0
    private val shelfScanning = 1
    /** Carts on the shelf, from the last library handed to the renderer. */
    private var shelfCarts = 0
    /** RomM's games downloading onto an empty shelf: its centred bar. */
    private val shelfDownloading = 8
    private val shelfFolderGone = 9
    private var downloadStatusShown = false
    private var downloadCountShown = -1
    private val shelfEmpty = 2
    private val shelfImportError = 3
    private val shelfPermissionExpired = 4
    private val shelfReadError = 5
    private val shelfPickerError = 6
    private val shelfPermissionError = 7

    private lateinit var view: SlotPresenter
    private var startupSplash: View? = null
    private lateinit var raAchievementOverlay: RaAchievementOverlay
    private lateinit var lidCover: LidCover
    /** RG Rotate's middle button is down: a Home Android sends meanwhile is
     *  its long press, the slot. button held. */
    private var rotateMiddleDown = false
    /** The middle button was held long enough: slot. ejected on its own. */
    private var rotateMiddleHeld = false
    /** Android takes the middle button at 0.5 s (Home, its release lost),
     *  so slot. counts the hold itself and ejects just before (measured on
     *  Siham's Rotate, 2026-11-14: Home came, the game only paused). */
    private val rotateMiddleHold = Runnable {
        if (!rotateMiddleDown) return@Runnable
        rotateMiddleHeld = true
        nativeSlotButtonReleased(true)
        wakeCarouselRender()
    }
    /** Keys swallowed by the closed RG Rotate, whose releases go too. */
    private val lidSwallowed = HashSet<Int>()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        getSystemService(InputManager::class.java)
            ?.registerInputDeviceListener(controllerListener, Handler(Looper.getMainLooper()))
        // A staged sign-out must finish before any native HTTP worker is started.
        val cleanSignOut = RaAccount.finishPendingSignOut(filesDir)
        // Device-specific *initial preference* only. Existing persisted
        // global/per-cartridge choices are never rewritten.
        // The Retroid Pocket Classic reports its maker as "Moorechip"
        // (getprop ro.product.manufacturer, Siham 2026-11-04): go by the model.
        val retroidPocketClassic = Build.MODEL.contains("Retroid Pocket Classic", ignoreCase = true)
        SlotLog.i(TAG, "Device: model=${Build.MODEL} manufacturer=${Build.MANUFACTURER} " +
            "retroid_pocket_classic=$retroidPocketClassic")
        nativeConfigure(filesDir.absolutePath, applicationInfo.nativeLibraryDir, retroidPocketClassic)
        nativeSetDevice(SlotDevice.displayName(Build.MANUFACTURER, Build.MODEL), Build.MODEL)
        // The slot. canvas takes the display's shape (3:2 on the KONKR, 16:9,
        // 4:3...) before the renderer builds its first face.
        windowManager.maximumWindowMetrics.bounds.let { panel ->
            SlotLog.i(TAG, "Panel ${panel.width()}x${panel.height()}")
            nativeSetPanelSize(panel.width(), panel.height())
            // A square panel (RG Rotate) has no landscape: asking for one
            // turned the screen 90 degrees for an instant on each unlock.
            if (panel.width() == panel.height()) {
                requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_NOSENSOR
            }
        }
        // App-private A12 A/B marker; no save or ROM folders are touched.
        val a12Recovery = !File(filesDir, "a12-mgba-legacy-pacing.flag").exists()
        nativeSetMgbaLinkRecoveryEnabled(a12Recovery)
        SlotLog.i(TAG, "A12 recovery mode: " + if (a12Recovery) "a11" else "legacy")
        nativeSetShareSaves(getSharedPreferences(PREFS, MODE_PRIVATE)
            .getBoolean(SHARE_GBA_SAVES, false))
        if (cleanSignOut && RaAccount.configuredUser(filesDir) != null) nativeRaAccountConfigured()
        cartSounds = CartSounds(this)
        val renderer = object : GLSurfaceView.Renderer {
            var ready = false
            private val graphicsRecovery = SlotGraphicsRecovery()
            private var surfaceWidth = 0
            private var surfaceHeight = 0

            private fun restoreGraphics(): Boolean {
                val restored = graphicsRecovery.attempt(SystemClock.uptimeMillis()) {
                    nativeSurfaceCreated().also { ok ->
                        if (!ok) SlotLog.e(TAG, "Slot GLES recovery failed; retrying with session retained")
                    }
                }
                if (restored) {
                    if (surfaceWidth > 0 && surfaceHeight > 0)
                        nativeSurfaceChanged(surfaceWidth, surfaceHeight)
                    nativePresentationResume()
                    runOnUiThread {
                        pushControllers()
                        uiHandler.postDelayed({ hideSlotStartupSplash() }, 650L)
                    }
                }
                return restored
            }
            private var probeStartNs = 0L
            private var lastDrawNs = 0L
            private var callbacks = 0
            private var gapOver25ms = 0
            private var gapOver40ms = 0
            private var gapUsMax = 0L
            private var nativeUsMax = 0L
            private var nativeOver16ms = 0
            private var nativeOver33ms = 0
            private var vsyncAgeTotalUs = 0L
            private var vsyncAgeMaxUs = 0L
            private var vsyncAgeCount = 0
            private val glPacingCorrelation = FramePacingCorrelation()
            fun resetTiming() {
                probeStartNs = 0L; lastDrawNs = 0L; callbacks = 0
                gapOver25ms = 0; gapOver40ms = 0; gapUsMax = 0L
                nativeUsMax = 0L; nativeOver16ms = 0; nativeOver33ms = 0
                vsyncAgeTotalUs = 0L; vsyncAgeMaxUs = 0L; vsyncAgeCount = 0
                glPacingCorrelation.reset()
                previousUiVsyncFrameNs = 0L
                latestVsyncFrameNs = 0L
                uiVsyncCallbacks.set(0L); uiVsyncOver25ms.set(0L); uiVsyncOver40ms.set(0L)
            }
            override fun onSurfaceCreated(gl: GL10?, config: EGLConfig?) {
                // The GL thread runs the core: give it Android's render-thread
                // priority so background work never delays a frame.
                try {
                    android.os.Process.setThreadPriority(
                        android.os.Process.THREAD_PRIORITY_DISPLAY)
                } catch (error: Exception) {
                    SlotLog.w(TAG, "GL thread display priority unavailable", error)
                }
                resetTiming()
                nativeEglPresentationReset()
                graphicsRecovery.reset()
                ready = restoreGraphics()
                glPacingCorrelation.reset()
            }
            override fun onSurfaceChanged(gl: GL10?, width: Int, height: Int) {
                surfaceWidth = width
                surfaceHeight = height
                nativeSurfaceChanged(width, height)
            }
            override fun onDrawFrame(gl: GL10?) {
                if (!ready) ready = restoreGraphics()
                val a12StartedNs = SystemClock.elapsedRealtimeNanos()
                // Beta8 observer: age of last Choreographer timestamp
                // when GL starts, not a measured display-present time.
                val vsyncStamp = latestVsyncFrameNs
                if (nativeIsPlaying() && vsyncStamp > 0) {
                    val ageUs = (System.nanoTime() - vsyncStamp) / 1000L
                    if (ageUs in 0L..200_000L) {
                        vsyncAgeTotalUs += ageUs
                        vsyncAgeMaxUs = maxOf(vsyncAgeMaxUs, ageUs)
                        vsyncAgeCount++
                    }
                }
                // Press read by this callback, timed to its display present.
                var pressedNs = 0L
                if (nativeIsPlaying()) pressedNs = EmulationDiagnostics.frame()
                var a12NativeUs = 0L
                if (ready) {
                    val beginNativeNs = SystemClock.elapsedRealtimeNanos()
                    // Swap-paced loop: this call's start time, right after
                    // the previous blocking swap, stands in for the VSync.
                    nativeDrawFrame(if (raSwapLoopApplied) System.nanoTime()
                        else if (displaySyncApplied) latestVsyncFrameNs else 0L)
                    a12NativeUs = (SystemClock.elapsedRealtimeNanos() - beginNativeNs) / 1000L
                    // Notify the existing AudioTrack worker that libretro
                    // may have produced samples. Never block GL on audio.
                    if (audioRunning.get() && nativeIsPlaying() &&
                        audioFrameSignal.availablePermits() == 0
                    ) audioFrameSignal.release()
                    // Drain on the render thread for frame-accurate insert/eject clicks.
                    var kind = nativePollCartSfx()
                    while (kind != 0) {
                        cartSounds.play(kind)
                        kind = nativePollCartSfx()
                    }
                } else {
                    GLES20.glClearColor(0f, 0f, 0f, 1f)
                    GLES20.glClear(GLES20.GL_COLOR_BUFFER_BIT)
                }
                // RetroArch's GPU Hard Sync in the swap-paced loop: queue only
                // a finished picture, so the compositor never waits on our GPU.
                if (raSwapLoopApplied) GLES20.glFinish()
                // Beta9: record the ID of the frame swapped AFTER this
                // callback and query earlier EGL presentation timestamps.
                // Driver can report unsupported; no rendering changes.
                if (ready && displaySyncApplied && nativeIsPlaying()) {
                    nativeEglPresentationTick(pressedNs)
                }
                // Diagnostic-only. EGL swaps after this callback returns:
                // callback gaps are not true presentation timestamps.
                if (ready && nativeIsPlaying()) {
                    if (probeStartNs == 0L) probeStartNs = a12StartedNs
                    var correlationGlGapUs = 0L
                    if (lastDrawNs != 0L) {
                        val gapUs = (a12StartedNs - lastDrawNs) / 1000L
                        correlationGlGapUs = gapUs
                        gapUsMax = maxOf(gapUsMax, gapUs)
                        if (gapUs > 25_000L) gapOver25ms++
                        if (gapUs > 40_000L) gapOver40ms++
                    }
                    lastDrawNs = a12StartedNs
                    glPacingCorrelation.record(correlationGlGapUs, a12NativeUs,
                        if (displaySyncApplied) latestVsyncSequence else 0L)
                    callbacks++
                    nativeUsMax = maxOf(nativeUsMax, a12NativeUs)
                    if (a12NativeUs > 16_667L) nativeOver16ms++
                    if (a12NativeUs > 33_333L) nativeOver33ms++
                    val spanNs = SystemClock.elapsedRealtimeNanos() - probeStartNs
                    if (spanNs >= 5_000_000_000L) {
                        val fpsX10 = callbacks.toLong() * 10_000_000_000L / spanNs
                        SlotLog.i(TAG, "GL callback pacing: callbacks=$callbacks fps_x10=$fpsX10 " +
                            "gap_over_25ms=$gapOver25ms gap_over_40ms=$gapOver40ms " +
                            "gap_us_max=$gapUsMax native_us_max=$nativeUsMax " +
                            "native_over_16ms=$nativeOver16ms native_over_33ms=$nativeOver33ms " +
                            "vsync_gl_age_mean_us=${if (vsyncAgeCount > 0) vsyncAgeTotalUs / vsyncAgeCount else -1} " +
                            "vsync_gl_age_max_us=$vsyncAgeMaxUs " +
                            "early_pcm_wakes_5s=${earlyPcmWakes.getAndSet(0)}")
                        val frameCorrelation = glPacingCorrelation.snapshotAndReset()
                        SlotLog.i(TAG, "GL vsync correlation: scheduler=ui " +
                            "ui_vsync_callbacks=${uiVsyncCallbacks.getAndSet(0L)} " +
                            "ui_vsync_over25=${uiVsyncOver25ms.getAndSet(0L)} " +
                            "ui_vsync_over40=${uiVsyncOver40ms.getAndSet(0L)} " +
                            "gl_gap_over25=${frameCorrelation.slowGaps} " +
                            "gl_gap_prior_native_over16=${frameCorrelation.slowGapsAfterExpensiveNative} " +
                            "gl_gap_prior_native_normal=${frameCorrelation.slowGapsAfterNormalNative} " +
                            "gl_gap_current_native_over16=${frameCorrelation.slowGapsWithExpensiveCurrentNative} " +
                            "gl_reused_vsync=${frameCorrelation.glCallbacksWithRepeatedVsync} " +
                            "ui_vsync_without_gl=${frameCorrelation.vsyncCallbacksWithoutGl} " +
                            "not_actual_present_time=true")
                        if (displaySyncApplied) {
                            SlotLog.i(TAG, nativeEglPresentationReport())
                        }
                        probeStartNs = a12StartedNs
                        callbacks = 0
                        gapOver25ms = 0
                        gapOver40ms = 0
                        gapUsMax = 0L
                        nativeUsMax = 0L
                        nativeOver16ms = 0
                        nativeOver33ms = 0
                        vsyncAgeTotalUs = 0L
                        vsyncAgeMaxUs = 0L
                        vsyncAgeCount = 0
                    }
                } else {
                    probeStartNs = 0L
                    lastDrawNs = 0L
                    glPacingCorrelation.reset()
                    uiVsyncCallbacks.getAndSet(0L)
                    uiVsyncOver25ms.getAndSet(0L)
                    uiVsyncOver40ms.getAndSet(0L)
                }
            }
        }
        // Drop the preferences of two beta experiments (controlled-EGL
        // presenter, latch wait), both measured worse than the plain loop.
        getSharedPreferences(PREFS, MODE_PRIVATE).edit()
            .remove("development_presenter").remove("latch_wait_v3").apply()
        view = SlotPresenter(this, renderer)
        val frame = FrameLayout(this)
        frame.addView(view.surfaceView, FrameLayout.LayoutParams(-1, -1))
        shelfStatus = SlotStatusIndicators(this, frame)
        taskIndicators = SlotShelfTaskIndicators(this, frame)
        libraryRefreshIndicator = taskIndicators.library
        autoLabelIndicator = taskIndicators.labels
        // There is no Android status view: the native Slot renderer owns all
        // shelf messages and controller hints, even during ROM rescanning.
        performanceHud = TextView(this).apply {
            typeface = SlotUi.font(this@MainActivity)
            textSize = 10.5f
            letterSpacing = .05f
            setTextColor(Color.argb(230, 242, 242, 245))
            // Subtle translucent HUD; keep the game visible behind it.
            setBackgroundColor(Color.argb(166, 23, 22, 25))
            setPadding(10, 5, 10, 5)
            isClickable = false
            isFocusable = false
            visibility = View.GONE
        }
        frame.addView(performanceHud,
            FrameLayout.LayoutParams(-2, -2, Gravity.BOTTOM or Gravity.END).apply {
                rightMargin = (12 * resources.displayMetrics.density).toInt()
                bottomMargin = (12 * resources.displayMetrics.density).toInt()
            })
        raAchievementOverlay = RaAchievementOverlay(this, frame)
        lidCover = LidCover(this) { if (::shelfStatus.isInitialized) shelfStatus else null }
        if (RgRotate.device) {
            // Android's own rotation (Anbernic "Direction follows system")
            // turns a frozen snapshot for 0.35 s over slot.'s cover
            // animation (videos, Siham 2026-11-14). A straight cut (446)
            // still showed slot.'s last frames sideways for 0.2 s: seamless
            // keeps each frame in the orientation it was drawn for.
            window.attributes = window.attributes.apply {
                rotationAnimation = WindowManager.LayoutParams.ROTATION_ANIMATION_SEAMLESS
            }
        }
        showSlotStartupSplash(frame)
        setContentView(frame)
        immersive()
        uiHandler.post(pulse)
        uiHandler.removeCallbacks(rumblePulse)
        uiHandler.post(rumblePulse)
        if (!cleanSignOut) {
            uiHandler.postDelayed({
                SlotToast.makeText(this,
                    "Last RA sign-out unfinished. Unlocks safe.",
                    Toast.LENGTH_LONG).show()
            }, 800L)
        }
        startRaNetworkWatch()
        startRaSmartCache()
        // Stage BIOS files before the first queued ROM launch, using the same
        // worker that loads ROMs; no race between BIOS import and core startup.
        getSharedPreferences(PREFS, MODE_PRIVATE).getString(BIOS_ROOT, null)
            ?.let { savedBios ->
                val uri = Uri.parse(savedBios)
                if (contentResolver.persistedUriPermissions.any { perm ->
                        perm.uri == uri && perm.isReadPermission
                    }) {
                    importBiosFolder(uri, persistSelection = false)
                } else {
                    SlotLog.w(TAG, "BIOS folder access expired; choose it again")
                }
            }
        val saved = getSharedPreferences(PREFS, MODE_PRIVATE).getString(ROM_ROOT, null)
        if (saved != null) {
            val uri = Uri.parse(saved)
            if (contentResolver.persistedUriPermissions.any {
                    it.uri == uri && it.isReadPermission
                }) {
                if (restoreCachedLibrary(uri)) {
                    // Use the same fast cached boot as before; verify the SAF
                    // tree off-thread behind the splash with no UI delay.
                    scanFolder(uri, quiet = true)
                } else scanFolder(uri)
            } else {
                nativeSetShelfStatus(shelfPermissionExpired)
            }
        } else {
            // The native first-run keycap appears on an empty shelf.
            nativeSetShelfStatus(shelfNone)
        }
    }

    /** The unmodified, original Slot v1.5.0 firmware bootlogo.bmp is a 24-bit BMP.
     *  Decode its BGR rows explicitly: Android BitmapFactory does not promise BMP support.
     *  This is an in-app startup screen, never a device/firmware boot partition write.
     */
    private fun readOriginalSlotBootLogo(): Bitmap? = try {
        val bytes = assets.open("slot-bootlogo.bmp").use { it.readBytes() }
        if (bytes.size < 54 || bytes[0] != 'B'.code.toByte() ||
            bytes[1] != 'M'.code.toByte()) {
            null
        } else {
            val data = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
            val offset = data.getInt(10)
            val width = data.getInt(18)
            val height = data.getInt(22)
            val planes = data.getShort(26).toInt() and 0xffff
            val bitDepth = data.getShort(28).toInt() and 0xffff
            val compression = data.getInt(30)
            val stride = if (width > 0) ((width.toLong() * 3L + 3L) / 4L) * 4L else 0L
            if (width <= 0 || height <= 0 || width > 2048 || height > 2048 ||
                planes != 1 || bitDepth != 24 || compression != 0 ||
                offset < 54 || offset.toLong() + stride * height > bytes.size) {
                null
            } else {
                val pixels = IntArray(width * height)
                for (y in 0 until height) {
                    val row = offset + ((height - 1 - y) * stride).toInt()
                    for (x in 0 until width) {
                        val at = row + x * 3
                        val b = bytes[at].toInt() and 0xff
                        val g = bytes[at + 1].toInt() and 0xff
                        val r = bytes[at + 2].toInt() and 0xff
                        pixels[y * width + x] = Color.rgb(r, g, b)
                    }
                }
                Bitmap.createBitmap(pixels, width, height, Bitmap.Config.ARGB_8888)
            }
        }
    } catch (error: Exception) {
        SlotLog.w(TAG, "Original Slot boot logo could not be loaded", error)
        null
    }

    private fun showSlotStartupSplash(frame: FrameLayout) {
        // Use the original approved Slot. v1 wordmark as a VectorDrawable.
        // The old 960x640 bitmap was enlarged from a 196x75 firmware logo and
        // looks soft at KONKR resolution. Preserve the original image as a
        // fallback, without touching firmware assets or the emulation UI.
        val vectorLogo = try {
            getDrawable(R.drawable.slot_boot_wordmark_vector)
        } catch (error: Exception) {
            SlotLog.w(TAG, "Vector Slot boot wordmark unavailable", error)
            null
        }
        val fallbackBitmap = if (vectorLogo == null) try {
            assets.open("slot-bootlogo-kpa-960x640.png").use(BitmapFactory::decodeStream)
        } catch (error: Exception) {
            SlotLog.w(TAG, "Original Slot boot logo fallback unavailable", error)
            null
        } else null
        if (vectorLogo == null && fallbackBitmap == null) return

        val overlay = FrameLayout(this).apply {
            setBackgroundColor(Color.rgb(11, 14, 17))
            isFocusable = false
            isClickable = false
        }
        val image = ImageView(this).apply {
            if (vectorLogo != null) setImageDrawable(vectorLogo)
            else setImageBitmap(fallbackBitmap)
            scaleType = ImageView.ScaleType.FIT_CENTER
            contentDescription = "slot. boot logo"
        }
        overlay.addView(image, FrameLayout.LayoutParams(-1, -1))
        frame.addView(overlay, FrameLayout.LayoutParams(-1, -1))
        startupSplash = overlay
    }

    private fun hideSlotStartupSplash() {
        val splash = startupSplash ?: return
        splash.animate().alpha(0f).setDuration(220L).withEndAction {
            (splash.parent as? FrameLayout)?.removeView(splash)
            startupSplash = null
        }.start()
    }

    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        if (RgRotate.lidKey(keyCode, event) && ::lidCover.isInitialized) {
            if (event.repeatCount == 0) {
                // How late Android hands slot. the lid's key (beta43).
                val now = SystemClock.uptimeMillis()
                SlotLog.i(TAG, "Rotate: [$now] lid key down, ${now - event.eventTime} ms after the sensor; lidClosed=${lidCover.closed}")
                lidCover.learnRotation(true)
            }
            // Repeats too: a held lid is shut, even if a release was misread.
            lidCover.close()
            return true
        }
        if (::lidCover.isInitialized && lidCover.closed) {
            // Shut, the Rotate hides its face buttons: one of them means the
            // lid is open again (its own key may have been missed asleep).
            if (event.repeatCount == 0 && RgRotate.opensLid(keyCode, event)) lidCover.open()
            else lidCover.awake()
            lidSwallowed.add(keyCode)
            return true
        }
        SlotBuildBehavior.emulatorButton(keyCode)?.let { button ->
            EmulationDiagnostics.input(keyCode, true, event, button)
            if (event.repeatCount == 0) {
                warmedCandidate.set(null)
                wakeCarouselRender()
                nativeKey(button, true)
            }
            return true
        }
        if (isRetroidSlotKey(keyCode, event) || RgRotate.middleKey(keyCode, event)) {
            // The Retroid's round centre key sends BACK: there it is the
            // slot. button, with the KONKR top key's exact behaviour. So is
            // the RG Rotate's middle (BACK) button. Its round side button
            // (F10) is left to Android, which switches launchers with it.
            if (event.repeatCount == 0) {
                if (RgRotate.middleKey(keyCode, event)) {
                    rotateMiddleDown = true
                    rotateMiddleHeld = false
                    uiHandler.removeCallbacks(rotateMiddleHold)
                    uiHandler.postDelayed(rotateMiddleHold, RgRotate.HOLD_MS)
                }
                EmulationDiagnostics.input(keyCode, true, event, KeyEvent.KEYCODE_BUTTON_MODE)
                warmedCandidate.set(null)
                // Stage the key before waking a dozing shelf: no stale frame.
                nativeKey(KeyEvent.KEYCODE_BUTTON_MODE, true)
                wakeCarouselRender()
            }
            return true
        }
        if (keyCode == KeyEvent.KEYCODE_BACK) {
            onBackPressed()
            return true
        }
        if (keyCode == KeyEvent.KEYCODE_BUTTON_MODE && event.repeatCount == 0) {
            SlotLog.i(TAG, "KONKR MENU BTN_MODE down: Android code=" + keyCode +
                " scan=" + event.scanCode + " device=" + event.deviceId)
        }
        // START / SELECT on the shelf are now rendered by Slot itself.
        // Log any otherwise unrecognized physical key so the KONKR's top-round
        // key can be mapped from real device evidence, not a guessed keycode.
        if (!readsKey(keyCode)) {
            if (event.repeatCount == 0) {
                SlotLog.i(TAG, "Unmapped hardware key down: code=" + keyCode +
                    " name=" + KeyEvent.keyCodeToString(keyCode) +
                    " device=" + event.deviceId + " scan=" + event.scanCode)
            }
            return super.onKeyDown(keyCode, event)
        }
        if (event.repeatCount == 0) {
            EmulationDiagnostics.input(keyCode, true, event, keyCode)
            warmedCandidate.set(null)
            wakeCarouselRender()
            sendKey(event, keyCode, true, fromStick(event))
        }
        return true
    }

    /** A controller's own device, or null for a key that has none (virtual). */
    private fun controllerOf(event: InputEvent): InputDevice? {
        val device = event.device ?: return null
        return if (device.isVirtual || event.deviceId <= 0) null else device
    }

    private fun isController(device: InputDevice): Boolean {
        val sources = device.sources
        return !device.isVirtual && (
            (sources and InputDevice.SOURCE_GAMEPAD) == InputDevice.SOURCE_GAMEPAD ||
            (sources and InputDevice.SOURCE_JOYSTICK) == InputDevice.SOURCE_JOYSTICK ||
            (sources and InputDevice.SOURCE_DPAD) == InputDevice.SOURCE_DPAD)
    }

    /** Tell slot. every controller connected now. The handheld's own input
     *  devices are all one controller, "builtin"; any other is known by its
     *  descriptor, which stays the same when it reconnects. */
    private fun pushControllers() {
        val list = JSONArray()
        knownControllers.clear()
        for (id in InputDevice.getDeviceIds()) {
            val device = InputDevice.getDevice(id) ?: continue
            if (!isController(device)) continue
            knownControllers.add(id)
            list.put(JSONObject()
                .put("id", id)
                .put("name", device.name ?: "Controller")
                .put("fp", if (isBuiltIn(device)) "builtin" else device.descriptor)
                // L3 / R3 (stick clicks): the Hotkeys page offers them only if present.
                .put("sticks", device.hasKeys(KeyEvent.KEYCODE_BUTTON_THUMBL,
                    KeyEvent.KEYCODE_BUTTON_THUMBR).all { it })
                // C and Z (six-button pads): Systems' Controls offer a C / Z row.
                .put("cz", device.hasKeys(KeyEvent.KEYCODE_BUTTON_C,
                    KeyEvent.KEYCODE_BUTTON_Z).all { it }))
        }
        nativeControllers(list.toString())
    }

    /** The handheld's own controls. Some report as external: the KONKR's
     *  are an internal USB "Microsoft X-box 360 pad" (045e:028e, dumpsys
     *  input 2026-11-06), and a Retroid's may be too. Anbernic's (the RG
     *  Rotate's) is an external "Retrogame_Joypad" (Siham 2026-11-15). */
    private fun isBuiltIn(device: InputDevice): Boolean =
        !device.isExternal ||
            (device.vendorId == 0x045e && device.productId == 0x028e) ||
            device.name.orEmpty().contains("Retroid", ignoreCase = true) ||
            device.name.orEmpty().equals("Retrogame_Joypad", ignoreCase = true)

    /** A key (or hat direction) of a controller: to Settings > Controls. */
    private fun sendKey(event: InputEvent, code: Int, pressed: Boolean, stick: Boolean = false) {
        val device = controllerOf(event)
        if (device == null) {
            nativeKey(code, pressed)
            return
        }
        if (event.deviceId !in knownControllers && isController(device)) pushControllers()
        nativePadKey(event.deviceId, code, pressed, stick)
    }

    /** A D-pad key Android made from a controller's left stick. */
    private fun fromStick(event: KeyEvent): Boolean =
        (event.flags and KeyEvent.FLAG_FALLBACK) != 0 &&
            event.keyCode in DPAD_KEYS

    /** Keys slot. reads: its own, plus any gamepad button a controller may
     *  send instead (C, Z, 1-16...), which Settings > Controls can map. */
    private var screenKeptOn = false

    /** A game running (not paused, not under the closed RG Rotate's cover)
     *  keeps the screen on: a cut-scene or a slow puzzle without a press no
     *  longer sends the console to sleep (Siham, 2026-11-14). Menus and the
     *  shelf keep Android's own screen timeout. */
    private fun keepScreenOn(on: Boolean) {
        if (on == screenKeptOn) return
        screenKeptOn = on
        if (on) window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        else window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
    }

    private fun readsKey(keyCode: Int): Boolean =
        keyCode in BUTTONS || KeyEvent.isGamepadButton(keyCode)

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        // RG Rotate: holding the middle button sends Android Home, which
        // brings back slot. (the home app). That hold is the slot. button's.
        val home = intent.hasCategory(Intent.CATEGORY_HOME) ||
            intent.getBooleanExtra(SlotHomeActivity.EXTRA_HOME, false)
        if (rotateMiddleDown && home) {
            rotateMiddleDown = false
            uiHandler.removeCallbacks(rotateMiddleHold)
            if (!rotateMiddleHeld) nativeSlotButtonReleased(true)
            rotateMiddleHeld = false
        }
    }

    /** What a failed copy to the save folders says: which copy (Daniel
     *  Byon, 2026-11-15). The full reasons go to logcat. */
    private fun exportNotice(failures: List<String>): String {
        val kinds = failures.filterNot { it.startsWith("Shared GBA save sync deferred") }
        val saves = kinds.any { it.startsWith("Save sync") || it.startsWith("RTC sync") }
        val states = kinds.any { it.startsWith("State sync") || it.startsWith("Manual state") }
        return when {
            saves && states -> "Game saved in slot., but it couldn't be copied to your save folders."
            saves -> "Game saved in slot., but the save couldn't be copied to your save folder."
            states -> "Game saved in slot., but the save state couldn't be copied to your state folder."
            else -> "Game saved, but some copies to your save folders failed."
        }
    }

    override fun dispatchTouchEvent(event: MotionEvent): Boolean {
        if (::lidCover.isInitialized && lidCover.closed) {
            lidCover.awake()
            return true
        }
        sendTouch(event)
        return super.dispatchTouchEvent(event)
    }

    /** Touch on the carousel (runtime/touch.rs): one finger, its position
     *  from 0 to 1 across the view. The native side ignores it anywhere but
     *  on the carousel and in a PokeDaisy game (its touchscreen). A second
     *  finger cancels. */
    private fun sendTouch(event: MotionEvent) {
        if (!::view.isInitialized) return
        val playing = nativeIsPlaying()
        val w = view.surfaceView.width.toFloat()
        val h = view.surfaceView.height.toFloat()
        if (w <= 0f || h <= 0f) return
        val action = when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> 0
            MotionEvent.ACTION_MOVE -> 1
            MotionEvent.ACTION_UP -> 2
            MotionEvent.ACTION_CANCEL, MotionEvent.ACTION_POINTER_DOWN -> 3
            else -> return
        }
        if (action == 1) {
            // Android batches moves to the frame: its samples in between
            // keep a fling's speed exact.
            for (i in 0 until event.historySize) {
                nativeTouch(1, event.getHistoricalX(0, i) / w,
                    event.getHistoricalY(0, i) / h, event.getHistoricalEventTime(i))
            }
        }
        nativeTouch(action, event.getX(0) / w, event.getY(0) / h, event.eventTime)
        if (!playing) wakeCarouselRender()
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus && ::view.isInitialized) requestUnbufferedJoystick()
    }

    // Android batches joystick motion to the next UI frame; RetroArch's
    // input queue reads it as soon as it arrives. Do the same for a D-pad
    // that reports as a hat. The request only counts once the views are in
    // the window and follows the focused view, so it is renewed whenever
    // the window gains focus, on both the game view and the window root.
    private fun requestUnbufferedJoystick() {
        for (target in listOf<View>(view.surfaceView, window.decorView)) {
            target.requestUnbufferedDispatch(InputDevice.SOURCE_CLASS_NONE)
            target.requestUnbufferedDispatch(InputDevice.SOURCE_CLASS_JOYSTICK)
        }
    }

    // A D-pad reporting as a hat (ABS_HAT0X/Y) reaches Android as joystick
    // motion, which the framework only turns into DPAD keys afterwards.
    // RetroArch reads the hat axes itself: so does slot., with no detour.
    override fun onGenericMotionEvent(event: MotionEvent): Boolean {
        val device = event.device
        if (event.action != MotionEvent.ACTION_MOVE ||
            !event.isFromSource(InputDevice.SOURCE_JOYSTICK) ||
            device?.getMotionRange(MotionEvent.AXIS_HAT_X) == null
        ) return super.onGenericMotionEvent(event)
        fun axis(value: Float) = if (value <= -0.5f) -1 else if (value >= 0.5f) 1 else 0
        val x = axis(event.getAxisValue(MotionEvent.AXIS_HAT_X))
        val y = axis(event.getAxisValue(MotionEvent.AXIS_HAT_Y))
        val hat = hats.getOrPut(event.deviceId) { IntArray(2) }
        val hatX = hat[0]
        val hatY = hat[1]
        if (x != hatX || y != hatY) {
            EmulationDiagnostics.hat(event, (x != 0 && x != hatX) || (y != 0 && y != hatY))
            warmedCandidate.set(null)
            wakeCarouselRender()
            if (x != hatX) {
                if (hatX < 0) sendKey(event, KeyEvent.KEYCODE_DPAD_LEFT, false)
                if (hatX > 0) sendKey(event, KeyEvent.KEYCODE_DPAD_RIGHT, false)
                if (x < 0) sendKey(event, KeyEvent.KEYCODE_DPAD_LEFT, true)
                if (x > 0) sendKey(event, KeyEvent.KEYCODE_DPAD_RIGHT, true)
                hat[0] = x
            }
            if (y != hatY) {
                if (hatY < 0) sendKey(event, KeyEvent.KEYCODE_DPAD_UP, false)
                if (hatY > 0) sendKey(event, KeyEvent.KEYCODE_DPAD_DOWN, false)
                if (y < 0) sendKey(event, KeyEvent.KEYCODE_DPAD_UP, true)
                if (y > 0) sendKey(event, KeyEvent.KEYCODE_DPAD_DOWN, true)
                hat[1] = y
            }
        }
        // A stick on the same device still goes through Android's own
        // stick-to-DPAD conversion; the hat state above is idempotent.
        // That conversion only releases a direction once it sees the stick
        // back under the threshold, so the first event after a move is
        // forwarded too, or the direction stays held.
        val stickMoved = Math.abs(event.getAxisValue(MotionEvent.AXIS_X)) >= 0.5f ||
            Math.abs(event.getAxisValue(MotionEvent.AXIS_Y)) >= 0.5f
        val forward = stickMoved || stickForwarded
        stickForwarded = stickMoved
        return if (forward) super.onGenericMotionEvent(event) else true
    }

    override fun onKeyUp(keyCode: Int, event: KeyEvent): Boolean {
        if (RgRotate.lidKey(keyCode, event) && ::lidCover.isInitialized) {
            // Android cancels the held lid key when slot. loses focus (the
            // lock screen taking over at sleep): that release is not the
            // lid opening (2.4.0-menus9 on Siham's Rotate, 2026-11-14).
            if (event.isCanceled) {
                SlotLog.i(TAG, "Rotate: canceled lid release ignored, still closed")
                return true
            }
            val now = SystemClock.uptimeMillis()
            SlotLog.i(TAG, "Rotate: [$now] lid key up, ${now - event.eventTime} ms after the sensor")
            lidCover.open()
            lidCover.learnRotation(false)
            return true
        }
        if (lidSwallowed.remove(keyCode)) return true
        SlotBuildBehavior.emulatorButton(keyCode)?.let { button ->
            EmulationDiagnostics.input(keyCode, false, event, button)
            warmedCandidate.set(null)
            wakeCarouselRender()
            nativeKey(button, false)
            return true
        }
        if (isRetroidSlotKey(keyCode, event) || RgRotate.middleKey(keyCode, event)) {
            EmulationDiagnostics.input(keyCode, false, event, KeyEvent.KEYCODE_BUTTON_MODE)
            warmedCandidate.set(null)
            wakeCarouselRender()
            val middle = RgRotate.middleKey(keyCode, event)
            if (middle) {
                rotateMiddleDown = false
                uiHandler.removeCallbacks(rotateMiddleHold)
                // Already ejected by the hold: the release is nothing more.
                if (rotateMiddleHeld) {
                    rotateMiddleHeld = false
                    return true
                }
            }
            if (event.isCanceled) {
                // The system took a long press (RG Rotate): no tap. The
                // middle button's is the slot. button held: eject.
                nativeSlotButtonReleased(middle)
            } else {
                nativeKey(KeyEvent.KEYCODE_BUTTON_MODE, false)
            }
            return true
        }
        if (keyCode == KeyEvent.KEYCODE_BACK) return true
        if (keyCode == KeyEvent.KEYCODE_BUTTON_MODE) {
            SlotLog.i(TAG, "KONKR MENU BTN_MODE up: Android code=" + keyCode)
        }
        if (!readsKey(keyCode)) return super.onKeyUp(keyCode, event)
        EmulationDiagnostics.input(keyCode, false, event, keyCode)
        warmedCandidate.set(null)
        wakeCarouselRender()
        sendKey(event, keyCode, false, fromStick(event))
        return true
    }

    /** BACK from a Retroid's built-in controller, not the navigation bar. */
    private fun isRetroidSlotKey(keyCode: Int, event: KeyEvent): Boolean {
        if (keyCode != KeyEvent.KEYCODE_BACK) return false
        val device = event.device ?: return false
        return !device.isVirtual && device.name.contains("Retroid", ignoreCase = true)
    }

    @Deprecated("Android 12 back key")
    override fun onBackPressed() {
        warmedCandidate.set(null)
        wakeCarouselRender()
        if (nativeIsPlaying()) {
            nativeExitGame()
        } else {
            nativeKey(KeyEvent.KEYCODE_BUTTON_B, true)
            nativeKey(KeyEvent.KEYCODE_BUTTON_B, false)
        }
    }

    private enum class SharedGbaChoice { MGBA, GPSP, CANCEL }

    /** Three-way conflict arbitration. Nothing is written to either
     * cartridge while the dialog is open; Cancel must abort ROM launch.
     */
    private fun askSharedGbaConflict(): SharedGbaChoice {
        val ready = CountDownLatch(1)
        val selected = java.util.concurrent.atomic.AtomicReference(SharedGbaChoice.CANCEL)
        runOnUiThread {
            if (isDestroyed || isFinishing) { ready.countDown(); return@runOnUiThread }
            try {
                val dialog = SlotModalDialog(this, "SAVE CONFLICT",
                    "mGBA and gpSP have different saves for this game. Choose which one to keep. " +
                        "Both are backed up.",
                    onDismiss = { ready.countDown() })
                // Default to the safe action: accidental A never changes SRAM.
                dialog.addButton("CANCEL") { dialog.dismiss() }
                dialog.addButton("KEEP mGBA SAVE") {
                    selected.set(SharedGbaChoice.MGBA)
                    dialog.dismiss()
                }
                dialog.addButton("KEEP gpSP SAVE") {
                    selected.set(SharedGbaChoice.GPSP)
                    dialog.dismiss()
                }
                dialog.show()
            } catch (error: Exception) {
                SlotLog.e(TAG, "Cannot open shared-save choice", error)
                ready.countDown()
            }
        }
        return if (ready.await(120L, TimeUnit.SECONDS)) selected.get()
            else SharedGbaChoice.CANCEL
    }

    /** Both RomM and this handheld have a newer save for the game: the
     *  player picks one, the other is kept beside it. Keeping this handheld's
     *  is the default, so an accidental A never replaces a save. */
    private fun askRommConflict(): RommSaves.Choice {
        val askedAt = SystemClock.elapsedRealtime()
        val ready = CountDownLatch(1)
        val selected = java.util.concurrent.atomic.AtomicReference(RommSaves.Choice.KEEP_HANDHELD)
        runOnUiThread {
            if (isDestroyed || isFinishing) { ready.countDown(); return@runOnUiThread }
            try {
                val dialog = SlotModalDialog(this, "ROMM SAVE",
                    "RomM and this handheld both have a newer save for this game. " +
                        "Which one do you want to play? The other one is kept.",
                    onDismiss = { ready.countDown() })
                dialog.addButton("KEEP THIS ONE") { dialog.dismiss() }
                dialog.addButton("USE ROMM'S") {
                    selected.set(RommSaves.Choice.USE_SERVER)
                    dialog.dismiss()
                }
                dialog.show()
            } catch (error: Exception) {
                SlotLog.e(TAG, "Cannot open the RomM save choice", error)
                ready.countDown()
            }
        }
        val answer = if (ready.await(120L, TimeUnit.SECONDS)) selected.get()
            else RommSaves.Choice.KEEP_HANDHELD
        nativeLaunchAsked(SystemClock.elapsedRealtime() - askedAt)
        return answer
    }

    /**
     * A save or save state deleted outside slot. (in the RetroArch folders,
     * or by hand in the slot. folder), asked before the game starts (Siham
     * 2026-11-18): PUT IT BACK as before, or DELETE IT everywhere. False:
     * cancelled (B), the game doesn't start.
     */
    private fun settleDeletedOutside(target: RetroArchStorage.Target): Boolean {
        val retroArch = RetroArchStorage.goneFromRetroArch(this, target)
        val inFolder = SaveDeletes.forGame(slotFolder.gonePaths(), target.core, target.stem,
            target.romIdentity, target.isGba && target.shareGbaSaves, target.saveCore)
        val kinds = retroArch + inFolder.values
        if (kinds.isEmpty()) return true
        val where = buildSet {
            if (retroArch.isNotEmpty()) add(SaveDeletes.Where.RETROARCH)
            if (inFolder.isNotEmpty()) add(SaveDeletes.Where.SLOT_FOLDER)
        }
        val gone = SaveDeletes.Gone(kinds, where, inFolder.keys)
        SlotLog.i(TAG, "Deleted outside slot.: ${target.core}/${target.stem} $kinds from $where")
        // A save alone: its game's save state holds it too, and would bring
        // it back at the next launch (Siham 2026-11-18).
        val withState = SaveDeletes.Kind.SAVE in kinds && SaveDeletes.Kind.STATE !in kinds &&
            RetroArchStorage.hasOwnState(this, target)
        when (askDeleted(SaveDeletes.message(target.stem, gone, withState))) {
            null -> return false
            SaveDeletes.Choice.PUT_BACK -> {
                if (retroArch.isNotEmpty()) {
                    RetroArchStorage.exportAfterSave(this, target).forEach { SlotLog.w(TAG, "Put back: $it") }
                }
                slotFolder.putBack(inFolder.keys)
            }
            SaveDeletes.Choice.DELETE -> {
                val all = if (withState) kinds + SaveDeletes.Kind.STATE else kinds
                RetroArchStorage.deleteEverywhere(this, target, all)
                if (SaveDeletes.Kind.SAVE in kinds) rommLater(target, upload = false) {
                    RommSaves.deleted(this, target)
                    null
                }
                if (SaveDeletes.Kind.STATE in all) rommLater(target, upload = false) {
                    RommStates.deleted(this, target)
                    null
                }
                slotFolder.deleted(inFolder.keys)
            }
        }
        return true
    }

    /** Which of a game's two saves to play (beta39): true for this
     *  handheld's, false for the folder's; null when cancelled (B). */
    private fun askTwoSaves(title: String, two: SlotFolder.TwoSavesFound): Boolean? {
        val askedAt = SystemClock.elapsedRealtime()
        val ready = CountDownLatch(1)
        val selected = java.util.concurrent.atomic.AtomicReference<Boolean?>(null)
        val day = java.text.SimpleDateFormat("d MMM HH:mm", java.util.Locale.ENGLISH)
        runOnUiThread {
            if (isDestroyed || isFinishing) { ready.countDown(); return@runOnUiThread }
            try {
                val dialog = SlotModalDialog(this, "TWO SAVES",
                    "Two saves for $title: one from this handheld (${day.format(java.util.Date(two.handheldModified))}), " +
                        "one from your slot. folder (${day.format(java.util.Date(two.folderModified))}). " +
                        "Which one should slot. use? The other one is kept.",
                    onDismiss = { ready.countDown() })
                dialog.addButton("THIS HANDHELD'S") {
                    selected.set(true)
                    dialog.dismiss()
                }
                dialog.addButton("THE FOLDER'S") {
                    selected.set(false)
                    dialog.dismiss()
                }
                dialog.show()
            } catch (error: Exception) {
                SlotLog.e(TAG, "Cannot open the two saves choice", error)
                ready.countDown()
            }
        }
        val answer = if (ready.await(120L, TimeUnit.SECONDS)) selected.get() else null
        nativeLaunchAsked(SystemClock.elapsedRealtime() - askedAt)
        return answer
    }

    /** A save another handheld deleted, seen through RomM (beta39): true
     *  to delete it here too; B or no answer keeps it. */
    /** Sync States with RomM (3.0): both the server's resume state and this
     *  handheld's changed since they last met. */
    private fun askRommStateConflict(): RommStates.Choice {
        val askedAt = SystemClock.elapsedRealtime()
        val ready = CountDownLatch(1)
        val selected = java.util.concurrent.atomic.AtomicReference(RommStates.Choice.KEEP_HANDHELD)
        runOnUiThread {
            if (isDestroyed || isFinishing) { ready.countDown(); return@runOnUiThread }
            try {
                val dialog = SlotModalDialog(this, "ROMM STATE",
                    "This game was also played from a state on another handheld. " +
                        "Which one do you want to resume? The other one is kept.",
                    onDismiss = { ready.countDown() })
                dialog.addButton("KEEP THIS ONE") { dialog.dismiss() }
                dialog.addButton("USE ROMM'S") {
                    selected.set(RommStates.Choice.USE_SERVER)
                    dialog.dismiss()
                }
                dialog.show()
            } catch (error: Exception) {
                SlotLog.e(TAG, "Cannot open the RomM state choice", error)
                ready.countDown()
            }
        }
        val answer = if (ready.await(120L, TimeUnit.SECONDS)) selected.get()
            else RommStates.Choice.KEEP_HANDHELD
        nativeLaunchAsked(SystemClock.elapsedRealtime() - askedAt)
        return answer
    }

    /** Sync States with RomM (3.0): the game's resume state is gone from
     *  the server. True to delete it here too. */
    private fun askStateDeletedElsewhere(title: String): Boolean {
        val askedAt = SystemClock.elapsedRealtime()
        val ready = CountDownLatch(1)
        val delete = java.util.concurrent.atomic.AtomicBoolean(false)
        runOnUiThread {
            if (isDestroyed || isFinishing) { ready.countDown(); return@runOnUiThread }
            try {
                val dialog = SlotModalDialog(this, "DELETED",
                    "The save state of $title was deleted from RomM. Delete it here too, " +
                        "or keep this handheld's?",
                    onDismiss = { ready.countDown() })
                dialog.addButton("DELETE IT") {
                    delete.set(true)
                    dialog.dismiss()
                }
                dialog.addButton("KEEP IT") { dialog.dismiss() }
                dialog.show()
            } catch (error: Exception) {
                SlotLog.e(TAG, "Cannot open the state deleted elsewhere choice", error)
                ready.countDown()
            }
        }
        val answer = ready.await(120L, TimeUnit.SECONDS) && delete.get()
        nativeLaunchAsked(SystemClock.elapsedRealtime() - askedAt)
        return answer
    }

    private fun askDeletedElsewhere(title: String): Boolean {
        val askedAt = SystemClock.elapsedRealtime()
        val ready = CountDownLatch(1)
        val delete = java.util.concurrent.atomic.AtomicBoolean(false)
        runOnUiThread {
            if (isDestroyed || isFinishing) { ready.countDown(); return@runOnUiThread }
            try {
                val dialog = SlotModalDialog(this, "DELETED",
                    "The save of $title was deleted from RomM. Delete it here too, " +
                        "or keep this handheld's?",
                    onDismiss = { ready.countDown() })
                dialog.addButton("DELETE IT") {
                    delete.set(true)
                    dialog.dismiss()
                }
                dialog.addButton("KEEP IT") { dialog.dismiss() }
                dialog.show()
            } catch (error: Exception) {
                SlotLog.e(TAG, "Cannot open the deleted elsewhere choice", error)
                ready.countDown()
            }
        }
        val answer = ready.await(120L, TimeUnit.SECONDS) && delete.get()
        nativeLaunchAsked(SystemClock.elapsedRealtime() - askedAt)
        return answer
    }

    /** The window of [settleDeletedOutside]. Null: cancelled (B). */
    private fun askDeleted(message: String): SaveDeletes.Choice? {
        val askedAt = SystemClock.elapsedRealtime()
        val ready = CountDownLatch(1)
        val selected = java.util.concurrent.atomic.AtomicReference<SaveDeletes.Choice?>(null)
        runOnUiThread {
            if (isDestroyed || isFinishing) { ready.countDown(); return@runOnUiThread }
            try {
                val dialog = SlotModalDialog(this, "DELETED", message, onDismiss = { ready.countDown() })
                dialog.addButton("PUT IT BACK") {
                    selected.set(SaveDeletes.Choice.PUT_BACK)
                    dialog.dismiss()
                }
                dialog.addButton("DELETE IT") {
                    selected.set(SaveDeletes.Choice.DELETE)
                    dialog.dismiss()
                }
                dialog.show()
            } catch (error: Exception) {
                SlotLog.e(TAG, "Cannot open the deleted save choice", error)
                ready.countDown()
            }
        }
        val answer = if (ready.await(120L, TimeUnit.SECONDS)) selected.get() else null
        nativeLaunchAsked(SystemClock.elapsedRealtime() - askedAt)
        return answer
    }

    /** Issue #116. Null: cancelled (B), the game doesn't start. */
    private fun askFolderSave(): RetroArchStorage.FolderSaveChoice? {
        val askedAt = SystemClock.elapsedRealtime()
        val ready = CountDownLatch(1)
        val selected = java.util.concurrent.atomic.AtomicReference<RetroArchStorage.FolderSaveChoice?>(null)
        runOnUiThread {
            if (isDestroyed || isFinishing) { ready.countDown(); return@runOnUiThread }
            try {
                val dialog = SlotModalDialog(this, "SAVE FOLDER",
                    "Your save folder has a different game save from where you left off in slot.",
                    onDismiss = { ready.countDown() })
                dialog.addButton("USE THE FOLDER'S SAVE") {
                    selected.set(RetroArchStorage.FolderSaveChoice.USE_FOLDER)
                    dialog.dismiss()
                }
                dialog.addButton("RESUME WHERE YOU LEFT OFF") {
                    selected.set(RetroArchStorage.FolderSaveChoice.RESUME)
                    dialog.dismiss()
                }
                dialog.show()
            } catch (error: Exception) {
                SlotLog.e(TAG, "Cannot open the save folder choice", error)
                ready.countDown()
            }
        }
        val answer = if (ready.await(120L, TimeUnit.SECONDS)) selected.get() else null
        nativeLaunchAsked(SystemClock.elapsedRealtime() - askedAt)
        return answer
    }

    private enum class CoreStateChoice { EXISTING, SHARED, CANCEL }

    /** This dialog is deliberately rare: only Share Saves ON, actual core
     *  change, and an existing destination-core auto-state. The ROM loader
     *  worker waits while the UI thread remains free to handle the answer.
     */
    private fun askCoreSwitchResume(core: String): CoreStateChoice {
        val ready = CountDownLatch(1)
        val selected = java.util.concurrent.atomic.AtomicReference(CoreStateChoice.CANCEL)
        runOnUiThread {
            if (isDestroyed || isFinishing) { ready.countDown(); return@runOnUiThread }
            try {
                val dialog = SlotModalDialog(this, "SAVE STATE",
                    "The $core save state may be older than your in-game save. " +
                        "Choose which one to load.",
                    onDismiss = { ready.countDown() })
                dialog.addButton("CANCEL") { dialog.dismiss() }
                dialog.addButton("IN-GAME SAVE") {
                    selected.set(CoreStateChoice.SHARED)
                    dialog.dismiss()
                }
                dialog.addButton("$core SAVE STATE") {
                    selected.set(CoreStateChoice.EXISTING)
                    dialog.dismiss()
                }
                dialog.show()
            } catch (error: Exception) {
                SlotLog.e(TAG, "Cannot open core-state choice", error)
                ready.countDown()
            }
        }
        return if (ready.await(120L, TimeUnit.SECONDS)) selected.get()
            else CoreStateChoice.CANCEL
    }

    private fun loadGameFromSaf(rawUri: String) {
        val uri = try { Uri.parse(rawUri) } catch (_: Exception) {
            nativeGameError(rawUri, "Invalid ROM URI")
            return
        }
        val queuedAt = SystemClock.elapsedRealtime()
        // Its warm copy, when one is under way, goes on for this launch.
        RomWarmCache.launching.set(rawUri)
        gameLoader.execute {
            val launchStart = SystemClock.elapsedRealtime()
            val queueWait = launchStart - queuedAt
            if (queueWait > 20) {
                SlotLog.i(TAG, "Slot launch queue delay: ${queueWait}ms (waiting for preceding save exports)")
            }
            // Which step failed, for the message on screen (runtime.rs).
            var step = "rom"
            try {
                // Document-provider IDs may contain the complete tree path:
                // keep only the terminal basename before using it for a
                // RetroArch save target, never a provider path or colon prefix.
                val name = uri.lastPathSegment.orEmpty().substringAfterLast('/')
                // SAF ZIPs contain an actual cartridge, not a libretro ZIP
                // input. Inspect the embedded extension before selecting a
                // core, a save target or a private cache extension.
                val ext = if (RomSource.isContainer(name)) {
                    contentResolver.openInputStream(uri)?.use { input ->
                        RomSource.inspect(input, name).extension
                    } ?: error("Cannot inspect cartridge ZIP")
                } else name.substringAfterLast('.', "").lowercase(java.util.Locale.ROOT)
                require(ext in Systems.ROM_EXTENSIONS) { "Unsupported ROM type" }
                // Saves, states and the cache by the game's portable name
                // (slot. 2.5), its files from 2.4 carried over first.
                val digest = GameKeys.digest(rawUri)
                GameKeys.carryGameFiles(File(filesDir, "Saves"), GameKeys.legacyDigest(rawUri), digest)
                val dir = File(cacheDir, "rom-cache").apply { mkdirs() }
                val target = File(dir, "$digest.$ext")
                // Always open via Android SAF; no assumptions about physical filesystem paths.
                // A warm copy of this ROM under way is waited for, not made twice.
                if (!target.isFile || target.length() < Systems.minRomBytes(ext)) RomWarmCache.awaitCopy(rawUri, target)
                if (!target.isFile || target.length() < Systems.minRomBytes(ext)) {
                    val temp = File(dir, "$digest.tmp")
                    try {
                        contentResolver.openInputStream(uri)?.use { input ->
                            RomSource.useRom(input, name) { raw, embeddedExt ->
                                require(embeddedExt == ext) { "ZIP cartridge type changed" }
                                temp.outputStream().buffered().use { output ->
                                    val buffer = ByteArray(64 * 1024)
                                    var copied = 0L
                                    while (true) {
                                        val n = raw.read(buffer)
                                        if (n < 0) break
                                        copied += n
                                        require(copied <= RomSource.MAX_ROM_BYTES) {
                                            "ROM exceeds 64 MiB"
                                        }
                                        output.write(buffer, 0, n)
                                    }
                                    require(copied >= Systems.minRomBytes(ext)) { "ROM too small" }
                                }
                            }
                        } ?: error("Cannot read this ROM")
                        check(temp.renameTo(target)) { "Cannot cache ROM" }
                    } finally { temp.delete() }
                }
                RomWarmCache.launching.compareAndSet(rawUri, null)
                RomWarmCache.played(target)
                step = "saves"
                val romReadyAt = SystemClock.elapsedRealtime()
                val savedCore = nativeCoreForUri(rawUri)
                // The native pick when it runs this system, else its default core.
                val core = Systems.coreFolder(savedCore, ext)
                val prefs = getSharedPreferences(PREFS, MODE_PRIVATE)
                val storage = SlotBuildBehavior.storagePolicy(
                    packageName,
                    prefs.getString(SAVE_ROOT, null),
                    prefs.getString(STATE_ROOT, null),
                    prefs.getBoolean(SHARE_GBA_SAVES, false)
                )
                val saves = storage.savesRoot?.let(Uri::parse)
                val states = storage.statesRoot?.let(Uri::parse)
                val shared = RetroArchStorage.target(this, rawUri, core, saves, states,
                    scannedFileName = if (RomSource.isContainer(name))
                        RomSource.archiveSaveFilename(name, ext) else null,
                    shareGbaSaves = storage.shareGbaSaves)
                // A save folder that can't be read no longer stops the game
                // (amadeus12, 2026-11-15): it starts with slot.'s own saves,
                // and nothing is copied to the folder this time, so a save
                // there that slot. couldn't read is never overwritten.
                // A save put in the folder outside slot. while slot. would
                // resume its own state: the player chooses (issue #116).
                // A save or state deleted outside slot.: the player says
                // first, put back or deleted everywhere (Siham 2026-11-18).
                // A copy with the slot. folder under way (slot. just came
                // back, the card just went in) finishes first: the game
                // starts from what it brings, and its deletions are known.
                // A whole copy (a folder just chosen) can take minutes: the
                // player is told instead of waiting (beta38). A copy that
                // only writes to the folder isn't waited for.
                slotFolder.copyPercent()?.let { percent ->
                    nativeGameError(rawUri, "say:Copying to your slot. folder… $percent%")
                    return@execute
                }
                val folderWaitAt = SystemClock.elapsedRealtime()
                // Only this game's saves, states, cheats and settings are
                // waited for (3.0.1-beta11): the copy takes them first, the
                // other games' follow after the launch. A game whose own
                // files haven't come in doesn't start without them.
                if (slotFolder.launchFilesPending()) {
                    val own = setOf(shared.stem, GameKeys.digest(rawUri), GameKeys.legacyDigest(rawUri),
                        GameKeys.key(rawUri), GameKeys.legacyKey(rawUri))
                    if (!slotFolder.awaitGameFiles(own, 15_000L)) {
                        SlotLog.w(TAG, "Slot folder still bringing ${shared.stem}'s files: not launched")
                        nativeGameError(rawUri, "say:Still copying this game's saves from your slot. folder. Try again in a moment.")
                        return@execute
                    }
                }
                (SystemClock.elapsedRealtime() - folderWaitAt).takeIf { it > 20 }?.let {
                    SlotLog.i(TAG, "Slot launch waited ${it}ms for the slot. folder")
                }
                if (!settleDeletedOutside(shared)) {
                    nativeGameError(rawUri, "quiet:Deleted save choice cancelled")
                    return@execute
                }
                // Two different saves met in the slot. folder (beta39): the
                // player says which one this game starts from.
                slotFolder.twoSavesFor(shared)?.let { two ->
                    val handheldWanted = askTwoSaves(shared.stem, two) ?: run {
                        nativeGameError(rawUri, "quiet:Two saves choice cancelled")
                        return@execute
                    }
                    val useSetAside = handheldWanted == two.handheldSetAside
                    slotFolder.settleTwoSaves(two, useSetAside)
                    if (useSetAside) {
                        RetroArchStorage.markFolderSave(filesDir, shared.saveCore, shared.stem)
                        RetroArchStorage.applyFolderSaveMark(filesDir, shared.saveCore, shared.stem,
                            shared.romIdentity, shared.core)
                    }
                    SlotLog.i(TAG, "Two saves of ${shared.stem}: " +
                        (if (handheldWanted) "this handheld's" else "the folder's") + " chosen")
                }
                var folderChoice: RetroArchStorage.FolderSaveChoice? = null
                val folderFailed = try {
                    var importing = true
                    var conflictResolved = false
                    while (importing) {
                        try {
                            RetroArchStorage.importBeforeLaunch(this@MainActivity, shared, folderChoice)
                            importing = false
                        } catch (error: RetroArchStorage.FolderSaveChanged) {
                            folderChoice = askFolderSave()
                            if (folderChoice == null) {
                                nativeGameError(rawUri, "Save choice cancelled")
                                return@execute
                            }
                        } catch (error: RetroArchStorage.SaveConflict) {
                            // Asked once, as before: still in conflict, the
                            // folder is treated as unreadable below.
                            if (conflictResolved) throw error
                            conflictResolved = true
                            SlotLog.w(TAG, "Conflicting mGBA/gpSP SRAM needs explicit choice", error)
                            when (val choice = askSharedGbaConflict()) {
                                SharedGbaChoice.CANCEL -> {
                                    nativeGameError(rawUri, "Shared save selection cancelled")
                                    return@execute
                                }
                                SharedGbaChoice.MGBA, SharedGbaChoice.GPSP -> {
                                    val source = if (choice == SharedGbaChoice.MGBA)
                                        "mGBA" else "gpSP"
                                    RetroArchStorage.resolveSharedGbaConflict(
                                        this@MainActivity, shared, source)
                                }
                            }
                        }
                    }
                    false
                } catch (error: Exception) {
                    if (shared.savesTree == null && shared.statesTree == null) throw error
                    SlotLog.e(TAG, "Save folder unreadable; starting with slot.'s own saves", error)
                    true
                }
                if (folderFailed) {
                    runOnUiThread {
                        if (!isDestroyed) SlotToast.makeText(this@MainActivity,
                            "Couldn't read your save folder, so this game uses slot.'s own save. " +
                                "Choose the folder again in START > Library.",
                            Toast.LENGTH_LONG).show()
                    }
                }
                // Sync Saves with RomM (2.5): the server's newer save comes
                // in first. Never stops the game.
                // Not while this game's save is still going up: it's the newest.
                if (rommBusy.containsKey(rommKey(shared))) {
                    SlotLog.i(TAG, "RomM: ${shared.stem}'s save is still being sent; launching with it")
                } else {
                    val rommAt = SystemClock.elapsedRealtime()
                    val romm = RommSaves.beforeLaunch(this@MainActivity, shared, { askRommConflict() },
                        { askDeletedElsewhere(shared.stem) })
                    if (romm.deleteHere) {
                        // Another handheld deleted it (DELETE IT): gone here
                        // too, with its state, which holds it as well.
                        val kinds = if (RetroArchStorage.hasOwnState(this@MainActivity, shared))
                            setOf(SaveDeletes.Kind.SAVE, SaveDeletes.Kind.STATE) else setOf(SaveDeletes.Kind.SAVE)
                        RetroArchStorage.deleteEverywhere(this@MainActivity, shared, kinds)
                        if (SaveDeletes.Kind.STATE in kinds) rommLater(shared, upload = false) {
                            RommStates.deleted(this@MainActivity, shared)
                            null
                        }
                    }
                    // Sync States with RomM (3.0, beta): the server's newer
                    // resume state, for this core, in what's left of the wait.
                    // One wait for both, 2 s at most: none left, the
                    // handheld's state, checked again at the next launch.
                    val left = RommSaves.LAUNCH_BUDGET_MS - (SystemClock.elapsedRealtime() - rommAt)
                    val stateNote = if (romm.deleteHere || !RommStates.enabled(this@MainActivity)) null
                    else if (left < 150L) {
                        RommStates.unchecked(this@MainActivity, shared)
                        null
                    } else {
                        val states = RommStates.beforeLaunch(this@MainActivity, shared, romm, left,
                            { askRommStateConflict() }, { askStateDeletedElsewhere(shared.stem) })
                        if (states.deleteHere) {
                            RetroArchStorage.deleteEverywhere(this@MainActivity, shared, setOf(SaveDeletes.Kind.STATE))
                        }
                        states.message
                    }
                    (romm.message ?: stateNote)?.let { note ->
                        runOnUiThread {
                            if (!isDestroyed) SlotToast.makeText(this@MainActivity, note, Toast.LENGTH_SHORT).show()
                        }
                    }
                }
                step = "core"
                // Share Saves never silently chooses an old same-core state
                // over freshly imported cross-core SRAM. A direct same-core
                // launch has no dialogue and behaves exactly as before.
                val previous = if (ext == "gba")
                    (prefs.getString("last_gba_core_$digest", null)
                        ?: prefs.getString("last_gba_core_${GameKeys.legacyDigest(rawUri)}", null)) else null
                val switched = !folderFailed && ext == "gba" && shared.shareGbaSaves &&
                    previous != null && previous != core
                var resumeChoice = -1 // Existing default Slot behavior.
                if (switched) {
                    val exists = RetroArchStorage.hasCoreAutoState(this@MainActivity, shared)
                    if (RetroArchStorage.shouldOfferCoreResume(
                            shared.shareGbaSaves, previous, core, exists)) {
                        when (askCoreSwitchResume(core)) {
                            CoreStateChoice.CANCEL -> {
                                nativeGameError(rawUri, "Core switch cancelled")
                                return@execute
                            }
                            CoreStateChoice.EXISTING -> {
                                // Do not delete the one-shot SRAM guard until
                                // an explicitly requested external RetroArch
                                // state has actually been decoded and staged.
                                // Existing protected generations remain higher
                                // priority than stale external auto-states.
                                RetroArchStorage.importExplicitExistingState(
                                    this@MainActivity, shared)
                                val lower = RetroArchStorage.coreLower(core)
                                // The previous SRAM-first attempt may have
                                // allocated a generation but never written a
                                // state (e.g. Android killed the app before
                                // Save & Eject). Explicitly choosing the OLD
                                // state must not be masked by that empty
                                // redirect. Completed generations stay valid.
                                val genIndex = File(filesDir,
                                    "Saves/$digest.$lower.state-generation.active")
                                if (genIndex.isFile) {
                                    val name = genIndex.readText()
                                    val valid = name.startsWith("$digest.$lower.gen-") &&
                                        name.matches(Regex("[a-zA-Z0-9.-]{1,180}"))
                                    val genState = File(filesDir, "Saves/$name.state")
                                    if (!valid) error("Invalid protected state generation")
                                    if (!genState.isFile) {
                                        if (!genIndex.delete()) {
                                            error("Cannot select existing state over empty generation")
                                        }
                                        val hold = File(filesDir,
                                            "Saves/$digest.$lower.state-export-protected")
                                        if (hold.isFile && !hold.delete()) {
                                            error("Cannot clear empty generation export guard")
                                        }
                                    }
                                }
                                val skipped = File(filesDir,
                                    "Saves/$digest.$lower.skip-resume.once")
                                if (skipped.isFile && !skipped.delete()) {
                                    error("Cannot restore existing state while SRAM guard is active")
                                }
                                resumeChoice = 1
                            }
                            CoreStateChoice.SHARED -> {
                                val lower = RetroArchStorage.coreLower(core)
                                RetroArchStorage.stage(File(filesDir,
                                    "Saves/$digest.$lower.skip-resume.once"), byteArrayOf(1))
                                resumeChoice = 0
                            }
                        }
                    } else {
                        // No old state on the new core: start from shared
                        // SRAM, not a state from the previous emulator.
                        resumeChoice = 0
                    }
                }
                if (folderFailed) activeTargets.remove(rawUri) else activeTargets[rawUri] = shared
                val readyAt = SystemClock.elapsedRealtime()
                SlotLog.i(TAG, "Slot launch preparation: ROM cache=${romReadyAt - launchStart}ms, " +
                    "RetroArch import=${readyAt - romReadyAt}ms, total=${readyAt - launchStart}ms")
                nativeSetNextResumeMode(resumeChoice)
                nativeGameReady(rawUri, target.absolutePath)
                if (ext == "gba") prefs.edit().putString("last_gba_core_$digest", core).apply()
            } catch (error: Exception) {
                SlotLog.e(TAG, "Could not prepare ROM ($step)", error)
                nativeGameError(rawUri, "$step: " + (error.message ?: "Cannot open ROM"))
            } finally {
                RomWarmCache.launching.compareAndSet(rawUri, null)
            }
        }
    }

    private fun publishSensors() {
        val manager = sensorManager
        val mask = sensorTypes.sumOf { (bit, type) ->
            if (manager?.getDefaultSensor(type) != null) bit else 0
        }
        nativeSensorsAvailable(mask)
    }

    private fun updateSensors() {
        val wanted = if (resumed && nativeIsPlaying()) nativeSensorsWanted() else 0
        if (wanted == sensorsListening) return
        val manager = sensorManager ?: return
        manager.unregisterListener(sensorListener)
        for ((bit, type) in sensorTypes) {
            if (wanted and bit == 0) continue
            val sensor = manager.getDefaultSensor(type) ?: continue
            manager.registerListener(sensorListener, sensor, SensorManager.SENSOR_DELAY_GAME)
        }
        sensorsListening = wanted
        SlotLog.i(TAG, "Sensors listening: $wanted")
    }

    private fun stopSensors() {
        if (sensorsListening == 0) return
        sensorManager?.unregisterListener(sensorListener)
        sensorsListening = 0
    }

    private fun stopRumble() {
        if (lastRumbleAmplitude != 0) {
            try { vibrator?.cancel() } catch (_: RuntimeException) { }
        }
        lastRumbleAmplitude = 0
        lastRumbleAt = 0L
        rumbleUntil = 0L
    }

    private fun pollRumble() {
        if (!resumed || !nativeGameUnpaused()) { stopRumble(); return }
        val packet = nativePollRumbleStrength()
        val current = packet and 0xffff
        val peak = packet ushr 16
        val strength = maxOf(current, peak)
        val now = SystemClock.elapsedRealtime()
        if (strength == 0) {
            if (now >= rumbleUntil) stopRumble()
            return
        }
        val device = vibrator ?: return
        if (!device.hasVibrator()) return
        try {
            val intensity = if (device.hasAmplitudeControl())
                ((strength + 256) / 257).coerceIn(1, 255)
            else VibrationEffect.DEFAULT_AMPLITUDE
            // A transient gets a brief physical pulse. Renew a sustained
            // signal before expiry, without restarting it on every frame.
            if (peak > current) rumbleUntil = maxOf(rumbleUntil, now + 32L)
            if (intensity != lastRumbleAmplitude || now - lastRumbleAt >= 32L) {
                device.vibrate(VibrationEffect.createOneShot(48L, intensity))
                lastRumbleAt = now
                lastRumbleAmplitude = intensity
            }
            if (current != 0) rumbleUntil = maxOf(rumbleUntil, now + 16L)
        } catch (error: RuntimeException) {
            stopRumble()
            SlotLog.w(TAG, "KONKR rumble unavailable", error)
        }
    }

    private fun startAudio() {
        if (SystemClock.elapsedRealtime() < audioRetryNotBeforeMs.get()) return
        if (!audioRunning.compareAndSet(false, true)) return
        val myGeneration = audioWorkerGeneration.incrementAndGet()
        audioFrameSignal.drainPermits()
        audioThread = Thread({
            // Android's streaming AudioTrack is fed from this app-owned
            // worker. Prefer the platform's documented audio scheduling
            // priority so transient GL/Wi-Fi activity cannot preempt the
            // PCM writer unnecessarily. Do NOT change the playback buffer
            // or add latency to hide actual missing linked game frames.
            try {
                android.os.Process.setThreadPriority(
                    android.os.Process.THREAD_PRIORITY_AUDIO)
                SlotLog.i(TAG, "Audio worker: THREAD_PRIORITY_AUDIO enabled")
            } catch (error: Exception) {
                SlotLog.w(TAG, "Audio priority unavailable; using default worker priority", error)
            }
            var player: AudioTrack? = null
            // F9: the worker owns one reusable PCM buffer (8192 bytes). A
            // JNI read supplies the valid sample count; only that prefix is
            // submitted to AudioTrack, including on a short final read.
            val samples = ShortArray(4096)
            // Do not start the PCM output clock while no libretro samples
            // exist. On the first chunk, preload into AudioTrack then play.
            var awaitingFirstPcm = false
            var rate = 0
            var requestedSpeed = 0
            var observedUnderruns = 0
            var audioProbeAt = 0L
            var emptyAudioReads = 0
            var playedShorts = 0L
            var audioBufferBytes = 0
            // Android's fast mixer (low-latency output) cannot time-stretch,
            // so Fast Forward runs on an ordinary track instead.
            var playerLowLatency = false
            // Diagnostic only: how fast AudioTrack's own playback-head clock
            // advances relative to submitted PCM. No buffer or DSP changes.
            var playbackHeadAtProbe = 0L
            // Read-only beta8 AudioTrack output headroom probe. Capacity is
            // not occupancy; use cumulative submitted frames - playback head.
            var submittedFramesSinceTrackCreate = 0L
            var minimumOutputFrames = Long.MAX_VALUE
            var maximumOutputFrames = 0L
            var outputDepthSamples = 0
            // Native PCM FIFO loss, sampled on this SAME 5s window.
            // These observations never alter the AudioTrack write path.
            var nativeClockBaseline = LongArray(0)
            // Beta9: AudioTrack's hardware timestamp describes its playback
            // clock independently of the emulation and GLSurfaceView clocks.
            val hardwareTimestamp = AudioTimestamp()
            var priorHardwareFrame = -1L
            var priorHardwareTimeNs = -1L
            try {
                while (audioRunning.get() &&
                    audioWorkerGeneration.get() == myGeneration) {
                    val wanted = nativeAudioSampleRate()
                    if (wanted <= 0) {
                        player?.pause()
                        SystemClock.sleep(35)
                        continue
                    }
                    val lowLatency = nativeAudioSpeedPermille() <= 1000
                    if (player == null || rate != wanted || lowLatency != playerLowLatency) {
                        player?.release()
                        val minBytes = AudioTrack.getMinBufferSize(
                            wanted, AudioFormat.CHANNEL_OUT_STEREO, AudioFormat.ENCODING_PCM_16BIT
                        )
                        if (minBytes <= 0) {
                            SystemClock.sleep(100)
                            continue
                        }
                        // Preserve the validated low-latency legacy AudioTrack buffer.
                        val bufferBytes = maxOf(16384, minBytes * 2)
                        audioBufferBytes = bufferBytes
                        nativeSetAudioOutputDepth(0, bufferBytes / 4)
                        SlotLog.i(TAG, "Audio diagnostic: rate=$wanted Hz, " +
                            "minBuffer=$minBytes bytes, allocated=$bufferBytes bytes")
                        player = AudioTrack.Builder()
                            .setAudioAttributes(AudioAttributes.Builder()
                                .setUsage(AudioAttributes.USAGE_GAME)
                                .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build())
                            .setAudioFormat(AudioFormat.Builder()
                                .setSampleRate(wanted)
                                .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                                .setChannelMask(AudioFormat.CHANNEL_OUT_STEREO).build())
                            .setBufferSizeInBytes(bufferBytes)
                            .setTransferMode(AudioTrack.MODE_STREAM)
                            .apply {
                                // Like RetroArch's audio at the panel rate: skip
                                // the normal mixer's extra buffering.
                                if (lowLatency) setPerformanceMode(AudioTrack.PERFORMANCE_MODE_LOW_LATENCY)
                            }
                            .build()
                        playerLowLatency = lowLatency
                        // Rate control must target the buffer Android really gave us.
                        val capacityFrames = player.bufferCapacityInFrames
                        if (capacityFrames > 0 && capacityFrames != bufferBytes / 4) {
                            audioBufferBytes = capacityFrames * 4
                            nativeSetAudioOutputDepth(0, capacityFrames)
                        }
                        SlotLog.i(TAG, "Audio output: low_latency_requested=$lowLatency " +
                            "performance_mode=${player.performanceMode} " +
                            "capacity_frames=$capacityFrames")
                        rate = wanted
                        requestedSpeed = 0
                        observedUnderruns = 0
                        diagnosticAudioUnderruns = 0
                        emptyAudioReads = 0
                        playedShorts = 0L
                        audioProbeAt = SystemClock.elapsedRealtime()
                        awaitingFirstPcm = true
                        playbackHeadAtProbe = 0L
                        submittedFramesSinceTrackCreate = 0L
                        minimumOutputFrames = Long.MAX_VALUE
                        maximumOutputFrames = 0L
                        outputDepthSamples = 0
                        nativeClockBaseline = nativeAudioClockCounters()
                        priorHardwareFrame = -1L
                        priorHardwareTimeNs = -1L
                    } else if (!awaitingFirstPcm &&
                        player.playState != AudioTrack.PLAYSTATE_PLAYING) {
                        // A paused legacy buffer is reused when the next game has
                        // the same sample rate; it still must be resumed.
                        player.play()
                    }
                    val speed = nativeAudioSpeedPermille().coerceIn(1000, 6000)
                    // A low-latency track always plays at 1x and may refuse
                    // playback parameters; Fast Forward recreates the track.
                    if (speed != requestedSpeed && !playerLowLatency) {
                        requestedSpeed = speed
                        try {
                            // Android time-stretches at the selected speed while preserving
                            // the ORIGINAL sample pitch (not chipmunk audio).
                            // If the Audio HAL cannot stretch, mute rather
                            // than silently playing an incorrect-pitch stream.
                            val params = PlaybackParams()
                                .setSpeed(speed / 1000f)
                                .setPitch(1.0f)
                                .setAudioFallbackMode(PlaybackParams.AUDIO_FALLBACK_MODE_MUTE)
                            player.playbackParams = params
                            nativeFastAudioSupported(true)
                            SlotLog.i(TAG, "FF audio: speed=${speed / 1000f}x, pitch=1.0x")
                        } catch (error: Exception) {
                            SlotLog.w(TAG, "Pitch-preserving FF unsupported; silent fallback", error)
                            nativeFastAudioSupported(false)
                            try {
                                player.playbackParams = PlaybackParams()
                                    .setSpeed(1.0f).setPitch(1.0f)
                            } catch (_: Exception) { }
                        }
                    }
                    val sampleCount = nativeReadAudioInto(samples)
                    if (sampleCount < 0 || sampleCount > samples.size) {
                        throw IllegalStateException("Invalid native PCM read count: $sampleCount")
                    }
                    EmulationDiagnostics.audio(samples, sampleCount)
                    if (sampleCount == 0) {
                        emptyAudioReads++
                        // Event-driven wakeup instead of sleeping blindly for 8ms:
                        // GLSurfaceView signals after each libretro callback.
                        // The 30ms timeout is a safety fallback for paused frames.
                        try {
                            audioFrameSignal.tryAcquire(30, TimeUnit.MILLISECONDS)
                        } catch (_: InterruptedException) {
                            Thread.currentThread().interrupt()
                            break
                        }
                        audioFrameSignal.drainPermits()
                        continue
                    }
                    audioFrameSignal.drainPermits()
                    // Immutable reference for the inlined PCM writer. Avoid
                    // capturing a mutable nullable AudioTrack in hot-path lambdas.
                    val track = player ?: continue
                    // Observe output depth BEFORE writing: post-write depth
                    // alone hides a playback starvation just before a fresh
                    // libretro chunk arrives. This is a read-only sample.
                    if (!awaitingFirstPcm) {
                        val headBefore = track.playbackHeadPosition.toLong() and 0xffffffffL
                        val depthBefore = (submittedFramesSinceTrackCreate - headBefore)
                            .coerceIn(0L, audioBufferBytes.toLong() / 4L)
                        minimumOutputFrames = minOf(minimumOutputFrames, depthBefore)
                        maximumOutputFrames = maxOf(maximumOutputFrames, depthBefore)
                        outputDepthSamples++
                        // RETROARCH dynamic rate control reads this fill level.
                        nativeSetAudioOutputDepth(depthBefore.toInt(), audioBufferBytes / 4)
                    }
                    // WRITE_BLOCKING can still return a positive *partial*
                    // count (e.g. on a route change). Never silently discard
                    // that PCM suffix, as it would insert an audible gap.
                    // The helper inlines the lambda: no new hot-path objects.
                    // A WRITE_BLOCKING write before play() must never be
                    // larger than the dormant track can hold: an oversized
                    // first burst could deadlock waiting for a playback clock
                    // we have intentionally not started yet. At most 2048
                    // interleaved shorts (~15.6ms at 65,536Hz stereo) are
                    // preloaded; remaining samples are drained AFTER play().
                    val firstLength = if (awaitingFirstPcm) {
                        minOf(sampleCount, 2048)
                    } else sampleCount
                    val firstWritten = AudioWritePolicy.writeComplete(
                        samples, start = 0, length = firstLength
                    ) { offset, remaining ->
                        track.write(samples, offset, remaining, AudioTrack.WRITE_BLOCKING)
                            .also { if (it < 0) throw IllegalStateException("AudioTrack write failed: $it") }
                    }
                    if (awaitingFirstPcm && firstWritten > 0) {
                        track.play()
                        awaitingFirstPcm = false
                        audioProbeAt = SystemClock.elapsedRealtime()
                        playbackHeadAtProbe =
                            track.playbackHeadPosition.toLong() and 0xffffffffL
                    }
                    val written = if (firstWritten == firstLength &&
                        sampleCount > firstLength && audioRunning.get()) {
                        firstWritten + AudioWritePolicy.writeComplete(
                            samples, start = firstLength,
                            length = sampleCount - firstLength
                        ) { offset, remaining ->
                            track.write(samples, offset, remaining, AudioTrack.WRITE_BLOCKING)
                                .also { if (it < 0) throw IllegalStateException("AudioTrack write failed: $it") }
                        }
                    } else firstWritten
                    if (written < sampleCount && audioRunning.get()) {
                        SlotLog.w(TAG, "AudioTrack incomplete PCM write: " +
                            "$written/${sampleCount} shorts")
                    }
                    playedShorts += written.toLong()
                    submittedFramesSinceTrackCreate += written.toLong() / 2L
                    // Sampling after each successful PCM submission is
                    // diagnostic only. No changed writes, latency or clock.
                    val outputHead = track.playbackHeadPosition.toLong() and 0xffffffffL
                    val outputDepth = (submittedFramesSinceTrackCreate - outputHead)
                        .coerceIn(0L, audioBufferBytes.toLong() / 4L)
                    minimumOutputFrames = minOf(minimumOutputFrames, outputDepth)
                    maximumOutputFrames = maxOf(maximumOutputFrames, outputDepth)
                    outputDepthSamples++
                    // Diagnostic only; do not modify audio queue, buffer size or
                    // event-driven wakeup. A rise here distinguishes Android
                    // playback underruns from mGBA mixer discontinuities.
                    val probeNow = SystemClock.elapsedRealtime()
                    if (probeNow - audioProbeAt >= 5_000L) {
                        val underflows = player.underrunCount
                        diagnosticAudioUnderruns = underflows
                        if (underflows > observedUnderruns) {
                            SlotLog.i(TAG, "AudioTrack underruns: +${underflows - observedUnderruns}" +
                                " over 5s (total=$underflows, rate=$wanted Hz)")
                        }
                        if (underflows > observedUnderruns) {
                            SlotLog.i(TAG, "Audio diagnostic: rate=$wanted, " +
                                "bufferBytes=$audioBufferBytes, " +
                                "writtenShorts=$playedShorts over 5s, " +
                                "emptyReads=$emptyAudioReads, " +
                                "underrunDelta=${underflows - observedUnderruns}")
                        }
                        val spanMs = (probeNow - audioProbeAt).coerceAtLeast(1L)
                        val expected = wanted.toLong() * 2L * spanMs / 1000L
                        val nativeClockNow = nativeAudioClockCounters()
                        val nativeWindow = AudioClockWindow.between(
                            nativeClockBaseline, nativeClockNow)
                        nativeClockBaseline = nativeClockNow
                        // Report the exact source of FIFO sample loss. This
                        // is one 5s log line, never a per-audio-read probe.
                        if (nativeWindow != null) {
                            SlotLog.i(TAG, "Audio FIFO loss: " +
                                "overflow_events=${nativeWindow.fifoOverflowEvents} " +
                                "dropped_shorts=${nativeWindow.fifoDroppedShorts} " +
                                "queued_now_shorts=${nativeAudioQueuedShorts()}")
                        }
                        val playbackHead = player.playbackHeadPosition.toLong() and 0xffffffffL
                        // Playback position is 32-bit and can wrap. Count actual
                        // advanced frames, not wall-clock guesses. On certain HALs
                        // this freezes during underrun, which is useful evidence.
                        val playbackFrames = (playbackHead - playbackHeadAtProbe) and 0xffffffffL
                        playbackHeadAtProbe = playbackHead
                        // Report even when there are no underruns, to isolate
                        // PCM producer starvation from AudioTrack HAL behaviour.
                        var hardwareRatePpm = Double.NaN
                        var hardwareSpanMs = 0L
                        if (track.getTimestamp(hardwareTimestamp)) {
                            if (priorHardwareFrame >= 0L &&
                                hardwareTimestamp.framePosition > priorHardwareFrame &&
                                hardwareTimestamp.nanoTime > priorHardwareTimeNs) {
                                val hardwareFrames = hardwareTimestamp.framePosition - priorHardwareFrame
                                val hardwareNs = hardwareTimestamp.nanoTime - priorHardwareTimeNs
                                hardwareSpanMs = hardwareNs / 1_000_000L
                                hardwareRatePpm = (
                                    hardwareFrames.toDouble() * 1_000_000_000.0 /
                                    hardwareNs.toDouble() / wanted.toDouble() - 1.0
                                ) * 1_000_000.0
                            }
                            priorHardwareFrame = hardwareTimestamp.framePosition
                            priorHardwareTimeNs = hardwareTimestamp.nanoTime
                        }
                        SlotLog.i(TAG, "AudioTrack hardware clock: " +
                            "rate_delta_ppm=${if (hardwareRatePpm.isFinite()) hardwareRatePpm else -1.0} " +
                            "hardware_span_ms=$hardwareSpanMs")
                        SlotLog.i(TAG, "AudioTrack queue depth: " +
                            "sample_count=$outputDepthSamples " +
                            "minimum_ms=${if (minimumOutputFrames == Long.MAX_VALUE) -1.0 else minimumOutputFrames * 1000.0 / wanted} " +
                            "maximum_ms=${maximumOutputFrames * 1000.0 / wanted} " +
                            "buffer_capacity_ms=${audioBufferBytes * 250.0 / wanted}")
                        SlotLog.i(TAG, "A12 PCM pacing: rate=$wanted span_ms=$spanMs " +
                            "written_shorts=$playedShorts expected_shorts=$expected " +
                            "playback_frames=$playbackFrames submitted_frames=${playedShorts / 2L} " +
                            "speed_permille=$requestedSpeed " +
                            "empty_reads=$emptyAudioReads queued_shorts=${nativeAudioQueuedShorts()} " +
                            "underruns=${underflows - observedUnderruns}")
                        observedUnderruns = underflows
                        playedShorts = 0L
                        minimumOutputFrames = Long.MAX_VALUE
                        maximumOutputFrames = 0L
                        outputDepthSamples = 0
                        emptyAudioReads = 0
                        audioProbeAt = probeNow
                    }
                }
            } catch (error: Exception) {
                if (audioRunning.get() && audioWorkerGeneration.get() == myGeneration) {
                    // A fatal negative write, route loss or stream exception
                    // gets a bounded 1s recovery delay. No UI-thread join or
                    // expanded PCM buffers; the existing heartbeat restarts it.
                    audioRetryNotBeforeMs.set(SystemClock.elapsedRealtime() + 1_000L)
                    SlotLog.e(TAG, "Android AudioTrack error; worker will recover", error)
                }
            } finally {
                runCatching { player?.stop() }.onFailure {
                    SlotLog.w(TAG, "AudioTrack stop cleanup failed", it)
                }
                runCatching { player?.release() }.onFailure {
                    SlotLog.w(TAG, "AudioTrack release cleanup failed", it)
                }
                if (audioWorkerGeneration.get() == myGeneration) {
                    audioRunning.set(false)
                    audioThread = null
                }
            }
        }, "SlotKONKR-Audio").apply { isDaemon = true; start() }
    }

    private fun stopAudio() {
        audioWorkerGeneration.incrementAndGet() // revoke previous thread ownership
        audioRunning.set(false)
        // Unblock the audio worker if it is waiting on a video frame.
        audioFrameSignal.release()
        audioThread?.interrupt()
        audioThread = null
    }

    /**
     * Read SAF ROM fingerprints exclusively off UI/audio/GL threads. Only
     * the complete ROM MD5 is forwarded to the native RA preparation queue.
     */
    private fun startRaSmartCache() {
        if (isDestroyed || RaAccount.configuredUser(filesDir) == null) return
        if (!raSmartRunning.compareAndSet(false, true)) return
        try {
            raSmartWorker.execute {
                try {
                    val manager = getSystemService(CONNECTIVITY_SERVICE) as? ConnectivityManager
                    val caps = manager?.getNetworkCapabilities(manager.activeNetwork)
                    if (caps?.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED) != true)
                        return@execute
                    RaSmartCache.indexAndPrefetch(applicationContext, filesDir,
                        isPlaying = { nativeIsPlaying() },
                        awaitForeground = {
                            // While Android is asleep, keep already submitted HTTP
                            // cache downloads going, but park the SAF indexer.
                            synchronized(raSmartForeground) {
                                while (!resumed && !Thread.currentThread().isInterrupted) {
                                    raSmartForeground.wait()
                                }
                            }
                            if (Thread.currentThread().isInterrupted)
                                throw InterruptedException()
                        },
                        submit = { hash, console -> nativeRaPrefetchHash(hash, console) })
                } catch (_: Exception) {
                    SlotLog.w(TAG, "RA Smart Cache indexing postponed")
                } finally {
                    raSmartRunning.set(false)
                }
            }
        } catch (_: java.util.concurrent.RejectedExecutionException) {
            raSmartRunning.set(false)
        }
    }

    /** Label passes run on the carousel, never in a game. */
    private fun autoLabelsMayRun(): Boolean =
        resumed && !isDestroyed && !nativeIsPlaying()

    private fun requestArtworkRefresh() {
        manualArtworkRefresh.set(true)
        artworkRefreshRequested.set(true)
        startAutoCartLabels()
    }

    /** The slot. folder's labels are in: those still missing are looked for. */
    internal fun folderArtworkIn() {
        uiHandler.removeCallbacks(labelsAfterFolder)
        startAutoCartLabels()
    }
    private val labelsAfterFolder = Runnable { startAutoCartLabels() }

    private fun startAutoCartLabels() {
        if (!autoLabelsMayRun()) return
        // A new handheld's labels come from its slot. folder first; only
        // those it hasn't are searched for (3.0.1-beta11).
        if (!slotFolder.artworkSettled()) {
            Log.i("SlotAutoLabels", "Deferred: the slot. folder's labels are still coming in")
            uiHandler.removeCallbacks(labelsAfterFolder)
            uiHandler.postDelayed(labelsAfterFolder, 30_000L)
            return
        }
        if (!autoLabelsRunning.compareAndSet(false, true)) {
            // A short Studio visit may return before the previous worker exits.
            autoLabelsRestartRequested.set(true)
            if (!autoLabelsRunning.compareAndSet(false, true)) return
        }
        autoLabelsRestartRequested.set(false)
        try {
            autoLabelsWorker.execute {
                var anotherBatch = false
                var passesFinished = false
                val artworkRevision = PortableArtwork.revision()
                // Only a pass that has carts to look at shows or reports
                // anything: one that finds nothing to do stays silent, not
                // "LABELS 2/2" again from the last pass's count (beta41).
                AutoCartLabelDiagnostics.passBegins()
                if (autoLabelsChainBase < 0) {
                    autoLabelsChainBase = AutoCartLabelDiagnostics.snapshot().saved
                }
                try {
                    val manager = getSystemService(CONNECTIVITY_SERVICE) as? ConnectivityManager
                    val caps = manager?.getNetworkCapabilities(manager.activeNetwork)
                    if (caps?.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED) != true) {
                        Log.i("SlotAutoLabels", "Deferred: Android default Internet network is not validated")
                        if (manualArtworkRefresh.get() && manualOfflineNotice.compareAndSet(false, true)) uiHandler.post {
                            if (!isDestroyed && resumed) SlotToast.makeText(this,
                                "ROM library scanned. Labels will resume when online.", Toast.LENGTH_SHORT).show()
                        }
                        return@execute
                    }
                    // Exactly the production 1.2.0 callback: enqueue a
                    // successful saved label for an idle carousel repaint.
                    // Status/UI snapshots are completely separate.
                    val keepGoing = { autoLabelsMayRun() }
                    if (artworkRefreshRequested.getAndSet(false)) {
                        PortableArtwork.beginRefresh(applicationContext)
                        AutoCartLabels.resetForRefresh()
                        HomeCartLabels.resetForRefresh()
                        autoLabelsOwnPassDone = false
                        val shelves = CartStudioCatalog.games(applicationContext).map { it.platform }.toSet()
                        if (!SlotArtSet.refreshLibrary(applicationContext, shelves, keepGoing)) {
                            artworkRefreshRequested.set(true)
                            return@execute
                        }
                        ArtNames.forget()
                        val required = shelves.toMutableSet()
                        if (Systems.GB in required || Systems.GBC in required) { required.add(Systems.GB); required.add(Systems.GBC) }
                        manualCatalogueFailed.set(!SlotArtSet.hasFreshIndexes(applicationContext, required.toList()))
                    }
                    val savedLabel: (String) -> Unit = { uri ->
                        autoLabelUpdates.add(uri)
                        autoLabelsNeedSync.set(true)
                        labelsSinceCheckpoint++
                        // Overlap the first copy with scraping, not with game
                        // launch. At most one intermediate copy in flight.
                        if (labelsSinceCheckpoint >= 128 && artworkCheckpointRunning.compareAndSet(false, true)) {
                            labelsSinceCheckpoint = 0
                            slotFolder.syncArtwork(pull = false) { ok ->
                                if (!ok) autoLabelsNeedSync.set(true)
                                artworkCheckpointRunning.set(false)
                            }
                        }
                    }
                    // PICO-8 carts carry their labels: no network needed.
                    Pico8Labels.fill(applicationContext, shouldContinue = keepGoing, onSaved = savedLabel)
                    HomeCartLabels.announce(applicationContext)
                    if (!autoLabelsOwnPassDone) {
                        // Both catalogue families precede slow external fallback.
                        val focus = ArtworkPriority.focus(applicationContext)
                        val platform = CartStudioCatalog.games(applicationContext)
                            .firstOrNull { it.uri == focus }?.platform
                        if (platform != null && platform in Systems.HOME_PLATFORMS) {
                            HomeCartLabels.fill(applicationContext, keepGoing, savedLabel, ownOnly = true)
                            AutoCartLabels.fill(applicationContext, keepGoing, savedLabel, ownOnly = true)
                        } else {
                            AutoCartLabels.fill(applicationContext, keepGoing, savedLabel, ownOnly = true)
                            HomeCartLabels.fill(applicationContext, keepGoing, savedLabel, ownOnly = true)
                        }
                        autoLabelsOwnPassDone = keepGoing()
                    }
                    // Existing permissive matching and paced sources.
                    anotherBatch = AutoCartLabels.fill(applicationContext, keepGoing, savedLabel)
                    anotherBatch = HomeCartLabels.fill(applicationContext, keepGoing, savedLabel) || anotherBatch
                    passesFinished = keepGoing()
                } catch (error: Exception) {
                    Log.w("SlotAutoLabels", "Automatic artwork deferred", error)
                } finally {
                    autoLabelsRunning.set(false)
                    val recheck = autoLabelsRestartRequested.getAndSet(false)
                    val report = AutoCartLabelDiagnostics.snapshot()
                    val added = (report.saved - autoLabelsChainBase.coerceAtLeast(0)).coerceAtLeast(0)
                    if (!anotherBatch && !recheck) autoLabelsChainBase = -1
                    if (!anotherBatch || recheck || !autoLabelsMayRun()) autoLabelsOwnPassDone = false
                    val progressKey = (report.checked.toLong() shl 32) or report.saved.toLong()
                    if (anotherBatch && !recheck) {
                        // A batch that moved nothing on: only retries wait.
                        if (progressKey == autoLabelsLastProgress && !autoLabelsQuiet) {
                            autoLabelsQuiet = true
                            uiHandler.post {
                                if (::autoLabelIndicator.isInitialized) {
                                    if (resumed && !isDestroyed && !nativeIsPlaying())
                                        autoLabelIndicator.finished(added)
                                    else autoLabelIndicator.hide()
                                }
                            }
                        } else if (progressKey != autoLabelsLastProgress) autoLabelsQuiet = false
                        autoLabelsLastProgress = progressKey
                    } else {
                        autoLabelsLastProgress = -1L
                        autoLabelsQuiet = false
                    }
                    if (!anotherBatch && !recheck && report.total > 0
                            && AutoCartLabelDiagnostics.passLooked) {
                        Log.i("SlotAutoLabels",
                            "ART_SUMMARY examined=${report.checked}/${report.total} " +
                                "saved=${report.saved} not_found=${report.notFound} " +
                                "network_error=${report.networkError} " +
                                "invalid_artwork=${report.invalidArtwork} " +
                                "other_error=${report.otherError}")
                        uiHandler.post {
                            if (::autoLabelIndicator.isInitialized) {
                                if (resumed && !isDestroyed && !nativeIsPlaying())
                                    autoLabelIndicator.finished(added)
                                else autoLabelIndicator.hide()
                            }
                        }
                    } else if (!anotherBatch && !recheck) {
                        // No indexed ROMs or network yet: never leave an
                        // old spinner visible while the worker is stopped.
                        uiHandler.post {
                            if (::autoLabelIndicator.isInitialized) autoLabelIndicator.hide()
                        }
                    }
                    if (PortableArtwork.revision() != artworkRevision) autoLabelsNeedSync.set(true)
                    // Keep changes together through the retry chain; a pause
                    // already has its normal outgoing sync. Only the artwork
                    // directories are walked after a completed search.
                    val finished = passesFinished && !anotherBatch && !recheck &&
                        !artworkRefreshRequested.get()
                    if (finished) {
                        AutoCartLabels.clearOwnPass()
                        HomeCartLabels.clearOwnPass()
                    }
                    if (finished && !isDestroyed) {
                        val manual = manualArtworkRefresh.getAndSet(false)
                        val serial = manualRefreshSerial.get()
                        val dirty = autoLabelsNeedSync.getAndSet(false)
                        val complete: (Boolean) -> Unit = { ok ->
                            if (!ok) autoLabelsNeedSync.set(true)
                            if (manual && !isDestroyed) try {
                                // BIOS reconciliation was queued before the scan.
                                gameLoader.execute {
                                    uiHandler.post {
                                        if (!isDestroyed && resumed && !manualArtworkRefresh.get() &&
                                            manualRefreshSerial.get() == serial) {
                                            val errors = report.networkError + report.otherError
                                            val message = when {
                                                !ok -> "Library scanned. Folder sync pending."
                                                manualBiosFailed.get() -> "Library scanned. BIOS folder unavailable."
                                                manualCatalogueFailed.get() || errors > 0 -> "Library scanned. Some labels could not be checked."
                                                else -> "Library is up to date."
                                            }
                                            SlotToast.makeText(this, message, Toast.LENGTH_SHORT).show()
                                        }
                                    }
                                }
                            } catch (_: java.util.concurrent.RejectedExecutionException) { }
                        }
                        if (dirty) slotFolder.syncArtwork(pull = autoLabelsMayRun(), asked = manual, finished = complete)
                        else complete(true)
                    }
                    if ((anotherBatch || recheck) && autoLabelsMayRun()) {
                        // A pause between bounded batches prevents continuous
                        // ROM reads and leaves the carousel's render thread free.
                        // Wait until the earliest bounded network/DAT
                        // retry is due. Fresh unchecked ROMs follow one
                        // second after the previous batch.
                        uiHandler.postDelayed({ startAutoCartLabels() },
                            AutoCartLabels.nextBatchDelayMs())
                    }
                }
            }
        } catch (_: java.util.concurrent.RejectedExecutionException) {
            autoLabelsRunning.set(false)
        }
    }

    /** Android-confirmed internet access, not Wi-Fi enabled or cached RA metadata.
     * Snapshotted when the game-ready notice is queued; no HTTP request on UI.
     */
    private fun hasValidatedInternet(): Boolean = try {
        val manager = raConnectivity
            ?: (getSystemService(CONNECTIVITY_SERVICE) as? ConnectivityManager)
        val capabilities = manager?.getNetworkCapabilities(manager.activeNetwork)
        capabilities?.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) == true &&
            capabilities?.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED) == true
    } catch (_: RuntimeException) {
        false
    }

    private fun startRaNetworkWatch() {
        try {
            val manager = getSystemService(CONNECTIVITY_SERVICE) as? ConnectivityManager
                ?: return
            manager.registerDefaultNetworkCallback(raConnectivityCallback)
            raConnectivity = manager
        } catch (error: Exception) {
            // No background polling fallback: the HTTP engine already retries
            // with bounded exponential backoff when notifications are unavailable.
            SlotLog.w(TAG, "RA network reconnect callback unavailable")
        }
    }

    /** Snapshot provided by the native renderer, never a network operation. */
    private fun retroAchievementsStatus(): String {
        val raw = nativeRaStatus() ?: return "Syncing"
        return try {
            val json = JSONObject(raw)
            // One or two words saying exactly what the worker is doing.
            val detail = json.optString("detail", "None")
            when (json.optString("state", "Disabled")) {
                "Ready" -> "Online"
                "Syncing" -> when (detail) {
                    "Games" -> "Caching games"
                    "Badges" -> "Caching badges"
                    "Retrying" -> "Retrying"
                    else -> "Syncing"
                }
                "Offline" -> "Offline"
                "Attention" -> when (detail) {
                    "Login" -> "Check login"
                    "Server" -> "Server error"
                    "Storage" -> "Storage error"
                    else -> "Needs attention"
                }
                else -> "Inactive"
            }
        } catch (_: Exception) { "Unknown" }
    }

    private fun refreshRaPanel(panel: RaPanelDialog, username: String) {
        scanner.execute {
            val hasPrevious = RaDiagnostics.lastSnapshot(username) != null
            val snapshot = RaDiagnostics.inspect(
                filesDir, username,
                onRulesReady = if (hasPrevious) null else { partial ->
                    runOnUiThread {
                        if (!isDestroyed) panel.update(retroAchievementsStatus(), hasValidatedInternet(), partial)
                    }
                }
            )
            runOnUiThread {
                if (!isDestroyed) panel.update(retroAchievementsStatus(), hasValidatedInternet(), snapshot)
            }
        }
    }

    /**
     * Confirm flush of earned awards, then await BOTH account-scoped native
     * workers before revoking credentials. Keep Slot running throughout.
     */
    private fun confirmRaSignOut(panel: RaPanelDialog, username: String) {
        if (nativeIsPlaying()) {
            SlotToast.makeText(this, "Eject your game before signing out.",
                Toast.LENGTH_LONG).show()
            return
        }
        scanner.execute {
            val pending = RaDiagnostics.pendingForSignOut(filesDir, username)
            runOnUiThread {
                if (isDestroyed) return@runOnUiThread
                val message = when {
                    pending == null ->
                        "Couldn't check for unlocks waiting to sync. They stay on this console. " +
                            "Sign in as $username again to sync them."
                    pending > 0 ->
                        "$pending unlocks are waiting to sync for $username. " +
                            "They stay on this console. Sign in again to sync them; " +
                            "very old unlocks may expire."
                    else ->
                        "Sign out of $username? Your unlocks and downloaded game data stay on this console."
                }
                val confirm = SlotModalDialog(
                    this,
                    "SIGN OUT",
                    "$message"
                )
                confirm.addButton("STAY SIGNED IN") { confirm.dismiss() }
                confirm.addButton("SIGN OUT") {
                    val unsaved = runCatching {
                        JSONObject(nativeRaStatus() ?: "{}").optBoolean("unsaved", false)
                    }.getOrDefault(false)
                    if (unsaved) {
                        confirm.setError("Unlocks are still being saved. " +
                            "Try again in a moment.")
                    } else {
                        confirm.dismiss()
                        flushRaBeforeSignOut(panel, username, 0)
                    }
                }
                confirm.show()
            }
        }
    }

    /** Try up to ~2.5s without blocking the UI or the game/emulator threads. */
    private fun flushRaBeforeSignOut(panel: RaPanelDialog, username: String, attempt: Int) {
        if (isDestroyed || nativeIsPlaying()) return
        if (attempt >= 25) {
            SlotToast.makeText(this,
                "Unlocks still saving. Sign-out cancelled.",
                Toast.LENGTH_LONG).show()
            return
        }
        view.queueEvent {
            val ready = nativeRaFlushReady()
            runOnUiThread {
                if (isDestroyed) return@runOnUiThread
                if (ready) {
                    stopRaBeforeSignOut(panel, username, 0)
                } else {
                    uiHandler.postDelayed({
                        flushRaBeforeSignOut(panel, username, attempt + 1)
                    }, 100L)
                }
            }
        }
    }

    /**
     * Native stop is asynchronous: wait for the evaluator AND for any HTTPS
     * request already in progress, before deleting the old session token.
     * On failure/timeout, leave the account configured and its awards intact.
     */
    private fun stopRaBeforeSignOut(panel: RaPanelDialog, username: String, attempt: Int) {
        if (isDestroyed || nativeIsPlaying()) return
        if (attempt >= 150) {
            SlotToast.makeText(this,
                "Couldn't sign out. Restart slot. and try again.",
                Toast.LENGTH_LONG).show()
            return
        }
        view.queueEvent {
            val ready = nativeRaStopReady()
            runOnUiThread {
                if (isDestroyed) return@runOnUiThread
                if (!ready) {
                    uiHandler.postDelayed({
                        stopRaBeforeSignOut(panel, username, attempt + 1)
                    }, 100L)
                    return@runOnUiThread
                }
                // The old-account workers have acknowledged termination.
                // Retain cached awards and game data for later sync.
                val success = RaAccount.beginSignOut(filesDir, username)
                    && RaAccount.finishPendingSignOut(filesDir)
                if (success) {
                    panel.dismiss()
                    SlotToast.makeText(this,
                        "Signed out of RA. Unlocks kept.",
                        Toast.LENGTH_LONG).show()
                    openRetroAchievementsAccount()
                } else {
                    SlotToast.makeText(this,
                        "Sign-out unfinished. Unlocks kept.",
                        Toast.LENGTH_LONG).show()
                }
            }
        }
    }

    /** The account panel uses the exact Slot label font instead of Android's AlertDialog. */
    private fun openRetroAchievementsAccount() {
        val existing = RaAccount.configuredUser(filesDir)
        if (existing != null) {
            lateinit var panel: RaPanelDialog
            panel = RaPanelDialog(
                activity = this, username = existing,
                onRefresh = { refreshRaPanel(panel, existing) },
                onSignOut = { confirmRaSignOut(panel, existing) },
                onDismiss = {
                    activeRaPanel = null
                    nativeExternalReturn()
                }
            )
            activeRaPanel = panel to existing
            panel.show()
            // Show last known values immediately when reopening the dashboard.
            // They are refreshed safely in the background on every opening.
            RaDiagnostics.lastSnapshot(existing)?.let {
                panel.update(retroAchievementsStatus(), hasValidatedInternet(), it)
            }
            refreshRaPanel(panel, existing)
            // Badge downloads continue in the native HTTP worker; update this
            // read-only dashboard automatically without requiring a new tap.
            val liveUpdates = object : Runnable {
                override fun run() {
                    if (isDestroyed || !panel.isShowing()) return
                    refreshRaPanel(panel, existing)
                    uiHandler.postDelayed(this, 15_000L)
                }
            }
            uiHandler.postDelayed(liveUpdates, 15_000L)
            return
        }

        // A successful login transitions directly to the RA dashboard.
        var openingDashboard = false
        val login = SlotModalDialog(
            this,
            "RETROACHIEVEMENTS",
            "Sign in to earn achievements (Casual mode).",
            onDismiss = { if (!openingDashboard) nativeExternalReturn() }
        )
        val username = login.addInput("USERNAME", "RetroAchievements username")
        val password = login.addInput("PASSWORD", "Password (not saved)", password = true)
        // Primary action first, then Cancel, in both visual and D-pad order.
        val signIn = login.addButton("SIGN IN") {
            val name = username.text.toString().trim()
            val secret = password.text.toString()
            if (name.isEmpty() || secret.isEmpty()) {
                login.setError("Enter both your RetroAchievements username and password.")
            } else {
                password.text.clear() // Don't leave credentials in the visible form.
                login.setError("Signing in…")
                login.setBusy(true)
                raLoginWorker.execute {
                    val result = RaAccount.authenticateAndSave(filesDir, name, secret)
                    runOnUiThread {
                        if (isDestroyed) return@runOnUiThread
                        when (result) {
                            is RaAccount.Result.Connected -> {
                                openingDashboard = true
                                login.dismiss()
                                nativeRaAccountConfigured()
                                startRaSmartCache()
                                openRetroAchievementsAccount()
                                SlotToast.makeText(this,
                                    "Signed in as " + result.username +
                                        ". Preparing achievements…",
                                    Toast.LENGTH_LONG).show()
                            }
                            is RaAccount.Result.Failed -> {
                                login.setBusy(false)
                                login.setError(result.message)
                            }
                        }
                    }
                }
            }
        }
        login.addButton("CANCEL") { login.dismiss() }
        password.setOnEditorActionListener { _, actionId, _ ->
            if (actionId == android.view.inputmethod.EditorInfo.IME_ACTION_DONE) {
                signIn.performClick()
                true
            } else false
        }
        login.show()
    }

    /** The clock and battery now (the Rotate woken shut: no old time). */
    internal fun refreshSystemStatus() {
        lastStatusPoll = -15000L
        pollSystemStatus()
    }

    private fun pollSystemStatus() {
        val now = SystemClock.elapsedRealtime()
        if (now - lastStatusPoll < 15000L) return
        lastStatusPoll = now
        // PR #2 by Its-TroyM: JNI passes locale-independent 24h ASCII;
        // Slot's native saved preference handles 12h AM/PM presentation.
        val clock = java.text.SimpleDateFormat("HH:mm", java.util.Locale.ROOT)
            .format(java.util.Date())
        val battery = registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED))
        val level = battery?.getIntExtra(BatteryManager.EXTRA_LEVEL, -1) ?: -1
        val scale = battery?.getIntExtra(BatteryManager.EXTRA_SCALE, -1) ?: -1
        val percent = if (level >= 0 && scale > 0) (100 * level / scale).coerceIn(0, 100)
            else -1
        val state = battery?.getIntExtra(BatteryManager.EXTRA_STATUS, -1) ?: -1
        // Some handhelds (the Retroid Pocket Classic) report NOT_CHARGING or
        // DISCHARGING while on a weak USB port: plugged in counts as charging.
        val plugged = battery?.getIntExtra(BatteryManager.EXTRA_PLUGGED, 0) ?: 0
        val charging = state == BatteryManager.BATTERY_STATUS_CHARGING ||
            state == BatteryManager.BATTERY_STATUS_FULL || plugged != 0
        nativeSystemDate(java.text.SimpleDateFormat("EEE d MMM", java.util.Locale.getDefault())
            .format(java.util.Date()))
        nativeSystemStatus(clock, percent, charging)

    }

    /** OFF is the safe default, including upgrades from pre-option builds.
     *  Never merge mGBA/gpSP state files; enabling opt-in affects only the
     *  next ROM launch and refuses genuine two-core SRAM conflicts.
     */
    private fun toggleShareGbaSaves() {
        if (nativeIsPlaying()) {
            SlotToast.makeText(this, "Eject the cartridge before changing this setting.",
                Toast.LENGTH_LONG).show()
            return
        }
        val prefs = getSharedPreferences(PREFS, MODE_PRIVATE)
        val enabled = !prefs.getBoolean(SHARE_GBA_SAVES, false)
        if (!prefs.edit().putBoolean(SHARE_GBA_SAVES, enabled).commit()) {
            SlotToast.makeText(this, "Couldn't save this setting.",
                Toast.LENGTH_LONG).show()
            return
        }
        nativeSetShareSaves(enabled)
        SlotToast.makeText(this,
            if (enabled) "mGBA, gpSP and Gambatte now share the same saves."
            else "Each core now keeps its own saves.",
            Toast.LENGTH_LONG).show()
    }

    private fun refreshLibrary() {
        val saved = getSharedPreferences(PREFS, MODE_PRIVATE).getString(ROM_ROOT, null)
        if (saved == null) {
            SlotToast.makeText(this, "Choose your ROM folder first.", Toast.LENGTH_SHORT).show()
            openRomFolderPicker()
            return
        }
        val uri = Uri.parse(saved)
        if (contentResolver.persistedUriPermissions.none { it.uri == uri && it.isReadPermission }) {
            SlotToast.makeText(this, "ROM folder access expired. Choose it again.", Toast.LENGTH_SHORT).show()
            openRomFolderPicker()
            return
        }
        manualRefreshSerial.incrementAndGet()
        manualBiosFailed.set(false)
        manualCatalogueFailed.set(false)
        manualOfflineNotice.set(false)
        getSharedPreferences(PREFS, MODE_PRIVATE).getString(BIOS_ROOT, null)?.let { savedBios ->
            val bios = Uri.parse(savedBios)
            if (contentResolver.persistedUriPermissions.any { it.uri == bios && it.isReadPermission }) {
                importBiosFolder(bios, persistSelection = false, quietRetries = 0, manualRefresh = true)
            } else {
                manualBiosFailed.set(true)
                SlotLog.w(TAG, "BIOS folder access expired; previous BIOS kept")
            }
        }
        scanFolder(uri, refreshArtwork = true)
    }

    private fun openSharedFolderPicker(requestCode: Int) {
        check(requestCode == SAVE_REQUEST || requestCode == STATE_REQUEST)
        SlotBuildBehavior.externalFolderRestrictionMessage(packageName)?.let { restriction ->
            nativeExternalReturn()
            SlotToast.makeText(this, restriction, Toast.LENGTH_LONG).show()
            return
        }
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or
                Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
                Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION)
        }
        try {
            @Suppress("DEPRECATION")
            startActivityForResult(intent, requestCode)
        } catch (error: ActivityNotFoundException) {
            nativeExternalReturn()
            SlotToast.makeText(this, "Couldn't open the folder picker.", Toast.LENGTH_LONG).show()
        }
    }

    private fun openBiosFolderPicker(initial: Uri? = null) {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
            addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or
                Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
            )
            if (initial != null && Build.VERSION.SDK_INT >= Build.VERSION_CODES.O)
                putExtra(android.provider.DocumentsContract.EXTRA_INITIAL_URI, initial)
        }
        try {
            @Suppress("DEPRECATION")
            startActivityForResult(intent, BIOS_REQUEST)
        } catch (error: ActivityNotFoundException) {
            nativeExternalReturn()
            SlotToast.makeText(this, "Couldn't open the folder picker.", Toast.LENGTH_LONG).show()
        }
    }

    /** Runs sequentially with ROM materialization, never on the UI/GL thread. */
    private fun importBiosFolder(uri: Uri, persistSelection: Boolean, quietRetries: Int = 3, manualRefresh: Boolean = false) {
        // The BIOS folder's card is out (beta41): the staged copy is used,
        // nothing is read, no retries filling the log.
        if (!persistSelection && !SlotFolder.volumeMounted(this, SlotFolder.volumeOf(uri))) {
            SlotLog.i(TAG, "BIOS folder's card is out: the copy already in slot. is used")
            if (manualRefresh) manualBiosFailed.set(true)
            return
        }
        gameLoader.execute {
            try {
                val imported = BiosLibrary.importFrom(this, uri, allowEmpty = !persistSelection)
                SlotLog.i(TAG, "BIOS imported (${imported.size}): ${imported.joinToString()}")
                // Restoring the already selected BIOS at startup stays silent.
                val prefs = getSharedPreferences(PREFS, MODE_PRIVATE)
                if (persistSelection) {
                    prefs.edit().putString(BIOS_ROOT, uri.toString()).apply()
                }
                if (persistSelection) runOnUiThread {
                    if (!isDestroyed) {
                        TaskPill.confirm("BIOS READY")
                        SlotToast.makeText(this, "BIOS ready. Used from your next game.",
                            Toast.LENGTH_LONG).show()
                    }
                }
            } catch (error: Exception) {
                SlotLog.e(TAG, "BIOS folder import failed", error)
                if (manualRefresh) manualBiosFailed.set(true)
                runOnUiThread {
                    if (isDestroyed) return@runOnUiThread
                    if (manualRefresh) {
                        SlotToast.makeText(this, "BIOS folder unavailable. Existing BIOS kept.", Toast.LENGTH_LONG).show()
                    } else if (!persistSelection) {
                        // Silent startup restore: right after a reboot (slot.
                        // as home app) the storage provider may not be ready
                        // yet. The previously staged BIOS copy is kept, so
                        // retry quietly instead of showing an error.
                        if (quietRetries > 0) {
                            uiHandler.postDelayed({
                                if (!isDestroyed) importBiosFolder(uri, false, quietRetries - 1)
                            }, 5_000L)
                        }
                    } else {
                        SlotToast.makeText(this, "Couldn't import BIOS files. Check the folder.",
                            Toast.LENGTH_LONG).show()
                    }
                }
            }
        }
    }

    private fun openRomFolderPicker(initial: Uri? = null) {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
            // Writable since 2.3: RomM saves the games it downloads there.
            addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or
                Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
                Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
            )
            if (initial != null) putExtra(android.provider.DocumentsContract.EXTRA_INITIAL_URI, initial)
            // Opens on the folder chosen before, to choose it again.
            else getSharedPreferences(PREFS, MODE_PRIVATE).getString(ROM_ROOT, null)?.let { saved ->
                try {
                    val tree = Uri.parse(saved)
                    putExtra(android.provider.DocumentsContract.EXTRA_INITIAL_URI,
                        android.provider.DocumentsContract.buildDocumentUriUsingTree(tree,
                            android.provider.DocumentsContract.getTreeDocumentId(tree)))
                } catch (_: Exception) {}
            }
        }
        try {
            @Suppress("DEPRECATION")
            startActivityForResult(intent, FOLDER_REQUEST)
        } catch (error: ActivityNotFoundException) {
            nativeExternalReturn()
            nativeSetShelfStatus(shelfPickerError)
            SlotLog.w(TAG, "Android folder picker unavailable", error)
        }
    }

    private fun openVisualPicker(kind: Int) {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = if (kind == THEME_REQUEST) "text/plain" else "image/png"
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        try {
            @Suppress("DEPRECATION")
            startActivityForResult(intent, kind)
        } catch (error: ActivityNotFoundException) {
            nativeExternalReturn()
            SlotToast.makeText(this, "Couldn't open the file picker.", Toast.LENGTH_LONG).show()
        }
    }

    /** About's X (#107): Android's "save as" page for slot.'s logs; About
     *  stays behind it. */
    private fun exportLogs() {
        nativeExternalBegin()
        try {
            @Suppress("DEPRECATION")
            startActivityForResult(LogExport.intent(), LogExport.REQUEST)
        } catch (error: ActivityNotFoundException) {
            nativeExternalReturn()
            SlotToast.makeText(this, "Couldn't open the file picker.", Toast.LENGTH_LONG).show()
        }
    }

    /** The Screenshot hotkey (Siham 2026-11-09): the PNG Rust read from the
     * screen, filed where Android keeps its own, Pictures/Screenshots. */
    private fun saveScreenshot() {
        val png = nativeTakeScreenshot() ?: return
        screenshotWorker.execute {
            val stamp = java.text.SimpleDateFormat("yyyyMMdd_HHmmss", java.util.Locale.US)
                .format(java.util.Date())
            val values = ContentValues().apply {
                put(MediaStore.Images.Media.DISPLAY_NAME, "slot_$stamp.png")
                put(MediaStore.Images.Media.MIME_TYPE, "image/png")
                put(MediaStore.Images.Media.RELATIVE_PATH,
                    Environment.DIRECTORY_PICTURES + "/Screenshots")
                put(MediaStore.Images.Media.IS_PENDING, 1)
            }
            val saved = runCatching {
                val uri = contentResolver.insert(
                    MediaStore.Images.Media.EXTERNAL_CONTENT_URI, values)
                    ?: error("No media entry")
                try {
                    contentResolver.openOutputStream(uri)?.use { it.write(png) }
                        ?: error("No media stream")
                    contentResolver.update(uri, ContentValues().apply {
                        put(MediaStore.Images.Media.IS_PENDING, 0)
                    }, null, null)
                } catch (error: Exception) {
                    contentResolver.delete(uri, null, null)
                    throw error
                }
            }.onFailure { SlotLog.w(TAG, "Cannot save screenshot", it) }.isSuccess
            runOnUiThread {
                if (!isDestroyed) SlotToast.makeText(this,
                    if (saved) "Screenshot saved." else "Couldn't save the screenshot.",
                    Toast.LENGTH_SHORT).show()
            }
        }
    }

    private fun visualFile(kind: Int): File {
        if (kind == THEME_REQUEST) return File(filesDir, "Config/theme.txt")
        // While the Shelf wallpaper is picked, import/delete acts on the
        // current shelf's own file (GBA.png, GB.png, GBC.png, Recents.png,
        // Favorites.png); otherwise on the single custom wallpaper.
        val target = nativeWallpaperTarget()
            ?.takeIf { WALLPAPER_TARGET_NAME.matches(it) } ?: "user.png"
        return File(filesDir, "Wallpapers/$target")
    }

    private fun resetVisualFile(kind: Int) {
        val file = visualFile(kind)
        if (file.exists() && !file.delete()) {
            SlotToast.makeText(this, "Couldn't remove the wallpaper. Try again.", Toast.LENGTH_LONG).show()
            return
        }
        nativeReloadVisualAssets()
    }

    private fun importVisualFile(kind: Int, uri: Uri) {
        try {
            val maxBytes = if (kind == THEME_REQUEST) 64 * 1024 else 6 * 1024 * 1024
            val bytes = contentResolver.openInputStream(uri)?.use { stream ->
                val output = ByteArrayOutputStream()
                val chunk = ByteArray(8192)
                while (true) {
                    val count = stream.read(chunk)
                    if (count < 0) break
                    if (output.size() + count > maxBytes) {
                        throw IllegalArgumentException("Selected file is too large")
                    }
                    output.write(chunk, 0, count)
                }
                output.toByteArray()
            } ?: throw IllegalArgumentException("Selected file could not be opened")
            if (kind == WALLPAPER_REQUEST) {
                val png = byteArrayOf(-119, 80, 78, 71, 13, 10, 26, 10)
                require(bytes.size >= 8 && bytes.copyOfRange(0, 8).contentEquals(png)) {
                    "Wallpaper must be a PNG up to 4096 × 4096."
                }
                val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
                BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
                require(bounds.outWidth in 1..4096 && bounds.outHeight in 1..4096) {
                    "Wallpaper must be a PNG up to 4096 × 4096."
                }
            }
            val target = visualFile(kind)
            target.parentFile?.mkdirs()
            val atomic = AtomicFile(target)
            val output = atomic.startWrite()
            try {
                output.write(bytes)
                atomic.finishWrite(output)
            } catch (error: Exception) {
                atomic.failWrite(output)
                throw error
            }
            nativeReloadVisualAssets()
            SlotToast.makeText(this, "Wallpaper updated.", Toast.LENGTH_SHORT).show()
        } catch (error: Exception) {
            SlotLog.e(TAG, "Cannot import Slot visual file", error)
            SlotToast.makeText(this, error.message?.takeIf { it.startsWith("Wallpaper must") }
                ?: "Couldn't import this file.", Toast.LENGTH_LONG).show()
        }
    }

    private fun cartLabelFile(romUri: String): File =
        CartLabelPaths.file(filesDir, romUri)

    private var pendingStudioPictureUri: String? = null

    /** X on a cart opened the native Cart Studio: fetch its pictures. */
    private fun requestStudioArt() {
        val uri = nativePollCartLabelUri() ?: return
        studioArtWorker.execute {
            val art = ownStudioArt(uri)
            nativeStudioArt(uri, art.first, art.second, 0, "")
        }
    }

    /** A cart's own real label (as the automatic labels prepare it) and logo. */
    private fun ownStudioArt(uri: String): Pair<ByteArray?, ByteArray?> =
            try {
                val platform = CartStudioCatalog.games(applicationContext).firstOrNull { it.uri == uri }
                // The names Search game... searches, for the systems it may borrow from.
                val group = if (platform?.platform in Systems.HOME_PLATFORMS) listOfNotNull(platform?.platform)
                    else listOf(Systems.GB, Systems.GBC, Systems.GBA)
                group.forEach { SlotArtSet.writeNames(applicationContext, it) }
                when {
                    platform == null -> null to null
                    platform.platform in Systems.HOME_PLATFORMS -> HomeCartLabels.studioArt(applicationContext, uri)
                    else -> AutoCartLabels.studioArt(applicationContext, platform)
                }
            } catch (error: Exception) {
                SlotLog.w(TAG, "Cart Studio art unavailable", error)
                null to null
            }

    /** Search game... picked a game (kind 1): that game's label for the cart. */
    private fun requestStudioGameArt(kind: Int) {
        val request = nativePollCartLabelUri() ?: return
        val parts = request.split('\t')
        if (parts.size < 4) return
        val (uri, platform, crc, name) = parts
        studioArtWorker.execute {
            var real: ByteArray? = null
            var logo: ByteArray? = null
            try {
                val target = CartStudioCatalog.games(applicationContext).firstOrNull { it.uri == uri }
                val entries = SlotArtSet.entries(applicationContext, platform, crc)
                if (target != null && entries != null) {
                    logo = entries.firstOrNull { it.type == "wheel" }
                        ?.let { SlotArtSet.file(applicationContext, it) }?.let(SlotArtSet::toPng)
                    real = if (target.platform in Systems.HOME_PLATFORMS) {
                        HomeCartLabels.studioGameLabel(applicationContext, uri, entries)
                    } else {
                        entries.firstOrNull { it.type == "support-texture" }
                            ?.let { SlotArtSet.file(applicationContext, it) }?.let(SlotArtSet::toPng)
                            ?.let { AutoCartLabels.cropPhysicalCartLabel(it, target.platform) }
                    }
                }
            } catch (error: Exception) {
                SlotLog.w(TAG, "Another game's label unavailable", error)
            }
            nativeStudioArt(uri, real, logo, kind, name)
        }
    }

    /** Picture...: the Android picker, for the cart being edited. */
    private fun requestStudioPicture() {
        val uri = nativePollCartLabelUri() ?: return
        pendingStudioPictureUri = uri
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "image/*"
        }
        @Suppress("DEPRECATION")
        try {
            returnFromSlotChild = true
            startActivityForResult(intent, STUDIO_PICTURE_REQUEST)
        } catch (error: Exception) {
            returnFromSlotChild = false
            pendingStudioPictureUri = null
            nativeExternalReturn()
            nativeStudioArt(uri, null, null, 2, "")
            SlotToast.makeText(this, "Couldn't open your pictures.", Toast.LENGTH_SHORT).show()
        }
    }

    /** A picture the player chose: at most 2048 px, as PNG; opaque in the
     *  first slot (a whole label), with transparency in the second (a logo). */
    private fun studioPicture(cart: String, picture: Uri) {
        studioArtWorker.execute {
            var opaque: ByteArray? = null
            var transparent: ByteArray? = null
            try {
                val bytes = contentResolver.openInputStream(picture)?.use { input ->
                    val out = java.io.ByteArrayOutputStream()
                    val buffer = ByteArray(64 * 1024)
                    while (true) {
                        val n = input.read(buffer)
                        if (n < 0) break
                        if (out.size() + n > 24 * 1024 * 1024) error("Picture too large")
                        out.write(buffer, 0, n)
                    }
                    out.toByteArray()
                }
                if (bytes != null) {
                    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
                    BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
                    var sample = 1
                    while (maxOf(bounds.outWidth, bounds.outHeight) / sample > 2048) sample *= 2
                    val bitmap = BitmapFactory.decodeByteArray(bytes, 0, bytes.size,
                        BitmapFactory.Options().apply { inSampleSize = sample; inPreferredConfig = Bitmap.Config.ARGB_8888 })
                    if (bitmap != null) {
                        try {
                            val pixels = IntArray(bitmap.width * bitmap.height)
                            bitmap.getPixels(pixels, 0, bitmap.width, 0, 0, bitmap.width, bitmap.height)
                            val clear = bitmap.hasAlpha() && pixels.any { (it ushr 24) < 250 }
                            val out = java.io.ByteArrayOutputStream()
                            if (bitmap.compress(Bitmap.CompressFormat.PNG, 100, out)) {
                                if (clear) transparent = out.toByteArray() else opaque = out.toByteArray()
                            }
                        } finally {
                            bitmap.recycle()
                        }
                    }
                }
            } catch (error: Exception) {
                SlotLog.w(TAG, "Cart Studio picture unreadable", error)
            }
            nativeStudioArt(cart, opaque, transparent, 2, "")
        }
    }

    private fun setStudioOverrides(json: String): Int =
        nativeSetLibrary(json).also {
            if (it >= 0) shelfCarts = it
        }

    private fun openCartLabelPicker() {
        val selected = nativePollCartLabelUri()
        if (selected.isNullOrBlank()) {
            SlotToast.makeText(this, "Select a cartridge first.", Toast.LENGTH_SHORT).show()
            return
        }
        pendingCartLabelUri = selected
        openVisualPicker(LABEL_REQUEST)
    }

    private fun removeSelectedCartLabel() {
        val selected = nativePollCartLabelUri() ?: return
        val file = cartLabelFile(selected)
        if (file.exists() && !file.delete()) {
            SlotToast.makeText(this, "Couldn't remove this label.", Toast.LENGTH_LONG).show()
            return
        }
        nativeReloadCartLabel(selected)
        SlotToast.makeText(this, "Custom label removed.", Toast.LENGTH_SHORT).show()
    }

    private fun importCartLabel(source: Uri, romUri: String) {
        try {
            // Only standalone PNG artwork: not a ROM/BIOS folder or a
            // serialized game state. Decode bounds before storing it.
            val bytes = contentResolver.openInputStream(source)?.use { stream ->
                val output = ByteArrayOutputStream()
                val chunk = ByteArray(8192)
                while (true) {
                    val count = stream.read(chunk)
                    if (count < 0) break
                    if (output.size() + count > 2 * 1024 * 1024) {
                        throw IllegalArgumentException("Cartridge label exceeds 2 MiB")
                    }
                    output.write(chunk, 0, count)
                }
                output.toByteArray()
            } ?: throw IllegalArgumentException("Cannot open label image")
            val signature = byteArrayOf(-119, 80, 78, 71, 13, 10, 26, 10)
            require(bytes.size >= 8 && bytes.copyOfRange(0, 8).contentEquals(signature)) {
                "Labels must be a PNG up to 2048 × 2048."
            }
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
            require(bounds.outWidth in 1..2048 && bounds.outHeight in 1..2048) {
                "Labels must be a PNG up to 2048 × 2048."
            }
            val target = cartLabelFile(romUri)
            target.parentFile?.mkdirs()
            val atomic = AtomicFile(target)
            val output = atomic.startWrite()
            try {
                output.write(bytes)
                atomic.finishWrite(output)
            } catch (error: Exception) {
                atomic.failWrite(output)
                throw error
            }
            nativeReloadCartLabel(romUri)
            SlotToast.makeText(this, "Label updated.", Toast.LENGTH_SHORT).show()
        } catch (error: Exception) {
            SlotLog.e(TAG, "Could not import cartridge label", error)
            SlotToast.makeText(this, error.message?.takeIf { it.startsWith("Labels must") }
                ?: "Couldn't import this label.", Toast.LENGTH_LONG).show()
        }
    }

    @Deprecated("Android 12 SAF activity result")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode == SlotFolder.REQUEST) {
            wakeCarouselRender()
            slotFolder.picked(resultCode, data)
            return
        }
        if (requestCode == LogExport.REQUEST) {
            nativeExternalReturn()
            wakeCarouselRender()
            val uri = data?.data
            if (resultCode == RESULT_OK && uri != null) {
                LogExport.write(this, uri, nativeJournal()) { saved ->
                    if (!isDestroyed) SlotToast.makeText(this,
                        if (saved) "Logs saved." else "Couldn't save the logs.",
                        Toast.LENGTH_SHORT).show()
                }
            }
            return
        }
        if (requestCode in setOf(FOLDER_REQUEST, BIOS_REQUEST, SAVE_REQUEST, STATE_REQUEST,
                THEME_REQUEST, WALLPAPER_REQUEST, LABEL_REQUEST, STUDIO_PICTURE_REQUEST)) {
            // A ROM or BIOS folder chosen: back to the shelf, to see it fill.
            // Cancelled, or any other picker: back to its menu row.
            val chosen = resultCode == RESULT_OK && data?.data != null
            if (chosen && (requestCode == FOLDER_REQUEST || requestCode == BIOS_REQUEST)) nativeExternalClose()
            else nativeExternalReturn()
            wakeCarouselRender()
        }
        if (requestCode == STUDIO_PICTURE_REQUEST) {
            val cart = pendingStudioPictureUri
            pendingStudioPictureUri = null
            val picture = data?.data
            if (cart != null) {
                if (resultCode == RESULT_OK && picture != null) studioPicture(cart, picture)
                else nativeStudioArt(cart, null, null, 2, "")
            }
            return
        }
        if (requestCode == LABEL_REQUEST) {
            val selected = pendingCartLabelUri
            pendingCartLabelUri = null
            if (resultCode == RESULT_OK && selected != null) {
                data?.data?.let { importCartLabel(it, selected) }
            }
            return
        }
        if (requestCode == THEME_REQUEST || requestCode == WALLPAPER_REQUEST) {
            if (resultCode == RESULT_OK) data?.data?.let { importVisualFile(requestCode, it) }
            return
        }
        if (requestCode == FOLDER_REQUEST && (resultCode != RESULT_OK || data?.data == null)) {
            romm.folderCancelled()
        }
        if (requestCode !in setOf(FOLDER_REQUEST, BIOS_REQUEST,
                SAVE_REQUEST, STATE_REQUEST) || resultCode != RESULT_OK) return
        val uri = data?.data ?: return
        try {
            val writable = requestCode == SAVE_REQUEST || requestCode == STATE_REQUEST ||
                requestCode == FOLDER_REQUEST
            val flags = if (writable) {
                Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
            } else {
                Intent.FLAG_GRANT_READ_URI_PERMISSION
            }
            try {
                contentResolver.takePersistableUriPermission(uri, flags)
            } catch (error: SecurityException) {
                // A ROM folder Android only lets slot. read still plays.
                if (requestCode != FOLDER_REQUEST) throw error
                contentResolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }
            when (requestCode) {
                FOLDER_REQUEST -> {
                    val prefs = getSharedPreferences(PREFS, MODE_PRIVATE)
                    val firstRomFolder = !prefs.contains(ROM_ROOT)
                    val same = prefs.getString(ROM_ROOT, null) == uri.toString()
                    prefs.edit().putString(ROM_ROOT, uri.toString()).apply()
                    // The same folder chosen again for RomM: only the right
                    // to write changed, the shelf stays as it is.
                    val forRomm = romm.folderChosen()
                    // From RomM the folder is often still empty: its own
                    // "games added" message speaks once the downloads land.
                    if (!(same && forRomm)) scanFolder(uri, quiet = forRomm, shown = true)
                    // A slot folder's BIOS folder follows; a new installation
                    // is never asked (3.0.1-beta12).
                    if (firstRomFolder && !forRomm && !prefs.contains(BIOS_ROOT))
                        uiHandler.post { suggestBiosFolder() }
                }
                BIOS_REQUEST -> importBiosFolder(uri, persistSelection = true)
                SAVE_REQUEST, STATE_REQUEST -> {
                    val key = if (requestCode == SAVE_REQUEST) SAVE_ROOT else STATE_ROOT
                    getSharedPreferences(PREFS, MODE_PRIVATE).edit()
                        .putString(key, uri.toString()).apply()
                    val states = requestCode == STATE_REQUEST
                    val kind = if (states) "Save State" else "Save"
                    SlotToast.makeText(this,
                        "$kind folder selected. Copying your ${if (states) "states" else "saves"} there…",
                        Toast.LENGTH_LONG).show()
                    transferToFolder(uri, states)
                }
            }
        } catch (error: SecurityException) {
            if (requestCode == FOLDER_REQUEST) {
                nativeSetShelfStatus(shelfPermissionError)
            } else {
                SlotToast.makeText(this, "Couldn't access this folder. Select it again.", Toast.LENGTH_LONG).show()
            }
            SlotLog.e(TAG, "Android folder permission error", error)
        }
    }

    /** A newly chosen save or state folder gets everything slot. keeps of
     *  that kind (RetroArchStorage.planTransfer / applyTransfer), on the game
     *  loader so no launch imports from or exports to the folder meanwhile.
     *  Where the folder already holds a different copy, the player chooses. */
    private fun transferToFolder(root: Uri, states: Boolean) {
        val noun = if (states) "state" else "save"
        gameLoader.execute {
            val result = try {
                val plan = RetroArchStorage.planTransfer(applicationContext, root, states)
                val keepSlot = plan.different.isEmpty() ||
                    askTransferConflict(states, plan.different.map {
                        if (states) it.name.substringBefore(".state") else it.name.substringBeforeLast('.')
                    }.distinct().size)
                RetroArchStorage.applyTransfer(applicationContext, plan, keepSlot)
            } catch (error: Exception) {
                SlotLog.e(TAG, "Folder transfer failed", error)
                null
            }
            val text = when {
                result == null -> "Couldn't copy your ${noun}s to the folder. Your ${noun}s are safe in slot."
                result.copied + result.replaced + result.kept == 0 && result.failed == 0 ->
                    if (result.same > 0) "Your ${noun}s were already in the folder." else "No ${noun}s to copy yet."
                else -> buildString {
                    val moved = result.copied + result.replaced
                    append("Copied $moved file${if (moved == 1) "" else "s"} to the folder.")
                    if (result.replaced > 0) append(" The folder's old copies were kept as .before-transfer files.")
                    if (result.kept > 0) append(" The folder's ${noun}s were kept; slot.'s are beside them as .slot files.")
                    if (result.failed > 0) append(" ${result.failed} couldn't be copied and stay in slot.")
                }
            }
            uiHandler.post { SlotToast.makeText(this, text, Toast.LENGTH_LONG).show() }
        }
    }

    /** Which copy a folder keeps for games it already holds differently:
     *  true for slot.'s. The other is always kept beside it. Called from the
     *  game loader, which waits while the UI thread shows the choice. */
    private fun askTransferConflict(states: Boolean, games: Int): Boolean {
        val ready = CountDownLatch(1)
        val keepSlot = java.util.concurrent.atomic.AtomicBoolean(false)
        val noun = if (states) "states" else "saves"
        runOnUiThread {
            if (isDestroyed || isFinishing) { ready.countDown(); return@runOnUiThread }
            try {
                val dialog = SlotModalDialog(this, "${noun.uppercase()} ALREADY IN THIS FOLDER",
                    "$games game${if (games == 1) " has" else "s have"} different $noun in this folder " +
                        "than in slot. Which should the folder keep? The other copy is kept beside it.",
                    onDismiss = { ready.countDown() })
                dialog.addButton("KEEP SLOT. ${noun.uppercase()}") {
                    keepSlot.set(true)
                    dialog.dismiss()
                }
                dialog.addButton("KEEP FOLDER ${noun.uppercase()}") { dialog.dismiss() }
                dialog.show()
            } catch (error: Exception) {
                SlotLog.e(TAG, "Cannot open the folder transfer choice", error)
                ready.countDown()
            }
        }
        // No answer keeps the folder's: nothing is lost either way.
        return ready.await(300L, TimeUnit.SECONDS) && keepSlot.get()
    }

    /** Read only a cache that belongs to this persisted SAF tree. */
    private fun cachedLibraryGames(root: Uri): String? {
        val file = File(filesDir, ROM_CACHE_NAME)
        if (!file.isFile || file.length() !in 1L..ROM_CACHE_MAX_BYTES.toLong()) return null
        return try {
            val metadata = JSONObject(file.readText(Charsets.UTF_8))
            if (metadata.optInt("schema") != ROM_CACHE_SCHEMA ||
                metadata.optString("root") != root.toString()) null
            else metadata.optJSONArray("games")?.toString()
        } catch (error: Exception) {
            SlotLog.w(TAG, "Invalid ROM cache; scanning without replacing it first", error)
            null
        }
    }

    /** Reuse cached labels immediately without rescanning on the UI thread. */
    private fun restoreCachedLibrary(root: Uri): Boolean {
        val games = cachedLibraryGames(root) ?: return false
        return try {
            val count = setStudioOverrides(games)
            if (count < 0) return false
            nativeSetShelfStatus(if (count == 0) shelfEmpty else shelfNone)
            SlotLog.i(TAG, "Restored $count cached ROMs without SAF rescan")
            true
        } catch (error: Exception) {
            SlotLog.w(TAG, "Cached library unavailable; scanning", error)
            false
        }
    }

    /** AtomicFile retains the previous library if the write fails. */
    private fun storeCachedLibrary(root: Uri, json: String) {
        val payload = JSONObject()
            .put("schema", ROM_CACHE_SCHEMA)
            .put("root", root.toString())
            .put("games", JSONArray(json))
            .toString().toByteArray(Charsets.UTF_8)
        if (payload.size > ROM_CACHE_MAX_BYTES) {
            SlotLog.w(TAG, "ROM library cache exceeds safe limit")
            return
        }
        val atomic = AtomicFile(File(filesDir, ROM_CACHE_NAME))
        val stream = atomic.startWrite()
        try {
            stream.write(payload)
            atomic.finishWrite(stream)
        } catch (error: Exception) {
            atomic.failWrite(stream)
            throw error
        }
    }

    /** The scanner never publishes directly. This method runs on the UI
     * thread only. Rust also rechecks the idle state at the exact GL swap,
     * guarding against a cartridge inserted between the two callbacks.
     */
    private fun publishPendingLibraryIfIdle() {
        val next = pendingLibraryIndex ?: return
        if (scanSerial.get() != next.serial) {
            pendingLibraryIndex = null
            libraryRefreshIndicator.hide()
            return
        }
        if (!resumed || !nativeCanRefreshLibrary()) return
        if (next.silent && (next.json.length to next.json.hashCode()) == lastPublishedIndex) {
            pendingLibraryIndex = null
            SlotLog.i(TAG, "RomM: shelf already shows these games")
            return
        }
        // RomM's games while more are coming: a group every 2.5 s at most.
        if (next.silent && romm.downloading() &&
            SystemClock.uptimeMillis() - lastSilentPublishAt < SILENT_PUBLISH_MS) {
            if (!silentHeldLogged) { silentHeldLogged = true; SlotLog.i(TAG, "RomM: games held for the next group") }
            return
        }
        if (next.silent) {
            silentHeldLogged = false
            SlotLog.i(TAG, "RomM: group to the shelf (more coming: ${romm.downloading()})")
        }
        val saved = getSharedPreferences(PREFS, MODE_PRIVATE).getString(ROM_ROOT, null)
        if (saved != next.root.toString()) {
            pendingLibraryIndex = null
            libraryRefreshIndicator.hide()
            return
        }
        pendingLibraryIndex = null
        if (!next.silent) libraryRefreshIndicator.show()
        else lastSilentPublishAt = SystemClock.uptimeMillis()
        lastPublishedIndex = next.json.length to next.json.hashCode()
        val count = try { nativeSetLibrary(next.prepared) } catch (error: Exception) {
            SlotLog.e(TAG, "Changed ROM library could not be applied", error)
            -1
        }
        if (count < 0) {
            libraryRefreshIndicator.hide()
            nativeSetShelfStatus(shelfImportError)
            return
        }
        shelfCarts = count
        nativeSetShelfStatus(if (count == 0) shelfEmpty else shelfNone)
        // Index persistence and RA diagnostics remain on background workers.
        scanner.execute {
            if (scanSerial.get() != next.serial) return@execute
            try {
                storeCachedLibrary(next.root, next.json)
                // RomM's games still coming: one search once they're all in.
                if (!(next.silent && romm.downloading())) {
                    startRaSmartCache()
                    if (next.refreshArtwork) requestArtworkRefresh() else startAutoCartLabels()
                }
            } catch (error: Exception) {
                SlotLog.w(TAG, "ROM cache write failed; displayed shelf is intact", error)
            }
        }
        // No "0 cartridges ready.": an empty shelf already says so.
        if (next.partial && !next.silent) {
            // Even after a quiet scan: the player should know why some
            // games are missing (issue #122).
            SlotToast.makeText(this,
                "ROM folder too big: $count cartridges ready, some games skipped.",
                Toast.LENGTH_LONG).show()
        } else if (!next.quiet && !next.refreshArtwork && count > 0) {
            SlotToast.makeText(this,
                if (count == 1) "1 cartridge ready." else "$count cartridges ready.",
                Toast.LENGTH_SHORT).show()
        }
        // Brief subtle spinner only for a changed library; never for
        // unchanged boot scans and never while a game is active.
        uiHandler.postDelayed({
            if (::libraryRefreshIndicator.isInitialized && !isDestroyed &&
                pendingLibraryIndex == null) libraryRefreshIndicator.hide()
        }, 700L)
    }

    /** `quiet`: no toast at the end. `shown`: its progress shows (top right
     *  over carts, centred on an empty shelf); only the scan behind the
     *  boot's cached shelf is hidden (RomM's show: Siham 2026-11-16). */
    private fun scanFolder(uri: Uri, quiet: Boolean = false, shown: Boolean = !quiet, refreshArtwork: Boolean = false) {
        val serial = scanSerial.incrementAndGet()
        pendingLibraryIndex = null
        if (::libraryRefreshIndicator.isInitialized) libraryRefreshIndicator.hide()
        if (shown) nativeSetShelfStatus(shelfScanning)
        // Refresh Library over a shelf of carts: the scan shows top right in
        // the task rail; on an empty shelf the renderer draws it centred.
        val inRail = shown && shelfCarts > 0
        if (inRail && ::libraryRefreshIndicator.isInitialized) libraryRefreshIndicator.showScan(0, 0)
        var lastReport = 0L
        val progress: ((Int, Int) -> Unit)? = if (!shown) null else { done, total ->
            val now = SystemClock.uptimeMillis()
            if (now - lastReport >= 100L || (total > 0 && done >= total)) {
                lastReport = now
                runOnUiThread {
                    if (!isDestroyed && scanSerial.get() == serial) {
                        nativeSetScanProgress(done, total)
                        if (inRail) libraryRefreshIndicator.showScan(done, total)
                    }
                }
            }
        }
        scansRunning.incrementAndGet()
        scanner.execute {
            try {
                val previous = cachedLibraryGames(uri)
                val result = RomLibrary.scan(this, uri, previous, progress,
                    shouldContinue = { !isDestroyed && scanSerial.get() == serial })
                if (scanSerial.get() != serial) return@execute
                // A bounded/incomplete SAF scan is not evidence that a ROM
                // was removed: RomLibrary kept every earlier cart. What it
                // found is added (issue #122: a fresh install with a folder
                // over the bounds kept its empty shelf); with nothing new,
                // the shelf stays as it is.
                val partial = result.truncated
                if (partial && (result.count == 0 ||
                        (previous != null && RomLibrary.sameIndex(previous, result.json)))) {
                    SlotLog.w(TAG, "Incomplete SAF ROM listing: retaining existing index")
                    val gone = romFolderGone(uri)
                    runOnUiThread {
                        if (isDestroyed || scanSerial.get() != serial) return@runOnUiThread
                        if (gone) { showRomFolderGone(uri); return@runOnUiThread }
                        if (shown) {
                            libraryRefreshIndicator.hide()
                            nativeSetShelfStatus(shelfNone)
                        }
                        if (!quiet) SlotToast.makeText(this,
                            "ROM folder too big: some games skipped.",
                            Toast.LENGTH_LONG).show()
                    }
                    return@execute
                }
                // RomM's labels for games this scan found (bug: lost when the
                // games came by a scan rather than an add).
                giveRommLabels(result.json)
                if (previous != null && RomLibrary.sameIndex(previous, result.json)) {
                    // Persist metadata-only migrations even without a shelf rebuild.
                    if (previous != result.json) storeCachedLibrary(uri, result.json)
                    SlotLog.i(TAG, "ROM library unchanged: " + result.count +
                        " carts; no renderer rebuild")
                    if (refreshArtwork) requestArtworkRefresh()
                    runOnUiThread {
                        if (!isDestroyed && scanSerial.get() == serial && shown) {
                            libraryRefreshIndicator.hide()
                            nativeSetShelfStatus(if (result.count == 0) shelfEmpty else shelfNone)
                            if (!quiet && !refreshArtwork) SlotToast.makeText(this, "Library is up to date.",
                                Toast.LENGTH_SHORT).show()
                        }
                    }
                    return@execute
                }
                // Process artwork metadata before handing a changed index
                // to the UI. No game/ROM bytes or state files are touched.
                val prepared = result.json
                runOnUiThread {
                    if (isDestroyed || scanSerial.get() != serial) return@runOnUiThread
                    pendingLibraryIndex = PendingLibraryIndex(
                        uri, result.json, prepared, serial, quiet, partial = partial, refreshArtwork = refreshArtwork)
                    publishPendingLibraryIfIdle()
                }
            } catch (_: java.util.concurrent.CancellationException) {
                SlotLog.i(TAG, "ROM scan superseded")
            } catch (error: OutOfMemoryError) {
                // Last resort (issue #122's bigger bound): the scan's copies
                // are dropped with its frames; the shelf stays as it was
                // rather than slot. closing (RomM did, 2026-11-19).
                SlotLog.e(TAG, "ROM scan ran out of memory; previous shelf retained", error)
                runOnUiThread {
                    if (isDestroyed || scanSerial.get() != serial) return@runOnUiThread
                    if (shown) {
                        libraryRefreshIndicator.hide()
                        nativeSetShelfStatus(if (shelfCarts == 0) shelfReadError else shelfNone)
                    }
                    SlotToast.makeText(this, "ROM folder too big to read.",
                        Toast.LENGTH_LONG).show()
                }
            } catch (error: Exception) {
                SlotLog.e(TAG, "SAF ROM scan failed; previous cache retained", error)
                val gone = romFolderGone(uri)
                runOnUiThread {
                    if (isDestroyed || scanSerial.get() != serial) return@runOnUiThread
                    if (gone) showRomFolderGone(uri)
                    else if (shown) {
                        libraryRefreshIndicator.hide()
                        nativeSetShelfStatus(shelfReadError)
                    }
                }
            } finally {
                // After the result above reached the UI thread.
                runOnUiThread {
                    if (scansRunning.decrementAndGet() == 0 && !isDestroyed) {
                        addAfterScan?.let { addAfterScan = null; addDownloadedGames(it) }
                    }
                }
            }
        }
    }

    /** The ROM folder was deleted: its storage is here (not a card taken
     *  out, nor Android's access taken back) but the folder isn't. Off the
     *  UI thread. Unsure answers are false: the carts stay. */
    private fun romFolderGone(tree: Uri): Boolean = try {
        if (tree.authority != "com.android.externalstorage.documents") false
        else {
            val doc = android.provider.DocumentsContract.getTreeDocumentId(tree)
            val volume = doc.substringBefore(':', "")
            val mounted = volume == "primary" ||
                getSystemService(android.os.storage.StorageManager::class.java).storageVolumes.any {
                    it.uuid.equals(volume, ignoreCase = true) && it.state == android.os.Environment.MEDIA_MOUNTED
                }
            mounted && try {
                contentResolver.query(android.provider.DocumentsContract.buildDocumentUriUsingTree(tree, doc),
                    arrayOf(android.provider.DocumentsContract.Document.COLUMN_DOCUMENT_ID), null, null, null)
                    ?.use { !it.moveToFirst() } ?: false
            } catch (_: java.io.FileNotFoundException) { true
            } catch (error: IllegalArgumentException) {
                // ExternalStorageProvider: "Missing file for primary:Roms".
                (error.message ?: "").contains("Missing", ignoreCase = true) ||
                    error.cause is java.io.FileNotFoundException
            }
        }
    } catch (_: SecurityException) { false } catch (_: Exception) { false }

    /** The deleted ROM folder's carts go; saves, states, labels, favourites
     *  and settings stay for when the games come back (Siham 2026-11-16). */
    private fun showRomFolderGone(tree: Uri) {
        SlotLog.w(TAG, "ROM folder no longer exists: the shelf empties")
        libraryRefreshIndicator.hide()
        if (!resumed || !nativeCanRefreshLibrary()) {
            // At boot or with a game on: once the shelf is free, while the
            // same folder is still the chosen one.
            uiHandler.postDelayed({
                val saved = getSharedPreferences(PREFS, MODE_PRIVATE).getString(ROM_ROOT, null)
                if (!isDestroyed && saved == tree.toString()) showRomFolderGone(tree)
            }, 1_000L)
            return
        }
        if (nativeSetLibrary("[]") >= 0) shelfCarts = 0
        nativeSetShelfStatus(shelfFolderGone)
        scanner.execute { try { storeCachedLibrary(tree, "[]") } catch (_: Exception) {} }
    }

    override fun onDestroy() {
        if (::lidCover.isInitialized) lidCover.onDestroy()
        getSystemService(InputManager::class.java)?.unregisterInputDeviceListener(controllerListener)
        releaseLinkMulticast()
        Choreographer.getInstance().removeFrameCallback(vsyncCallback)
        raConnectivity?.let { manager ->
            try { manager.unregisterNetworkCallback(raConnectivityCallback) }
            catch (_: Exception) { /* Callback may have already been removed. */ }
        }
        raConnectivity = null
        scanSerial.incrementAndGet()
        pendingLibraryIndex = null
        if (::libraryRefreshIndicator.isInitialized) libraryRefreshIndicator.hide()
        scanner.shutdownNow()
        raLoginWorker.shutdownNow()
        raSmartWorker.shutdownNow()
        gameLoader.shutdownNow()
        romWarmWorker.shutdownNow()
        autoLabelsWorker.shutdownNow()
        uiHandler.removeCallbacks(pulse)
        uiHandler.removeCallbacks(rumblePulse)
        stopRumble()
        synchronized(raSmartForeground) { raSmartForeground.notifyAll() }
        if (::raAchievementOverlay.isInitialized) raAchievementOverlay.destroy()
        if (::shelfStatus.isInitialized) shelfStatus.close()
        stopAudio()
        if (::view.isInitialized) view.close()
        cartSounds.release()
        super.onDestroy()
    }

    /** Paused by the Home Android sends for the RG Rotate's held middle
     *  button, which slot. (the home app) answers itself: no suspend, the
     *  eject carries on as smoothly as SELECT + START. */
    private var homeHoldPause = false

    override fun onPause() {
        stopRumble()
        if (RgRotate.device && rotateMiddleDown) {
            homeHoldPause = true
            SlotLog.i(TAG, "Rotate: middle button Home; light pause")
            super.onPause()
            return
        }
        suspendForPause()
        uiHandler.removeCallbacks(slotFolderPush)
        slotFolder.sync(pull = false)
        super.onPause()
    }

    override fun onStop() {
        // Home really took slot. away: the suspend the light pause skipped.
        if (homeHoldPause) {
            homeHoldPause = false
            suspendForPause()
        }
        super.onStop()
    }

    private fun suspendForPause() {
        resumed = false
        warmedCandidate.set(null)
        uiHandler.removeCallbacks(pulse)
        uiHandler.removeCallbacks(rumblePulse)
        stopRumble()
        stopSensors()
        carouselDozing = false
        // Rust nativeSuspend aborts discovery/linked play; never keep a
        // multicast lock across Android background sleep.
        releaseLinkMulticast()
        if (::shelfStatus.isInitialized) shelfStatus.onPause()
        if (::autoLabelIndicator.isInitialized) autoLabelIndicator.hide()
        Choreographer.getInstance().removeFrameCallback(vsyncCallback)
        if (displaySyncApplied) {
            view.renderMode = GLSurfaceView.RENDERMODE_CONTINUOUSLY
            displaySyncApplied = false
            raSwapLoopApplied = false
        }
        stopAudio()
        cartSounds.pause()
        // Preserve SRAM and automatic resume state while the GL context is still current.
        val complete = CountDownLatch(1)
        view.queueEvent { try { nativeSuspend() } finally { complete.countDown() } }
        try {
            if (!complete.await(2, TimeUnit.SECONDS)) SlotLog.w(TAG, "Suspend save timed out")
        } catch (_: InterruptedException) {
            Thread.currentThread().interrupt()
        }
        view.onPause()
        // The final SRAM write above can enqueue a RetroArch/Sync export.
        drainPendingSaveFlush()
        nativeResetInput()
    }

    override fun onResume() {
        super.onResume()
        reportTouchscreen()
        if (updateAfterPermission) {
            updateAfterPermission = false
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O || packageManager.canRequestPackageInstalls()) {
                uiHandler.post { installUpdate() }
            }
        }
        if (homeHoldPause) {
            homeHoldPause = false
            if (::lidCover.isInitialized) lidCover.onResume()
            wakeCarouselRender()
            immersive()
            return
        }
        if (returnFromSlotChild) {
            // Release the external-page guard, retaining the menu and its row.
            returnFromSlotChild = false
            nativeExternalReturn()
        }
        resumed = true
        // The slot. folder: what another handheld changed comes in (B2),
        // out of a game only.
        uiHandler.removeCallbacks(slotFolderPush)
        slotFolder.sync(pull = nativeCanRefreshLibrary())
        bench.maybeStart()
        // Controllers may have come or gone while slot. was away.
        pushControllers()
        publishSensors()
        publishDisplayRefresh()
        synchronized(raSmartForeground) { raSmartForeground.notifyAll() }
        carouselDozing = false
        view.renderMode = GLSurfaceView.RENDERMODE_CONTINUOUSLY
        uiHandler.removeCallbacks(pulse)
        uiHandler.removeCallbacks(rumblePulse)
        stopRumble()
        uiHandler.post(pulse)
        uiHandler.removeCallbacks(rumblePulse)
        uiHandler.post(rumblePulse)
        if (::shelfStatus.isInitialized) shelfStatus.onResume()
        if (::lidCover.isInitialized) lidCover.onResume()
        // Android's quick settings may return without a fresh onAvailable().
        // Waking a retry is safe even when no network is present.
        nativeRaNetworkAvailable()
        if (RaAccount.configuredUser(filesDir) != null) {
            nativeRaAccountConfigured()
            startRaSmartCache()
        }
        startAutoCartLabels()
        slotUpdater.onForeground()
        lastStatusPoll = -15000L
        cartSounds.resume()
        view.onResume()
        view.queueEvent { nativePresentationResume() }
        wakeCarouselRender()
        updateDisplaySyncMode()
        immersive()
    }

    private fun immersive() {
        @Suppress("DEPRECATION")
        window.decorView.systemUiVisibility = (
            View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY or
            View.SYSTEM_UI_FLAG_FULLSCREEN or
            View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or
            View.SYSTEM_UI_FLAG_LAYOUT_STABLE or
            View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or
            View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
        )
    }
}
