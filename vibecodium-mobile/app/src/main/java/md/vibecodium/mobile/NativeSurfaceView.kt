package md.vibecodium.mobile

import android.app.Activity
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.ContextWrapper
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.PointF
import android.graphics.Rect
import android.graphics.SurfaceTexture
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.HandlerThread
import android.os.Looper
import android.os.Process
import android.os.SystemClock
import android.text.InputType
import android.text.TextUtils
import android.view.ActionMode
import android.view.Choreographer
import android.view.KeyEvent
import android.view.Menu
import android.view.MenuItem
import android.view.MotionEvent
import android.view.Surface
import android.view.TextureView
import android.view.VelocityTracker
import android.view.View
import android.view.ViewConfiguration
import android.widget.FrameLayout
import android.widget.Magnifier
import android.widget.OverScroller
import kotlin.math.roundToInt

import android.view.accessibility.AccessibilityNodeInfo
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.ExtractedText
import android.view.inputmethod.ExtractedTextRequest
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputMethodManager
import android.view.inputmethod.SurroundingText
import androidx.annotation.RequiresApi
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.accessibility.AccessibilityNodeInfoCompat
import androidx.customview.widget.ExploreByTouchHelper
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.Executor
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger

/**
 * Shared TextureView plumbing: one native ANativeWindow per view and a single
 * render thread so JNI never blocks the UI thread with buffer locks.
 */
abstract class NativeSurfaceView(context: Context) : FrameLayout(context), TextureView.SurfaceTextureListener {
    val surfaceId: Int = NEXT_ID.incrementAndGet()
    private var currentSurface: Surface? = null

    protected val renderer: java.util.concurrent.Executor get() = RENDERER

    /** UI-thread handler used by callbacks that run on the render thread. */
    protected val mainHandler: Handler = Handler(Looper.getMainLooper())

    @Volatile
    private var attached = false

    @Volatile
    private var pendingState: (() -> Unit)? = null

    protected val textureView: TextureView = TextureView(context)

    protected open fun computeBandMargin(viewportHeight: Int): Int = 0

    protected open fun onBandLayoutChanged() = Unit

