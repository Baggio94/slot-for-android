package fyi.slot.konkr

import android.content.Context
import android.util.Log
import org.json.JSONObject
import java.io.File
import java.security.MessageDigest
import java.util.Locale

/**
 * Sync Saves with RomM (slot. 2.5, Library > Saves & States): a game's
 * in-game save goes to the player's RomM server after each save in slot.,
 * and a newer one from the server comes back before the game starts, so the
 * same progress follows them on every handheld and in RomM's other clients.
 *
 * Only the in-game save (.srm, or .eeprom and .eep for the Lynx and the Pokemon mini); the
 * resume state has its own sync, RommStates (3.0).
 * Off by default. What slot. last exchanged with the server is remembered
 * per game (the server's save and the save's fingerprint), so:
 *  - the server changed, the handheld didn't: the server's save comes in;
 *  - the handheld changed, the server didn't: it is sent after the game;
 *  - both changed: the player chooses, and the other one is kept aside.
 * Nothing here ever stops a game: no network, no RomM, it plays as before.
 */
internal object RommSaves {
    private const val TAG = "SlotRomm"
    private const val PREFS = "slot-romm-saves"
    /** How long a launch waits on the server, at most (beta37). */
    internal const val LAUNCH_BUDGET_MS = 2_000L

    fun enabled(context: Context): Boolean =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getBoolean("enabled", false)

    /** Library's value: 0 RomM not connected, 1 off, 2 on. */
    fun state(context: Context): Int = when {
        !Romm.signedIn(context) -> 0
        enabled(context) -> 2
        else -> 1
    }

    /** Turns the sync on or off; false when RomM isn't connected. */
    fun toggle(context: Context): Boolean {
        if (!Romm.signedIn(context)) return false
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
            .putBoolean("enabled", !enabled(context)).apply()
        return true
    }

    /** What the server's save was at the last exchange, and ours then. */
    internal data class Record(val romId: Long, val saveId: Long, val serverStamp: String, val sha1: String,
                               val rtcSha1: String = "")

    private fun recordFile(context: Context, target: RetroArchStorage.Target): File =
        File(File(context.filesDir, "RommSync"),
            GameKeys.digest(target.romUri) + "." + RetroArchStorage.coreLower(target.core) + ".json")

    /** The launch couldn't check the server (it didn't answer in time):
     *  the save goes up only once the server is known not to have changed,
     *  else the next launch asks, as when both sides changed. */
    private fun uncheckedFile(context: Context, target: RetroArchStorage.Target): File =
        File(recordFile(context, target).path.removeSuffix(".json") + ".unchecked")

    private fun record(context: Context, target: RetroArchStorage.Target): Record? = try {
        val json = JSONObject(recordFile(context, target).readText(Charsets.UTF_8))
        Record(json.getLong("rom"), json.getLong("save"), json.getString("stamp"), json.getString("sha1"),
            json.optString("rtc"))
    } catch (_: Exception) { null }

    private fun keep(context: Context, target: RetroArchStorage.Target, record: Record) {
        val file = recordFile(context, target)
        file.parentFile?.mkdirs()
        val temp = File(file.parentFile, file.name + ".tmp")
        temp.writeText(JSONObject().put("rom", record.romId).put("save", record.saveId)
            .put("stamp", record.serverStamp).put("sha1", record.sha1).put("rtc", record.rtcSha1)
            .toString(), Charsets.UTF_8)
        if (!temp.renameTo(file)) temp.delete()
    }

    internal fun sha1(bytes: ByteArray): String = MessageDigest.getInstance("SHA-1").digest(bytes)
        .joinToString("") { "%02x".format(Locale.ROOT, it.toInt() and 0xff) }

    /** The core's name on the server, as RetroArch-core clients use it. */
    private fun emulator(target: RetroArchStorage.Target): String = RetroArchStorage.coreLower(target.core)

