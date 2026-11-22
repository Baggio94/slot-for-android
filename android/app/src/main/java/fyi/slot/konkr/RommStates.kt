package fyi.slot.konkr

import android.content.Context
import org.json.JSONObject
import java.io.File
import java.time.OffsetDateTime
import java.util.Locale

/**
 * Sync States with RomM (slot. 3.0, beta; Library > Saves & States): a
 * game's resume state goes to the player's RomM server after each Save &
 * Eject, and a newer one from the server comes back before the game starts,
 * for the same core only, so a game picks up where another handheld left it.
 *
 * The same logic as RommSaves, which it needs on (a state holds its game's
 * save too). The file is RetroArch's: <rom>.state.auto, the core's name as
 * "emulator", the same bytes slot. writes to RetroArch's states folder, with
 * a picture of the game. What slot. last exchanged is remembered per game
 * and core (the server's state, its date, the state's fingerprint):
 *  - the server changed, the handheld didn't: the server's state comes in;
 *  - the handheld changed, the server didn't: it is sent after the game;
 *  - both changed: the player chooses, and the other one is kept aside.
 * A state is never resumed over a newer save: the server's save newer than
 * its state, or this handheld's save kept over the server's, leaves the
 * server's state where it is. Nothing here ever stops a game.
 * Scenarios: Siham's validated list of 2026-11-22.
 */
internal object RommStates {
    private const val TAG = "SlotRomm"
    private const val PREFS = "slot-romm-saves"
    private const val KEY = "states"

    fun enabled(context: Context): Boolean =
        RommSaves.enabled(context) &&
            context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getBoolean(KEY, false)

    /** Library's value: 0 RomM not connected, 1 off, 2 on, 3 off and idle
     *  (Sync Saves with RomM is off). */
    fun state(context: Context): Int = when {
        !Romm.signedIn(context) -> 0
        !RommSaves.enabled(context) -> 3
        enabled(context) -> 2
        else -> 1
    }

    /** Turns the sync on or off; a message when it can't be. */
    fun toggle(context: Context): String? {
        if (!Romm.signedIn(context)) return "Connect to RomM first, in Library > RomM Server."
        if (!RommSaves.enabled(context)) return "Turn on Sync Saves with RomM first."
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        prefs.edit().putBoolean(KEY, !prefs.getBoolean(KEY, false)).apply()
        return null
    }

    /** What the server's state was at the last exchange, and ours then
     *  (the uncompressed state's fingerprint). */
    internal data class Record(val romId: Long, val stateId: Long, val serverStamp: String, val sha1: String)

    private fun recordFile(context: Context, target: RetroArchStorage.Target): File =
        File(File(context.filesDir, "RommSync"),
            GameKeys.digest(target.romUri) + "." + RetroArchStorage.coreLower(target.core) + ".state.json")

    private fun uncheckedFile(context: Context, target: RetroArchStorage.Target): File =
        File(recordFile(context, target).path.removeSuffix(".json") + ".unchecked")

    private fun record(context: Context, target: RetroArchStorage.Target): Record? = try {
        val json = JSONObject(recordFile(context, target).readText(Charsets.UTF_8))
        Record(json.getLong("rom"), json.getLong("state"), json.getString("stamp"), json.getString("sha1"))
    } catch (_: Exception) { null }

    private fun keep(context: Context, target: RetroArchStorage.Target, record: Record) {
        val file = recordFile(context, target)
        file.parentFile?.mkdirs()
        val temp = File(file.parentFile, file.name + ".tmp")
        temp.writeText(JSONObject().put("rom", record.romId).put("state", record.stateId)
            .put("stamp", record.serverStamp).put("sha1", record.sha1).toString(), Charsets.UTF_8)
        if (!temp.renameTo(file)) temp.delete()
    }

    /** The state's name on the server, as RetroArch names it. */
    internal fun fileName(target: RetroArchStorage.Target): String = target.stem + ".state.auto"

    /** This game's resume state for this core among the server's: the
     *  newest with RetroArch's name and exactly this core (never another
     *  core's, nor mGBA's for gpSP: their states differ). */
    internal fun newest(states: List<Romm.ServerSave>, target: RetroArchStorage.Target): Romm.ServerSave? {
        val core = RetroArchStorage.coreLower(target.core)
        return states.filter { state ->
            state.emulator.lowercase(Locale.ROOT) == core && listOf(state.nameNoTags, state.fileName).any {
                it.endsWith(".state.auto", ignoreCase = true) &&
                    Romm.sameName(it.dropLast(".state.auto".length), target.stem)
            }
        }.maxWithOrNull { a, b -> compareStamps(a.updatedAt, b.updatedAt) }
    }

