package fyi.slot.konkr

import android.content.Context
import android.content.res.Configuration
import android.graphics.BitmapFactory
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.text.InputType
import android.view.Gravity
import android.view.View
import android.view.WindowManager
import android.view.inputmethod.EditorInfo
import android.widget.EditText
import android.widget.ImageView
import android.widget.TextView
import java.util.Locale

/**
 * One visual vocabulary for app-owned Android surfaces, taken from Slot's
 * upstream label.ttf (SIL OFL) and its charcoal / cream palette.
 * OS file/permission/IME dialogs and the original Cart Studio web editor
 * intentionally remain owned by Android / upstream Studio.
 */
internal object SlotUi {
    val background: Int = Color.rgb(23, 22, 25)
    @Volatile var menuBackground: Int = Color.rgb(32, 32, 36)
        private set
    val surface: Int = Color.rgb(37, 36, 43)
    @Volatile var selected: Int = Color.rgb(60, 60, 68)
        private set
    val ink: Int = Color.rgb(246, 244, 239)
    val muted: Int = Color.rgb(174, 171, 180)
    val rule: Int = Color.rgb(91, 89, 96)
    val accent: Int = Color.rgb(210, 188, 134)
    val error: Int = Color.rgb(241, 160, 146)

    /** The menu pages' text (Apps, RetroAchievements, the dialogs): white,
     * or dark on Light's and Original's light grounds. Toasts and the shelf's
     * indicators keep `ink`. */
    @Volatile var menuInk: Int = ink
        private set
    @Volatile var menuMuted: Int = muted
        private set
    @Volatile var menuRule: Int = rule
        private set
    @Volatile var menuSurface: Int = surface
        private set
    /** Set while the text is dark: Slot's white-rastered faces redrawn from
     * the ground (dark) to the ink (light), as the native Draw::Ink does. */
    @Volatile var menuInkFilter: android.graphics.ColorFilter? = null
        private set

    /** An Apps tile or the Set as Home button: Dark's own greys, else the
     * theme's bar (chosen) on its tiles. */
    fun appTile(chosen: Boolean, home: Boolean = false): Int = when {
        menuSurface != surface -> if (chosen) selected else menuSurface
        home -> if (chosen) Color.rgb(89, 87, 102) else Color.rgb(56, 55, 64)
        else -> if (chosen) Color.rgb(68, 66, 77) else Color.rgb(37, 36, 43)
    }

    private fun blend(a: Int, b: Int, t: Float): Int = Color.rgb(
        (Color.red(a) + (Color.red(b) - Color.red(a)) * t).toInt(),
        (Color.green(a) + (Color.green(b) - Color.green(a)) * t).toInt(),
        (Color.blue(a) + (Color.blue(b) - Color.blue(a)) * t).toInt())

    /** Rust publishes the active theme, including custom colours, OLED,
     * Light and Original. `darkInk`: the RGB of the menus' dark text, or -1. */
    fun updateMenuPalette(packed: Long, darkInk: Int = -1) {
        val ground = 0xff000000.toInt() or ((packed ushr 24).toInt() and 0xffffff)
        val pill = 0xff000000.toInt() or (packed.toInt() and 0xffffff)
        if (ground == menuBackground && pill == selected &&
            (darkInk == -1) == (menuInkFilter == null) &&
            (darkInk == -1 || (0xff000000.toInt() or darkInk) == menuInk)) return
        menuBackground = ground
        selected = pill
        if (darkInk == -1) {
            menuInk = ink
            menuMuted = muted
            menuRule = rule
            // Dark and OLED keep their tiles; Original's dark plastics tint theirs.
            menuSurface = if (ground == Color.rgb(32, 32, 36) || ground == Color.BLACK) surface
                else blend(ground, pill, .45f)
            menuInkFilter = null
        } else {
            val dark = 0xff000000.toInt() or darkInk
            menuInk = dark
            menuMuted = blend(dark, ground, .38f)
            menuRule = blend(dark, ground, .7f)
            menuSurface = blend(ground, Color.BLACK, .06f)
            val k = floatArrayOf(Color.red(dark) - Color.red(ground).toFloat(),
                Color.green(dark) - Color.green(ground).toFloat(),
                Color.blue(dark) - Color.blue(ground).toFloat())
            val paper = floatArrayOf(Color.red(ground).toFloat(), Color.green(ground).toFloat(),
                Color.blue(ground).toFloat())
            val luma = floatArrayOf(.299f, .587f, .114f)
            menuInkFilter = android.graphics.ColorMatrixColorFilter(android.graphics.ColorMatrix(
                FloatArray(20) { i ->
                    val (row, col) = i / 5 to i % 5
                    when {
                        row == 3 -> if (col == 3) 1f else 0f
                        col < 3 -> k[row] / 255f * luma[col]
                        col == 4 -> paper[row]
                        else -> 0f
                    }
                }))
        }
    }

