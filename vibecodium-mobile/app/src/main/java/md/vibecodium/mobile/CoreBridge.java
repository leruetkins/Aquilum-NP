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

    public static native String nativeOutline(String text);

    public static native String nativeStats(String text);

    public static native String nativeMarkdown(String text);

    public static native String nativeAnalyze(String text);
}
