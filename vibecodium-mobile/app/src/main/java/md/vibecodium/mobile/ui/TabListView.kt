package md.vibecodium.mobile.ui

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.text.TextUtils
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import md.vibecodium.mobile.Tab

data class TabListColors(
    val activeBackground: Int,
    val text: Int,
    val muted: Int,
    val accent: Int,
)

data class TabListEntry(
    val index: Int,
    val tab: Tab,
    val active: Boolean,
    val bookmarked: Boolean,
)

/** Native dropdown list for open tabs. */
class TabListView(context: Context) : FrameLayout(context) {
    private val recycler = RecyclerView(context).apply {
        layoutManager = LinearLayoutManager(context)
        itemAnimator = null
        overScrollMode = OVER_SCROLL_ALWAYS
        setHasFixedSize(true)
        setBackgroundColor(Color.TRANSPARENT)
    }
    private val scrollbar = TabScrollbarView(context, recycler)
    private val adapter = Adapter()
    private var entries: List<TabListEntry> = emptyList()
    private var colors: TabListColors? = null
    private var onSelect: ((Int) -> Unit)? = null
    private var onClose: ((Int) -> Unit)? = null

    init {
        addView(recycler, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        addView(scrollbar, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        recycler.adapter = adapter
        recycler.addOnScrollListener(object : RecyclerView.OnScrollListener() {
            override fun onScrolled(recyclerView: RecyclerView, dx: Int, dy: Int) {
                scrollbar.invalidate()
            }
        })
    }

    fun setData(
        entries: List<TabListEntry>,
        colors: TabListColors,
        onSelect: (Int) -> Unit,
        onClose: (Int) -> Unit,
    ) {
        this.onSelect = onSelect
        this.onClose = onClose
        val colorsChanged = colors != this.colors
        this.colors = colors
        scrollbar.setBarColor((colors.muted and 0x00ffffff) or (0x59 shl 24))
        if (entries == this.entries && !colorsChanged) return
        val first = (recycler.layoutManager as LinearLayoutManager).findFirstVisibleItemPosition()
        val offset = first.takeIf { it >= 0 }?.let { position ->
            (recycler.layoutManager as LinearLayoutManager).findViewByPosition(position)?.top
        }
        this.entries = entries
        adapter.entries = entries
        adapter.notifyDataSetChanged()
        scrollbar.invalidate()
        if (first >= 0 && entries.isNotEmpty()) {
            (recycler.layoutManager as LinearLayoutManager)
                .scrollToPositionWithOffset(first.coerceAtMost(entries.lastIndex), offset ?: 0)
        }
    }

    private inner class Adapter : RecyclerView.Adapter<Holder>() {
        var entries: List<TabListEntry> = emptyList()

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): Holder = Holder(context)

        override fun getItemCount(): Int = entries.size

        override fun onBindViewHolder(holder: Holder, position: Int) {
            val entry = entries[position]
            val rowColors = colors ?: return
            holder.bind(entry, rowColors, onSelect, onClose)
        }
    }

    private class Holder(context: Context) : RecyclerView.ViewHolder(
        LinearLayout(context).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            layoutParams = RecyclerView.LayoutParams(
                RecyclerView.LayoutParams.MATCH_PARENT,
                dp(context, 54f),
            )
            setPadding(dp(context, 4f), 0, dp(context, 4f), 0)
        },
    ) {
        private val check = TextView(context).apply {
            gravity = Gravity.CENTER
            layoutParams = LinearLayout.LayoutParams(dp(context, 24f), ViewGroup.LayoutParams.MATCH_PARENT)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 16f)
            text = "✓"
            visibility = View.INVISIBLE
        }
        private val textColumn = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER_VERTICAL
            layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f)
        }
        private val title = TextView(context).apply {
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
            maxLines = 1
            ellipsize = TextUtils.TruncateAt.END
        }
        private val path = TextView(context).apply {
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 11f)
            maxLines = 1
            ellipsize = TextUtils.TruncateAt.END
            setPadding(0, dp(context, 1f), 0, 0)
        }
        private val star = TextView(context).apply {
            gravity = Gravity.CENTER
            layoutParams = LinearLayout.LayoutParams(dp(context, 20f), ViewGroup.LayoutParams.MATCH_PARENT)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 13f)
            text = "★"
            visibility = View.GONE
        }
        private val close = TextView(context).apply {
            gravity = Gravity.CENTER
            layoutParams = LinearLayout.LayoutParams(dp(context, 32f), ViewGroup.LayoutParams.MATCH_PARENT)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 20f)
            text = "×"
            contentDescription = "Close tab"
            isClickable = true
            isFocusable = true
        }

        init {
            val layout = itemView as LinearLayout
            textColumn.addView(title)
            textColumn.addView(path)
            layout.addView(check)
            layout.addView(textColumn)
            layout.addView(star)
            layout.addView(close)
        }

        fun bind(
            entry: TabListEntry,
            colors: TabListColors,
            onSelect: ((Int) -> Unit)?,
            onClose: ((Int) -> Unit)?,
        ) {
            val layout = itemView as LinearLayout
            layout.background = if (entry.active) {
                GradientDrawable().apply {
                    cornerRadius = dp(itemView.context, 10f).toFloat()
                    setColor(colors.activeBackground)
                }
            } else {
                null
            }
            layout.setOnClickListener { onSelect?.invoke(entry.index) }
            check.visibility = if (entry.active) View.VISIBLE else View.INVISIBLE
            check.setTextColor(colors.accent)
            title.text = entry.tab.title + if (entry.tab.dirty) " •" else ""
            title.setTextColor(colors.text)
            path.text = entry.tab.path
            path.setTextColor(colors.muted)
            star.visibility = if (entry.bookmarked) View.VISIBLE else View.GONE
            star.setTextColor(colors.accent)
            close.setTextColor(colors.muted)
            close.setOnClickListener { onClose?.invoke(entry.index) }
        }
    }

    private class TabScrollbarView(
        context: Context,
        private val recycler: RecyclerView,
    ) : View(context) {
        private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            color = 0x55ffffff
            style = Paint.Style.FILL
        }

        fun setBarColor(color: Int) {
            paint.color = color
            invalidate()
        }

        override fun onDraw(canvas: Canvas) {
            super.onDraw(canvas)
            val range = recycler.computeVerticalScrollRange()
            val extent = recycler.computeVerticalScrollExtent()
            if (range <= extent || range <= 0) return
            val density = resources.displayMetrics.density
            val barW = 3f * density
            val right = width - 2f * density
            val left = right - barW
            val inset = 2f * density
            val trackH = (height - inset * 2f).coerceAtLeast(1f)
            val fraction = (extent.toFloat() / range).coerceIn(0.08f, 1f)
            val thumbH = (trackH * fraction).coerceAtLeast(20f * density).coerceAtMost(trackH)
            val maxScroll = (range - extent).coerceAtLeast(1)
            val progress = (recycler.computeVerticalScrollOffset().toFloat() / maxScroll)
                .coerceIn(0f, 1f)
            val top = inset + (trackH - thumbH) * progress
            canvas.drawRoundRect(left, top, right, top + thumbH, barW / 2f, barW / 2f, paint)
        }
    }

    private companion object {
        fun dp(context: Context, value: Float): Int =
            (value * context.resources.displayMetrics.density).toInt()
    }
}