    @Volatile private var legendSplits: Map<String, Int>? = null

    /** Where a legend asset's word starts, in texels (export_apps_legend). */
    private fun legendSplit(context: Context, asset: String): Int? {
        val splits = legendSplits ?: runCatching {
            context.assets.open("apps_controls/legend-splits.txt").bufferedReader().use { reader ->
                reader.readLines().mapNotNull { line ->
                    val parts = line.trim().split(' ')
                    if (parts.size == 2) parts[1].toIntOrNull()?.let { parts[0] to it } else null
                }.toMap()
            }
        }.getOrDefault(emptyMap()).also { legendSplits = it }
        return splits[asset]
    }

    /** Light and Original: a legend's keycap as drawn, its word alone in the
     * menus' ink (as the native legends); unchanged in Dark and OLED. */
    fun inkedLegend(context: Context, asset: String, image: android.graphics.Bitmap): android.graphics.Bitmap {
        val filter = menuInkFilter ?: return image
        val split = (legendSplit(context, asset) ?: 0).coerceIn(0, image.width)
        val out = android.graphics.Bitmap.createBitmap(image.width, image.height,
            android.graphics.Bitmap.Config.ARGB_8888)
        val canvas = android.graphics.Canvas(out)
        val keycap = android.graphics.Rect(0, 0, split, image.height)
        val word = android.graphics.Rect(split, 0, image.width, image.height)
        canvas.drawBitmap(image, keycap, keycap, null)
        canvas.drawBitmap(image, word, word, android.graphics.Paint().apply { colorFilter = filter })
        return out
    }

    fun rowBackground(context: Context, focused: Boolean): GradientDrawable =
        GradientDrawable().apply {
            setColor(if (focused) selected else Color.TRANSPARENT)
            cornerRadius = menuPixel(context, 11).toFloat()
        }

    fun menuHeader(context: Context, title: String): View =
        android.widget.LinearLayout(context).apply {
            orientation = android.widget.LinearLayout.VERTICAL
            val path = when (title.uppercase(Locale.ROOT)) {
                "APPS", "RETROACHIEVEMENTS" -> "MENU"
                "ROMM" -> "MENU › LIBRARY"
                "SIGN OUT" -> "MENU › RETROACHIEVEMENTS"
                "SAVE CONFLICT", "SAVE STATE" -> "LIBRARY"
                else -> "slot."
            }
            addView(label(context, path, 13f, menuInk).apply {
                setTextSize(android.util.TypedValue.COMPLEX_UNIT_PX, menuPixel(context, 13).toFloat())
                letterSpacing = .15f
                alpha = .5f
                setPadding(menuPixel(context, 12), 0, 0, 0)
            }, android.widget.LinearLayout.LayoutParams(-1, menuPixel(context, 18)))
            addView(menuHeading(context, title),
                android.widget.LinearLayout.LayoutParams(-1, menuPixel(context, 32)))
        }

    @Volatile private var cachedFont: Typeface? = null

    fun font(context: Context): Typeface {
        cachedFont?.let { return it }
        val typeface = runCatching {
            Typeface.createFromAsset(context.applicationContext.assets, "label.ttf")
        }.getOrElse { Typeface.create("sans-serif-condensed", Typeface.BOLD) }
        cachedFont = typeface
        return typeface
    }

    fun dp(context: Context, amount: Int): Int =
        (context.resources.displayMetrics.density * amount + 0.5f).toInt()

