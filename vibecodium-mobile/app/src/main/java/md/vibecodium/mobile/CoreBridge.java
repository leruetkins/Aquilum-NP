package md.vibecodium.mobile;

/**
 * Static JNI surface for the Rust core. The class name and package are part of
 * the exported symbol names (Java_md_vibecodium_mobile_CoreBridge_*).
 *
 * Every method returns a JSON envelope:
 *   {"ok":true,"data":...} or {"ok":false,"error":"..."}
 */
public final class CoreBridge {
    static {
        System.loadLibrary("vibecodium_core");
    }

    private CoreBridge() {
    }

    public static native String nativeVersion();

    public static native String nativeOpenVault(String path);

    public static native String nativeCreateVault(String path);

    public static native String nativeCloseVault();

    public static native String nativeVaultRoot();

    public static native String nativeListNotes();

    public static native String nativeReadNote(String path);

    public static native String nativeWriteNote(String path, String text);

    public static native String nativeCreateNote(String path, String text);

    public static native String nativeDeleteNote(String path);

    public static native String nativeRenameNote(String oldPath, String newPath);

    public static native String nativeSearch(String query, int limit);

    // -------------------------------------------------------- indexed search

    public static native String nativeSearchIndexReset();

    public static native String nativeSearchIndexAdd(String path, String text, long mtime, long len);

    public static native String nativeSearchIndexRemove(String path);

    public static native String nativeSearchIndexFinish();

    public static native String nativeSearchIndexStatus();

    public static native String nativeSearchIndexLoad(String path);

    public static native String nativeSearchIndexSave(String path);

    public static native String nativeSearchIndexQuery(String query, int limit);

    public static native String nativeSearchLocate(String query, String text);

    public static native String nativeOutline(String text);

    public static native String nativeStats(String text);

    public static native String nativeMarkdown(String text);

    public static native String nativeAnalyze(String text);

    public static native void nativeSurfaceAttach(
            int id, android.view.Surface surface, int width, int height, float density);

    public static native void nativeSurfaceDetach(int id);

    public static native void nativeSurfaceResize(int id, int width, int height);

    public static native void nativeSurfaceBeginUpdate(int id);

    public static native void nativeSurfaceEndUpdate(int id);

    public static native void nativeSurfaceRender(int id);

    public static native void nativeSurfaceInvalidate(int id);

    public static native void nativeSurfaceSetTree(int id, String json);

    public static native void nativeSurfaceSetEditor(int id, String text, int selection, String language);

    public static native String nativeSurfaceTouch(int id, float x, float y, int action);

    /**
     * Lock-free absolute scroll request: stores the value for the next paint
     * without taking the surface lock, so input and fling ticks never wait on
     * a frame in progress.
     */
    public static native void nativeSurfaceScroll(int id, float y);

    public static native float nativeTreeScroll(int id);

    public static native String nativeTreeViewport(int id);

    public static native void nativeTreeSetScroll(int id, float y);

    public static native String nativeEditorInsert(int id, String text);

    public static native String nativeEditorReplace(int id, int start, int end, String text);

    public static native String nativeEditorDelete(int id, int before, int after);

    public static native void nativeEditorSelection(int id, int start, int end);

    public static native void nativeEditorSelectAt(int id, float x, float y, boolean extend);

    public static native void nativeEditorSelectWord(int id, float x, float y);

    public static native String nativeEditorState(int id);

    public static native String nativeEditorSelectionState(int id);

    public static native String nativeEditorLayoutInfo(int id);

    public static native String nativeEditorSelectionHandles(int id);

    public static native boolean nativeEditorDragHandle(int id, int which, float x, float y);

    public static native void nativeEditorSetBottomInset(int id, float inset);

    public static native void nativeEditorSetTopInset(int id, float inset);

    public static native void nativeEditorSetCaretVisible(int id, boolean visible);

    public static native void nativeEditorSetInsertionHandleVisible(int id, boolean visible);

    public static native float nativeEditorScroll(int id);

    public static native String nativeEditorViewport(int id);

    public static native void nativeEditorSetScroll(int id, float y, float x);

    public static int[] rasterizeEmoji(String text, float px) {
        if (text == null || text.isEmpty()) return null;
        android.text.TextPaint paint = new android.text.TextPaint(android.graphics.Paint.ANTI_ALIAS_FLAG);
        paint.setTextSize(px);
        int w = Math.max(1, (int) Math.ceil(paint.measureText(text)));
        android.graphics.Paint.FontMetrics fm = paint.getFontMetrics();
        int h = Math.max(1, (int) Math.ceil(fm.bottom - fm.top));
        android.graphics.Bitmap bitmap = android.graphics.Bitmap.createBitmap(w, h, android.graphics.Bitmap.Config.ARGB_8888);
        android.graphics.Canvas canvas = new android.graphics.Canvas(bitmap);
        paint.setColor(0xFFFFFFFF);
        canvas.drawText(text, 0, -fm.top, paint);
        int[] result = new int[w * h + 2];
        result[0] = w;
        result[1] = h;
        bitmap.getPixels(result, 2, w, 0, 0, w, h);
        bitmap.recycle();
        return result;
    }
}