    /** The cores whose saves are this game's on the server: with Share Core
     *  Saves, a GBA game's mGBA and gpSP saves are one save (beta39: the
     *  other core's newer one was passed over, and stayed after DELETE IT). */
    internal fun emulators(target: RetroArchStorage.Target): Set<String> =
        if (target.isGba && target.shareGbaSaves) setOf("mgba", "gpsp", emulator(target))
        else setOf(emulator(target))

    private fun isOurs(save: Romm.ServerSave, target: RetroArchStorage.Target): Boolean =
        save.emulator.lowercase(Locale.ROOT) in emulators(target)

    /** The game's clock (Pokémon Ruby, Sapphire, Emerald, Crystal), beside
     *  its save: sent and brought in with it (beta39). */
    private fun rtcOf(target: RetroArchStorage.Target): File = target.rtcLocal

    /** The cart's system, from its portable name (`gba/…`). */
    internal fun platformOf(target: RetroArchStorage.Target): String? {
        val name = GameKeys.name(target.romUri)
        if (!name.contains('/') || name.startsWith("content:")) return null
        return name.substringBefore('/').uppercase(Locale.ROOT)
            .takeIf { it in Systems.PLATFORMS }
    }

    /** What to do when both the server and the handheld changed. */
    enum class Choice { KEEP_HANDHELD, USE_SERVER }

    /** What became of the save at a launch, for its state (3.0): nothing
     *  came, the server's came, or this handheld's was kept over a newer one. */
    enum class Pick { NONE, BROUGHT, KEPT_HANDHELD }

    /** What the launch should know: a message to show, if any; [deleteHere]:
     *  another handheld deleted this save and the player chose to delete it
     *  here too (beta39); [pick] and [serverAt], the server's newest save's
     *  date, for Sync States with RomM (3.0). */
    class Outcome(val message: String?, val deleteHere: Boolean = false,
                  val pick: Pick = Pick.NONE, val serverAt: String = "")

    /**
     * Before a game starts: brings the server's save in when it is the
     * newer one. [ask] is called (on this worker thread, blocking) when both
     * sides changed since the last exchange.
     */
    fun beforeLaunch(context: Context, target: RetroArchStorage.Target, ask: () -> Choice,
                     askDeletedElsewhere: () -> Boolean = { false }): Outcome {
        if (!enabled(context) || !Romm.signedIn(context)) return Outcome(null)
        val platform = platformOf(target) ?: return Outcome(null)
        val started = System.nanoTime()
        return try {
            Romm.within(LAUNCH_BUDGET_MS) { checkServer(context, target, platform, ask, askDeletedElsewhere) }.also {
                uncheckedFile(context, target).delete()
            }
        } catch (error: Romm.SignedOut) {
            Outcome("Signed out of RomM: saves aren't synced.")
        } catch (error: Exception) {
            if (error is java.net.SocketTimeoutException || error is java.net.ConnectException ||
                error is java.net.UnknownHostException || error is java.net.NoRouteToHostException) {
                try { uncheckedFile(context, target).apply { parentFile?.mkdirs() }.writeText("1") } catch (_: Exception) {}
                TaskPill.problem("ROMM OFFLINE")
            }
            SlotLog.w(TAG, "RomM save check skipped after ${(System.nanoTime() - started) / 1_000_000}ms: " +
                "the handheld's save", error)
            Outcome(null)
        }
    }

