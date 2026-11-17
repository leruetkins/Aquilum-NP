package md.vibecodium.mobile.ui

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.text.Spannable
import android.text.SpannableString
import android.text.TextUtils
import android.text.style.BackgroundColorSpan
import android.text.style.ForegroundColorSpan
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import md.vibecodium.mobile.MatchRange
import md.vibecodium.mobile.R
import md.vibecodium.mobile.SearchFileResult
import md.vibecodium.mobile.SearchMatch

data class SearchResultColors(
    val headerBackground: Int,
    val text: Int,
    val muted: Int,
    val error: Int,
    val accent: Int,
    val outline: Int,
    val surface: Int,
    val divider: Int,
    val highlight: Int,
    val matchedText: Int,
)

/**
 * Native search result list.
 *
 * Compose still owns the search field and the count row; the potentially long
 * result list is a RecyclerView so scrolling does not run Compose layout for
 * every visible row. The scrollbar is an Android View with the same geometry
 * as the explorer's overlay and uses RecyclerView's exact scroll metrics.
 */
class SearchResultsView(context: Context) : android.widget.FrameLayout(context) {
    private data class Row(
        val file: SearchFileResult,
        val match: SearchMatch?,
    ) {
        val isHeader: Boolean get() = match == null
    }

    private val recycler = RecyclerView(context).apply {
        layoutManager = LinearLayoutManager(context)
        itemAnimator = null
        overScrollMode = OVER_SCROLL_ALWAYS
        setHasFixedSize(false)
        clipToPadding = false
        setBackgroundColor(Color.TRANSPARENT)
    }
    private val scrollbar = SearchScrollbarView(context, recycler)
    private val adapter = Adapter()
    private val dividerPaint = Paint().apply { strokeWidth = 1f }
    private var rows: List<Row> = emptyList()
    private var colors: SearchResultColors? = null
    private var onToggle: ((String) -> Unit)? = null
    private var onOpenFile: ((SearchFileResult) -> Unit)? = null
    private var onOpenMatch: ((SearchFileResult, SearchMatch) -> Unit)? = null

