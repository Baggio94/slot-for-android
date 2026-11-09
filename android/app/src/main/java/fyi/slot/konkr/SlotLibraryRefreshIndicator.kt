package fyi.slot.konkr

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.RectF
import android.os.SystemClock
import android.view.Gravity
import android.view.View
import android.widget.LinearLayout
import android.widget.TextView

/**
 * Library scan feedback in the shared transparent status rail. Visibility
 * is GONE when idle so the other task can take its position automatically.
 */
internal class SlotLibraryRefreshIndicator(context: Context, rail: LinearLayout) {
    private val scale =
        SlotUi.canvasScale(context).coerceIn(1f, 4f)
    private fun dim(n: Int) = (n * scale + .5f).toInt()
    private val root = LinearLayout(context).apply {
        orientation = LinearLayout.HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
        setPadding(dim(3), 0, dim(3), 0)
        isClickable = false
        isFocusable = false
        visibility = View.GONE
    }
    private val spinner = object : View(context) {
        val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            style = Paint.Style.STROKE
            strokeWidth = dim(2).toFloat()
            strokeCap = Paint.Cap.ROUND
            color = SlotUi.ink
        }
        override fun onDraw(canvas: Canvas) {
            super.onDraw(canvas)
            if (root.visibility != View.VISIBLE) return
            val pad = paint.strokeWidth + 1f
            canvas.drawArc(RectF(pad, pad, width - pad, height - pad),
                (SystemClock.uptimeMillis() / 4L % 360L).toFloat(),
                265f, false, paint)
            postInvalidateDelayed(40L)
        }
    }
    private val caption: TextView =
        SlotUi.label(context, "UPDATING LIBRARY", 9f, SlotUi.ink)
    /** A scan's thin bar under the caption: a sweep while the folders are
     * listed (total 0), then filled as the carts are read. */
    private var done = 0
    private var total = 0
    private val bar = object : View(context) {
        val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = SlotUi.ink }
        override fun onDraw(canvas: Canvas) {
            super.onDraw(canvas)
            if (root.visibility != View.VISIBLE || visibility != View.VISIBLE) return
            val w = width.toFloat()
            val h = height.toFloat()
            paint.alpha = 56
            canvas.drawRect(0f, 0f, w, h, paint)
            paint.alpha = 220
            if (total > 0) {
                canvas.drawRect(0f, 0f, w * done.coerceAtMost(total) / total, h, paint)
            } else {
                val t = (SystemClock.uptimeMillis() % 1600L) / 1600f
                val swing = if (t < .5f) t * 2f else 2f - t * 2f
                val seg = w * .3f
                val x = swing * (w - seg)
                canvas.drawRect(x, 0f, x + seg, h, paint)
                postInvalidateDelayed(40L)
            }
        }
    }

    init {
        root.addView(spinner, LinearLayout.LayoutParams(dim(13), dim(13)))
        caption.setPadding(dim(6), 0, 0, 0)
        val text = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER_VERTICAL
        }
        text.addView(caption)
        bar.visibility = View.GONE
        text.addView(bar, LinearLayout.LayoutParams(-1, dim(2)).apply {
            leftMargin = dim(6)
            topMargin = dim(2)
        })
        root.addView(text)
        rail.addView(root, LinearLayout.LayoutParams(-2, dim(22)).apply {
            bottomMargin = dim(2)
            gravity = Gravity.END
        })
    }

    fun setLightTheme(light: Boolean) {
        val ink = if (light) Color.BLACK else SlotUi.ink
        caption.setTextColor(ink)
        if (bar.paint.color != ink) {
            bar.paint.color = ink
            bar.invalidate()
        }
        if (spinner.paint.color != ink) {
            spinner.paint.color = ink
            spinner.invalidate()
        }
    }

    fun show() {
        caption.text = "UPDATING LIBRARY"
        bar.visibility = View.GONE
        root.visibility = View.VISIBLE
        spinner.invalidate()
    }

    /** Refresh Library: "SCANNING 240/1200" over a thin bar (Siham
     * 2026-11-09); carts found so far while the folders are listed. */
    fun showScan(done: Int, total: Int) {
        this.done = done
        this.total = total
        caption.text = when {
            total > 0 -> "SCANNING ${done.coerceAtMost(total)}/$total"
            done > 0 -> "SCANNING $done"
            else -> "SCANNING"
        }
        bar.visibility = View.VISIBLE
        root.visibility = View.VISIBLE
        spinner.invalidate()
        bar.invalidate()
    }
    fun hide() { root.visibility = View.GONE }
}