    private fun checkServer(context: Context, target: RetroArchStorage.Target, platform: String,
                      ask: () -> Choice, askDeletedElsewhere: () -> Boolean): Outcome {
        return run {
            // A deletion the server didn't take yet (offline): again now;
            // until it does, nothing comes back from the server.
            if (record(context, target)?.saveId == DELETE_PENDING) {
                deleted(context, target)
                if (record(context, target)?.saveId == DELETE_PENDING) return Outcome(null)
            }
            val kept = record(context, target)
            val romId = kept?.romId?.takeIf { it > 0 } ?: Romm.romIdFor(context, platform, target.stem)
                ?: return Outcome(null)
            val saves = Romm.saves(context, romId)
            val server = newest(saves, target)
            val local = target.saveLocal.takeIf { it.isFile }?.readBytes()
            if (server == null) {
                // The server had this game's save and has none now: another
                // handheld deleted it (DELETE IT). This one asks (Siham
                // 2026-11-19); kept, it goes up again after the game.
                if (kept != null && kept.saveId > 0 && local != null) {
                    val delete = askDeletedElsewhere()
                    keep(context, target, Record(romId, DELETED, "", ""))
                    SlotLog.i(TAG, "RomM: ${target.stem}'s save was deleted on another handheld; " +
                        if (delete) "deleted here too" else "kept here")
                    return Outcome(null, deleteHere = delete)
                }
                return Outcome(null)
            }
            val localSha = local?.let(::sha1)
            val at = server.updatedAt
            if (kept != null && kept.serverStamp == server.updatedAt) return Outcome(null, serverAt = at) // Server unchanged.
            val bytes = Romm.downloadSave(context, server)
            val serverSha = sha1(bytes)
            if (serverSha == localSha) {
                keep(context, target, Record(romId, server.id, server.updatedAt, serverSha))
                return Outcome(null, serverAt = at)
            }
            val handheldChanged = local != null && (kept == null || kept.sha1 != localSha)
            val useServer = !handheldChanged || ask() == Choice.USE_SERVER
            if (useServer) {
                if (local != null) {
                    // The handheld's save, kept beside the new one.
                    File(target.saveLocal.parentFile, target.saveLocal.name + ".before-romm").writeBytes(local)
                }
                RetroArchStorage.stage(target.saveLocal, bytes)
                bringRtc(context, target, saves)
                // The game starts from this save, not from slot.'s older
                // save state (the same guard as Keep Folder Saves).
                RetroArchStorage.markFolderSave(context.filesDir, target.core, target.stem)
                RetroArchStorage.applyFolderSaveMark(context.filesDir, target.core, target.stem, target.romIdentity)
                keep(context, target, Record(romId, server.id, server.updatedAt, serverSha))
                Outcome("Save brought in from RomM.", pick = Pick.BROUGHT, serverAt = at)
            } else {
                // The server's save, kept beside ours; ours goes up after the game.
                File(target.saveLocal.parentFile, target.saveLocal.name + ".romm").writeBytes(bytes)
                keep(context, target, Record(romId, server.id, server.updatedAt, ""))
                return Outcome(null, pick = Pick.KEPT_HANDHELD, serverAt = at)
            }
        }
    }

    /** Record saveId: the save was deleted, the server too. */
    private const val DELETED = -1L
    /** Record saveId: the save was deleted, the server not yet. */
    private const val DELETE_PENDING = -2L

    /**
     * The player deleted this game's save everywhere (SaveDeletes): the
     * server's saves of this game from slot.'s core go too, and nothing
     * older comes back from the server (only a save made after it). Offline,
     * the deletion waits for the next launch.
     */
    fun deleted(context: Context, target: RetroArchStorage.Target) {
        if (!enabled(context) || !Romm.signedIn(context)) return
        val platform = platformOf(target) ?: return
        val kept = record(context, target)
        try {
            val romId = kept?.romId?.takeIf { it > 0 } ?: Romm.romIdFor(context, platform, target.stem) ?: return
            val saves = Romm.saves(context, romId)
            val ours = saves.filter {
                (Systems.isSaveName(it.fileName) || it.fileName.endsWith(".rtc", ignoreCase = true)) &&
                    isOurs(it, target)
            }
            Romm.deleteSaves(context, ours.map { it.id })
            // What stays on the server (another client's) is "unchanged".
            val left = newest(saves - ours.toSet(), target)?.takeUnless { isOurs(it, target) }
            keep(context, target, Record(romId, DELETED, left?.updatedAt.orEmpty(), ""))
            SlotLog.i(TAG, "RomM: ${ours.size} save(s) of ${target.stem} deleted")
        } catch (error: Exception) {
            SlotLog.w(TAG, "RomM save deletion waits", error)
            keep(context, target, Record(kept?.romId ?: 0L, DELETE_PENDING, kept?.serverStamp.orEmpty(), ""))
        }
    }