    /** The KONKR's density, against which every other panel is matched. */
    const val KONKR_DPI = 240
    private const val KONKR_SHORT_SIDE = 640

    /** The display's short side in pixels (640 on the KONKR). */
    fun panelShortSide(context: Context): Int {
        val bounds = runCatching {
            context.getSystemService(WindowManager::class.java).maximumWindowMetrics.bounds
        }.getOrNull()
        val metrics = context.resources.displayMetrics
        val (w, h) = bounds?.let { it.width() to it.height() }
            ?: (metrics.widthPixels to metrics.heightPixels)
        return minOf(w, h).coerceAtLeast(1)
    }

    /**
     * Android-drawn screens keep the KONKR's proportions on any panel: dp and
     * sp are rescaled so a dp covers the same share of the screen height.
     * The KONKR itself keeps its system density untouched.
     */
    fun panelDensityOverride(base: Context): Configuration? {
        val short = panelShortSide(base)
        if (short == KONKR_SHORT_SIDE) return null
        return Configuration().apply {
            densityDpi = (KONKR_DPI * short / KONKR_SHORT_SIDE.toFloat() + .5f).toInt()
                .coerceAtLeast(120)
        }
    }

    /** Panel pixels per Slot canvas pixel: the canvas is 480 high (4/3 on the KONKR). */
    fun canvasScale(context: Context): Float =
        context.resources.displayMetrics.let { minOf(it.widthPixels, it.heightPixels) } / 480f

    fun plate(context: Context, fill: Int = surface, focused: Boolean = false): GradientDrawable =
        GradientDrawable().apply {
            setColor(fill)
            cornerRadius = dp(context, 4).toFloat()
            setStroke(dp(context, if (focused) 2 else 1), if (focused) ink else rule)
        }

    fun label(
        context: Context, value: String, sp: Float = 13f,
        color: Int = ink, upper: Boolean = true,
    ): TextView = TextView(context).apply {
        text = if (upper) value.uppercase(Locale.ROOT) else value
        typeface = font(context)
        textSize = sp
        letterSpacing = .09f
        includeFontPadding = false
        setTextColor(color)
        gravity = Gravity.CENTER_VERTICAL
    }

    /** Native menus paint quick_value_face at (32, 33), with .79
     * geometry scale and .68 alpha, on a 720×480 canvas. Keep the real
     * Rust rasterization and transparent side padding, not Android text.
     */
    /** apps_controls PNGs are drawn at 3x Slot's logical pixels. */
    const val LEGEND_DENSITY = 3f

    fun menuPixel(context: Context, px: Int): Int =
        (px * canvasScale(context) + .5f).toInt()

    /** Native Slot menu headings start at x=32; entries begin at x=37. */
    fun menuEntryInset(context: Context): Int = menuPixel(context, 5)

    fun menuHeading(context: Context, title: String): View {
        val asset = when (title.uppercase(Locale.ROOT)) {
            "APPS" -> "heading-apps.png"
            "RETROACHIEVEMENTS" -> "heading-ra.png"
            "SAVE CONFLICT" -> "heading-save-conflict.png"
            "SAVE STATE" -> "heading-save-state.png"
            "SIGN OUT" -> "heading-sign-out.png"
            "LINK — SHARE CARTRIDGE" -> "heading-link-consent.png"
            "MULTIPLAYER — DIRECT JOIN" -> "heading-direct-join.png"
            else -> null
        }
        val image = asset?.let { file ->
            runCatching {
                context.assets.open("apps_controls/$file").use {
                    BitmapFactory.decodeStream(it)
                }
            }.getOrNull()
        }
        if (image == null) return label(context, title, 16f, menuInk).apply {
            setTextSize(android.util.TypedValue.COMPLEX_UNIT_PX, menuPixel(context, 26).toFloat())
            setPadding(menuPixel(context, 16), 0, 0, 0)
        }
        return android.widget.FrameLayout(context).apply {
            addView(ImageView(context).apply {
                setImageBitmap(image)
                scaleType = ImageView.ScaleType.FIT_XY
                alpha = 1f
                colorFilter = menuInkFilter
            }, android.widget.FrameLayout.LayoutParams(
                menuPixel(context, (image.width * .79f / LEGEND_DENSITY).toInt()),
                menuPixel(context, (image.height * .79f / LEGEND_DENSITY).toInt()),
                Gravity.START or Gravity.TOP))
        }
    }

