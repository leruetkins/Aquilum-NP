package md.vibecodium.mobile

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.graphics.PointF
import android.graphics.Rect
import android.os.Bundle
import android.os.SystemClock
import android.text.InputType
import android.view.ActionMode
import android.view.Choreographer
import android.view.KeyEvent
import android.view.Menu
import android.view.MenuItem
import android.view.MotionEvent
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.VelocityTracker
import android.view.View
import android.view.ViewConfiguration
import android.view.accessibility.AccessibilityNodeInfo
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputMethodManager
import androidx.core.view.ViewCompat
import androidx.core.view.accessibility.AccessibilityNodeInfoCompat
import androidx.customview.widget.ExploreByTouchHelper
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

/**
 * Shared SurfaceView plumbing: one native ANativeWindow per view and a single
 * render thread so JNI never blocks the UI thread with buffer locks.
 */
abstract class NativeSurfaceView(context: Context) : SurfaceView(context), SurfaceHolder.Callback {
    val surfaceId: Int = NEXT_ID.incrementAndGet()

    private val renderer: java.util.concurrent.Executor get() = RENDERER
    private val renderQueued = AtomicBoolean(false)
    private val scrollQueued = AtomicBoolean(false)
    private val scrollEpoch = AtomicInteger(0)
    private val pendingScroll = AtomicReference<ScrollRequest?>(null)

    private data class ScrollRequest(val y: Float, val x: Float, val epoch: Int)

    @Volatile
    private var attached = false

    @Volatile
    private var pendingState: (() -> Unit)? = null

    init {
        holder.addCallback(this)
        setWillNotDraw(true)
        isFocusable = true
        isFocusableInTouchMode = true
    }

    override fun surfaceCreated(holder: SurfaceHolder) {
        renderer.execute {
            val density = resources.displayMetrics.density
            val frame = holder.surfaceFrame
            val width = frame.width().takeIf { it > 0 } ?: this.width.coerceAtLeast(1)
            val height = frame.height().takeIf { it > 0 } ?: this.height.coerceAtLeast(1)
            CoreBridge.nativeSurfaceAttach(surfaceId, holder.surface, width, height, density)
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
        }
    }

    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
        renderer.execute {
            if (!attached) return@execute
            CoreBridge.nativeSurfaceBeginUpdate(surfaceId)
            CoreBridge.nativeSurfaceResize(surfaceId, width.coerceAtLeast(1), height.coerceAtLeast(1))
            pendingState?.let { update ->
                pendingState = null
                update()
            }
            CoreBridge.nativeSurfaceEndUpdate(surfaceId)
        }
    }

    override fun surfaceDestroyed(holder: SurfaceHolder) {
        renderer.execute {
            if (attached) {
                CoreBridge.nativeSurfaceDetach(surfaceId)
                attached = false
            }
        }
    }

    protected fun postState(block: () -> Unit) {
        renderer.execute {
            if (attached) {
                pendingState = null
                block()
                CoreBridge.nativeSurfaceRender(surfaceId)
            } else {
                pendingState = block
            }
        }
    }

    /** Re-publishes Kotlin-owned state after a Surface is recreated. */
    protected open fun onSurfaceReady() = Unit

    protected fun isSurfaceAttached(): Boolean = attached

    private val vsyncScheduled = AtomicBoolean(false)
    private val vsyncRender = Choreographer.FrameCallback {
        vsyncScheduled.set(false)
        if (!renderQueued.compareAndSet(false, true)) return@FrameCallback
        renderer.execute {
            renderQueued.set(false)
            if (attached) CoreBridge.nativeSurfaceRender(surfaceId)
        }
    }

    /**
     * Renders on the next display frame. Input events arrive at arbitrary
     * phases; posting immediately can miss a vsync and produce a periodic
     * stutter even when the scroll delta itself is smooth.
     */
    protected fun requestRender() {
        if (!vsyncScheduled.compareAndSet(false, true)) return
        Choreographer.getInstance().postFrameCallback(vsyncRender)
    }

    /** Redraws every pixel even when the native scroll and size are unchanged. */
    protected fun requestFullRender() {
        renderer.execute {
            if (attached) CoreBridge.nativeSurfaceInvalidate(surfaceId)
        }
    }

    /** Applies the latest scroll on the render thread without blocking the UI thread. */
    protected fun submitScroll(y: Float, x: Float = 0f) {
        pendingScroll.set(ScrollRequest(y, x, scrollEpoch.get()))
        scheduleScroll()
    }

    private fun scheduleScroll() {
        if (!scrollQueued.compareAndSet(false, true)) return
        renderer.execute {
            scrollQueued.set(false)
            val request = pendingScroll.getAndSet(null)
            if (request != null && attached && request.epoch == scrollEpoch.get()) {
                applyNativeScroll(request.y, request.x)
                CoreBridge.nativeSurfaceRender(surfaceId)
            }
            if (pendingScroll.get() != null) scheduleScroll()
        }
    }

    /** Flushes the last fling tick before a touch so Rust's drag base matches. */
    protected fun flushPendingScroll() {
        val request = pendingScroll.getAndSet(null)
        if (request != null && attached && request.epoch == scrollEpoch.get()) {
            applyNativeScroll(request.y, request.x)
        }
        scrollEpoch.incrementAndGet()
    }

    protected open fun applyNativeScroll(y: Float, x: Float) = Unit

    /** Runs a background task in the same serial order as native rendering. */
    protected fun postRenderTask(task: () -> Unit) {
        renderer.execute(task)
    }

    override fun onDetachedFromWindow() {
        super.onDetachedFromWindow()
        // surfaceDestroyed is responsible for the native window; the executor
        // is left alive until the view is fully detached so that queued state
        // updates still run in order.
    }

    private companion object {
        val NEXT_ID = AtomicInteger(1)
        val RENDERER = Executors.newSingleThreadExecutor { runnable ->
            Thread(runnable, "native-surface").apply { isDaemon = true }
        }
    }
}