    init {
        textureView.surfaceTextureListener = this
        addView(textureView, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        clipChildren = true
        clipToPadding = true
        isFocusable = true
        isFocusableInTouchMode = true
    }

    override fun onLayout(changed: Boolean, left: Int, top: Int, right: Int, bottom: Int) {
        val w = right - left
        val h = bottom - top
        val margin = computeBandMargin(h)
        if (textureView.left != 0 || textureView.top != -margin || textureView.right != w || textureView.bottom != h + margin) {
            textureView.layout(0, -margin, w, h + margin)
        }
        for (i in 0 until childCount) {
            val child = getChildAt(i)
            if (child != textureView) {
                if (child.left != 0 || child.top != 0 || child.right != w || child.bottom != h) {
                    child.layout(0, 0, w, h)
                }
            }
        }
        if (changed) {
            onBandLayoutChanged()
        }
    }

    override fun onSurfaceTextureAvailable(surfaceTexture: SurfaceTexture, width: Int, height: Int) {
        val w = width.coerceAtLeast(1)
        val h = height.coerceAtLeast(1)
        surfaceTexture.setDefaultBufferSize(w, h)
        val surface = Surface(surfaceTexture)
        currentSurface = surface
        val viewportH = this.height.coerceAtLeast(1)
        renderer.execute {
            val density = resources.displayMetrics.density
            CoreBridge.nativeSurfaceAttach(surfaceId, surface, w, h, density)
            CoreBridge.nativeSurfaceSetViewportHeight(surfaceId, viewportH)
            attached = true
            // Publish text, insets and the stored viewport before the first
            // frame, so a recreated Surface never flashes the top of the note.
            CoreBridge.nativeSurfaceBeginUpdate(surfaceId)
            onSurfaceReady()
            pendingState?.let { update ->
                pendingState = null
                update()
            }
            CoreBridge.nativeSurfaceEndUpdate(surfaceId)
            val rendered = CoreBridge.nativeSurfaceRender(surfaceId)
            onRenderFinished(rendered)
            mainHandler.post {
                textureView.postInvalidateOnAnimation()
            }
        }
    }

    override fun onSurfaceTextureSizeChanged(surfaceTexture: SurfaceTexture, width: Int, height: Int) {
        val w = width.coerceAtLeast(1)
        val h = height.coerceAtLeast(1)
        surfaceTexture.setDefaultBufferSize(w, h)
        val viewportH = this.height.coerceAtLeast(1)
        renderer.execute {
            if (!attached) return@execute
            CoreBridge.nativeSurfaceBeginUpdate(surfaceId)
            CoreBridge.nativeSurfaceResize(surfaceId, w, h)
            CoreBridge.nativeSurfaceSetViewportHeight(surfaceId, viewportH)
            pendingState?.let { update ->
                pendingState = null
                update()
            }
            CoreBridge.nativeSurfaceEndUpdate(surfaceId)
            val rendered = CoreBridge.nativeSurfaceRender(surfaceId)
            onRenderFinished(rendered)
            mainHandler.post {
                textureView.postInvalidateOnAnimation()
            }
        }
    }

    override fun onSurfaceTextureDestroyed(surfaceTexture: SurfaceTexture): Boolean {
        val s = currentSurface
        currentSurface = null
        attached = false
        vsyncScheduled.set(false)
        renderDirty.set(false)
        mainHandler.post {
            Choreographer.getInstance().removeFrameCallback(vsyncRender)
        }
        renderer.execute {
            CoreBridge.nativeSurfaceDetach(surfaceId)
            s?.release()
        }
        return true
    }

    override fun onSurfaceTextureUpdated(surfaceTexture: SurfaceTexture) = Unit

    protected open fun onRenderFinished(rendered: Float) = Unit

    protected fun postState(block: () -> Unit) {
        renderer.execute {
            if (attached) {
                pendingState = null
                block()
                val rendered = CoreBridge.nativeSurfaceRender(surfaceId)
                onRenderFinished(rendered)
                mainHandler.post {
                    textureView.postInvalidateOnAnimation()
                }
            } else {
                pendingState = block
            }
        }
    }

    /** Re-publishes Kotlin-owned state after a Surface is recreated. */
    protected open fun onSurfaceReady() = Unit

    protected fun isSurfaceAttached(): Boolean = attached

    private val vsyncScheduled = AtomicBoolean(false)
    private val renderDirty = AtomicBoolean(false)
    private val renderRunning = AtomicBoolean(false)

    private val vsyncRender = Choreographer.FrameCallback {
        vsyncScheduled.set(false)
        if (!renderDirty.get()) return@FrameCallback
        if (renderRunning.compareAndSet(false, true)) {
            renderDirty.set(false)
            renderer.execute {
                try {
                    if (attached) {
                        val t0 = System.nanoTime()
                        CoreBridge.nativeSurfaceRender(surfaceId)
                        val dtMs = (System.nanoTime() - t0) / 1_000_000f
                        android.util.Log.d("VibecodiumPaint", "nativeSurfaceRender took ${dtMs}ms")
                    }
                } finally {
                    renderRunning.set(false)
                    if (renderDirty.get() && attached) {
                        scheduleVsync()
                    }
                }
            }
        } else {
            scheduleVsync()
        }
    }

    private fun scheduleVsync() {
        if (!vsyncScheduled.compareAndSet(false, true)) return
        if (Looper.myLooper() == Looper.getMainLooper()) {
            Choreographer.getInstance().postFrameCallback(vsyncRender)
        } else {
            mainHandler.post {
                if (vsyncScheduled.get()) {
                    Choreographer.getInstance().postFrameCallback(vsyncRender)
                }
            }
        }
    }

    /**
     * Renders on the dedicated render thread synchronized with hardware VSYNC.
     * High-frequency touch moves update atomic scroll state; UI-thread Choreographer
     * guarantees at most one frame is rendered per display refresh interval, eliminating
     * buffer queue congestion, thread blocking in ANativeWindow_lock, and stutter in TextureView.
     */
    protected open fun requestRender() {
        renderDirty.set(true)
        scheduleVsync()
    }

    /** Invalidates and repaints on the render thread. */
    protected open fun requestFullRender() {
        renderer.execute {
            if (!attached) return@execute
            CoreBridge.nativeSurfaceInvalidate(surfaceId)
            requestRender()
        }
    }

    /** Schedules an animation callback on the UI thread's vsync clock. */
    protected fun postFrame(callback: Choreographer.FrameCallback) {
        if (Looper.myLooper() == Looper.getMainLooper()) {
            Choreographer.getInstance().postFrameCallback(callback)
        } else {
            mainHandler.post { Choreographer.getInstance().postFrameCallback(callback) }
        }
    }

    protected fun cancelFrame(callback: Choreographer.FrameCallback) {
        if (Looper.myLooper() == Looper.getMainLooper()) {
            Choreographer.getInstance().removeFrameCallback(callback)
        } else {
            mainHandler.post { Choreographer.getInstance().removeFrameCallback(callback) }
        }
    }

    /** Queues the latest scroll for the next paint without blocking on one. */
    protected fun submitScroll(y: Float) {
        CoreBridge.nativeSurfaceScroll(surfaceId, y)
    }

    /** Runs a background task in the same serial order as native rendering. */
    protected fun postRenderTask(task: () -> Unit) {
        renderer.execute(task)
    }

    override fun onDetachedFromWindow() {
        vsyncScheduled.set(false)
        renderDirty.set(false)
        if (Looper.myLooper() == Looper.getMainLooper()) {
            Choreographer.getInstance().removeFrameCallback(vsyncRender)
        } else {
            mainHandler.post { Choreographer.getInstance().removeFrameCallback(vsyncRender) }
        }
        super.onDetachedFromWindow()
        // surfaceDestroyed is responsible for the native window; the executor
        // is left alive until the view is fully detached so that queued state
        // updates still run in order.
    }

    private companion object {
        val NEXT_ID = AtomicInteger(1)

        /**
         * One dedicated render thread shared by every surface. It keeps a
         * Looper so `Choreographer` can drive frames from here instead of the
         * UI thread.
         */
        val RENDER_THREAD: HandlerThread =
            HandlerThread("native-surface", Process.THREAD_PRIORITY_DISPLAY).apply { start() }
        val RENDER_HANDLER: Handler = Handler(RENDER_THREAD.looper)
        val RENDERER: Executor = Executor { command -> RENDER_HANDLER.post(command) }

        @Volatile
        var RENDER_CHOREOGRAPHER: Choreographer? = null

        /** Must only be called on the render thread; it owns that vsync clock. */
        fun renderChoreographer(): Choreographer {
            RENDER_CHOREOGRAPHER?.let { return it }
            return Choreographer.getInstance().also { RENDER_CHOREOGRAPHER = it }
        }
    }
}



private const val COMMIT_RENAME_ACTION = "{\"kind\":\"commit_rename\"}"

/** Native explorer. Kotlin computes the visible rows; Rust paints and hit-tests. */
class NativeTreeView(
    context: Context,
    private val onAction: (String) -> Unit,
    private val onTouchStart: () -> Unit = {},
) : NativeSurfaceView(context) {    private data class TreeRowInfo(
        val name: String,
        val path: String,
        val folder: Boolean,
        val note: Boolean,
    )

    private var rows: List<TreeRowInfo> = emptyList()
    private var lastJson: String? = null

    private val helper: ExploreByTouchHelper by lazy {
        object : ExploreByTouchHelper(this) {
            private fun rowHeight() = (36f * resources.displayMetrics.density).roundToInt().toFloat()

            override fun getVirtualViewAt(x: Float, y: Float): Int {
                val scroll = CoreBridge.nativeTreeScroll(surfaceId)
                val index = ((y + scroll) / rowHeight()).toInt()
                return if (index in rows.indices) index else View.NO_ID
            }

            override fun getVisibleVirtualViews(virtualViewIds: MutableList<Int>) {
                if (rows.isEmpty()) return
                val rowHeight = rowHeight()
                val scroll = CoreBridge.nativeTreeScroll(surfaceId)
                val first = (scroll / rowHeight).toInt().coerceAtLeast(0)
                val visible = (height / rowHeight).toInt() + 2
                for (index in first until minOf(rows.size, first + visible)) {
                    virtualViewIds.add(index)
                }
            }

            override fun onPopulateNodeForVirtualView(
                virtualViewId: Int,
                node: AccessibilityNodeInfoCompat,
            ) {
                val row = rows.getOrNull(virtualViewId)
                if (row == null) {
                    node.contentDescription = ""
                    node.setBoundsInParent(Rect(0, 0, 1, 1))
                    return
                }
                node.text = row.name
                node.contentDescription = when {
                    row.folder -> "Folder ${row.name}"
                    row.note -> "Note ${row.name}"
                    else -> "File ${row.name}"
                }
                node.isFocusable = true
                node.isClickable = true
                node.isEnabled = true
                node.addAction(AccessibilityNodeInfoCompat.ACTION_CLICK)
                val rowHeight = rowHeight()
                val scroll = CoreBridge.nativeTreeScroll(surfaceId)
                val top = (virtualViewId * rowHeight - scroll).toInt()
                node.setBoundsInParent(Rect(0, top, width, top + rowHeight.toInt()))
            }

            override fun onPerformActionForVirtualView(
                virtualViewId: Int,
                action: Int,
                arguments: Bundle?,
            ): Boolean {
                if (action != AccessibilityNodeInfoCompat.ACTION_CLICK) return false
                val row = rows.getOrNull(virtualViewId) ?: return false
                val kind = when {
                    row.folder -> "toggle"
                    row.note -> "open_note"
                    else -> "open_file"
                }
                val actionJson = JSONObject()
                    .put("kind", kind)
                    .put("path", row.path)
                    .toString()
                post { onAction(actionJson) }
                return true
            }
        }
    }

    private val treeScroller = OverScroller(context)
    @Volatile private var treeMaxScroll = 0f
    private var velocityTracker: VelocityTracker? = null
    @Volatile private var treeFlinging = false
    private val treeFling = object : Choreographer.FrameCallback {
        override fun doFrame(frameTimeNanos: Long) {
            if (!treeFlinging) return
            val more = treeScroller.computeScrollOffset()
            val currentY = treeScroller.currY.toFloat().coerceIn(0f, treeMaxScroll)
            submitScroll(currentY)
            requestRender()
            if (more && !treeScroller.isFinished && currentY > 0f && currentY < treeMaxScroll) {
                postFrame(this)
            } else {
                treeFlinging = false
                mainHandler.postDelayed({ helper.invalidateRoot() }, 100L)
            }
        }
    }

    private var drawerVisible = false
    private var drawerTouchFloor = 0L
    @Volatile
    private var interactive = true
    /** While true, a completed tap commits the inline editor instead of acting on the row. */
    @Volatile
    private var renaming = false

    fun setRenaming(value: Boolean) {
        renaming = value
    }

    private val touchSlopPx = ViewConfiguration.get(context).scaledTouchSlop.toFloat()
    private var longPressRow: TreeRowInfo? = null
    private var longPressAtX = 0f
    private var longPressAtY = 0f
    private var longPressFired = false
    private val longPressAction = Runnable {
        val row = longPressRow ?: return@Runnable
        longPressRow = null
        longPressFired = true
        performHapticFeedback(android.view.HapticFeedbackConstants.LONG_PRESS)
        post {
            onAction(JSONObject().put("kind", "rename").put("path", row.path).toString())
        }
    }

    init {
        textureView.isOpaque = false
        ViewCompat.setAccessibilityDelegate(this, helper)
    }

    /**
     * The drawer stays in the hierarchy while closed (to keep this surface
     * warm). On the first open its surface can still be positioned off-screen,
     * so force a layout and a full redraw after it moves in.
     */
    /** The native tree must not consume gestures while a Compose overlay
     *  (Search/Bookmarks/Recent) covers it. */
    fun setInteractive(value: Boolean) {
        if (value == interactive) return
        interactive = value
        if (!value) {
            stopTreeFling()
            recycleTreeVelocity()
        }
    }

    fun setDrawerVisible(visible: Boolean) {
        if (visible == drawerVisible) return
        drawerVisible = visible
        if (!visible) return
        // The tap that opened the drawer can leave an ACTION_UP or a second
        // contact behind. Those events must not select a tree row and close
        // the drawer again.
        drawerTouchFloor = android.os.SystemClock.uptimeMillis() + 180L
        post {
            requestLayout()
            invalidate()
            requestFullRender()
            postDelayed({ requestFullRender() }, 80L)
        }
    }

    override fun onSurfaceReady() {
        lastJson?.let { CoreBridge.nativeSurfaceSetTree(surfaceId, it) }
    }

    override fun dispatchHoverEvent(event: MotionEvent): Boolean {
        return helper.dispatchHoverEvent(event) || super.dispatchHoverEvent(event)
    }

    override fun onTouchEvent(event: android.view.MotionEvent): Boolean {
        if (!interactive) return false
        if (event.eventTime < drawerTouchFloor) {
            if (event.actionMasked == android.view.MotionEvent.ACTION_DOWN) {
                stopTreeFling()
                recycleTreeVelocity()
            }
            return true
        }
        val action = when (event.actionMasked) {
            android.view.MotionEvent.ACTION_DOWN -> 0
            android.view.MotionEvent.ACTION_MOVE -> 1
            android.view.MotionEvent.ACTION_UP -> 2
            android.view.MotionEvent.ACTION_CANCEL -> -1
            else -> return true
        }
        if (action == 0) {
            onTouchStart()
            stopTreeFling()
            velocityTracker?.recycle()
            velocityTracker = VelocityTracker.obtain().also { it.addMovement(event) }
            beginLongPress(event.x, event.y)
        } else if (action == 1 || action == 2) {
            velocityTracker?.addMovement(event)
        }
        if (action == 1 && longPressRow != null && movedPastLongPressSlop(event)) {
            cancelRenameLongPress()
        }
        if (action == 2 || action == -1) {
            val rename = longPressFired
            cancelRenameLongPress()
            longPressFired = false
            if (rename && action == 2) {
                recycleTreeVelocity()
                requestRender()
                return true
            }
        }
        val result = if (action in 0..2) {
            CoreBridge.nativeSurfaceTouch(surfaceId, event.x, event.y, action)
        } else {
            ""
        }
        if (action == 2) {
            if (result.isNotEmpty()) {
                post { onAction(if (renaming) COMMIT_RENAME_ACTION else result) }
            } else {
                val tracker = velocityTracker
                tracker?.computeCurrentVelocity(1000)
                val vy = tracker?.yVelocity ?: 0f
                recycleTreeVelocity()
                val minimum = ViewConfiguration.get(context).scaledMinimumFlingVelocity.toFloat()
                if (kotlin.math.abs(vy) > minimum) startTreeFling(vy)
            }
        } else if (action == -1) {
            recycleTreeVelocity()
        }
        if (action == 2) helper.invalidateRoot()
        requestRender()
        return true
    }

    private fun recycleTreeVelocity() {
        velocityTracker?.recycle()
        velocityTracker = null
    }

    private fun treeRowHeightPx(): Float =
        (36f * resources.displayMetrics.density).roundToInt().toFloat()

    private fun beginLongPress(x: Float, y: Float) {
        cancelRenameLongPress()
        longPressFired = false
        if (!interactive) return
        val scroll = CoreBridge.nativeTreeScroll(surfaceId)
        val index = ((y + scroll) / treeRowHeightPx()).toInt()
        val row = rows.getOrNull(index) ?: return
        longPressRow = row
        longPressAtX = x
        longPressAtY = y
        mainHandler.postDelayed(longPressAction, ViewConfiguration.getLongPressTimeout().toLong())
    }

    private fun cancelRenameLongPress() {
        mainHandler.removeCallbacks(longPressAction)
        longPressRow = null
    }

    private fun movedPastLongPressSlop(event: android.view.MotionEvent): Boolean {
        val dx = event.x - longPressAtX
        val dy = event.y - longPressAtY
        return dx * dx + dy * dy > touchSlopPx * touchSlopPx
    }

    /** Scrolls the tree so row [index] is fully visible; used by inline rename. */
    fun revealRow(index: Int) {
        if (index !in rows.indices) return
        val rowHeight = treeRowHeightPx()
        val viewport = runCatching { JSONObject(CoreBridge.nativeTreeViewport(surfaceId)) }.getOrNull()
            ?: return
        val scroll = viewport.optDouble("scroll", 0.0).toFloat()
        val maxScroll = viewport.optDouble("max_y", 0.0).toFloat()
        val heightF = height.toFloat()
        if (heightF <= 0f) return
        val top = index * rowHeight
        val bottom = top + rowHeight
        val target = when {
            top < scroll -> top
            bottom > scroll + heightF -> bottom - heightF
            else -> return
        }.coerceIn(0f, maxScroll)
        if (target != scroll) {
            CoreBridge.nativeTreeSetScroll(surfaceId, target)
            requestRender()
        }
    }

    private fun stopTreeFling() {
        treeFlinging = false
        treeScroller.abortAnimation()
        cancelFrame(treeFling)
    }

    private fun startTreeFling(vy: Float) {
        val viewport = runCatching { JSONObject(CoreBridge.nativeTreeViewport(surfaceId)) }.getOrNull()
            ?: return
        val scroll = viewport.optDouble("scroll", 0.0).toFloat()
        val maxY = viewport.optDouble("max_y", 0.0).toFloat()
        if (maxY <= 0f) return
        treeMaxScroll = maxY
        treeScroller.abortAnimation()
        treeScroller.fling(
            0,
            scroll.roundToInt(),
            0,
            (-vy).roundToInt(),
            0, 0,
            0, maxY.roundToInt(),
        )
        treeFlinging = true
        postFrame(treeFling)
    }

    override fun onDetachedFromWindow() {
        stopTreeFling()
        recycleTreeVelocity()
        super.onDetachedFromWindow()
    }

    fun setRows(json: String) {
        // Compose calls AndroidView.update on every recomposition of the
        // drawer, including unrelated state (settings sections, sync
        // progress). Re-parsing and re-sending a large tree costs more than
        // the frame, so keep the last payload.
        if (json == lastJson) return
        lastJson = json
        val parsed = runCatching {
            val array = JSONObject(json).optJSONArray("rows") ?: JSONArray()
            List(array.length()) { index ->
                val row = array.getJSONObject(index)
                TreeRowInfo(
                    name = row.optString("name"),
                    path = row.optString("path"),
                    folder = row.optBoolean("folder"),
                    note = row.optBoolean("note"),
                )
            }
        }.getOrDefault(emptyList())
        rows = parsed
        contentDescription = "File tree, ${parsed.size} rows"
        helper.invalidateRoot()
        postState { CoreBridge.nativeSurfaceSetTree(surfaceId, json) }
    }
}

/**
 * Native source editor. Text layout, cursor and scroll are Rust-owned; the
 * Android IME still comes from the normal InputConnection path.
 */
class NativeEditorView(
    context: Context,
    private val onTextChanged: (String, Int) -> Unit,
    private val idleBottomInsetPx: Float = 64f * context.resources.displayMetrics.density,
) : NativeSurfaceView(context) {
    override fun computeBandMargin(viewportHeight: Int): Int {
        val h = if (viewportHeight > 0) viewportHeight else safeViewportHeight()
        return (h * 0.6f).roundToInt()
    }

    private fun safeViewportHeight(): Int {
        if (height > 0) return height
        if (measuredHeight > 0) return measuredHeight
        return resources.displayMetrics.heightPixels
    }

    private fun ensureScrollMaxY(): Float {
        val info = runCatching { JSONObject(CoreBridge.nativeEditorViewport(surfaceId)) }.getOrNull()
        if (info != null && info.has("max_y")) {
            scrollMaxY = info.optDouble("max_y", 0.0).toFloat().coerceAtLeast(0f)
        }
        return maxOf(scrollMaxY, knownScroll)
    }

    private var bandOrigin = 0f
    private val pendingRenderedOrigin = java.util.concurrent.atomic.AtomicReference<Float?>(null)
    private val isRenderingBand = AtomicBoolean(false)
    @Volatile
    private var forcePendingBandRender = false
    private var pendingBandOrigin: Float? = null

    private inner class ScrollbarOverlayView(context: Context) : View(context) {
        private val barPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            color = 0x55ffffff
            style = Paint.Style.FILL
        }
        private var curScroll = 0f
        private var curMax = 0f

        fun update(scroll: Float, max: Float) {
            curScroll = scroll
            curMax = max
            postInvalidateOnAnimation()
        }

        override fun onDraw(canvas: Canvas) {
            if (curMax <= 0.5f) return
            val w = width.toFloat()
            val h = height.toFloat()
            if (w <= 0f || h <= 0f) return
            val density = resources.displayMetrics.density
            val barW = 3f * density
            val right = w - 2f * density
            val left = right - barW
            val trackTop = 6f * density
            val bottomOffset = if (bottomInset > idleBottomInsetPx + 10f) bottomInset + 6f * density else 6f * density
            val trackH = (h - trackTop - bottomOffset).coerceAtLeast(1f)
            val visibleH = (h - (if (bottomInset > idleBottomInsetPx + 10f) bottomInset else 0f)).coerceAtLeast(1f)
            val contentH = curMax + visibleH
            val ratio = (visibleH / contentH).coerceIn(0f, 1f)
            val minThumb = minOf(28f * density, trackH)
            val thumbH = (trackH * ratio).coerceIn(minThumb, trackH)
            val progress = (curScroll / curMax).coerceIn(0f, 1f)
            val thumbTop = trackTop + (trackH - thumbH) * progress
            canvas.drawRoundRect(left, thumbTop, right, thumbTop + thumbH, barW / 2f, barW / 2f, barPaint)
        }
    }

