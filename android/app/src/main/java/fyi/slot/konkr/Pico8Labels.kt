package fyi.slot.konkr

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import android.util.AtomicFile
import android.util.Log
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.io.File

/**
 * PICO-8 carts bring their own label: a .p8.png is a 160x205 picture of the
 * cart, and its 128x128 label sits at (16, 24). That square is cut out
 * into the cart's label file once; slot-ui enlarges it pixel for pixel and
 * prints the title under it. Nothing is downloaded. A label already there
 * (Cart Studio's) is never replaced.
 */
internal object Pico8Labels {
    private const val TAG = "SlotPico8Labels"
    private const val LIBRARY = "rom-library-cache-v1.json"
    private const val MAX_LIBRARY_BYTES = 8L * 1024 * 1024
    private const val MAX_CART_BYTES = 1L * 1024 * 1024
    const val CART_W = 160
    const val CART_H = 205
    const val LABEL_X = 16
    const val LABEL_Y = 24
    const val LABEL_SIZE = 128

    /** Carts tried this run that gave no label, by uri: not read again. */
    private val failed = java.util.Collections.synchronizedSet(HashSet<String>())

    private data class Cart(val uri: String, val filename: String)

    private fun carts(context: Context): List<Cart> {
        val file = File(context.filesDir, LIBRARY)
        if (!file.isFile || file.length() !in 1..MAX_LIBRARY_BYTES) return emptyList()
        val root = try { JSONObject(file.readText(Charsets.UTF_8)) } catch (_: Exception) { return emptyList() }
        val rows = root.optJSONArray("games") ?: return emptyList()
        return (0 until rows.length().coerceAtMost(5000)).mapNotNull { i ->
            val row = rows.optJSONObject(i) ?: return@mapNotNull null
            val uri = row.optString("uri")
            val filename = row.optString("filename")
            if (row.optString("platform") != Systems.P8 || !uri.startsWith("content://")) return@mapNotNull null
            if (!filename.lowercase(java.util.Locale.ROOT).endsWith(".p8.png")) return@mapNotNull null
            Cart(uri, filename)
        }
    }

    /** Cut every PICO-8 cart's label that is missing. Returns how many were saved. */
    fun fill(context: Context, shouldContinue: () -> Boolean, onSaved: (String) -> Unit): Int {
        var saved = 0
        for (cart in carts(context)) {
            if (!shouldContinue()) break
            val target = CartLabelPaths.file(context.filesDir, cart.uri)
            if (target.exists() || cart.uri in failed) continue
            val png = try {
                context.contentResolver.openInputStream(Uri.parse(cart.uri))?.use { input ->
                    val out = ByteArrayOutputStream()
                    val buffer = ByteArray(16 * 1024)
                    while (out.size() <= MAX_CART_BYTES) {
                        val n = input.read(buffer)
                        if (n < 0) break
                        out.write(buffer, 0, n)
                    }
                    if (out.size() > MAX_CART_BYTES) null else label(out.toByteArray())
                }
            } catch (error: Exception) {
                Log.w(TAG, "Unreadable cart ${cart.filename.take(80)}", error)
                null
            }
            if (png == null) { failed += cart.uri; continue }
            try {
                target.parentFile?.mkdirs()
                val atomic = AtomicFile(target)
                val out = atomic.startWrite()
                try {
                    out.write(png)
                    atomic.finishWrite(out)
                } catch (error: Exception) {
                    atomic.failWrite(out)
                    throw error
                }
                saved++
                onSaved(cart.uri)
            } catch (error: Exception) {
                Log.w(TAG, "Label not saved for ${cart.filename.take(80)}", error)
                failed += cart.uri
            }
        }
        if (saved > 0) Log.i(TAG, "PICO8_LABELS saved=$saved")
        return saved
    }

    /** The cart picture's label as a 128x128 PNG, or null if it is not one. */
    fun label(cart: ByteArray): ByteArray? {
        if (!Systems.isPico8Png(cart)) return null
        val options = BitmapFactory.Options().apply { inPreferredConfig = Bitmap.Config.ARGB_8888 }
        val picture = BitmapFactory.decodeByteArray(cart, 0, cart.size, options) ?: return null
        try {
            if (picture.width != CART_W || picture.height != CART_H) return null
            val square = Bitmap.createBitmap(picture, LABEL_X, LABEL_Y, LABEL_SIZE, LABEL_SIZE)
            try {
                // The low two bits of every channel carry the cart's data:
                // clear them, so the label is the colours PICO-8 shows.
                val pixels = IntArray(LABEL_SIZE * LABEL_SIZE)
                square.getPixels(pixels, 0, LABEL_SIZE, 0, 0, LABEL_SIZE, LABEL_SIZE)
                for (i in pixels.indices) pixels[i] = (pixels[i] and 0x00fcfcfc) or (0xff shl 24)
                val clean = Bitmap.createBitmap(pixels, LABEL_SIZE, LABEL_SIZE, Bitmap.Config.ARGB_8888)
                try {
                    return ByteArrayOutputStream().use { out ->
                        if (!clean.compress(Bitmap.CompressFormat.PNG, 100, out)) return null
                        out.toByteArray()
                    }
                } finally { clean.recycle() }
            } finally { if (square !== picture) square.recycle() }
        } finally { picture.recycle() }
    }
}