    fun button(context: Context, value: String, onClick: () -> Unit): TextView =
        label(context, value, 13f).apply {
            gravity = Gravity.CENTER
            isFocusable = true
            isClickable = true
            minHeight = dp(context, 41)
            setPadding(dp(context, 11), dp(context, 7), dp(context, 11), dp(context, 7))
            background = plate(context)
            setOnFocusChangeListener { view, focused ->
                view.background = plate(context, if (focused) selected else surface, focused)
            }
            setOnClickListener { onClick() }
        }

    /** Controller-key legend with a real Slot-font cream keycap, not Unicode. */
    fun hint(context: Context, key: String, action: String): android.widget.LinearLayout =
        android.widget.LinearLayout(context).apply {
            orientation = android.widget.LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER
            val cap = label(context, key, 12f, menuBackground).apply {
                gravity = Gravity.CENTER
                minWidth = dp(context, 23)
                minHeight = dp(context, 21)
                setPadding(dp(context, 5), 0, dp(context, 5), 0)
                setBackgroundColor(menuInk)
            }
            addView(cap)
            val name = label(context, action, 12f, menuMuted).apply {
                setPadding(dp(context, 7), 0, 0, 0)
            }
            addView(name)
        }

    /** The same upstream-rastered keycaps used by the native Slot renderer.
     * Assets are generated at APK build-time by export_apps_legend.rs.
     * Retain the label-font approximation only as a safe asset fallback.
     */
    fun nativeHint(context: Context, key: String, action: String): View {
        val assetName = when (key.uppercase(Locale.ROOT) to action.uppercase(Locale.ROOT)) {
            "A" to "SELECT" -> "select.png"
            "B" to "BACK" -> "back.png"
            "START" to "REFRESH" -> "refresh.png"
            "X" to "SIGN OUT" -> "sign-out.png"
            "L1" to "PAIRING CODE" -> "l1-pairing-code.png"
            "R1" to "ACCOUNT" -> "r1-account.png"
            "LEFT RIGHT" to "CHANGE" -> "change.png"
            else -> return hint(context, key, action)
        }
        val image = runCatching {
            context.assets.open("apps_controls/$assetName").use { BitmapFactory.decodeStream(it) }
        }.getOrNull() ?: return hint(context, key, action)
        val scale = canvasScale(context).coerceIn(1f, 4f) / LEGEND_DENSITY
        // A centered container allows weighted RA footer cells and larger
        // Cart Studio bars without stretching upstream's exact 24px raster.
        return android.widget.FrameLayout(context).apply {
            contentDescription = "$key $action"
            addView(ImageView(context).apply {
                setImageBitmap(inkedLegend(context, assetName, image))
                scaleType = ImageView.ScaleType.FIT_CENTER
                isClickable = false
            }, android.widget.FrameLayout.LayoutParams(
                (image.width * scale).toInt().coerceAtLeast(1),
                (image.height * scale).toInt().coerceAtLeast(1),
                Gravity.CENTER
            ))
        }
    }

    fun field(context: Context, hint: String, password: Boolean = false): EditText =
        EditText(context).apply {
            this.hint = hint
            isSingleLine = true
            inputType = if (password)
                InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD
            else
                InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
            // Android changes a password field's font when inputType is set:
            // re-apply upstream label.ttf AFTER masking/IME flags.
            typeface = font(context)
            textSize = 15f
            setTextColor(menuInk)
            setHintTextColor(menuMuted)
            letterSpacing = .04f
            imeOptions = if (password) EditorInfo.IME_ACTION_DONE
                else EditorInfo.IME_ACTION_NEXT
            setSelectAllOnFocus(false)
            setPadding(dp(context, 13), dp(context, 10),
                dp(context, 13), dp(context, 10))
            minHeight = dp(context, 48)
            background = plate(context)
            setOnFocusChangeListener { view, focused ->
                view.background = plate(context, surface, focused)
            }
        }
}