    private val scrollbarOverlay = ScrollbarOverlayView(context).apply {
        isClickable = false
        isFocusable = false
    }

    private fun applyScrollTranslation() {
        val h = safeViewportHeight()
        val m = computeBandMargin(h)
        textureView.translationY = ((bandOrigin + m) - knownScroll).roundToInt().toFloat()
    }

    override fun onBandLayoutChanged() {
        applyScrollTranslation()
    }

    override fun onSurfaceTextureUpdated(surfaceTexture: SurfaceTexture) {
        val pending = pendingRenderedOrigin.getAndSet(null)
        if (pending != null) {
            bandOrigin = pending.roundToInt().toFloat()
            applyScrollTranslation()
            android.util.Log.d("VibecodiumRender", "onSurfaceTextureUpdated bandOrigin=$bandOrigin transY=${textureView.translationY}")
        }
    }

    private fun checkBandRender(scroll: Float) {
        val h = safeViewportHeight()
        if (h <= 0) return
        val m = computeBandMargin(h).toFloat()
        if (m <= 0f) return
        val topHeadroom = scroll - bandOrigin
        val bottomHeadroom = (bandOrigin + 2f * m) - scroll
        val threshold = m * 0.35f
        val maxOrigin = maxOf(scrollMaxY, knownScroll)
        if ((topHeadroom < threshold && bandOrigin > 0.5f) ||
            (bottomHeadroom < threshold && bandOrigin < maxOrigin - 0.5f)) {
            val targetOrigin = (scroll - m).coerceIn(0f, maxOrigin).roundToInt().toFloat()
            if (kotlin.math.abs(targetOrigin - bandOrigin) >= threshold) {
                scheduleBandRender(targetOrigin)
            }
        }
    }

    private fun scheduleBandRender(targetOrigin: Float, force: Boolean = false) {
        val maxOrigin = maxOf(ensureScrollMaxY(), knownScroll)
        val origin = targetOrigin.coerceIn(0f, maxOrigin).roundToInt().toFloat()
        if (force) {
            forcePendingBandRender = true
        } else if (kotlin.math.abs(origin - bandOrigin) < 1f && !isRenderingBand.get()) {
            return
        }
        pendingBandOrigin = origin
        if (isRenderingBand.compareAndSet(false, true)) {
            forcePendingBandRender = false
            dispatchBandRender(origin)
        }
    }

    protected override fun requestRender() {
        scheduleBandRender(bandOrigin, force = true)
    }

    protected override fun requestFullRender() {
        scheduleBandRender(bandOrigin, force = true)
    }

    private fun dispatchBandRender(origin: Float) {
        val roundedOrigin = origin.roundToInt().toFloat()
        renderer.execute {
            var renderedOrigin: Float? = null
            try {
                if (!isSurfaceAttached()) {
                    return@execute
                }
                CoreBridge.nativeEditorSetBand(surfaceId, roundedOrigin)
                val rendered = CoreBridge.nativeSurfaceRender(surfaceId)
                renderedOrigin = rendered
                pendingRenderedOrigin.set(rendered)
                android.util.Log.d("VibecodiumRender", "nativeSurfaceRender done rendered=$rendered")
            } finally {
                val finalRendered = renderedOrigin
                isRenderingBand.set(false)
                mainHandler.post {
                    val pendingRendered = pendingRenderedOrigin.getAndSet(null)
                    if (pendingRendered != null) {
                        bandOrigin = pendingRendered.roundToInt().toFloat()
                        applyScrollTranslation()
                        textureView.postInvalidateOnAnimation()
                        android.util.Log.d("VibecodiumRender", "mainHandler applied bandOrigin=$bandOrigin transY=${textureView.translationY}")
                    }
                    val pending = pendingBandOrigin
                    val m = computeBandMargin(safeViewportHeight()).toFloat()
                    val force = forcePendingBandRender
                    forcePendingBandRender = false
                    val lastOrigin = finalRendered ?: bandOrigin
                    val shouldRender = force ||
                        (pending != null && kotlin.math.abs(pending - lastOrigin) > m * 0.35f)
                    if (shouldRender && pending != null) {
                        scheduleBandRender(pending, force = force)
                    } else {
                        pendingBandOrigin = null
                    }
                }
            }
        }
    }

    private companion object {
        const val MENU_COPY = 1
        const val MENU_CUT = 2
        const val MENU_PASTE = 3
        const val MENU_SELECT_ALL = 4
    }

    private var mirrorText = ""
    private var mirrorSelection = 0
    private var mirrorAnchor = 0
    private var lastLanguage = ""
    @Volatile
    private var lastPath: String? = null
    private val prefs = context.getSharedPreferences("vibecodium", Context.MODE_PRIVATE)
    private var downX = 0f
    private var downY = 0f
    private var lastMotionY = 0f
    private var activePointerId = android.view.MotionEvent.INVALID_POINTER_ID
    private var dragged = false
    private var selecting = false
    private var selectingDrag = false
    private var longPressWordStart = -1
    private var longPressWordEnd = -1
    private var lastTapTime = 0L
    private var lastTapX = 0f
    private var lastTapY = 0f
    private var layoutRowHeight = (22f * resources.displayMetrics.density).roundToInt().toFloat()
    private var layoutTop = 0f
    private var visualFirst = IntArray(0)
    private var visualCount = IntArray(0)
    private var startHandle: PointF? = null
    private var endHandle: PointF? = null
    private var insertionHandle: PointF? = null
    private var handleSize = 24f * resources.displayMetrics.density
    private var handleRowHeight = (22f * resources.displayMetrics.density).roundToInt().toFloat()
    private var draggingHandle = -1
    private var dragOffsetX = 0f
    private var dragOffsetY = 0f
    private var actionMode: ActionMode? = null
    private var insertionActionMode: ActionMode? = null
    private var magnifier: Magnifier? = null
    private val hideInsertionRunnable = Runnable { applyInsertionHandleVisible(false) }
    private var bottomInset = -1f
    private var topInset = -1f
    private var inPreview = false
    private var caretVisible = true
    private val caretBlink = object : Runnable {
        override fun run() {
            if ((!isFocused && !inPreview) || selectionNonEmpty()) {
                applyCaretVisible(true)
                return
            }
            applyCaretVisible(!caretVisible)
            postDelayed(this, 500L)
        }
    }
    private val scroller = OverScroller(context)
    private val saveAfterFling = Runnable { postRenderTask { saveViewport() } }
    @Volatile private var flinging = false
    private var velocityTracker: VelocityTracker? = null
    @Volatile private var scrollMaxY = 0f
    private var dragStartScroll = 0f
    private var onScrollChanged: ((Float, Float, Boolean) -> Unit)? = null
    private var onCaretPlaced: (() -> Unit)? = null
    @Volatile private var knownScroll = 0f
    private var wasHidden = false
    private val flingRunnable = object : Choreographer.FrameCallback {
        override fun doFrame(frameTimeNanos: Long) {
            if (!flinging) return
            val more = scroller.computeScrollOffset()
            val currentY = scroller.currY.toFloat().coerceIn(0f, scrollMaxY).roundToInt().toFloat()
            knownScroll = currentY
            applyScrollTranslation()
            onScrollChanged?.invoke(currentY, scrollMaxY, true)
            scrollbarOverlay.update(currentY, scrollMaxY)
            checkBandRender(currentY)
            if (more && !scroller.isFinished && currentY > 0f && currentY < scrollMaxY) {
                postFrame(this)
            } else {
                flinging = false
                val settledScroll = knownScroll
                postRenderTask {
                    CoreBridge.nativeEditorSetScroll(surfaceId, settledScroll, 0f)
                }
                val m = computeBandMargin(safeViewportHeight()).toFloat()
                val maxOrigin = maxOf(scrollMaxY, knownScroll)
                scheduleBandRender((knownScroll - m).coerceIn(0f, maxOrigin))
                mainHandler.post {
                    resumeCaretBlink()
                    removeCallbacks(saveAfterFling)
                    postDelayed(saveAfterFling, 400L)
                    postDelayed({ helper.invalidateRoot() }, 100L)
                }
            }
        }
    }
    private var composingStart = -1
    private var composingEnd = -1
    private var lineStarts = intArrayOf(0)
    private val longPress = Runnable {
        if (!dragged) {
            draggingHandle = -1
            dismissMagnifier()
            selecting = true
            selectingDrag = false
            CoreBridge.nativeEditorSetScroll(surfaceId, knownScroll, 0f)
            CoreBridge.nativeEditorSelectWord(surfaceId, downX, downY)
            syncFromNative(notify = true)
            longPressWordStart = minOf(mirrorAnchor, mirrorSelection)
            longPressWordEnd = maxOf(mirrorAnchor, mirrorSelection)
            requestRender()
            helper.invalidateRoot()
            showSelectionActions()
        }
    }