    /** Two of the server's dates: RomM's ISO dates, compared as instants
     *  (else as text, which orders them too when they share a format). */
    internal fun compareStamps(a: String, b: String): Int = try {
        OffsetDateTime.parse(a.replace(" ", "T")).compareTo(OffsetDateTime.parse(b.replace(" ", "T")))
    } catch (_: Exception) {
        a.compareTo(b)
    }

    /** What to do about the server's state at a launch (pure, tested). */
    internal enum class Plan {
        /** Nothing to do: no state there, unchanged, or the same bytes. */
        NOTHING,
        /** The server's save is newer than its state: the save wins. */
        SAVE_IS_NEWER,
        /** This handheld's save was kept over the server's: so is its state. */
        HANDHELD_SAVE_KEPT,
        /** Another handheld deleted it: ask. */
        DELETED_ELSEWHERE,
        /** Bring the server's in. */
        BRING,
        /** Both changed: ask. */
        ASK,
    }

    internal fun plan(kept: Record?, serverStamp: String?, local: ByteArray?, pick: RommSaves.Pick,
                      serverSaveAt: String, staleHere: Boolean): Plan {
        if (serverStamp == null) {
            return if (kept != null && kept.stateId > 0 && local != null) Plan.DELETED_ELSEWHERE else Plan.NOTHING
        }
        if (kept != null && kept.serverStamp == serverStamp) return Plan.NOTHING
        if (pick == RommSaves.Pick.KEPT_HANDHELD) return Plan.HANDHELD_SAVE_KEPT
        if (serverSaveAt.isNotEmpty() && compareStamps(serverSaveAt, serverStamp) > 0) return Plan.SAVE_IS_NEWER
        // This handheld saved in-game after its own state, and the save
        // didn't come from the server: that save is the newest progress.
        if (staleHere && pick != RommSaves.Pick.BROUGHT) return Plan.HANDHELD_SAVE_KEPT
        val handheldChanged = local != null && (kept == null || kept.sha1 != RommSaves.sha1(local))
        return if (handheldChanged) Plan.ASK else Plan.BRING
    }

    /** What to do when both the server and the handheld changed. */
    enum class Choice { KEEP_HANDHELD, USE_SERVER }

    class Outcome(val message: String?, val deleteHere: Boolean = false)

    /**
     * Before a game starts, after its save: brings the server's state in
     * when it is the newer one, within [budgetMs]. [ask] and
     * [askDeletedElsewhere] are called on this worker thread, blocking.
     */
    fun beforeLaunch(context: Context, target: RetroArchStorage.Target, save: RommSaves.Outcome,
                     budgetMs: Long, ask: () -> Choice, askDeletedElsewhere: () -> Boolean): Outcome {
        if (!enabled(context) || !Romm.signedIn(context)) return Outcome(null)
        val platform = RommSaves.platformOf(target) ?: return Outcome(null)
        val started = System.nanoTime()
        return try {
            Romm.within(budgetMs) { checkServer(context, target, platform, save, ask, askDeletedElsewhere) }.also {
                uncheckedFile(context, target).delete()
            }
        } catch (error: Romm.SignedOut) {
            Outcome("Signed out of RomM: states aren't synced.")
        } catch (error: Exception) {
            if (error is java.net.SocketTimeoutException || error is java.net.ConnectException ||
                error is java.net.UnknownHostException || error is java.net.NoRouteToHostException ||
                error is java.util.concurrent.TimeoutException) {
                try { uncheckedFile(context, target).apply { parentFile?.mkdirs() }.writeText("1") } catch (_: Exception) {}
            }
            SlotLog.w(TAG, "RomM state check skipped after ${(System.nanoTime() - started) / 1_000_000}ms: " +
                "the handheld's state", error)
            Outcome(null)
        }
    }

    /** The launch had no time left to check: the state goes up after the
     *  game only if the server didn't change meanwhile (as RommSaves). */
    fun unchecked(context: Context, target: RetroArchStorage.Target) {
        try { uncheckedFile(context, target).apply { parentFile?.mkdirs() }.writeText("1") } catch (_: Exception) {}
        SlotLog.i(TAG, "RomM state of ${target.core}/${target.stem} not checked: no time left at the launch")
    }