/**
 * Time-based cubic ease-out with a finite velocity at the end. Distance grows
 * with the measured fling velocity (no fixed px/s cap), so fast gestures can
 * travel much farther through long documents while the tail still reaches 0
 * exactly at [duration].
 */
private class SmoothFling {
    private var startScroll = 0f
    private var total = 0.0
    private var duration = 0.0
    private var startNanos = 0L
    private val power = 3.0
    private val maxDuration = 4.0

    fun start(startScroll: Float, velocity: Double) {
        this.startScroll = startScroll
        val speed = kotlin.math.abs(velocity).coerceAtLeast(200.0)
        duration = (0.45 + speed / 9000.0 * 1.6).coerceAtMost(maxDuration)
        total = velocity.coerceIn(-60000.0, 60000.0) * duration / power
        startNanos = System.nanoTime()
    }

    fun position(now: Long): Float {
        val progress = (((now - startNanos) / 1_000_000_000.0) / duration)
            .coerceIn(0.0, 1.0)
        val distance = total * (1.0 - Math.pow(1.0 - progress, power))
        return (startScroll + distance).toFloat()
    }

    fun finished(now: Long): Boolean =
        (now - startNanos) / 1_000_000_000.0 >= duration
}

/** Native explorer. Kotlin computes the visible rows; Rust paints and hit-tests. */
class NativeTreeView(
    context: Context,
    private val onAction: (String) -> Unit,
    private val onTouchStart: () -> Unit = {},
) : NativeSurfaceView(context) {
    private data class TreeRowInfo(
        val name: String,
        val path: String,
        val folder: Boolean,
        val note: Boolean,
    )

    private var rows: List<TreeRowInfo> = emptyList()
    private var lastJson: String? = null

    private val helper: ExploreByTouchHelper by lazy {
        object : ExploreByTouchHelper(this) {
            private fun rowHeight() = 36f * resources.displayMetrics.density

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

    private val treeSmooth = SmoothFling()
    private var treeMaxScroll = 0f
    private var velocityTracker: VelocityTracker? = null
    private var treeFlinging = false
    private val treeFling = object : Choreographer.FrameCallback {
        override fun doFrame(frameTimeNanos: Long) {
            if (!treeFlinging) return
            val raw = treeSmooth.position(frameTimeNanos)
            val clamped = raw.coerceIn(0f, treeMaxScroll)
            submitScroll(clamped)
            if (treeSmooth.finished(frameTimeNanos) || kotlin.math.abs(raw - clamped) > 0.5f) {
                treeFlinging = false
                postDelayed({ helper.invalidateRoot() }, 100L)
            } else {
                Choreographer.getInstance().postFrameCallback(this)
            }
        }
    }

    private var drawerVisible = false
    private var drawerTouchFloor = 0L

    init {
        setZOrderMediaOverlay(true)
        ViewCompat.setAccessibilityDelegate(this, helper)
    }

    /**
     * The drawer stays in the hierarchy while closed (to keep this surface
     * warm). On the first open its surface can still be positioned off-screen,
     * so force a layout and a full redraw after it moves in.
     */
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

    override fun applyNativeScroll(y: Float, x: Float) {
        CoreBridge.nativeTreeSetScroll(surfaceId, y)
    }

    override fun dispatchHoverEvent(event: MotionEvent): Boolean {
        return helper.dispatchHoverEvent(event) || super.dispatchHoverEvent(event)
    }

    override fun onTouchEvent(event: android.view.MotionEvent): Boolean {
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
        } else if (action == 1 || action == 2) {
            velocityTracker?.addMovement(event)
        }
        val result = if (action in 0..2) {
            CoreBridge.nativeSurfaceTouch(surfaceId, event.x, event.y, action)
        } else {
            ""
        }
        if (action == 2) {
            if (result.isNotEmpty()) {
                post { onAction(result) }
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

    private fun stopTreeFling() {
        treeFlinging = false
        Choreographer.getInstance().removeFrameCallback(treeFling)
        flushPendingScroll()
    }

    private fun startTreeFling(vy: Float) {
        val viewport = runCatching { JSONObject(CoreBridge.nativeTreeViewport(surfaceId)) }.getOrNull()
            ?: return
        val scroll = viewport.optDouble("scroll", 0.0)
        val maxY = viewport.optDouble("max_y", 0.0)
        if (maxY <= 0.0) return
        treeMaxScroll = maxY.toFloat()
        treeSmooth.start(scroll.toFloat(), (-vy).toDouble())
        treeFlinging = true
        Choreographer.getInstance().postFrameCallback(treeFling)
    }

    override fun onDetachedFromWindow() {
        stopTreeFling()
        recycleTreeVelocity()
        super.onDetachedFromWindow()
    }

    fun setRows(json: String) {
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
    private var dragged = false
    private var selecting = false
    private var lastTapTime = 0L
    private var lastTapX = 0f
    private var lastTapY = 0f
    private var layoutRowHeight = 22f * resources.displayMetrics.density
    private var layoutTop = 0f
    private var visualFirst = IntArray(0)
    private var visualCount = IntArray(0)
    private var startHandle: PointF? = null
    private var endHandle: PointF? = null
    private var handleSize = 24f * resources.displayMetrics.density
    private var handleRowHeight = 22f * resources.displayMetrics.density
    private var draggingHandle = -1
    private var dragOffsetX = 0f
    private var dragOffsetY = 0f
    private var actionMode: ActionMode? = null
    private var bottomInset = -1f
    private var topInset = -1f
    private var inPreview = false
    private var caretVisible = true
    private val caretBlink = object : Runnable {
        override fun run() {
            if (!isFocused || selectionNonEmpty()) {
                applyCaretVisible(true)
                return
            }
            applyCaretVisible(!caretVisible)
            postDelayed(this, 500L)
        }
    }
    private val smoothFling = SmoothFling()
    private val saveAfterFling = Runnable { postRenderTask { saveViewport() } }
    private var flinging = false
    private var velocityTracker: VelocityTracker? = null
    private var scrollMaxY = 0f
    private var dragStartScroll = 0f
    private var onScrollChanged: ((Float, Float) -> Unit)? = null
    private var onCaretPlaced: (() -> Unit)? = null
    @Volatile private var knownScroll = 0f
    private var wasHidden = false
    private val flingRunnable = object : Choreographer.FrameCallback {
        override fun doFrame(frameTimeNanos: Long) {
            if (!flinging) return
            val raw = smoothFling.position(frameTimeNanos)
            val clamped = raw.coerceIn(0f, scrollMaxY)
            knownScroll = clamped
            submitScroll(clamped, 0f)
            onScrollChanged?.invoke(clamped, scrollMaxY)
            if (smoothFling.finished(frameTimeNanos) ||
                kotlin.math.abs(raw - clamped) > 0.5f
            ) {
                flinging = false
                resumeCaretBlink()
                postDelayed({ requestFullRender() }, 80L)
                removeCallbacks(saveAfterFling)
                postDelayed(saveAfterFling, 400L)
                postDelayed({ helper.invalidateRoot() }, 100L)
            } else {
                Choreographer.getInstance().postFrameCallback(this)
            }
        }
    }
    private var composingStart = -1
    private var composingEnd = -1
    private var lineStarts = intArrayOf(0)
    private val longPress = Runnable {
        if (!dragged) {
            selecting = true
            CoreBridge.nativeEditorSelectWord(surfaceId, downX, downY)
            syncFromNative(notify = true)
            requestRender()
            helper.invalidateRoot()
            showSelectionActions()
        }
    }

    private val selectionCallback = object : ActionMode.Callback2() {
        override fun onCreateActionMode(mode: ActionMode, menu: Menu): Boolean {
            if (selectionNonEmpty()) {
                menu.add(Menu.NONE, MENU_COPY, 0, "Copy")
                menu.add(Menu.NONE, MENU_CUT, 1, "Cut")
            }
            menu.add(Menu.NONE, MENU_PASTE, 2, "Paste")
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
        setZOrderMediaOverlay(true)
        ViewCompat.setAccessibilityDelegate(this, helper)
        bottomInset = idleBottomInsetPx
    }

    fun setOnScrollChanged(listener: ((Float, Float) -> Unit)?) {
        onScrollChanged = listener
    }

    fun setBottomInset(inset: Float) {        val next = inset.coerceAtLeast(0f)
        if (kotlin.math.abs(next - bottomInset) < 0.5f) return
        bottomInset = next
        CoreBridge.nativeEditorSetBottomInset(surfaceId, next)
        knownScroll = CoreBridge.nativeEditorScroll(surfaceId)
        requestRender()
    }

    fun setTopInset(inset: Float) {
        val next = inset.coerceAtLeast(0f)
        if (kotlin.math.abs(next - topInset) < 0.5f) return
        topInset = next
        CoreBridge.nativeEditorSetTopInset(surfaceId, next)
        requestRender()
    }

    fun setEditorPreview(preview: Boolean) {
        if (preview == inPreview) return
        inPreview = preview
        if (!preview) {
            requestFullRender()
            return
        }
        stopFling()
        recycleVelocity()
        actionMode?.finish()
        stopCaretBlink()
        clearFocus()
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
        if (isFocused && !selectionNonEmpty()) {
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
        if (!isFocused || selectionNonEmpty()) return
        applyCaretVisible(true)
        postDelayed(caretBlink, 500L)
    }

    override fun onFocusChanged(
        gainFocus: Boolean,
        direction: Int,
        previouslyFocusedRect: Rect?,
    ) {
        super.onFocusChanged(gainFocus, direction, previouslyFocusedRect)
        if (gainFocus) {
            startCaretBlink()
        } else {
            stopCaretBlink()
            actionMode?.finish()
        }
    }

    override fun onWindowFocusChanged(hasWindowFocus: Boolean) {
        super.onWindowFocusChanged(hasWindowFocus)
        if (!hasWindowFocus) {
            saveViewport()
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
            val imm = context.getSystemService(Context.INPUT_METHOD_SERVICE) as? InputMethodManager
            windowToken?.let { imm?.hideSoftInputFromWindow(it, 0) }
        } else if (wasHidden) {
            wasHidden = false
            restoreViewport()
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
            22.0 * resources.displayMetrics.density,
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
        caretVisible = true
        CoreBridge.nativeEditorSetCaretVisible(surfaceId, true)
        lastPath?.let { path ->
            loadViewport(path)?.let { (scroll, scrollX) ->
                CoreBridge.nativeEditorSetScroll(surfaceId, scroll, scrollX)
            }
        }
        knownScroll = CoreBridge.nativeEditorScroll(surfaceId)
        if (isFocused && !selectionNonEmpty()) startCaretBlink()
        syncHandles()
    }

    override fun applyNativeScroll(y: Float, x: Float) {
        CoreBridge.nativeEditorSetScroll(surfaceId, y, x)
        knownScroll = y
    }

    fun setEditor(text: String, selection: Int, language: String?, path: String) {
        val nextLanguage = language.orEmpty()
        val pathChanged = lastPath != path
        if (!pathChanged && mirrorText == text && mirrorSelection == selection && lastLanguage == nextLanguage) return
        if (pathChanged) {
            saveViewport()
            lastPath = path
        }
        val previous = mirrorSelection
        val previousText = mirrorText
        mirrorText = text
        mirrorSelection = selection.coerceIn(0, text.length)
        if (mirrorSelection != previous || previousText != text) {
            mirrorAnchor = mirrorSelection
        }
        lastLanguage = nextLanguage
        refreshLines()
        val viewport = if (pathChanged) loadViewport(path) else null
        if (pathChanged) {
            knownScroll = viewport?.first ?: 0f
        }
        contentDescription = if (nextLanguage.isEmpty()) {
            "Markdown source editor, ${text.count { it == '\n' } + 1} lines"
        } else {
            "Code source editor ($nextLanguage), ${text.count { it == '\n' } + 1} lines"
        }
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
            knownScroll = CoreBridge.nativeEditorScroll(surfaceId)
        }
    }

    private fun saveViewport() {
        val path = lastPath ?: return
        if (path.isEmpty()) return
        val viewport = runCatching { JSONObject(CoreBridge.nativeEditorViewport(surfaceId)) }
            .getOrNull()
        val nativeScroll = viewport?.optDouble("scroll", 0.0)?.toFloat()
        // When the app leaves the foreground the Surface can already be
        // recreated with a fresh zero scroll. Keep the last real position
        // instead of overwriting the stored viewport with that zero.
        val scroll = when {
            nativeScroll == null -> knownScroll
            nativeScroll <= 0.5f && knownScroll > 0.5f -> knownScroll
            else -> nativeScroll
        }
        val scrollX = viewport?.optDouble("scroll_x", 0.0)?.toFloat() ?: 0f
        knownScroll = scroll
        prefs.edit()
            .putString("viewport:$path", "$scroll,$scrollX")
            .apply()
    }

    /** Reapplies the stored position after the app returns from the background. */
    private fun restoreViewport() {
        if (flinging) return
        val path = lastPath ?: return
        val viewport = loadViewport(path)
        val scroll = viewport?.first ?: knownScroll
        val scrollX = viewport?.second ?: 0f
        knownScroll = scroll
        postRenderTask {
            if (!isSurfaceAttached()) return@postRenderTask
            CoreBridge.nativeEditorSetScroll(surfaceId, scroll, scrollX)
            CoreBridge.nativeSurfaceRender(surfaceId)
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
        return super.onKeyDown(keyCode, event)
    }

    override fun onCreateInputConnection(outAttrs: EditorInfo): InputConnection {
        outAttrs.inputType = InputType.TYPE_CLASS_TEXT or
            InputType.TYPE_TEXT_FLAG_MULTI_LINE or
            InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
        outAttrs.imeOptions = EditorInfo.IME_FLAG_NO_EXTRACT_UI or
            EditorInfo.IME_FLAG_NO_FULLSCREEN or
            EditorInfo.IME_ACTION_NONE
        return object : BaseInputConnection(this@NativeEditorView, false) {
            override fun commitText(text: CharSequence?, newCursorPosition: Int): Boolean {
                val value = text?.toString().orEmpty()
                if (composingStart >= 0) {
                    applyReplace(composingStart, composingEnd, value)
                    composingStart = -1
                    composingEnd = -1
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
                    } else {
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

            override fun finishComposingText(): Boolean {
                composingStart = -1
                composingEnd = -1
                return true
            }

            override fun deleteSurroundingText(beforeLength: Int, afterLength: Int): Boolean {
                composingStart = -1
                composingEnd = -1
                applyDelete(beforeLength, afterLength)
                return true
            }

            override fun sendKeyEvent(event: KeyEvent): Boolean {
                if (event.action != KeyEvent.ACTION_DOWN) return true
                return when (event.keyCode) {
                    KeyEvent.KEYCODE_DEL -> {
                        applyDelete(1, 0)
                        true
                    }
                    KeyEvent.KEYCODE_ENTER -> {
                        applyInsert("\n")
                        true
                    }
                    else -> false
                }
            }

            override fun getTextBeforeCursor(n: Int, flags: Int): CharSequence {
                val end = mirrorSelection.coerceIn(0, mirrorText.length)
                return mirrorText.substring((end - n).coerceAtLeast(0), end)
            }

            override fun getTextAfterCursor(n: Int, flags: Int): CharSequence {
                val start = mirrorSelection.coerceIn(0, mirrorText.length)
                return mirrorText.substring(start, (start + n).coerceAtMost(mirrorText.length))
            }

            override fun getSelectedText(flags: Int): CharSequence? {
                val start = mirrorAnchor.coerceIn(0, mirrorText.length)
                val end = mirrorSelection.coerceIn(0, mirrorText.length)
                if (start == end) return null
                return mirrorText.substring(minOf(start, end), maxOf(start, end))
            }

            override fun setSelection(start: Int, end: Int): Boolean {
                composingStart = -1
                composingEnd = -1
                selectNative(start, end)
                return true
            }
        }
    }

    private fun selectionNonEmpty(): Boolean = mirrorAnchor != mirrorSelection

    private fun syncHandles() {
        if (!selectionNonEmpty()) {
            startHandle = null
            endHandle = null
            return
        }
        val info = runCatching { JSONObject(CoreBridge.nativeEditorSelectionHandles(surfaceId)) }
            .getOrNull() ?: return
        val start = info.optJSONObject("start")
        val end = info.optJSONObject("end")
        if (start == null || end == null) {
            startHandle = null
            endHandle = null
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
        handleSize = info.optDouble("size", handleSize.toDouble()).toFloat()
        handleRowHeight = info.optDouble("row_height", handleRowHeight.toDouble()).toFloat()
    }

    private fun showSelectionActions() {
        if (!selectionNonEmpty()) {
            actionMode?.finish()
            return
        }
        stopCaretBlink()
        syncHandles()
        val current = actionMode
        if (current == null) {
            actionMode = startActionMode(selectionCallback, ActionMode.TYPE_FLOATING)
        } else {
            current.invalidate()
            current.invalidateContentRect()
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
        requestRender()
        helper.invalidateRoot()
    }

    override fun onTouchEvent(event: android.view.MotionEvent): Boolean {
        val action = when (event.actionMasked) {
            android.view.MotionEvent.ACTION_DOWN -> 0
            android.view.MotionEvent.ACTION_MOVE -> 1
            android.view.MotionEvent.ACTION_UP -> 2
            android.view.MotionEvent.ACTION_CANCEL -> -1
            else -> return true
        }
        val slop = 8f * resources.displayMetrics.density
        when (action) {
            0 -> {
                stopFling()
                suspendCaretBlink()
                downX = event.x
                downY = event.y
                dragged = false
                selecting = false
                removeCallbacks(longPress)
                val viewport = runCatching {
                    JSONObject(CoreBridge.nativeEditorViewport(surfaceId))
                }.getOrNull()
                scrollMaxY = viewport?.optDouble("max_y", 0.0)?.toFloat() ?: 0f
                dragStartScroll = viewport
                    ?.optDouble("scroll", knownScroll.toDouble())
                    ?.toFloat()
                    ?: knownScroll
                syncHandles()
                val handle = handleAt(event.x, event.y)
                if (handle >= 0) {
                    draggingHandle = handle
                    val point = if (handle == 0) startHandle else endHandle
                    if (point != null) {
                        dragOffsetX = event.x - point.x
                        dragOffsetY = event.y - point.y
                    }
                    actionMode?.finish()
                    velocityTracker?.recycle()
                    velocityTracker = null
                    return true
                }
                postDelayed(longPress, android.view.ViewConfiguration.getLongPressTimeout().toLong())
                velocityTracker?.recycle()
                velocityTracker = VelocityTracker.obtain().also { it.addMovement(event) }
            }
            1 -> {
                velocityTracker?.addMovement(event)
                if (draggingHandle >= 0) {
                    CoreBridge.nativeEditorDragHandle(
                        surfaceId,
                        draggingHandle,
                        event.x - dragOffsetX,
                        event.y - dragOffsetY,
                    )
                    refreshSelectionFromNative()
                    saveViewport()
                    return true
                }
                if (selecting) {
                    CoreBridge.nativeEditorSelectAt(surfaceId, event.x, event.y, true)
                    refreshSelectionFromNative()
                    requestRender()
                    return true
                }
                if (kotlin.math.abs(event.y - downY) > slop ||
                    kotlin.math.abs(event.x - downX) > slop
                ) {
                    dragged = true
                    removeCallbacks(longPress)
                }
            }
            2 -> {
                removeCallbacks(longPress)
                velocityTracker?.addMovement(event)
                if (draggingHandle >= 0) {
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
                    showSelectionActions()
                    return true
                }
                if (selecting) {
                    CoreBridge.nativeEditorSelectAt(surfaceId, event.x, event.y, true)
                    selecting = false
                    recycleVelocity()
                    syncFromNative(notify = true)
                    saveViewport()
                    requestRender()
                    showSelectionActions()
                    return true
                }
            }
            -1 -> {
                removeCallbacks(longPress)
                selecting = false
                draggingHandle = -1
                recycleVelocity()
            }
        }
        if (action in 0..2) {
            CoreBridge.nativeSurfaceTouch(surfaceId, event.x, event.y, action)
        }
        if (action == 1 && draggingHandle < 0 && !selecting) {
            // Mirror the native drag formula instead of asking Rust for the
            // scroll: a JNI read locks the surface mutex that the paint holds,
            // which stalled the UI thread for every move of the finger.
            val dy = event.y - downY
            val dx = event.x - downX
            val threshold = 8f * resources.displayMetrics.density
            if (kotlin.math.abs(dy) > threshold && kotlin.math.abs(dx) <= kotlin.math.abs(dy)) {
                val adjusted = dy - kotlin.math.sign(dy) * threshold
                knownScroll = (dragStartScroll - adjusted).coerceIn(0f, scrollMaxY)
                onScrollChanged?.invoke(knownScroll, scrollMaxY)
            }
        }
        if (action == 2 && !dragged) {
            val now = SystemClock.uptimeMillis()
            val doubleTap = now - lastTapTime < 320L &&
                kotlin.math.abs(event.x - lastTapX) < slop &&
                kotlin.math.abs(event.y - lastTapY) < slop
            lastTapTime = now
            lastTapX = event.x
            lastTapY = event.y
            if (doubleTap) {
                CoreBridge.nativeEditorSelectWord(surfaceId, event.x, event.y)
                syncFromNative(notify = true)
                requestRender()
                helper.invalidateRoot()
                showSelectionActions()
                return true
            }
            syncFromNative(notify = true)
            saveViewport()
            if (selectionNonEmpty()) {
                showSelectionActions()
            } else {
                actionMode?.finish()
                startCaretBlink()
                onCaretPlaced?.invoke()
            }
        } else if (action == 2 && dragged) {
            val tracker = velocityTracker
            tracker?.computeCurrentVelocity(1000)
            val vy = tracker?.yVelocity ?: 0f
            recycleVelocity()
            val minimum = ViewConfiguration.get(context).scaledMinimumFlingVelocity.toFloat()
            if (kotlin.math.abs(vy) > minimum) {
                startFling(vy)
            }
        }
        if (action == 2) {
            // Guarantee a complete frame after a drag or tap, even if an
            // incremental scroll band was rendered from a stale buffer. The
            // repaint itself runs on the render thread; a fresh fling already
            // queued one, so do not stack a second full paint on top of it.
            if (!flinging) {
                resumeCaretBlink()
                requestFullRender()
            }
            helper.invalidateRoot()
        } else {
            requestRender()
        }
        return true
    }

    private fun handleAt(x: Float, y: Float): Int {
        if (!selectionNonEmpty()) return -1
        val reach = 8f * resources.displayMetrics.density
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
    }

    private fun refreshSelectionFromNative() {
        val state = runCatching { JSONObject(CoreBridge.nativeEditorSelectionState(surfaceId)) }
            .getOrNull() ?: return
        mirrorSelection = state.optInt("selection", mirrorSelection).coerceIn(0, mirrorText.length)
        mirrorAnchor = state.optInt("anchor", mirrorAnchor).coerceIn(0, mirrorText.length)
        syncHandles()
        actionMode?.invalidateContentRect()
        requestRender()
        helper.invalidateRoot()
    }

    fun showKeyboard() {
        requestFocus()
        val imm = context.getSystemService(Context.INPUT_METHOD_SERVICE) as? InputMethodManager
        imm?.showSoftInput(this, InputMethodManager.SHOW_IMPLICIT)
    }

    fun hideKeyboard() {
        clearFocus()
        val imm = context.getSystemService(Context.INPUT_METHOD_SERVICE) as? InputMethodManager
        windowToken?.let { imm?.hideSoftInputFromWindow(it, 0) }
    }

    fun setOnCaretPlaced(listener: (() -> Unit)?) {
        onCaretPlaced = listener
    }

    private fun recycleVelocity() {
        velocityTracker?.recycle()
        velocityTracker = null
    }

    private fun stopFling() {
        flinging = false
        Choreographer.getInstance().removeFrameCallback(flingRunnable)
        flushPendingScroll()
    }

    private fun startFling(vy: Float) {
        val viewport = runCatching { JSONObject(CoreBridge.nativeEditorViewport(surfaceId)) }.getOrNull()
            ?: return
        val scroll = viewport.optDouble("scroll", 0.0)
        val maxY = viewport.optDouble("max_y", 0.0)
        if (maxY <= 0.0) return
        scrollMaxY = maxY.toFloat()
        removeCallbacks(saveAfterFling)
        suspendCaretBlink()
        requestFullRender()
        smoothFling.start(scroll.toFloat(), (-vy).toDouble())
        flinging = true
        Choreographer.getInstance().postFrameCallback(flingRunnable)
    }

    override fun onDetachedFromWindow() {
        saveViewport()
        stopFling()
        recycleVelocity()
        removeCallbacks(longPress)
        removeCallbacks(caretBlink)
        removeCallbacks(saveAfterFling)
        super.onDetachedFromWindow()
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
        if (notify) onTextChanged(text, selection)
    }

    private fun applyInsert(text: String) {
        if (text.isEmpty()) return
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
        knownScroll = CoreBridge.nativeEditorScroll(surfaceId)
        requestRender()
    }

    private fun applyReplace(start: Int, end: Int, text: String) {
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
        knownScroll = CoreBridge.nativeEditorScroll(surfaceId)
        requestRender()
    }

    private fun applyDelete(before: Int, after: Int) {
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
        knownScroll = CoreBridge.nativeEditorScroll(surfaceId)
        requestRender()
    }
}
