package fyi.slot.konkr

import android.app.Activity
import android.app.Dialog
import android.content.Context
import android.graphics.Color
import android.graphics.drawable.ColorDrawable
import android.view.Gravity
import android.view.WindowInsets
import android.view.KeyEvent
import android.view.View
import android.view.ViewGroup
import android.view.Window
import android.view.WindowManager
import android.view.inputmethod.InputMethodManager
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView

/**
 * Single native-Slot-style confirmation / form surface for application-owned
 * dialogs. A vertical selected-row list, authentic font and controller legend
 * match Slot's menus. Android-owned permission and IME screens stay native.
 *
 * B cancels; D-pad navigates all fields/actions; A selects (or edits a field).
 * Sensitive confirmations default to the first, caller-provided safe action.
 */
internal class SlotModalDialog(
    private val activity: Activity,
    title: String,
    message: String,
    private val onDismiss: () -> Unit = {},
) {
    private val isRaLogin = title.equals("RETROACHIEVEMENTS", ignoreCase = true)
    private val isRaSignOut = title.equals("SIGN OUT", ignoreCase = true)
    private var busy = false
    private val buttons = ArrayList<TextView>()
    private val secretFields = ArrayList<EditText>()
    private val fields = ArrayList<EditText>()
    /** Each field's label, shown and hidden with it. */
    private val fieldLabels = HashMap<EditText, View>()
    private var messageText: TextView? = null
    /** Rows whose value Left / Right (or A) change: the value's step. */
    private val choices = HashMap<TextView, (Int) -> Unit>()
    private var onL1: (() -> Unit)? = null
    private var onR1: (() -> Unit)? = null
    private lateinit var legend: LinearLayout
    private val dialog = object : Dialog(activity, android.R.style.Theme_Black_NoTitleBar_Fullscreen) {
        override fun dispatchKeyEvent(event: KeyEvent): Boolean {
            // Physical KONKR buttons, never synthetic Mac keyboard mapping.
            val keyCode = event.keyCode
            when (keyCode) {
                KeyEvent.KEYCODE_BUTTON_B, KeyEvent.KEYCODE_BACK -> {
                    if (event.action == KeyEvent.ACTION_DOWN &&
                        event.repeatCount == 0 && !busy) {
                        // B while typing closes the keyboard only: the page
                        // and its text stay (Siham 2026-11-09).
                        val imeShown = window?.decorView?.rootWindowInsets
                            ?.isVisible(WindowInsets.Type.ime()) == true
                        val focus = currentFocus
                        if (imeShown && focus is EditText) {
                            (activity.getSystemService(Context.INPUT_METHOD_SERVICE)
                                as? InputMethodManager)?.hideSoftInputFromWindow(focus.windowToken, 0)
                        } else dismiss()
                    }
                    return true
                }
                KeyEvent.KEYCODE_BUTTON_L1, KeyEvent.KEYCODE_BUTTON_R1 -> {
                    val action = if (keyCode == KeyEvent.KEYCODE_BUTTON_L1) onL1 else onR1
                    if (action != null) {
                        if (event.action == KeyEvent.ACTION_DOWN &&
                            event.repeatCount == 0 && !busy) action()
                        return true
                    }
                }
                KeyEvent.KEYCODE_DPAD_UP, KeyEvent.KEYCODE_DPAD_DOWN -> {
                    if (event.action == KeyEvent.ACTION_DOWN &&
                        event.repeatCount == 0 && !busy) {
                        moveFocus(if (event.keyCode == KeyEvent.KEYCODE_DPAD_DOWN) 1 else -1)
                    }
                    return true
                }
                KeyEvent.KEYCODE_DPAD_LEFT, KeyEvent.KEYCODE_DPAD_RIGHT -> {
                    // Preserve editing/cursor semantics when IME has focus.
                    if (currentFocus is EditText) return super.dispatchKeyEvent(event)
                    if (event.action == KeyEvent.ACTION_DOWN &&
                        event.repeatCount == 0 && !busy) {
                        val step = if (event.keyCode == KeyEvent.KEYCODE_DPAD_RIGHT) 1 else -1
                        val choice = (currentFocus as? TextView)?.let { choices[it] }
                        if (choice != null) choice(step) else moveFocus(step)
                    }
                    return true
                }
                KeyEvent.KEYCODE_BUTTON_A, KeyEvent.KEYCODE_DPAD_CENTER -> {
                    if (event.action == KeyEvent.ACTION_DOWN &&
                        event.repeatCount == 0 && !busy) {
                        val focus = currentFocus
                        if (focus is EditText) {
                            focus.requestFocus()
                            val imm = activity.getSystemService(Context.INPUT_METHOD_SERVICE)
                                as? InputMethodManager
                            imm?.showSoftInput(focus, InputMethodManager.SHOW_IMPLICIT)
                        } else if (focus is TextView && choices.containsKey(focus)) {
                            choices[focus]?.invoke(1)
                        } else {
                            (focus as? TextView)?.takeIf { it in buttons && it.isEnabled }
                                ?.performClick()
                        }
                    }
                    return true
                }
            }
            return super.dispatchKeyEvent(event)
        }
    }
    private val root = LinearLayout(activity).apply {
        orientation = LinearLayout.VERTICAL
        setPadding(SlotUi.menuPixel(activity, 32), SlotUi.menuPixel(activity, 20),
            SlotUi.menuPixel(activity, 32), dp(9))
        setBackgroundColor(SlotUi.menuBackground)
    }
    private val body = LinearLayout(activity).apply {
        orientation = LinearLayout.VERTICAL
        // The RA heading image contains a ~16 logical-pixel transparent
        // gutter; its first visible letter begins at x=32+16. Native Slot
        // menu entries begin 5 pixels after the title's visible ink.
        // Match the account dashboard: text and inputs begin at x=53.
        // Keep the smaller trailing inset to preserve input-field width.
        val inset = if (isRaLogin) {
            SlotUi.menuPixel(activity, 16) + SlotUi.menuEntryInset(activity)
        } else if (isRaSignOut) {
            SlotUi.dp(activity, 12)
        } else {
            SlotUi.menuEntryInset(activity)
        }
        setPadding(inset, 0, if (isRaLogin) SlotUi.menuEntryInset(activity) else inset, 0)
    }
    private val scroll = ScrollView(activity).apply {
        isVerticalScrollBarEnabled = false
        clipToPadding = false
        isFillViewport = false
    }
    private val actions = LinearLayout(activity).apply {
        orientation = LinearLayout.VERTICAL
    }
    private val errorText = SlotUi.label(activity, "", 12f, SlotUi.error, upper = false).apply {
        visibility = View.GONE
        setPadding(0, dp(8), 0, 0)
    }

    private fun focusTargets(): List<View> =
        (fields + buttons).filter { it.isEnabled && it.visibility == View.VISIBLE }

    private fun moveFocus(delta: Int) {
        val items = focusTargets()
        if (items.isEmpty()) return
        val active = items.indexOf(dialog.currentFocus)
        val next = if (active < 0) 0
            else (active + delta + items.size) % items.size
        items[next].requestFocus()
    }

    init {
        root.addView(SlotUi.menuHeader(activity, title),
            LinearLayout.LayoutParams(-1, SlotUi.menuPixel(activity, 50)))
        root.addView(View(activity), LinearLayout.LayoutParams(-1,
            SlotUi.menuPixel(activity, if (isRaLogin) 9 else 19)))
        if (message.isNotBlank()) {
            val text = SlotUi.label(activity, message, 15f, SlotUi.muted,
                upper = false).apply {
                setPadding(0, 0, 0, dp(14))
                maxLines = if (isRaLogin) 1 else 5
            }
            messageText = text
            body.addView(text)
        }
        body.addView(errorText)
        scroll.addView(body, android.widget.FrameLayout.LayoutParams(-1, -2))
        root.addView(scroll, LinearLayout.LayoutParams(-1, 0, 1f))
        root.addView(actions, LinearLayout.LayoutParams(-1, -2))
        legend = LinearLayout(activity).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER
        }
        legend.addView(SlotUi.nativeHint(activity, "A", "SELECT"))
        legend.addView(SlotUi.nativeHint(activity, "B", "BACK"),
            LinearLayout.LayoutParams(-2, -2).apply { leftMargin = dp(35) })
        root.addView(legend, LinearLayout.LayoutParams(-1,
            SlotUi.menuPixel(activity, 53)))
        dialog.requestWindowFeature(Window.FEATURE_NO_TITLE)
        dialog.setContentView(root)
        dialog.setCanceledOnTouchOutside(false)
        dialog.setOnDismissListener {
            secretFields.forEach { it.text.clear() }
            onDismiss()
        }
    }

    fun addInput(label: String, hint: String, password: Boolean = false): EditText {
        // Fixed logical pixels are essential on the 960x640 KONKR:
        // high OEM density formerly made two 48dp fields overflow the RA
        // viewport until the first D-pad focus movement scrolled it.
        val labelHeight = if (isRaLogin) SlotUi.menuPixel(activity, 22) else dp(27)
        val fieldHeight = if (isRaLogin) SlotUi.menuPixel(activity, 45) else dp(48)
        val fieldGap = if (isRaLogin) SlotUi.menuPixel(activity, 16) else dp(10)
        val caption = SlotUi.label(activity, label, 13f, SlotUi.muted)
        body.addView(caption,
            LinearLayout.LayoutParams(-1, labelHeight).apply {
                topMargin = if (isRaLogin) SlotUi.menuPixel(activity, 3) else dp(4)
                // Give USERNAME/PASSWORD visible breathing room above the box.
                // The compact 960x640 RA viewport still fits without scrolling.
                if (isRaLogin) bottomMargin = SlotUi.menuPixel(activity, 7)
            })
        val field = SlotUi.field(activity, hint, password)
        if (isRaLogin) {
            // The box is 45 canvas pixels: Android's 48dp minimum and 10dp
            // insets outgrew it on the KONKR and cut the text's top off.
            val side = SlotUi.menuPixel(activity, 13)
            field.setPadding(side, 0, side, 0)
            field.minHeight = 0
            field.minimumHeight = 0
            field.gravity = Gravity.CENTER_VERTICAL or Gravity.START
            field.includeFontPadding = false
        }
        field.contentDescription = label
        body.addView(field, LinearLayout.LayoutParams(-1, fieldHeight).apply {
            bottomMargin = fieldGap
        })
        fields.add(field)
        fieldLabels[field] = caption
        if (password) secretFields.add(field)
        body.removeView(errorText)
        body.addView(errorText)
        return field
    }

    fun addButton(label: String, onClick: () -> Unit): TextView {
        val button = SlotUi.label(activity, label, 16f, SlotUi.muted).apply {
            gravity = Gravity.CENTER_VERTICAL
            val inset = if (isRaLogin) {
                SlotUi.menuPixel(activity, 16) + SlotUi.menuEntryInset(activity)
            } else if (isRaSignOut) {
                SlotUi.dp(activity, 12)
            } else SlotUi.menuEntryInset(activity)
            setPadding(inset, 0, inset, 0)
            isFocusable = true
            isClickable = true
            contentDescription = label
            setOnFocusChangeListener { view, focused ->
                view.background = SlotUi.rowBackground(activity, focused)
                setTextColor(if (focused) SlotUi.ink else SlotUi.muted)
            }
            setOnClickListener { if (!busy) onClick() }
        }
        actions.addView(button, LinearLayout.LayoutParams(-1,
            SlotUi.menuPixel(activity, 52)))
        buttons.add(button)
        return button
    }

    /**
     * A row with a value Left / Right (or A) step through, as slot.'s own
     * settings: "REGION   < USA >". [onChange] hears the new index.
     */
    fun addChoice(label: String, values: List<String>, selected: Int, onChange: (Int) -> Unit): TextView {
        var index = selected.coerceIn(0, values.size - 1)
        val row = SlotUi.label(activity, "", 16f, SlotUi.muted).apply {
            gravity = Gravity.CENTER_VERTICAL
            setPadding(SlotUi.menuEntryInset(activity), 0, SlotUi.menuEntryInset(activity), 0)
            isFocusable = true
            contentDescription = label
            setOnFocusChangeListener { view, focused ->
                view.background = SlotUi.rowBackground(activity, focused)
                setTextColor(if (focused) SlotUi.ink else SlotUi.muted)
            }
        }
        fun show() { row.text = "$label   < ${values[index]} >" }
        show()
        choices[row] = { step ->
            index = (index + step + values.size) % values.size
            show()
            onChange(index)
        }
        body.addView(row, LinearLayout.LayoutParams(-1, SlotUi.menuPixel(activity, 46)))
        buttons.add(row)
        body.removeView(errorText)
        body.addView(errorText)
        return row
    }

    /** The legend's keys, in place of A SELECT and B BACK. */
    fun setLegend(vararg keys: Pair<String, String>) {
        legend.removeAllViews()
        keys.forEachIndexed { i, (key, action) ->
            legend.addView(SlotUi.nativeHint(activity, key, action),
                LinearLayout.LayoutParams(-2, -2).apply { if (i > 0) leftMargin = dp(35) })
        }
    }

    /** The sentence under the title, for a form with several modes. */
    fun setMessage(message: String) {
        messageText?.text = message
    }

    /** Shows or hides a field with its label (a form's other mode). */
    fun setFieldVisible(field: EditText, visible: Boolean) {
        val state = if (visible) View.VISIBLE else View.GONE
        field.visibility = state
        fieldLabels[field]?.visibility = state
        if (!visible && dialog.currentFocus == field) focusTargets().firstOrNull()?.requestFocus()
    }

    /** L1 and R1 switch a form's modes; the legend names them in place of
     *  A, an obvious key, so it still fits narrow screens. */
    fun setShoulders(left: String, right: String, onLeft: () -> Unit, onRight: () -> Unit) {
        onL1 = onLeft
        onR1 = onRight
        legend.removeAllViews()
        legend.addView(SlotUi.nativeHint(activity, "L1", left))
        legend.addView(SlotUi.nativeHint(activity, "R1", right),
            LinearLayout.LayoutParams(-2, -2).apply { leftMargin = dp(35) })
        legend.addView(SlotUi.nativeHint(activity, "B", "BACK"),
            LinearLayout.LayoutParams(-2, -2).apply { leftMargin = dp(35) })
    }

    /** The first field or button shown, focused. */
    fun focusFirst() {
        focusTargets().firstOrNull()?.let(::focusStart)
    }

    /** A page of buttons only (RomM's account page) opened with nothing
     *  selected when Android was in touch mode, where a plain button can't
     *  take focus (Siham 2026-11-10): the first one is made able to. */
    private fun focusStart(view: View) {
        if (view.requestFocus()) return
        view.isFocusableInTouchMode = true
        view.requestFocus()
    }

    fun setError(message: String) {
        errorText.text = message
        errorText.visibility = if (message.isBlank()) View.GONE else View.VISIBLE
    }

    fun setBusy(value: Boolean) {
        busy = value
        buttons.forEach { it.isEnabled = !value; it.alpha = if (value) .52f else 1f }
        dialog.setCancelable(!value)
    }

    fun show() {
        dialog.show()
        dialog.window?.apply {
            setWindowAnimations(R.style.SlotWindowAnimation)
            setBackgroundDrawable(ColorDrawable(SlotUi.menuBackground))
            addFlags(WindowManager.LayoutParams.FLAG_FULLSCREEN)
            setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_STATE_ALWAYS_HIDDEN or
                WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE)
            setLayout(ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT)
            decorView.systemUiVisibility = View.SYSTEM_UI_FLAG_FULLSCREEN or
                View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
        }
        // Safe first choice on confirmations; first editable field on forms.
        // The initial focused EditText must not rely on D-pad scrolling to
        // finish its layout. Start at the top after all inputs are measured.
        if (isRaLogin) scroll.post { scroll.scrollTo(0, 0) }
        (fields.firstOrNull { it.visibility == View.VISIBLE } ?: buttons.firstOrNull())?.let { first ->
            focusStart(first)
            // A window not laid out yet can refuse it: once more after.
            first.post { if (dialog.currentFocus == null) focusStart(first) }
        }
    }

    fun dismiss() = dialog.dismiss()
    fun isShowing(): Boolean = dialog.isShowing
    private fun dp(amount: Int) = SlotUi.dp(activity, amount)
}
