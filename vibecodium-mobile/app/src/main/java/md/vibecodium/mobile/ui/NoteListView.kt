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

data class NoteListColors(
    val activeBackground: Int,
    val text: Int,
    val muted: Int,
    val accent: Int,
    val divider: Int,
)

data class NoteListEntry(
    val path: String,
    val title: String,
    val detail: String,
    val active: Boolean,
    val bookmarked: Boolean,
    val closable: Boolean = false,
)

/** Native note list shared by Bookmarks and Recent. */
class NoteListView(context: Context) : FrameLayout(context) {
    private val recycler = RecyclerView(context).apply {
        layoutManager = LinearLayoutManager(context)
        itemAnimator = null
        overScrollMode = OVER_SCROLL_ALWAYS
        setHasFixedSize(true)
        setBackgroundColor(Color.TRANSPARENT)
    }
    private val scrollbar = NoteScrollbarView(context, recycler)
    private val adapter = Adapter()
    private val dividerPaint = Paint().apply { style = Paint.Style.FILL }
    private var entries: List<NoteListEntry> = emptyList()
    private var colors: NoteListColors? = null
    private var onOpen: ((String) -> Unit)? = null
    private var onClose: ((String) -> Unit)? = null

    init {
        addView(recycler, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        addView(scrollbar, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        recycler.adapter = adapter
        recycler.addItemDecoration(object : RecyclerView.ItemDecoration() {
            override fun onDrawOver(canvas: Canvas, parent: RecyclerView, state: RecyclerView.State) {
                val thickness = parent.resources.displayMetrics.density
                for (index in 0 until parent.childCount) {
                    val child = parent.getChildAt(index)
                    canvas.drawRect(
                        child.left.toFloat(),
                        child.bottom - thickness,
                        child.right.toFloat(),
                        child.bottom.toFloat(),
                        dividerPaint,
                    )
                }
            }
        })
        recycler.addOnScrollListener(object : RecyclerView.OnScrollListener() {
            override fun onScrolled(recyclerView: RecyclerView, dx: Int, dy: Int) {
                scrollbar.invalidate()
            }
        })
    }

    fun setData(
        entries: List<NoteListEntry>,
        colors: NoteListColors,
        onOpen: (String) -> Unit,
        onClose: ((String) -> Unit)? = null,
    ) {
        this.onOpen = onOpen
        this.onClose = onClose
        val colorsChanged = colors != this.colors
        this.colors = colors
        dividerPaint.color = colors.divider
        recycler.invalidateItemDecorations()
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
        var entries: List<NoteListEntry> = emptyList()

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): Holder = Holder(context)

        override fun getItemCount(): Int = entries.size

        override fun onBindViewHolder(holder: Holder, position: Int) {
            val rowColors = colors ?: return
            holder.bind(entries[position], rowColors, onOpen, onClose)
        }
    }

    private class Holder(context: Context) : RecyclerView.ViewHolder(
        LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER_VERTICAL
            layoutParams = RecyclerView.LayoutParams(
                RecyclerView.LayoutParams.MATCH_PARENT,
                dp(context, 58f),
            )
            setPadding(dp(context, 14f), dp(context, 6f), dp(context, 14f), dp(context, 6f))
        },
    ) {
        private val titleRow = LinearLayout(context).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }
        private val title = TextView(context).apply {
            layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
            maxLines = 1
            ellipsize = TextUtils.TruncateAt.END
        }
        private val close = TextView(context).apply {
            layoutParams = LinearLayout.LayoutParams(dp(context, 28f), ViewGroup.LayoutParams.WRAP_CONTENT)
            gravity = Gravity.CENTER
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 17f)
            text = "×"
            visibility = View.GONE
        }
        private val detailRow = LinearLayout(context).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(0, dp(context, 1f), 0, 0)
        }
        private val detail = TextView(context).apply {
            layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 11f)
            maxLines = 1
            ellipsize = TextUtils.TruncateAt.END
        }
        private val star = TextView(context).apply {
            layoutParams = LinearLayout.LayoutParams(dp(context, 18f), ViewGroup.LayoutParams.WRAP_CONTENT)
            gravity = Gravity.CENTER
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 12f)
            text = "★"
            visibility = View.GONE
        }

        init {
            val layout = itemView as LinearLayout
            titleRow.addView(title)
            titleRow.addView(close)
            detailRow.addView(detail)
            detailRow.addView(star)
            layout.addView(titleRow)
            layout.addView(detailRow)
        }

        fun bind(
            entry: NoteListEntry,
            colors: NoteListColors,
            onOpen: ((String) -> Unit)?,
            onClose: ((String) -> Unit)?,
        ) {
            val layout = itemView as LinearLayout
            layout.background = if (entry.active) {
                GradientDrawable().apply {
                    cornerRadius = dp(itemView.context, 8f).toFloat()
                    setColor(colors.activeBackground)
                }
            } else {
                null
            }
            layout.setOnClickListener { onOpen?.invoke(entry.path) }
            title.text = entry.title
            title.setTextColor(colors.text)
            title.setTypeface(Typeface.DEFAULT, Typeface.NORMAL)
            detail.text = entry.detail
            detail.setTextColor(colors.muted)
            star.visibility = if (entry.bookmarked) View.VISIBLE else View.GONE
            star.setTextColor(colors.accent)
            close.visibility = if (entry.closable) View.VISIBLE else View.GONE
            close.setTextColor(colors.muted)
            close.setOnClickListener { onClose?.invoke(entry.path) }
        }
    }

    private class NoteScrollbarView(
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