    private fun checkServer(context: Context, target: RetroArchStorage.Target, platform: String,
                            save: RommSaves.Outcome, ask: () -> Choice,
                            askDeletedElsewhere: () -> Boolean): Outcome {
        if (record(context, target)?.stateId == DELETE_PENDING) {
            deleted(context, target)
            if (record(context, target)?.stateId == DELETE_PENDING) return Outcome(null)
        }
        val kept = record(context, target)
        val romId = kept?.romId?.takeIf { it > 0 } ?: Romm.romIdFor(context, platform, target.stem)
            ?: return Outcome(null)
        val server = newest(Romm.states(context, romId), target)
        val local = RetroArchStorage.currentAutoState(context, target)
        val stale = RetroArchStorage.staleStateMark(context, target).isFile
        val plan = plan(kept, server?.updatedAt, local, save.pick, save.serverAt, stale)
        val name = "${target.core}/${target.stem}"
        fun seen(sha: String) = server?.let { keep(context, target, Record(romId, it.id, it.updatedAt, sha)) }
        when (plan) {
            Plan.NOTHING -> {
                if (server != null && kept?.serverStamp != server.updatedAt) seen(local?.let(RommSaves::sha1).orEmpty())
                return Outcome(null)
            }
            Plan.DELETED_ELSEWHERE -> {
                val delete = askDeletedElsewhere()
                keep(context, target, Record(romId, DELETED, "", ""))
                SlotLog.i(TAG, "RomM state of $name deleted on another handheld; " +
                    if (delete) "deleted here too" else "kept here")
                return Outcome(null, deleteHere = delete)
            }
            Plan.SAVE_IS_NEWER, Plan.HANDHELD_SAVE_KEPT -> {
                // The server's state stays there; this handheld's goes up
                // after the game, as its save does.
                seen(if (plan == Plan.SAVE_IS_NEWER) local?.let(RommSaves::sha1).orEmpty() else "")
                SlotLog.i(TAG, "RomM state of $name not resumed: " +
                    if (plan == Plan.SAVE_IS_NEWER) "the server's save is newer" else "this handheld's save is kept")
                return Outcome(null)
            }
            Plan.BRING, Plan.ASK -> {}
        }
        server ?: return Outcome(null)
        if (RetroArchStorage.autoStateProtected(context, target)) {
            seen("")
            SlotLog.i(TAG, "RomM state of $name not resumed: a core switch set the older states aside")
            return Outcome(null)
        }
        val raw = try {
            RetroArchCompression.decode(Romm.downloadState(context, server))
        } catch (error: Exception) {
            SlotLog.w(TAG, "RomM state of $name unreadable; the handheld's kept", error)
            seen(local?.let(RommSaves::sha1).orEmpty())
            return Outcome(null)
        }
        val serverSha = RommSaves.sha1(raw)
        if (local != null && serverSha == RommSaves.sha1(local)) {
            seen(serverSha)
            return Outcome(null)
        }
        if (plan == Plan.ASK && ask() == Choice.KEEP_HANDHELD) {
            // The server's state, kept aside; ours goes up after the game.
            aside(context, target, "romm").writeBytes(raw)
            seen("")
            SlotLog.i(TAG, "RomM state of $name: this handheld's kept, the server's set aside")
            return Outcome(null)
        }
        return try {
            local?.let { aside(context, target, "before-romm").writeBytes(it) }
            RetroArchStorage.adoptAutoState(context, target, raw)
            // The save came with it: the state is at least as new, resume it.
            if (save.pick == RommSaves.Pick.BROUGHT) RetroArchStorage.staleStateMark(context, target).delete()
            seen(serverSha)
            SlotLog.i(TAG, "RomM state of $name brought in (${raw.size} bytes)")
            Outcome("State brought in from RomM.")
        } catch (error: Exception) {
            // Not a RetroArch state slot. can resume: it stays on the server.
            SlotLog.w(TAG, "RomM state of $name not resumable; the handheld's kept", error)
            seen(local?.let(RommSaves::sha1).orEmpty())
            Outcome(null)
        }
    }

    /** A message the player sees once, not at every Save & Eject. */
    private fun once(context: Context, message: String): String? {
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        if (prefs.getString("states-told", null) == message) return null
        prefs.edit().putString("states-told", message).apply()
        return message
    }