    private val selectionCallback = object : ActionMode.Callback2() {
        override fun onCreateActionMode(mode: ActionMode, menu: Menu): Boolean {
            if (selectionNonEmpty()) {
                menu.add(Menu.NONE, MENU_COPY, 0, "Copy")
                if (!inPreview) {
                    menu.add(Menu.NONE, MENU_CUT, 1, "Cut")
                }
            }
            if (!inPreview) {
                menu.add(Menu.NONE, MENU_PASTE, 2, "Paste")
            }
            menu.add(Menu.NONE, MENU_SELECT_ALL, 3, "Select all")
            return true
        }

        override fun onPrepareActionMode(mode: ActionMode, menu: Menu): Boolean = false

        override fun onActionItemClicked(mode: ActionMode, item: MenuItem): Boolean {
            when (item.itemId) {
                MENU_COPY -> copySelection()
                MENU_CUT -> {
                    copySelection()
                    applyDelete(0, 0)
                }
                MENU_PASTE -> pasteClipboard()
                MENU_SELECT_ALL -> {
                    selectNative(0, mirrorText.length)
                    mode.invalidate()
                    mode.invalidateContentRect()
                    return true
                }
            }
            mode.finish()
            return true
        }

        override fun onDestroyActionMode(mode: ActionMode) {
            actionMode = null
        }

        override fun onGetContentRect(mode: ActionMode, view: View, outRect: Rect) {
            syncHandles()
            val start = startHandle
            val end = endHandle
            if (start == null || end == null) {
                outRect.set(0, 0, 0, 0)
                return
            }
            val left = minOf(start.x - handleSize, end.x).toInt().coerceAtLeast(0)
            val right = maxOf(start.x, end.x + handleSize).toInt().coerceAtMost(width)
            val top = (minOf(start.y, end.y) - handleRowHeight).toInt().coerceAtLeast(0)
            val bottom = (maxOf(start.y, end.y) + handleSize).toInt().coerceAtMost(height)
            outRect.set(left, top, right, bottom)
        }
    }

    private val insertionCallback = object : ActionMode.Callback2() {
        override fun onCreateActionMode(mode: ActionMode, menu: Menu): Boolean {
            val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
            val hasClip = clipboard?.hasPrimaryClip() == true
            if (hasClip) {
                menu.add(Menu.NONE, MENU_PASTE, 0, "Paste")
            }
            menu.add(Menu.NONE, MENU_SELECT_ALL, 1, "Select all")
            return true
        }

        override fun onPrepareActionMode(mode: ActionMode, menu: Menu): Boolean = false

        override fun onActionItemClicked(mode: ActionMode, item: MenuItem): Boolean {
            when (item.itemId) {
                MENU_PASTE -> pasteClipboard()
                MENU_SELECT_ALL -> {
                    selectNative(0, mirrorText.length)
                    mode.finish()
                    showSelectionActions()
                    return true
                }
            }
            mode.finish()
            return true
        }

        override fun onDestroyActionMode(mode: ActionMode) {
            insertionActionMode = null
        }

        override fun onGetContentRect(mode: ActionMode, view: View, outRect: Rect) {
            val ins = insertionHandle
            if (ins == null) {
                outRect.set(0, 0, 0, 0)
                return
            }
            val left = (ins.x - handleSize).toInt().coerceAtLeast(0)
            val right = (ins.x + handleSize).toInt().coerceAtMost(width)
            val top = (ins.y - handleRowHeight * 1.5f).toInt().coerceAtLeast(0)
            val bottom = (ins.y + handleSize).toInt().coerceAtMost(height)
            outRect.set(left, top, right, bottom)
        }
    }

