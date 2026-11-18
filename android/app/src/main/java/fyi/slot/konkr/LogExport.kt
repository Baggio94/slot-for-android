package fyi.slot.konkr

import android.app.Activity
import android.app.ActivityManager
import android.app.ApplicationExitInfo
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.util.Log
import java.io.Writer
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.concurrent.Executors

/**
 * About's X (#107): slot.'s own logs saved where the player picks, to join to
 * a GitHub issue. Nothing runs until asked: Android keeps the log itself, and
 * it is read once, on a worker, when the file is chosen. Android only gives an
 * app its own lines.
 */
internal object LogExport {
    const val REQUEST = 4714
    private const val TAG = "SlotLogs"
    /** The log's last lines: a few minutes of play, the crash before them. */
    private const val LINES = 20000
    /** An ANR's trace, cut past this. */
    private const val TRACE_BYTES = 256 * 1024

    private val worker = Executors.newSingleThreadExecutor()

    private fun stamp(time: Long, pattern: String) =
        SimpleDateFormat(pattern, Locale.US).format(Date(time))

    /** Android's "save as" page, on a dated file name. */
    fun intent(): Intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
        addCategory(Intent.CATEGORY_OPENABLE)
        type = "text/plain"
        putExtra(Intent.EXTRA_TITLE,
            "slot-logs-${stamp(System.currentTimeMillis(), "yyyyMMdd-HHmmss")}.txt")
    }

    /** slot.'s own journal (SlotLog): there even where Android's log keeps
     *  only errors (beta38). */
    private fun journal(native: String, out: java.io.Writer) {
        out.write("\n== slot.'s journal ==\n")
        for ((at, line) in SlotLog.snapshot(native)) {
            out.write(stamp(at, "MM-dd HH:mm:ss.SSS"))
            out.write(" ")
            out.write(line)
            out.write("\n")
        }
    }

    /** Writes the logs into [uri] off the UI thread; [done] hears whether it worked. */
    fun write(activity: Activity, uri: Uri, native: String, done: (Boolean) -> Unit) {
        val context = activity.applicationContext
        worker.execute {
            val ok = try {
                context.contentResolver.openOutputStream(uri, "wt")?.bufferedWriter()?.use { out ->
                    header(context, out)
                    exits(context, out)
                    journal(native, out)
                    logcat(out)
                } != null
            } catch (error: Exception) {
                Log.w(TAG, "Logs not saved", error)
                false
            }
            activity.runOnUiThread { done(ok) }
        }
    }

    private fun header(context: Context, out: Writer) {
        val info = context.packageManager.getPackageInfo(context.packageName, 0)
        @Suppress("DEPRECATION")
        val code = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) info.longVersionCode
            else info.versionCode.toLong()
        out.write("slot. ${info.versionName} ($code)\n")
        out.write("Device: ${Build.MANUFACTURER} ${Build.MODEL} (${Build.DEVICE})\n")
        out.write("Android ${Build.VERSION.RELEASE} (API ${Build.VERSION.SDK_INT}), build ${Build.DISPLAY}\n")
        out.write("Saved: ${stamp(System.currentTimeMillis(), "yyyy-MM-dd HH:mm:ss Z")}\n\n")
    }

    /** Why slot. last stopped, as Android recorded it (Android 11 and later). */
    private fun exits(context: Context, out: Writer) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return
        val manager = context.getSystemService(ActivityManager::class.java) ?: return
        val exits = try {
            manager.getHistoricalProcessExitReasons(context.packageName, 0, 5)
        } catch (error: Exception) { return }
        out.write("== Last exits ==\n")
        for (exit in exits) {
            out.write("${stamp(exit.timestamp, "yyyy-MM-dd HH:mm:ss")}  ${reason(exit.reason)}" +
                "  status ${exit.status}  ${exit.description.orEmpty()}\n")
            // An ANR's trace is text; a native crash's is a binary tombstone.
            if (exit.reason == ApplicationExitInfo.REASON_ANR) {
                try {
                    exit.traceInputStream?.use { stream ->
                        val bytes = stream.readNBytesCompat(TRACE_BYTES)
                        out.write(String(bytes, Charsets.UTF_8))
                        out.write("\n")
                    }
                } catch (_: Exception) {}
            }
        }
        out.write("\n")
    }

    private fun java.io.InputStream.readNBytesCompat(max: Int): ByteArray {
        val buffer = java.io.ByteArrayOutputStream()
        val chunk = ByteArray(8192)
        while (buffer.size() < max) {
            val n = read(chunk, 0, minOf(chunk.size, max - buffer.size()))
            if (n <= 0) break
            buffer.write(chunk, 0, n)
        }
        return buffer.toByteArray()
    }

    private fun reason(code: Int): String = when (code) {
        ApplicationExitInfo.REASON_CRASH -> "crash"
        ApplicationExitInfo.REASON_CRASH_NATIVE -> "native crash"
        ApplicationExitInfo.REASON_ANR -> "not responding"
        ApplicationExitInfo.REASON_LOW_MEMORY -> "low memory"
        ApplicationExitInfo.REASON_EXIT_SELF -> "closed itself"
        ApplicationExitInfo.REASON_SIGNALED -> "signaled"
        ApplicationExitInfo.REASON_USER_REQUESTED -> "closed by the player"
        ApplicationExitInfo.REASON_USER_STOPPED -> "stopped by the player"
        ApplicationExitInfo.REASON_EXCESSIVE_RESOURCE_USAGE -> "excessive resource usage"
        ApplicationExitInfo.REASON_DEPENDENCY_DIED -> "dependency died"
        ApplicationExitInfo.REASON_INITIALIZATION_FAILURE -> "initialization failure"
        ApplicationExitInfo.REASON_PERMISSION_CHANGE -> "permission change"
        else -> "other ($code)"
    }

    /** slot.'s lines in Android's log, the last [LINES]. */
    private fun logcat(out: Writer) {
        out.write("== Log ==\n")
        val process = ProcessBuilder("logcat", "-d", "-v", "threadtime", "-t", LINES.toString())
            .redirectErrorStream(true).start()
        try {
            process.inputStream.bufferedReader().use { reader ->
                val buffer = CharArray(8192)
                while (true) {
                    val n = reader.read(buffer)
                    if (n < 0) break
                    out.write(buffer, 0, n)
                }
            }
        } finally {
            process.destroy()
        }
    }
}