    init {
        addView(
            recycler,
            LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT),
        )
        addView(
            scrollbar,
            LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT),
        )
        recycler.adapter = adapter
        recycler.addItemDecoration(object : RecyclerView.ItemDecoration() {
            override fun onDrawOver(canvas: Canvas, parent: RecyclerView, state: RecyclerView.State) {
                val count = parent.childCount
                val thickness = parent.resources.displayMetrics.density
                for (index in 0 until count) {
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
        results: List<SearchFileResult>,
        collapsed: Set<String>,
        colors: SearchResultColors,
        onToggle: (String) -> Unit,
        onOpenFile: (SearchFileResult) -> Unit,
        onOpenMatch: (SearchFileResult, SearchMatch) -> Unit,
    ) {
        val next = buildList {
            results.forEach { result ->
                add(Row(result, null))
                if (result.path !in collapsed) {
                    result.matches.forEach { match -> add(Row(result, match)) }
                }
            }
        }
        this.onToggle = onToggle
        this.onOpenFile = onOpenFile
        this.onOpenMatch = onOpenMatch
        this.currentCollapsed = collapsed
        dividerPaint.color = colors.divider
        recycler.invalidateItemDecorations()
        scrollbar.setBarColor((colors.muted and 0x00ffffff) or (0x59 shl 24))
        if (next == rows && colors == this.colors) return
        val firstPosition = (recycler.layoutManager as LinearLayoutManager)
            .findFirstVisibleItemPosition()
        val firstOffset = firstPosition.takeIf { it >= 0 }
            ?.let { position ->
                (recycler.layoutManager as LinearLayoutManager)
                    .findViewByPosition(position)?.top
            }
        rows = next
        this.colors = colors
        adapter.rows = next
        adapter.notifyDataSetChanged()
        scrollbar.invalidate()
        if (firstPosition >= 0 && rows.isNotEmpty()) {
            (recycler.layoutManager as LinearLayoutManager)
                .scrollToPositionWithOffset(firstPosition.coerceAtMost(rows.lastIndex), firstOffset ?: 0)
        } else if (rows.isNotEmpty()) {
            recycler.scrollToPosition(0)
        }
    }

    private inner class Adapter : RecyclerView.Adapter<RecyclerView.ViewHolder>() {
        var rows: List<Row> = emptyList()

        override fun getItemCount(): Int = rows.size

        override fun getItemViewType(position: Int): Int =
            if (rows[position].isHeader) TYPE_HEADER else TYPE_MATCH

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder =
            if (viewType == TYPE_HEADER) HeaderHolder(context) else MatchHolder(context)

        override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) {
            val row = rows[position]
            val rowColors = colors ?: return
            if (holder is HeaderHolder) {
                holder.bind(
                    row.file,
                    rowColors,
                    collapsed = row.file.path in currentCollapsed,
                    onToggle = onToggle,
                    onOpenFile = onOpenFile,
                )
            } else if (holder is MatchHolder) {
                holder.bind(
                    row.file,
                    row.match ?: return,
                    rowColors,
                    onOpenMatch = onOpenMatch,
                )
            }
        }
    }

    private var currentCollapsed: Set<String> = emptySet()

    private class HeaderHolder(context: Context) : RecyclerView.ViewHolder(
        LinearLayout(context).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            layoutParams = RecyclerView.LayoutParams(
                RecyclerView.LayoutParams.MATCH_PARENT,
                dp(context, 40f),
            )
            setPadding(dp(context, 4f), 0, dp(context, 12f), 0)
        },
    ) {
        private val chevron = ImageView(context).apply {
            layoutParams = LinearLayout.LayoutParams(dp(context, 32f), dp(context, 32f))
            scaleType = ImageView.ScaleType.CENTER_INSIDE
            setPadding(dp(context, 7f), dp(context, 7f), dp(context, 7f), dp(context, 7f))
        }
        private val name = TextView(context).apply {
            layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
            setTypeface(Typeface.DEFAULT, Typeface.BOLD)
            maxLines = 1
            ellipsize = TextUtils.TruncateAt.END
            setPadding(dp(context, 4f), 0, dp(context, 8f), 0)
        }
        private val badge = TextView(context).apply {
            gravity = Gravity.CENTER
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 10f)
            setPadding(dp(context, 6f), dp(context, 1f), dp(context, 6f), dp(context, 1f))
        }

        init {
            val layout = itemView as LinearLayout
            layout.addView(chevron)
            layout.addView(name)
            layout.addView(badge)
        }

        fun bind(
            file: SearchFileResult,
            colors: SearchResultColors,
            collapsed: Boolean,
            onToggle: ((String) -> Unit)?,
            onOpenFile: ((SearchFileResult) -> Unit)?,
        ) {
            val layout = itemView as LinearLayout
            layout.setBackgroundColor(colors.headerBackground)
            chevron.visibility = if (file.matches.isEmpty()) View.INVISIBLE else View.VISIBLE
            chevron.setImageResource(
                if (collapsed) R.drawable.ic_chevron_right else R.drawable.ic_chevron_down,
            )
            chevron.setColorFilter(colors.muted)
            chevron.setOnClickListener { onToggle?.invoke(file.path) }
            name.text = file.name.removeSuffix(".md")
            name.setTextColor(colors.error)
            name.setOnClickListener { onOpenFile?.invoke(file) }
            if (file.matchCount > 0) {
                badge.visibility = View.VISIBLE
                badge.text = file.matchCount.toString()
                badge.setTextColor(colors.muted)
                badge.background = GradientDrawable().apply {
                    cornerRadius = dp(itemView.context, 6f).toFloat()
                    setColor(colors.surface)
                    setStroke(dp(itemView.context, 1f), colors.outline)
                }
            } else {
                badge.visibility = View.GONE
            }
        }
    }

    private class MatchHolder(context: Context) : RecyclerView.ViewHolder(
        LinearLayout(context).apply {
            orientation = LinearLayout.HORIZONTAL
            layoutParams = RecyclerView.LayoutParams(
                RecyclerView.LayoutParams.MATCH_PARENT,
                dp(context, 56f),
            )
            setPadding(dp(context, 4f), 0, dp(context, 12f), 0)
        },
    ) {
        private val line = TextView(context).apply {
            gravity = Gravity.TOP or Gravity.CENTER_HORIZONTAL
            layoutParams = LinearLayout.LayoutParams(dp(context, 40f), ViewGroup.LayoutParams.MATCH_PARENT)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 10f)
            setTypeface(Typeface.create("sans-serif", Typeface.NORMAL))
            setPadding(0, dp(context, 7f), 0, 0)
            maxLines = 1
        }
        private val snippet = TextView(context).apply {
            layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.MATCH_PARENT, 1f)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 12f)
            setLineSpacing(0f, 1f)
            includeFontPadding = false
            maxLines = 2
            ellipsize = TextUtils.TruncateAt.END
            gravity = Gravity.TOP
            setPadding(dp(context, 6f), dp(context, 6f), 0, dp(context, 4f))
        }

        init {
            val layout = itemView as LinearLayout
            layout.addView(line)
            layout.addView(snippet)
        }

        fun bind(
            file: SearchFileResult,
            match: SearchMatch,
            colors: SearchResultColors,
            onOpenMatch: ((SearchFileResult, SearchMatch) -> Unit)?,
        ) {
            val layout = itemView as LinearLayout
            layout.setBackgroundColor(Color.TRANSPARENT)
            layout.setOnClickListener { onOpenMatch?.invoke(file, match) }
            line.text = "${match.line}:"
            line.setTextColor(colors.muted)
            snippet.setTextColor(colors.text)
            snippet.text = highlighted(match.snippet, match.ranges, colors)
        }
    }

    private class SearchScrollbarView(
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
            val topInset = 6f * density
            val bottomInset = 6f * density
            val trackH = (height - topInset - bottomInset).coerceAtLeast(1f)
            val fraction = (extent.toFloat() / range).coerceIn(0.08f, 1f)
            val thumbH = (trackH * fraction).coerceAtLeast(24f * density).coerceAtMost(trackH)
            val maxScroll = (range - extent).coerceAtLeast(1)
            val progress = (recycler.computeVerticalScrollOffset().toFloat() / maxScroll)
                .coerceIn(0f, 1f)
            val top = topInset + (trackH - thumbH) * progress
            canvas.drawRoundRect(
                left,
                top,
                right,
                top + thumbH,
                barW / 2f,
                barW / 2f,
                paint,
            )
        }
    }

    private companion object {
        const val TYPE_HEADER = 0
        const val TYPE_MATCH = 1

        fun dp(context: Context, value: Float): Int =
            (value * context.resources.displayMetrics.density).toInt()

        fun highlighted(
            text: String,
            ranges: List<MatchRange>,
            colors: SearchResultColors,
        ): CharSequence {
            if (ranges.isEmpty()) return text
            val spanned = SpannableString(text)
            ranges.forEach { range ->
                val start = range.start.coerceIn(0, text.length)
                val end = range.end.coerceIn(start, text.length)
                if (end > start) {
                    spanned.setSpan(
                        BackgroundColorSpan(colors.highlight),
                        start,
                        end,
                        Spannable.SPAN_EXCLUSIVE_EXCLUSIVE,
                    )
                    spanned.setSpan(
                        ForegroundColorSpan(colors.matchedText),
                        start,
                        end,
                        Spannable.SPAN_EXCLUSIVE_EXCLUSIVE,
                    )
                }
            }
            return spanned
        }
    }
}