    private fun updateMagnifier(sourceX: Float, sourceY: Float) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            try {
                if (magnifier == null) {
                    magnifier = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                        Magnifier.Builder(this)
                            .setSize(
                                (110f * resources.displayMetrics.density).roundToInt(),
                                (48f * resources.displayMetrics.density).roundToInt(),
                            )
                            .setCornerRadius(24f * resources.displayMetrics.density)
                            .setInitialZoom(1.25f)
                            .build()
                    } else {
                        Magnifier(this)
                    }
                }
                magnifier?.show(sourceX.coerceIn(0f, width.toFloat()), sourceY.coerceIn(0f, height.toFloat()))
            } catch (_: Throwable) {
            }
        }
    }

    private fun dismissMagnifier() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            try {
                magnifier?.dismiss()
            } catch (_: Throwable) {
            }
        }
    }

    private val helper: ExploreByTouchHelper by lazy {
        object : ExploreByTouchHelper(this) {
            private fun lineText(index: Int): String {
                if (index !in lineStarts.indices) return ""
                val start = lineStarts[index].coerceIn(0, mirrorText.length)
                val end = if (index + 1 < lineStarts.size) {
                    (lineStarts[index + 1] - 1).coerceIn(start, mirrorText.length)
                } else {
                    mirrorText.length
                }
                return mirrorText.substring(start, end)
            }

            override fun getVirtualViewAt(x: Float, y: Float): Int {
                refreshLayoutInfo()
                val scroll = CoreBridge.nativeEditorScroll(surfaceId)
                val visual = ((y + scroll - layoutTop) / layoutRowHeight).toInt()
                return lineAtVisual(visual).takeIf { it >= 0 } ?: View.NO_ID
            }

            override fun getVisibleVirtualViews(virtualViewIds: MutableList<Int>) {
                if (mirrorText.isEmpty()) return
                refreshLayoutInfo()
                val scroll = CoreBridge.nativeEditorScroll(surfaceId)
                val firstVisual =
                    ((scroll - layoutTop) / layoutRowHeight).toInt().coerceAtLeast(0)
                val lastVisual = ((scroll + height - layoutTop) / layoutRowHeight).toInt()
                val firstLine = lineAtVisual(firstVisual).let { if (it >= 0) it else 0 }
                for (index in firstLine until lineStarts.size) {
                    val top = visualFirst.getOrElse(index) { index }
                    if (top > lastVisual) break
                    virtualViewIds.add(index)
                }
            }

            override fun onPopulateNodeForVirtualView(
                virtualViewId: Int,
                node: AccessibilityNodeInfoCompat,
            ) {
                if (virtualViewId !in lineStarts.indices) {
                    node.contentDescription = ""
                    node.setBoundsInParent(Rect(0, 0, 1, 1))
                    return
                }
                refreshLayoutInfo()
                val text = lineText(virtualViewId)
                node.text = text
                node.contentDescription = "Line ${virtualViewId + 1}: $text"
                node.isFocusable = true
                node.isClickable = true
                node.isEnabled = true
                node.addAction(AccessibilityNodeInfoCompat.ACTION_CLICK)
                val scroll = CoreBridge.nativeEditorScroll(surfaceId)
                val top = (lineTop(virtualViewId) - scroll).toInt()
                val lineHeight = linePixelHeight(virtualViewId).toInt()
                node.setBoundsInParent(Rect(0, top, width, top + lineHeight))
            }

            override fun onPerformActionForVirtualView(
                virtualViewId: Int,
                action: Int,
                arguments: Bundle?,
            ): Boolean {
                if (action != AccessibilityNodeInfoCompat.ACTION_CLICK) return false
                if (virtualViewId !in lineStarts.indices) return false
                refreshLayoutInfo()
                val scroll = CoreBridge.nativeEditorScroll(surfaceId)
                val y = lineTop(virtualViewId) - scroll + linePixelHeight(virtualViewId) / 2f
                CoreBridge.nativeEditorSelectAt(surfaceId, 0f, y, false)
                syncFromNative(notify = true)
                requestFocus()
                val imm = context.getSystemService(Context.INPUT_METHOD_SERVICE) as? InputMethodManager
                imm?.showSoftInput(this@NativeEditorView, InputMethodManager.SHOW_IMPLICIT)
                requestRender()
                return true
            }
        }
    }

    init {
        textureView.isOpaque = true
        ViewCompat.setAccessibilityDelegate(this, helper)
        addView(scrollbarOverlay, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        bottomInset = idleBottomInsetPx
    }

    fun setOnScrollChanged(listener: ((Float, Float, Boolean) -> Unit)?) {
        if (onScrollChanged === listener) return
        onScrollChanged = listener
        if (listener != null) {
            listener.invoke(knownScroll, scrollMaxY, false)
        }
    }

    fun setBottomInset(inset: Float) {
        val next = inset.coerceAtLeast(0f)
        if (kotlin.math.abs(next - bottomInset) < 0.5f) return
        val wasIncreased = next > bottomInset + 10f
        bottomInset = next
        postRenderTask {
            if (!isSurfaceAttached()) return@postRenderTask
            CoreBridge.nativeEditorSetBottomInset(surfaceId, next)
            val info = runCatching { JSONObject(CoreBridge.nativeEditorViewport(surfaceId)) }.getOrNull()
            val newMaxY = info?.optDouble("max_y", 0.0)?.toFloat()?.coerceAtLeast(0f) ?: scrollMaxY
            val rustScroll = info?.optDouble("scroll", knownScroll.toDouble())?.toFloat() ?: knownScroll
            val nextScroll = if (wasIncreased) rustScroll else knownScroll.coerceIn(0f, newMaxY)
            mainHandler.post {
                val scrollChanged = nextScroll != knownScroll
                knownScroll = nextScroll
                scrollMaxY = newMaxY
                applyScrollTranslation()
                if (scrollChanged) {
                    checkBandRender(knownScroll)
                    requestRender()
                }
                scrollbarOverlay.update(knownScroll, scrollMaxY)
                onScrollChanged?.invoke(knownScroll, scrollMaxY, false)
            }
        }
    }

    fun setTopInset(inset: Float) {
        val next = inset.coerceAtLeast(0f)
        if (kotlin.math.abs(next - topInset) < 0.5f) return
        topInset = next
        postRenderTask {
            if (!isSurfaceAttached()) return@postRenderTask
            CoreBridge.nativeEditorSetTopInset(surfaceId, next)
            val info = runCatching { JSONObject(CoreBridge.nativeEditorViewport(surfaceId)) }.getOrNull()
            val newMaxY = info?.optDouble("max_y", 0.0)?.toFloat()?.coerceAtLeast(0f) ?: scrollMaxY
            mainHandler.post {
                scrollMaxY = newMaxY
                applyScrollTranslation()
                scrollbarOverlay.update(knownScroll, scrollMaxY)
                onScrollChanged?.invoke(knownScroll, scrollMaxY, false)
            }
        }
        requestRender()
    }

    fun setEditorPreview(preview: Boolean) {
        if (preview == inPreview) return
        inPreview = preview
        postRenderTask {
            CoreBridge.nativeEditorSetPreview(surfaceId, preview)
        }
        stopFling()
        recycleVelocity()
        actionMode?.finish()
        insertionActionMode?.finish()
        if (preview) {
            applyInsertionHandleVisible(false)
            hideKeyboard()
            clearFocus()
        }
        applyCaretVisible(true)
        startCaretBlink()
        requestFullRender()
    }

    /**
     * Pushes a caret visibility change only when it actually flips. The
     * repaint is queued on the render thread, so the blink no longer paints a
     * whole frame on the UI thread (which used to stall an ongoing scroll).
     */
    private fun applyCaretVisible(visible: Boolean) {
        if (caretVisible == visible) return
        caretVisible = visible
        CoreBridge.nativeEditorSetCaretVisible(surfaceId, visible)
        requestFullRender()
    }

    private fun startCaretBlink() {
        removeCallbacks(caretBlink)
        applyCaretVisible(true)
        if ((isFocused || inPreview) && !selectionNonEmpty()) {
            postDelayed(caretBlink, 500L)
        }
    }

    private fun stopCaretBlink() {
        removeCallbacks(caretBlink)
        applyCaretVisible(true)
    }

    private fun suspendCaretBlink() {
        removeCallbacks(caretBlink)
        applyCaretVisible(true)
    }

    private fun resumeCaretBlink() {
        removeCallbacks(caretBlink)
        if ((!isFocused && !inPreview) || selectionNonEmpty()) return
        applyCaretVisible(true)
        postDelayed(caretBlink, 500L)
    }

    override fun onFocusChanged(
        gainFocus: Boolean,
        direction: Int,
        previouslyFocusedRect: Rect?,
    ) {
        super.onFocusChanged(gainFocus, direction, previouslyFocusedRect)
        if (gainFocus || inPreview) {
            startCaretBlink()
        } else {
            actionMode?.finish()
            insertionActionMode?.finish()
            if (inPreview) {
                stopCaretBlink()
                applyInsertionHandleVisible(false)
            }
        }
    }

    override fun onWindowFocusChanged(hasWindowFocus: Boolean) {
        super.onWindowFocusChanged(hasWindowFocus)
        if (!hasWindowFocus) {
            saveViewport()
        } else {
            ensureScrollMaxY()
            restoreViewport()
            if (!inPreview) {
                focusEditor()
                hideKeyboard()
            } else if (!selectionNonEmpty()) {
                startCaretBlink()
            }
            requestFullRender()
            mainHandler.post {
                applyScrollTranslation()
                textureView.postInvalidateOnAnimation()
            }
        }
    }

    override fun onWindowVisibilityChanged(visibility: Int) {
        super.onWindowVisibilityChanged(visibility)
        if (visibility != View.VISIBLE) {
            wasHidden = true
            saveViewport()
            actionMode?.finish()
            removeCallbacks(caretBlink)
            clearFocus()
            hideKeyboard()
        } else if (wasHidden) {
            wasHidden = false
            ensureScrollMaxY()
            restoreViewport()
            if (!inPreview) {
                focusEditor()
                hideKeyboard()
            }
            requestFullRender()
            mainHandler.post {
                applyScrollTranslation()
                textureView.postInvalidateOnAnimation()
            }
        }
    }

    override fun dispatchHoverEvent(event: MotionEvent): Boolean {
        return helper.dispatchHoverEvent(event) || super.dispatchHoverEvent(event)
    }

    private fun refreshLines() {
        val starts = ArrayList<Int>()
        starts.add(0)
        mirrorText.forEachIndexed { index, character ->
            if (character == '\n') starts.add(index + 1)
        }
        lineStarts = starts.toIntArray()
        helper.invalidateRoot()
    }

    private fun refreshLayoutInfo() {
        val info = runCatching { JSONObject(CoreBridge.nativeEditorLayoutInfo(surfaceId)) }
            .getOrNull() ?: return
        layoutRowHeight = info.optDouble(
            "row_height",
            (22.0 * resources.displayMetrics.density).roundToInt().toDouble(),
        ).toFloat()
        layoutTop = info.optDouble("top", 0.0).toFloat()
        val first = info.optJSONArray("first") ?: JSONArray()
        val count = info.optJSONArray("count") ?: JSONArray()
        val size = maxOf(first.length(), count.length(), lineStarts.size)
        visualFirst = IntArray(size) { first.optInt(it, 0) }
        visualCount = IntArray(size) { maxOf(1, count.optInt(it, 1)) }
    }

    private fun lineAtVisual(visual: Int): Int {
        if (visualFirst.isEmpty()) return -1
        var low = 0
        var high = visualFirst.size - 1
        var result = -1
        while (low <= high) {
            val middle = (low + high) / 2
            if (visualFirst[middle] <= visual) {
                result = middle
                low = middle + 1
            } else {
                high = middle - 1
            }
        }
        if (result < 0) return -1
        val top = visualFirst[result]
        val lineHeight = visualCount.getOrElse(result) { 1 }
        return if (visual in top until top + lineHeight) result else -1
    }

    private fun lineTop(index: Int): Float =
        layoutTop + visualFirst.getOrElse(index) { index } * layoutRowHeight

    private fun linePixelHeight(index: Int): Float =
        visualCount.getOrElse(index) { 1 } * layoutRowHeight

    override fun onSurfaceReady() {
        CoreBridge.nativeSurfaceSetEditor(
            surfaceId,
            mirrorText,
            mirrorSelection,
            lastLanguage,
        )
        CoreBridge.nativeEditorSelection(surfaceId, mirrorAnchor, mirrorSelection)
        CoreBridge.nativeEditorSetBottomInset(
            surfaceId,
            if (bottomInset >= 0f) bottomInset else idleBottomInsetPx,
        )
        if (topInset >= 0f) {
            CoreBridge.nativeEditorSetTopInset(surfaceId, topInset)
        }
        CoreBridge.nativeEditorSetPreview(surfaceId, inPreview)
        caretVisible = true
        CoreBridge.nativeEditorSetCaretVisible(surfaceId, true)
        CoreBridge.nativeEditorSetInsertionHandleVisible(surfaceId, !inPreview)
        lastPath?.let { path ->
            loadViewport(path)?.let { (scroll, scrollX) ->
                CoreBridge.nativeEditorSetScroll(surfaceId, scroll, scrollX)
            }
        }
        val info = runCatching { JSONObject(CoreBridge.nativeEditorViewport(surfaceId)) }.getOrNull()
        scrollMaxY = info?.optDouble("max_y", 0.0)?.toFloat()?.coerceAtLeast(0f) ?: 0f
        knownScroll = CoreBridge.nativeEditorScroll(surfaceId).roundToInt().toFloat()
        val m = computeBandMargin(safeViewportHeight()).toFloat()
        val maxOrigin = maxOf(scrollMaxY, knownScroll)
        bandOrigin = (knownScroll - m).coerceIn(0f, maxOrigin).roundToInt().toFloat()
        CoreBridge.nativeEditorSetBand(surfaceId, bandOrigin)
        mainHandler.post {
            applyScrollTranslation()
            scrollbarOverlay.update(knownScroll, scrollMaxY)
            onScrollChanged?.invoke(knownScroll, scrollMaxY, false)
            if (!inPreview) {
                focusEditor()
            } else if (!selectionNonEmpty()) {
                startCaretBlink()
            }
            syncHandles()
        }
    }

    fun setEditor(text: String, selection: Int, language: String?, path: String) {
        val nextLanguage = language.orEmpty()
        val pathChanged = lastPath != path
        if (!pathChanged && mirrorText == text && lastLanguage == nextLanguage) return
        if (pathChanged) {
            saveViewport()
            lastPath = path
            scrollMaxY = 0f
            scrollbarOverlay.update(knownScroll, 0f)
        }
        val previous = mirrorSelection
        val previousText = mirrorText
        mirrorText = text
        mirrorSelection = selection.coerceIn(0, text.length)
        if (mirrorSelection != previous || previousText != text) {
            mirrorAnchor = mirrorSelection
        }
        composingStart = -1
        composingEnd = -1
        lastLanguage = nextLanguage
        refreshLines()
        val viewport = if (pathChanged) loadViewport(path) else null
        if (pathChanged) {
            knownScroll = viewport?.first ?: 0f
            val m = computeBandMargin(safeViewportHeight()).toFloat()
            bandOrigin = (knownScroll - m).coerceAtLeast(0f).roundToInt().toFloat()
            applyScrollTranslation()
        }
        contentDescription = if (nextLanguage.isEmpty()) {
            "Markdown source editor, ${text.count { it == '\n' } + 1} lines"
        } else {
            "Code source editor ($nextLanguage), ${text.count { it == '\n' } + 1} lines"
        }
        val imm = context.getSystemService(Context.INPUT_METHOD_SERVICE) as? InputMethodManager
        if (pathChanged) {
            imm?.restartInput(this)
        }
        notifySelectionChanged()
        postState {
            CoreBridge.nativeSurfaceSetEditor(
                surfaceId,
                text,
                mirrorSelection,
                nextLanguage,
            )
            CoreBridge.nativeEditorSelection(surfaceId, mirrorSelection, mirrorSelection)
            if (pathChanged) {
                CoreBridge.nativeEditorSetScroll(
                    surfaceId,
                    viewport?.first ?: 0f,
                    viewport?.second ?: 0f,
                )
            }
            val info = runCatching { JSONObject(CoreBridge.nativeEditorViewport(surfaceId)) }.getOrNull()
            val newMaxY = info?.optDouble("max_y", 0.0)?.toFloat()?.coerceAtLeast(0f) ?: 0f
            val newScroll = CoreBridge.nativeEditorScroll(surfaceId).roundToInt().toFloat()
            scrollMaxY = newMaxY
            knownScroll = newScroll
            val m = computeBandMargin(safeViewportHeight()).toFloat()
            val maxOrigin = maxOf(scrollMaxY, knownScroll)
            bandOrigin = (knownScroll - m).coerceIn(0f, maxOrigin).roundToInt().toFloat()
            CoreBridge.nativeEditorSetBand(surfaceId, bandOrigin)
            mainHandler.post {
                applyScrollTranslation()
                scrollbarOverlay.update(knownScroll, scrollMaxY)
                onScrollChanged?.invoke(knownScroll, scrollMaxY, false)
            }
        }
        if (pathChanged) {
            mainHandler.post {
                if (!selectionNonEmpty()) {
                    startCaretBlink()
                }
                syncHandles()
            }
        }
    }

    private fun saveViewport() {
        val path = lastPath ?: return
        if (path.isEmpty()) return
        val viewport = runCatching { JSONObject(CoreBridge.nativeEditorViewport(surfaceId)) }
            .getOrNull()
        val scrollX = viewport?.optDouble("scroll_x", 0.0)?.toFloat() ?: 0f
        prefs.edit()
            .putString("viewport:$path", "$knownScroll,$scrollX")
            .apply()
    }

    /** Reapplies the stored position after the app returns from the background. */
    private fun restoreViewport() {
        if (flinging) return
        val path = lastPath ?: return
        val viewport = loadViewport(path)
        val scroll = viewport?.first ?: knownScroll
        val scrollX = viewport?.second ?: 0f
        knownScroll = scroll.roundToInt().toFloat()
        val maxOrigin = maxOf(ensureScrollMaxY(), knownScroll)
        val m = computeBandMargin(safeViewportHeight()).toFloat()
        bandOrigin = (knownScroll - m).coerceIn(0f, maxOrigin).roundToInt().toFloat()
        applyScrollTranslation()
        postRenderTask {
            if (!isSurfaceAttached()) return@postRenderTask
            CoreBridge.nativeEditorSetBand(surfaceId, bandOrigin)
            CoreBridge.nativeEditorSetScroll(surfaceId, scroll, scrollX)
            val rendered = CoreBridge.nativeSurfaceRender(surfaceId)
            onRenderFinished(rendered)
            mainHandler.post {
                applyScrollTranslation()
                textureView.postInvalidateOnAnimation()
            }
        }
    }

    /** Forces a complete viewport and render refresh when an overlay (like settings) is closed. */
    fun onOverlayDismissed() {
        if (!inPreview) {
            focusEditor()
        }
        ensureScrollMaxY()
        restoreViewport()
        requestFullRender()
        syncHandles()
        if (!selectionNonEmpty()) {
            startCaretBlink()
        }
        mainHandler.post {
            applyScrollTranslation()
            textureView.postInvalidateOnAnimation()
        }
    }

    private fun loadViewport(path: String): Pair<Float, Float>? {
        val raw = prefs.getString("viewport:$path", null) ?: return null
        val parts = raw.split(',')
        if (parts.size != 2) return null
        val scroll = parts[0].toFloatOrNull() ?: return null
        val scrollX = parts[1].toFloatOrNull() ?: return null
        return scroll to scrollX
    }

    override fun onCheckIsTextEditor(): Boolean = true

    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        if (event.isCtrlPressed) {
            when (keyCode) {
                KeyEvent.KEYCODE_A -> {
                    selectNative(0, mirrorText.length)
                    showSelectionActions()
                    return true
                }
                KeyEvent.KEYCODE_C, KeyEvent.KEYCODE_X -> {
                    val start = minOf(mirrorAnchor, mirrorSelection)
                    val end = maxOf(mirrorAnchor, mirrorSelection)
                    if (end > start) {
                        val clip = context.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
                        clip?.setPrimaryClip(ClipData.newPlainText("vibecodium", mirrorText.substring(start, end)))
                    }
                    if (keyCode == KeyEvent.KEYCODE_X && end > start) {
                        selectNative(start, end)
                        applyDelete(0, 0)
                    }
                    return true
                }
                KeyEvent.KEYCODE_V -> {
                    val clip = context.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
                    val text = clip?.primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(context)
                    if (!text.isNullOrEmpty()) applyInsert(text.toString())
                    return true
                }
            }
        }
        if (keyCode == KeyEvent.KEYCODE_TAB) {
            applyInsert("    ")
            return true
        }
        if (keyCode == KeyEvent.KEYCODE_DEL) {
            if (selectionNonEmpty()) {
                applyDelete(0, 0)
                return true
            }
            if (event.isCtrlPressed || event.isAltPressed) {
                applyDelete(previousWordLength(mirrorText, mirrorSelection), 0)
                return true
            }
            applyDelete(1, 0)
            return true
        }
        if (keyCode == KeyEvent.KEYCODE_ENTER) {
            applyInsert("\n")
            return true
        }
        return super.onKeyDown(keyCode, event)
    }

    private fun previousWordLength(text: String, cursor: Int): Int {
        if (cursor <= 0 || text.isEmpty()) return 0
        val clamped = cursor.coerceIn(0, text.length)
        if (clamped <= 0) return 0

        // 1. If cursor is right after a newline, delete only the newline so lines join cleanly.
        val prev = text[clamped - 1]
        if (prev == '\n') {
            return if (clamped >= 2 && text[clamped - 2] == '\r') 2 else 1
        }
        if (prev == '\r') {
            return 1
        }

        var i = clamped

        // 2. Skip horizontal whitespace (spaces, tabs) immediately preceding the cursor.
        while (i > 0 && (text[i - 1] == ' ' || text[i - 1] == '\t')) {
            i--
        }

        // If only whitespace existed on this line before the cursor, delete that whitespace.
        if (i == 0 || text[i - 1] == '\n' || text[i - 1] == '\r') {
            return maxOf(1, clamped - i)
        }

        // 3. Delete the preceding token (word or symbol run).
        val tokenChar = text[i - 1]
        if (Character.isLetterOrDigit(tokenChar)) {
            while (i > 0 && Character.isLetterOrDigit(text[i - 1])) {
                i--
            }
        } else {
            while (i > 0 && !Character.isLetterOrDigit(text[i - 1]) && !Character.isWhitespace(text[i - 1])) {
                i--
            }
        }

        // Avoid splitting a surrogate pair
        if (i in 1 until text.length && Character.isLowSurrogate(text[i]) && Character.isHighSurrogate(text[i - 1])) {
            i--
        }

        val codePoints = runCatching {
            text.codePointCount(i, clamped)
        }.getOrDefault(clamped - i)

        return maxOf(1, codePoints)
    }

    private var extractedTextToken = 0

    private open inner class EditorInputConnection : BaseInputConnection(this@NativeEditorView, false) {
        private var batchDepth = 0

        override fun beginBatchEdit(): Boolean {
            batchDepth++
            return true
        }

        override fun endBatchEdit(): Boolean {
            if (batchDepth > 0) batchDepth--
            if (batchDepth == 0) {
                notifySelectionChanged()
                requestRender()
            }
            return batchDepth > 0
        }

        override fun commitText(text: CharSequence?, newCursorPosition: Int): Boolean {
            val value = text?.toString().orEmpty()
            if (composingStart >= 0) {
                applyReplace(composingStart, composingEnd, value)
                composingStart = -1
                composingEnd = -1
            } else if (value.isEmpty() && selectionNonEmpty()) {
                applyDelete(0, 0)
            } else {
                applyInsert(value)
            }
            return true
        }

        override fun setComposingText(text: CharSequence?, newCursorPosition: Int): Boolean {
            val start = if (composingStart >= 0) {
                composingStart
            } else {
                minOf(mirrorAnchor, mirrorSelection)
            }
            val value = text?.toString().orEmpty()
            if (value.isEmpty()) {
                if (composingStart >= 0) {
                    applyReplace(composingStart, composingEnd, "")
                } else if (selectionNonEmpty()) {
                    applyDelete(0, 0)
                }
                composingStart = -1
                composingEnd = -1
                return true
            }
            if (composingStart >= 0) {
                applyReplace(composingStart, composingEnd, value)
            } else {
                applyInsert(value)
            }
            composingStart = start
            composingEnd = mirrorSelection
            return true
        }

        override fun setComposingRegion(start: Int, end: Int): Boolean {
            val len = mirrorText.length
            composingStart = minOf(start, end).coerceIn(0, len)
            composingEnd = maxOf(start, end).coerceIn(0, len)
            notifySelectionChanged()
            return true
        }

        override fun finishComposingText(): Boolean {
            val changed = composingStart >= 0
            composingStart = -1
            composingEnd = -1
            if (changed) notifySelectionChanged()
            return true
        }

        override fun deleteSurroundingText(beforeLength: Int, afterLength: Int): Boolean {
            composingStart = -1
            composingEnd = -1
            if (selectionNonEmpty()) {
                applyDelete(0, 0)
                return true
            }
            val cursor = mirrorSelection.coerceIn(0, mirrorText.length)
            val actualBefore = minOf(beforeLength, cursor)
            val actualAfter = minOf(afterLength, mirrorText.length - cursor)
            if (actualBefore <= 0 && actualAfter <= 0) return true
            applyDelete(actualBefore, actualAfter)
            return true
        }

        override fun deleteSurroundingTextInCodePoints(beforeLength: Int, afterLength: Int): Boolean {
            composingStart = -1
            composingEnd = -1
            if (selectionNonEmpty()) {
                applyDelete(0, 0)
                return true
            }
            val cursor = mirrorSelection.coerceIn(0, mirrorText.length)
            val actualBefore = minOf(beforeLength, cursor)
            val actualAfter = minOf(afterLength, mirrorText.length - cursor)
            if (actualBefore <= 0 && actualAfter <= 0) return true
            applyDelete(actualBefore, actualAfter)
            return true
        }

        @Suppress("DEPRECATION")
        override fun sendKeyEvent(event: KeyEvent): Boolean {
            if (event.action != KeyEvent.ACTION_DOWN && event.action != KeyEvent.ACTION_MULTIPLE) {
                return true
            }
            return when (event.keyCode) {
                KeyEvent.KEYCODE_DEL -> {
                    if (selectionNonEmpty()) {
                        applyDelete(0, 0)
                        return true
                    }
                    if (event.isCtrlPressed || event.isAltPressed) {
                        applyDelete(previousWordLength(mirrorText, mirrorSelection), 0)
                        return true
                    }
                    val count = if (event.action == KeyEvent.ACTION_MULTIPLE) {
                        maxOf(1, event.repeatCount)
                    } else {
                        1
                    }
                    val cursor = mirrorSelection.coerceIn(0, mirrorText.length)
                    val toDelete = if (count > 1) {
                        minOf(count, previousWordLength(mirrorText, cursor))
                    } else {
                        minOf(1, cursor)
                    }
                    if (toDelete > 0) {
                        applyDelete(toDelete, 0)
                    }
                    true
                }
                KeyEvent.KEYCODE_ENTER -> {
                    val count = if (event.action == KeyEvent.ACTION_MULTIPLE) {
                        maxOf(1, event.repeatCount)
                    } else {
                        1
                    }
                    applyInsert("\n".repeat(count))
                    true
                }
                else -> super.sendKeyEvent(event)
            }
        }

        override fun getTextBeforeCursor(n: Int, flags: Int): CharSequence {
            val selStart = minOf(mirrorAnchor, mirrorSelection).coerceIn(0, mirrorText.length)
            return mirrorText.substring((selStart - n).coerceAtLeast(0), selStart)
        }

        override fun getTextAfterCursor(n: Int, flags: Int): CharSequence {
            val selEnd = maxOf(mirrorAnchor, mirrorSelection).coerceIn(0, mirrorText.length)
            return mirrorText.substring(selEnd, (selEnd + n).coerceAtMost(mirrorText.length))
        }

        override fun getSelectedText(flags: Int): CharSequence? {
            val start = mirrorAnchor.coerceIn(0, mirrorText.length)
            val end = mirrorSelection.coerceIn(0, mirrorText.length)
            if (start == end) return null
            return mirrorText.substring(minOf(start, end), maxOf(start, end))
        }

        override fun getCursorCapsMode(reqModes: Int): Int {
            val cursor = mirrorSelection.coerceIn(0, mirrorText.length)
            return TextUtils.getCapsMode(mirrorText, cursor, reqModes)
        }

        override fun getExtractedText(request: ExtractedTextRequest?, flags: Int): ExtractedText {
            if (request != null) {
                extractedTextToken = request.token
            }
            val et = ExtractedText()
            et.startOffset = 0
            et.partialStartOffset = -1
            et.partialEndOffset = -1
            val len = mirrorText.length
            et.selectionStart = mirrorAnchor.coerceIn(0, len)
            et.selectionEnd = mirrorSelection.coerceIn(0, len)
            et.text = mirrorText
            et.flags = 0
            return et
        }

        override fun setSelection(start: Int, end: Int): Boolean {
            composingStart = -1
            composingEnd = -1
            selectNative(start, end)
            return true
        }
    }

    @RequiresApi(Build.VERSION_CODES.S)
    private inner class EditorInputConnectionApi31 : EditorInputConnection() {
        override fun getSurroundingText(beforeLength: Int, afterLength: Int, flags: Int): SurroundingText? {
            val text = mirrorText
            val length = text.length
            val selStart = mirrorAnchor.coerceIn(0, length)
            val selEnd = mirrorSelection.coerceIn(0, length)
            val minSel = minOf(selStart, selEnd)
            val maxSel = maxOf(selStart, selEnd)
            val startPos = (minSel - maxOf(0, beforeLength)).coerceIn(0, length)
            val endPos = (maxSel + maxOf(0, afterLength)).coerceIn(startPos, length)
            val sub = text.substring(startPos, endPos)
            return SurroundingText(
                sub,
                selStart - startPos,
                selEnd - startPos,
                startPos,
            )
        }
    }

    override fun onCreateInputConnection(outAttrs: EditorInfo): InputConnection? {
        if (inPreview) return null
        outAttrs.inputType = InputType.TYPE_CLASS_TEXT or
            InputType.TYPE_TEXT_FLAG_MULTI_LINE or
            InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
        outAttrs.imeOptions = EditorInfo.IME_FLAG_NO_EXTRACT_UI or
            EditorInfo.IME_FLAG_NO_FULLSCREEN or
            EditorInfo.IME_ACTION_NONE
        outAttrs.initialSelStart = mirrorAnchor
        outAttrs.initialSelEnd = mirrorSelection
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            EditorInputConnectionApi31()
        } else {
            EditorInputConnection()
        }
    }

    private fun selectionNonEmpty(): Boolean = mirrorAnchor != mirrorSelection

    private fun syncHandles() {
        val info = runCatching { JSONObject(CoreBridge.nativeEditorSelectionHandles(surfaceId)) }
            .getOrNull() ?: return
        handleSize = info.optDouble("size", handleSize.toDouble()).toFloat()
        handleRowHeight = info.optDouble("row_height", handleRowHeight.toDouble()).toFloat()

        if (selectionNonEmpty()) {
            val start = info.optJSONObject("start")
            val end = info.optJSONObject("end")
            if (start == null || end == null) {
                startHandle = null
                endHandle = null
                insertionHandle = null
                return
            }
            startHandle = PointF(
                start.optDouble("x", 0.0).toFloat(),
                start.optDouble("y", 0.0).toFloat(),
            )
            endHandle = PointF(
                end.optDouble("x", 0.0).toFloat(),
                end.optDouble("y", 0.0).toFloat(),
            )
            insertionHandle = null
        } else {
            startHandle = null
            endHandle = null
            val ins = info.optJSONObject("insertion")
            if (ins != null) {
                insertionHandle = PointF(
                    ins.optDouble("x", 0.0).toFloat(),
                    ins.optDouble("y", 0.0).toFloat(),
                )
            } else {
                insertionHandle = null
            }
        }
    }

    private fun showSelectionActions() {
        if (!selectionNonEmpty()) {
            actionMode?.finish()
            return
        }
        stopCaretBlink()
        insertionActionMode?.finish()
        syncHandles()
        val current = actionMode
        if (current == null) {
            actionMode = runCatching { startActionMode(selectionCallback, ActionMode.TYPE_FLOATING) }.getOrNull()
        } else {
            current.invalidate()
            current.invalidateContentRect()
        }
    }

    private fun showInsertionActions() {
        if (inPreview) return
        actionMode?.finish()
        insertionActionMode?.finish()
        syncHandles()
        if (insertionHandle == null) return
        insertionActionMode = runCatching { startActionMode(insertionCallback, ActionMode.TYPE_FLOATING) }.getOrNull()
    }

    fun applyInsertionHandleVisible(visible: Boolean) {
        val targetVisible = if (inPreview) false else visible
        removeCallbacks(hideInsertionRunnable)
        CoreBridge.nativeEditorSetInsertionHandleVisible(surfaceId, targetVisible)
        requestRender()
        if (targetVisible) {
            syncHandles()
        } else {
            insertionHandle = null
            insertionActionMode?.finish()
        }
    }

    private fun copySelection(): Boolean {
        val start = minOf(mirrorAnchor, mirrorSelection)
        val end = maxOf(mirrorAnchor, mirrorSelection)
        if (end <= start) return false
        val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
        clipboard?.setPrimaryClip(ClipData.newPlainText("vibecodium", mirrorText.substring(start, end)))
        return true
    }

    private fun pasteClipboard() {
        val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
        val text = clipboard?.primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(context)
        if (!text.isNullOrEmpty()) applyInsert(text.toString())
    }

    private fun selectNative(start: Int, end: Int) {
        CoreBridge.nativeEditorSelection(surfaceId, start, end)
        mirrorAnchor = start.coerceIn(0, mirrorText.length)
        mirrorSelection = end.coerceIn(0, mirrorText.length)
        if (mirrorAnchor != mirrorSelection) {
            stopCaretBlink()
        } else {
            startCaretBlink()
        }
        syncHandles()
        actionMode?.invalidateContentRect()
        notifySelectionChanged()
        requestRender()
        helper.invalidateRoot()
    }

    override fun onTouchEvent(event: android.view.MotionEvent): Boolean {
        val slop = 8f * resources.displayMetrics.density
        when (event.actionMasked) {
            android.view.MotionEvent.ACTION_DOWN -> {
                activePointerId = event.getPointerId(0)
                stopFling()
                suspendCaretBlink()
                downX = event.x
                downY = event.y
                lastMotionY = event.y
                dragged = false
                selecting = false
                selectingDrag = false
                longPressWordStart = -1
                longPressWordEnd = -1
                removeCallbacks(longPress)
                dragStartScroll = knownScroll
                val scrollForRust = knownScroll
                postRenderTask {
                    CoreBridge.nativeEditorSetScroll(surfaceId, scrollForRust, 0f)
                }
                insertionActionMode?.finish()
                val handle = handleAt(event.x, event.y)
                if (handle >= 0) {
                    draggingHandle = handle
                    val point = when (handle) {
                        0 -> startHandle
                        1 -> endHandle
                        else -> insertionHandle
                    }
                    if (point != null) {
                        dragOffsetX = event.x - point.x
                        dragOffsetY = event.y - point.y
                        if (handle != 2) {
                            updateMagnifier(point.x, point.y - handleRowHeight / 2f)
                        }
                    }
                    actionMode?.finish()
                    insertionActionMode?.finish()
                    recycleVelocity()
                    if (handle == 2) {
                        postDelayed(longPress, android.view.ViewConfiguration.getLongPressTimeout().toLong())
                    }
                    return true
                }
                postDelayed(longPress, android.view.ViewConfiguration.getLongPressTimeout().toLong())
                velocityTracker?.recycle()
                velocityTracker = VelocityTracker.obtain().also { it.addMovement(event) }
            }
            android.view.MotionEvent.ACTION_MOVE -> {
                velocityTracker?.addMovement(event)
                val pointerIndex = event.findPointerIndex(activePointerId)
                if (pointerIndex < 0) return true
                val x = event.getX(pointerIndex)
                val y = event.getY(pointerIndex)

                if (draggingHandle >= 0) {
                    val threshold = 8f * resources.displayMetrics.density
                    val totalDy = kotlin.math.abs(y - downY)
                    val totalDx = kotlin.math.abs(x - downX)
                    if (!dragged) {
                        if (totalDx > threshold || totalDy > threshold) {
                            dragged = true
                            removeCallbacks(longPress)
                        } else {
                            return true
                        }
                    }
                    CoreBridge.nativeEditorDragHandle(
                        surfaceId,
                        draggingHandle,
                        x - dragOffsetX,
                        y - dragOffsetY,
                    )
                    refreshSelectionFromNative()
                    val targetX = x - dragOffsetX
                    val targetY = y - dragOffsetY - handleRowHeight / 2f
                    updateMagnifier(targetX, targetY)
                    saveViewport()
                    return true
                }
                if (selecting) {
                    val threshold = 8f * resources.displayMetrics.density
                    val totalDx = kotlin.math.abs(x - downX)
                    val totalDy = kotlin.math.abs(y - downY)
                    if (!selectingDrag) {
                        if (totalDx < threshold && totalDy < threshold) {
                            return true
                        }
                        selectingDrag = true
                    }
                    CoreBridge.nativeEditorSelectAt(surfaceId, x, y, true)
                    refreshSelectionFromNative()
                    if (longPressWordStart >= 0 && longPressWordEnd >= longPressWordStart) {
                        val currSel = mirrorSelection
                        if (currSel in (longPressWordStart + 1) until longPressWordEnd) {
                            selectNative(longPressWordStart, longPressWordEnd)
                        }
                    }
                    updateMagnifier(x, y - handleRowHeight / 2f)
                    requestRender()
                    return true
                }

                val threshold = 8f * resources.displayMetrics.density
                var deltaY = lastMotionY - y
                if (!dragged) {
                    val totalDy = y - downY
                    val totalDx = x - downX
                    if (kotlin.math.abs(totalDy) > threshold || kotlin.math.abs(totalDx) > threshold) {
                        dragged = true
                        removeCallbacks(longPress)
                        if (deltaY > 0f) {
                            deltaY = (deltaY - threshold).coerceAtLeast(0f)
                        } else {
                            deltaY = (deltaY + threshold).coerceAtMost(0f)
                        }
                    }
                }
                if (dragged) {
                    lastMotionY = y
                    val maxScroll = maxOf(scrollMaxY, knownScroll)
                    val nextScroll = (knownScroll + deltaY).coerceIn(0f, maxScroll).roundToInt().toFloat()
                    if (nextScroll != knownScroll) {
                        knownScroll = nextScroll
                        applyScrollTranslation()
                        onScrollChanged?.invoke(knownScroll, scrollMaxY, true)
                        scrollbarOverlay.update(knownScroll, scrollMaxY)
                        checkBandRender(knownScroll)
                    }
                }
            }
            android.view.MotionEvent.ACTION_POINTER_DOWN -> {
                val index = event.actionIndex
                activePointerId = event.getPointerId(index)
                lastMotionY = event.getY(index)
                downX = event.getX(index)
                downY = lastMotionY
            }
            android.view.MotionEvent.ACTION_POINTER_UP -> {
                val actionIndex = event.actionIndex
                if (event.getPointerId(actionIndex) == activePointerId) {
                    val newIndex = if (actionIndex == 0) 1 else 0
                    activePointerId = event.getPointerId(newIndex)
                    lastMotionY = event.getY(newIndex)
                    downX = event.getX(newIndex)
                    downY = lastMotionY
                }
            }
            android.view.MotionEvent.ACTION_UP -> {
                dismissMagnifier()
                removeCallbacks(longPress)
                velocityTracker?.addMovement(event)
                activePointerId = android.view.MotionEvent.INVALID_POINTER_ID
                if (draggingHandle >= 0) {
                    val releasedHandle = draggingHandle
                    val wasDragged = dragged
                    CoreBridge.nativeEditorDragHandle(
                        surfaceId,
                        draggingHandle,
                        event.x - dragOffsetX,
                        event.y - dragOffsetY,
                    )
                    draggingHandle = -1
                    recycleVelocity()
                    syncFromNative(notify = true)
                    saveViewport()
                    requestRender()
                    if (selectionNonEmpty()) {
                        showSelectionActions()
                    } else {
                        if (!wasDragged && releasedHandle == 2) {
                            showInsertionActions()
                        }
                        applyInsertionHandleVisible(true)
                    }
                    return true
                }
                if (selecting) {
                    dismissMagnifier()
                    val wasDragging = selectingDrag
                    val wordStart = longPressWordStart
                    val wordEnd = longPressWordEnd
                    selecting = false
                    selectingDrag = false
                    longPressWordStart = -1
                    longPressWordEnd = -1
                    recycleVelocity()
                    if (!wasDragging) {
                        if (wordStart in 0..wordEnd && wordEnd <= mirrorText.length) {
                            selectNative(wordStart, wordEnd)
                        }
                    } else {
                        syncFromNative(notify = true)
                    }
                    saveViewport()
                    requestRender()
                    showSelectionActions()
                    return true
                }
                if (!dragged) {
                    CoreBridge.nativeEditorSetScroll(surfaceId, knownScroll, 0f)
                    val now = SystemClock.uptimeMillis()
                    val doubleTap = now - lastTapTime < 320L &&
                        kotlin.math.abs(event.x - lastTapX) < slop &&
                        kotlin.math.abs(event.y - lastTapY) < slop
                    lastTapTime = now
                    lastTapX = event.x
                    lastTapY = event.y
                    if (doubleTap) {
                        CoreBridge.nativeEditorSelectWord(surfaceId, event.x, event.y)
                        syncSelectionFromNative()
                        requestFocus()
                        requestRender()
                        helper.invalidateRoot()
                        showSelectionActions()
                        return true
                    } else {
                        val t0 = SystemClock.uptimeMillis()
                        CoreBridge.nativeEditorSelectAt(surfaceId, event.x, event.y, false)
                        val t1 = SystemClock.uptimeMillis()
                        syncSelectionFromNative()
                        val t2 = SystemClock.uptimeMillis()
                        if (selectionNonEmpty()) {
                            showSelectionActions()
                        } else {
                            actionMode?.finish()
                            insertionActionMode?.finish()
                            if (!inPreview) {
                                requestFocus()
                                startCaretBlink()
                                applyInsertionHandleVisible(true)
                                onTextChanged(mirrorText, mirrorSelection)
                                onCaretPlaced?.invoke()
                            } else {
                                startCaretBlink()
                            }
                        }
                        val t3 = SystemClock.uptimeMillis()
                        requestRender()
                        val t4 = SystemClock.uptimeMillis()
                        helper.invalidateRoot()
                        android.util.Log.d("VibecodiumTap", "Tap timing: selectAt=${t1-t0}ms, syncSel=${t2-t1}ms, actions=${t3-t2}ms, reqRender=${t4-t3}ms, total=${t4-t0}ms")
                        return true
                    }
                } else {
                    val tracker = velocityTracker
                    tracker?.computeCurrentVelocity(1000)
                    val vy = tracker?.yVelocity ?: 0f
                    recycleVelocity()
                    val minimum = ViewConfiguration.get(context).scaledMinimumFlingVelocity.toFloat()
                    if (kotlin.math.abs(vy) > minimum) {
                        startFling(vy)
                    } else {
                        resumeCaretBlink()
                        val scrollForRust = knownScroll
                        postRenderTask {
                            CoreBridge.nativeEditorSetScroll(surfaceId, scrollForRust, 0f)
                        }
                        val m = computeBandMargin(safeViewportHeight()).toFloat()
                        val maxOrigin = maxOf(scrollMaxY, knownScroll)
                        scheduleBandRender((knownScroll - m).coerceIn(0f, maxOrigin))
                        saveViewport()
                        helper.invalidateRoot()
                    }
                }
            }
            android.view.MotionEvent.ACTION_CANCEL -> {
                activePointerId = android.view.MotionEvent.INVALID_POINTER_ID
                dismissMagnifier()
                removeCallbacks(longPress)
                selecting = false
                selectingDrag = false
                longPressWordStart = -1
                longPressWordEnd = -1
                draggingHandle = -1
                recycleVelocity()
                if (dragged) {
                    resumeCaretBlink()
                    val scrollForRust = knownScroll
                    postRenderTask {
                        CoreBridge.nativeEditorSetScroll(surfaceId, scrollForRust, 0f)
                    }
                    val m = computeBandMargin(safeViewportHeight()).toFloat()
                    val maxOrigin = maxOf(scrollMaxY, knownScroll)
                    scheduleBandRender((knownScroll - m).coerceIn(0f, maxOrigin))
                    saveViewport()
                }
            }
        }
        return true
    }

    private fun handleAt(x: Float, y: Float): Int {
        val reach = 16f * resources.displayMetrics.density
        if (selectionNonEmpty()) {
            fun contains(handle: PointF?, isEnd: Boolean): Boolean {
                if (handle == null) return false
                val left = if (isEnd) handle.x - reach else handle.x - handleSize - reach
                val right = if (isEnd) handle.x + handleSize + reach else handle.x + reach
                return x in left..right &&
                    y in (handle.y - reach)..(handle.y + handleSize + reach)
            }
            val startHit = contains(startHandle, isEnd = false)
            val endHit = contains(endHandle, isEnd = true)
            return when {
                startHit && endHit -> if (kotlin.math.abs(x - (startHandle?.x ?: 0f)) <
                    kotlin.math.abs(x - (endHandle?.x ?: 0f))
                ) 0 else 1
                startHit -> 0
                endHit -> 1
                else -> -1
            }
        } else {
            val ins = insertionHandle ?: return -1
            val left = ins.x - handleSize / 2f - reach
            val right = ins.x + handleSize / 2f + reach
            val top = ins.y - 4f * resources.displayMetrics.density
            val bottom = ins.y + handleSize + reach
            return if (x in left..right && y in top..bottom) 2 else -1
        }
    }

    private fun refreshSelectionFromNative() {
        val state = runCatching { JSONObject(CoreBridge.nativeEditorSelectionState(surfaceId)) }
            .getOrNull() ?: return
        mirrorSelection = state.optInt("selection", mirrorSelection).coerceIn(0, mirrorText.length)
        mirrorAnchor = state.optInt("anchor", mirrorAnchor).coerceIn(0, mirrorText.length)
        syncHandles()
        actionMode?.invalidateContentRect()
        notifySelectionChanged()
        requestRender()
        helper.invalidateRoot()
    }

    private fun findActivity(): Activity? {
        var ctx: Context? = context
        while (ctx is ContextWrapper) {
            if (ctx is Activity) return ctx
            ctx = ctx.baseContext
        }
        return null
    }

    fun focusEditor() {
        if (inPreview) return
        requestFocus()
        applyCaretVisible(true)
        startCaretBlink()
        applyInsertionHandleVisible(true)
        syncHandles()
        requestRender()
        post {
            if (inPreview) return@post
            requestFocus()
            applyCaretVisible(true)
            startCaretBlink()
            applyInsertionHandleVisible(true)
            syncHandles()
            requestRender()
        }
    }

    fun showKeyboard() {
        if (inPreview) return
        focusEditor()
        val imm = context.getSystemService(Context.INPUT_METHOD_SERVICE) as? InputMethodManager
        val window = findActivity()?.window
        if (window != null) {
            WindowCompat.getInsetsController(window, this).show(WindowInsetsCompat.Type.ime())
        }
        imm?.showSoftInput(this, 0)
        post {
            if (inPreview) return@post
            if (window != null) {
                WindowCompat.getInsetsController(window, this).show(WindowInsetsCompat.Type.ime())
            }
            imm?.showSoftInput(this, 0)
        }
    }

    fun hideKeyboard() {
        val imm = context.getSystemService(Context.INPUT_METHOD_SERVICE) as? InputMethodManager
        windowToken?.let { imm?.hideSoftInputFromWindow(it, 0) }
        val window = findActivity()?.window
        if (window != null) {
            WindowCompat.getInsetsController(window, this).hide(WindowInsetsCompat.Type.ime())
        }
        post {
            windowToken?.let { imm?.hideSoftInputFromWindow(it, 0) }
            if (window != null) {
                WindowCompat.getInsetsController(window, this).hide(WindowInsetsCompat.Type.ime())
            }
        }
    }

    fun recalculateScrollMetrics() {
        postRenderTask {
            if (!isSurfaceAttached()) return@postRenderTask
            val info = runCatching { JSONObject(CoreBridge.nativeEditorViewport(surfaceId)) }.getOrNull()
            val newMaxY = info?.optDouble("max_y", 0.0)?.toFloat()?.coerceAtLeast(0f) ?: scrollMaxY
            val rustScroll = info?.optDouble("scroll", knownScroll.toDouble())?.toFloat() ?: knownScroll
            val targetScroll = rustScroll.coerceIn(0f, newMaxY)
            mainHandler.post {
                knownScroll = targetScroll
                scrollMaxY = newMaxY
                applyScrollTranslation()
                scrollbarOverlay.update(knownScroll, scrollMaxY)
                onScrollChanged?.invoke(knownScroll, scrollMaxY, false)
            }
        }
    }

    fun onKeyboardDismissed() {
        if (inPreview) return
        applyCaretVisible(true)
        startCaretBlink()
        applyInsertionHandleVisible(true)
        requestRender()
        recalculateScrollMetrics()
    }

    fun setOnCaretPlaced(listener: (() -> Unit)?) {
        if (onCaretPlaced === listener) return
        onCaretPlaced = listener
    }

    private fun recycleVelocity() {
        velocityTracker?.recycle()
        velocityTracker = null
    }

    private fun stopFling() {
        flinging = false
        scroller.abortAnimation()
        cancelFrame(flingRunnable)
    }

    private fun startFling(vy: Float) {
        val viewport = runCatching { JSONObject(CoreBridge.nativeEditorViewport(surfaceId)) }.getOrNull()
        val maxY = viewport?.optDouble("max_y", scrollMaxY.toDouble())?.toFloat() ?: scrollMaxY
        if (maxY <= 0f) return
        scrollMaxY = maxY
        removeCallbacks(saveAfterFling)
        suspendCaretBlink()
        scroller.abortAnimation()
        scroller.fling(
            0,
            knownScroll.roundToInt(),
            0,
            (-vy).roundToInt(),
            0, 0,
            0, maxY.roundToInt(),
        )
        flinging = true
        postFrame(flingRunnable)
    }

    override fun onDetachedFromWindow() {
        saveViewport()
        stopFling()
        recycleVelocity()
        dismissMagnifier()
        actionMode?.finish()
        insertionActionMode?.finish()
        removeCallbacks(longPress)
        removeCallbacks(caretBlink)
        removeCallbacks(saveAfterFling)
        removeCallbacks(hideInsertionRunnable)
        super.onDetachedFromWindow()
    }

    private fun notifySelectionChanged() {
        if (Looper.myLooper() == Looper.getMainLooper()) {
            val imm = context.getSystemService(Context.INPUT_METHOD_SERVICE) as? InputMethodManager
            imm?.updateSelection(this, mirrorAnchor, mirrorSelection, composingStart, composingEnd)
            if (extractedTextToken != 0) {
                val et = ExtractedText()
                et.startOffset = 0
                et.partialStartOffset = -1
                et.partialEndOffset = -1
                val len = mirrorText.length
                et.selectionStart = mirrorAnchor.coerceIn(0, len)
                et.selectionEnd = mirrorSelection.coerceIn(0, len)
                et.text = mirrorText
                imm?.updateExtractedText(this, extractedTextToken, et)
            }
        } else {
            mainHandler.post { notifySelectionChanged() }
        }
    }

    private fun syncSelectionFromNative() {
        val packed = CoreBridge.nativeEditorGetSelection(surfaceId)
        val selection = (packed ushr 32).toInt().coerceIn(0, mirrorText.length)
        val anchor = (packed.toInt()).coerceIn(0, mirrorText.length)
        mirrorSelection = selection
        mirrorAnchor = anchor
        syncHandles()
        actionMode?.invalidateContentRect()
        notifySelectionChanged()
    }

    private fun syncFromNative(notify: Boolean) {
        val state = runCatching { JSONObject(CoreBridge.nativeEditorState(surfaceId)) }.getOrNull() ?: return
        val text = state.optString("text")
        val selection = state.optInt("selection", 0).coerceIn(0, text.length)
        mirrorText = text
        mirrorSelection = selection
        mirrorAnchor = state.optInt("anchor", selection).coerceIn(0, text.length)
        refreshLines()
        syncHandles()
        actionMode?.invalidateContentRect()
        notifySelectionChanged()
        if (notify) onTextChanged(text, selection)
    }

    private fun updateScrollMetricsFromNative() {
        val info = runCatching { JSONObject(CoreBridge.nativeEditorViewport(surfaceId)) }.getOrNull()
        val newMaxY = info?.optDouble("max_y", 0.0)?.toFloat()?.coerceAtLeast(0f) ?: scrollMaxY
        scrollMaxY = newMaxY
        scrollbarOverlay.update(knownScroll, scrollMaxY)
        onScrollChanged?.invoke(knownScroll, scrollMaxY, false)
    }

    private fun applyInsert(text: String) {
        applyInsertionHandleVisible(false)
        if (text.isEmpty()) {
            if (selectionNonEmpty()) applyDelete(0, 0)
            return
        }
        val state = runCatching { JSONObject(CoreBridge.nativeEditorInsert(surfaceId, text)) }.getOrNull()
            ?: return
        mirrorText = state.optString("text")
        mirrorSelection = state.optInt("selection", 0).coerceIn(0, mirrorText.length)
        mirrorAnchor = mirrorSelection
        refreshLines()
        syncHandles()
        actionMode?.finish()
        startCaretBlink()
        onTextChanged(mirrorText, mirrorSelection)
        knownScroll = CoreBridge.nativeEditorScroll(surfaceId).roundToInt().toFloat()
        applyScrollTranslation()
        updateScrollMetricsFromNative()
        checkBandRender(knownScroll)
        notifySelectionChanged()
        requestRender()
    }

    private fun applyReplace(start: Int, end: Int, text: String) {
        applyInsertionHandleVisible(false)
        val state = runCatching {
            JSONObject(CoreBridge.nativeEditorReplace(surfaceId, start, end, text))
        }.getOrNull() ?: return
        mirrorText = state.optString("text")
        mirrorSelection = state.optInt("selection", 0).coerceIn(0, mirrorText.length)
        mirrorAnchor = mirrorSelection
        refreshLines()
        syncHandles()
        actionMode?.finish()
        startCaretBlink()
        onTextChanged(mirrorText, mirrorSelection)
        knownScroll = CoreBridge.nativeEditorScroll(surfaceId).roundToInt().toFloat()
        applyScrollTranslation()
        updateScrollMetricsFromNative()
        checkBandRender(knownScroll)
        notifySelectionChanged()
        requestRender()
    }

    private fun applyDelete(before: Int, after: Int) {
        applyInsertionHandleVisible(false)
        val state = runCatching { JSONObject(CoreBridge.nativeEditorDelete(surfaceId, before, after)) }
            .getOrNull() ?: return
        mirrorText = state.optString("text")
        mirrorSelection = state.optInt("selection", 0).coerceIn(0, mirrorText.length)
        mirrorAnchor = mirrorSelection
        refreshLines()
        syncHandles()
        actionMode?.finish()
        startCaretBlink()
        onTextChanged(mirrorText, mirrorSelection)
        knownScroll = CoreBridge.nativeEditorScroll(surfaceId).roundToInt().toFloat()
        applyScrollTranslation()
        updateScrollMetricsFromNative()
        checkBandRender(knownScroll)
        notifySelectionChanged()
        requestRender()
    }
}