    /** After a save in slot.: sends it when changed. Only a completed upload
     *  calls [onUploaded]; unchanged, deferred and failed saves never confirm. */
    fun afterSave(context: Context, target: RetroArchStorage.Target,
                  onUploaded: () -> Unit = {}): String? {
        if (!enabled(context) || !Romm.signedIn(context)) return null
        val platform = platformOf(target) ?: return null
        val local = target.saveLocal.takeIf { it.isFile }?.readBytes() ?: return null
        val localSha = sha1(local)
        return try {
            val kept = record(context, target)
            if (kept != null && kept.sha1 == localSha) return null
            // A deletion the server hasn't taken yet goes first.
            if (kept?.saveId == DELETE_PENDING) {
                deleted(context, target)
                if (record(context, target)?.saveId == DELETE_PENDING) return null
            }
            val romId = kept?.romId?.takeIf { it > 0 } ?: Romm.romIdFor(context, platform, target.stem) ?: return null
            val unchecked = uncheckedFile(context, target)
            if (unchecked.isFile) {
                // The launch didn't reach the server: send only over what
                // slot. last saw there. A save another handheld sent since
                // stays, and the next launch asks which to keep.
                val server = newest(Romm.saves(context, romId), target)
                if (server != null && (kept == null || server.updatedAt != kept.serverStamp)) {
                    SlotLog.i(TAG, "RomM: the server's save of ${target.stem} changed while slot. couldn't " +
                        "check it; not sent, the next launch asks")
                    return null
                }
                unchecked.delete()
            }
            val sent = Romm.uploadSave(context, romId, emulator(target), target.saveName, local)
            keep(context, target, Record(romId, sent.id, sent.updatedAt, localSha, sendRtc(context, target, romId, kept)))
            onUploaded()
            null
        } catch (error: Romm.SignedOut) {
            "Signed out of RomM: this save wasn't sent."
        } catch (error: Exception) {
            SlotLog.w(TAG, "RomM save upload skipped", error)
            TaskPill.problem("SAVE NOT SENT")
            if (error.message?.contains("can't keep saves") == true) "This RomM account can't keep saves."
            else null
        }
    }

    /** The game's clock after its save, when it changed; its fingerprint. */
    private fun sendRtc(context: Context, target: RetroArchStorage.Target, romId: Long, kept: Record?): String {
        val rtc = rtcOf(target).takeIf { it.isFile }?.readBytes() ?: return ""
        val sha = sha1(rtc)
        if (kept?.rtcSha1 == sha) return sha
        return try {
            Romm.uploadSave(context, romId, emulator(target), target.stem + ".rtc", rtc)
            sha
        } catch (error: Exception) {
            SlotLog.w(TAG, "RomM: ${target.stem}'s clock not sent", error)
            ""
        }
    }

    /** With a save brought in, its clock too, when the server has one. */
    private fun bringRtc(context: Context, target: RetroArchStorage.Target, saves: List<Romm.ServerSave>) {
        val server = saves.filter { it.fileName.endsWith(".rtc", ignoreCase = true) && isOurs(it, target) }
            .maxByOrNull { it.updatedAt } ?: return
        try {
            val bytes = Romm.downloadSave(context, server)
            if (bytes.isNotEmpty()) RetroArchStorage.stage(rtcOf(target), bytes)
        } catch (error: Exception) {
            SlotLog.w(TAG, "RomM: ${target.stem}'s clock not brought in", error)
        }
    }

    /** The server's newest save for this game: our core's first, else any
     *  save of the same kind (.srm, .eeprom, .eep) another client sent. */
    internal fun newest(saves: List<Romm.ServerSave>, target: RetroArchStorage.Target): Romm.ServerSave? {
        val kind = "." + Systems.saveExtension(target.core)
        val srm = saves.filter { it.fileName.endsWith(kind, ignoreCase = true) }
        val ours = srm.filter { isOurs(it, target) }
        return (ours.ifEmpty { srm }).maxByOrNull { it.updatedAt }
    }
}