    /** Where a state set aside is kept, on this handheld only. */
    private fun aside(context: Context, target: RetroArchStorage.Target, why: String): File =
        File(File(context.filesDir, "RommSync"),
            GameKeys.digest(target.romUri) + "." + RetroArchStorage.coreLower(target.core) + ".state.$why")
            .apply { parentFile?.mkdirs() }

    private const val DELETED = -1L
    private const val DELETE_PENDING = -2L

    /** The game's resume state was deleted everywhere: this core's on the
     *  server goes too. Offline, the deletion waits for the next launch. */
    fun deleted(context: Context, target: RetroArchStorage.Target) {
        if (!enabled(context) || !Romm.signedIn(context)) return
        val platform = RommSaves.platformOf(target) ?: return
        val kept = record(context, target)
        try {
            val romId = kept?.romId?.takeIf { it > 0 } ?: Romm.romIdFor(context, platform, target.stem) ?: return
            val states = Romm.states(context, romId)
            val core = RetroArchStorage.coreLower(target.core)
            val ours = states.filter { state ->
                state.emulator.lowercase(Locale.ROOT) == core && listOf(state.nameNoTags, state.fileName).any {
                    it.endsWith(".state.auto", ignoreCase = true) &&
                        Romm.sameName(it.dropLast(".state.auto".length), target.stem)
                }
            }
            Romm.deleteStates(context, ours.map { it.id })
            keep(context, target, Record(romId, DELETED, "", ""))
            SlotLog.i(TAG, "RomM: ${ours.size} state(s) of ${target.stem} deleted")
        } catch (error: Exception) {
            SlotLog.w(TAG, "RomM state deletion waits", error)
            keep(context, target, Record(kept?.romId ?: 0L, DELETE_PENDING, kept?.serverStamp.orEmpty(), ""))
        }
    }

    /** After a Save & Eject: sends the resume state, with a picture of the
     *  game, when it changed since the exchange. A message, if any. */
    fun afterSave(context: Context, target: RetroArchStorage.Target): String? {
        if (!enabled(context) || !Romm.signedIn(context)) return null
        val platform = RommSaves.platformOf(target) ?: return null
        val export = RetroArchStorage.autoStateExport(context, target)
        val raw = export.takeIf { it.isFile && it.length() > 0 }?.readBytes() ?: return null
        val localSha = RommSaves.sha1(raw)
        return try {
            val kept = record(context, target)
            if (kept != null && kept.sha1 == localSha) return null
            if (kept?.stateId == DELETE_PENDING) {
                deleted(context, target)
                if (record(context, target)?.stateId == DELETE_PENDING) return null
            }
            val romId = kept?.romId?.takeIf { it > 0 } ?: Romm.romIdFor(context, platform, target.stem) ?: return null
            val unchecked = uncheckedFile(context, target)
            if (unchecked.isFile) {
                // The launch didn't reach the server: send only over what
                // slot. last saw there; else the next launch asks.
                val server = newest(Romm.states(context, romId), target)
                if (server != null && (kept == null || server.updatedAt != kept.serverStamp)) {
                    SlotLog.i(TAG, "RomM: the server's state of ${target.stem} changed while slot. couldn't " +
                        "check it; not sent, the next launch asks")
                    return null
                }
                unchecked.delete()
            }
            val picture = File(export.path + ".png").takeIf { it.isFile && it.length() in 1..2_000_000 }?.readBytes()
            val sent = Romm.uploadState(context, romId, RetroArchStorage.coreLower(target.core), fileName(target),
                RetroArchCompression.encode(raw), picture)
            keep(context, target, Record(romId, sent.id, sent.updatedAt, localSha))
            context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().remove("states-told").apply()
            SlotLog.i(TAG, "RomM state of ${target.core}/${target.stem} sent (${raw.size} bytes)")
            null
        } catch (error: Romm.SignedOut) {
            "Signed out of RomM: this state wasn't sent."
        } catch (error: Exception) {
            SlotLog.w(TAG, "RomM state upload skipped", error)
            TaskPill.problem("STATE NOT SENT")
            when {
                error.message?.contains("can't keep states") == true && error.message?.contains("server") == true ->
                    once(context, "This RomM server can't keep states.")
                error.message?.contains("can't keep states") == true ->
                    once(context, "This RomM account can't keep states.")
                else -> null
            }
        }
    }
}
