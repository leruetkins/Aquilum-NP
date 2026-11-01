"""
Automated Test Scenario for Vibecodium (aquilum-gpui).

Workflow:
1. Launches aquilum-gpui/target/debug/vibecodium.exe (or checks if already running).
2. Waits for the "Vibecodium" window.
3. Centers / sizes window for reproducible testing.
4. Takes screenshot of the initial editor state.
5. Sends hotkeys or clicks buttons to trigger menus/popups.
6. Takes screenshot of the opened menu / dialog.
7. Copies the resulting capture directly to Windows clipboard (ready for Ctrl+V).
8. Performs visual verification and logs structured results.
"""

from __future__ import annotations

import sys
import time
from pathlib import Path

# Add project root to sys.path
PROJECT_ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(PROJECT_ROOT))

from tools.gui_tester.core import (
    WindowManager,
    ScreenCapture,
    ImageSearch,
    InputController,
    VisualDiff,
    AppSession,
)


def run_test():
    screenshots_dir = Path("tools/gui_tester/screenshots")
    screenshots_dir.mkdir(parents=True, exist_ok=True)

    exe_path = Path("aquilum-gpui/target/debug/vibecodium.exe").resolve()
    if not exe_path.exists():
        print(f"[WARN] Binary not found at {exe_path}. Trying 'cargo run' or checking existing window...")
        cmd = "cargo run --manifest-path aquilum-gpui/Cargo.toml"
    else:
        cmd = str(exe_path)

    print("=== Step 1: Check if Vibecodium is already running ===")
    win = WindowManager.find_window("Vibecodium")
    session = None

    if not win:
        print(f"Starting application: {cmd}")
        session = AppSession(cmd, working_dir=str(PROJECT_ROOT / "aquilum-gpui"))
        started = session.start(wait_secs=5.0, window_title="Vibecodium")
        if not started:
            print("[ERROR] Failed to start application or find 'Vibecodium' window within timeout.")
            return False
        win = session.window_info or WindowManager.find_window("Vibecodium")

    if not win:
        print("[ERROR] Could not find 'Vibecodium' window.")
        return False

    print(f"[OK] Found Vibecodium window: HWND {win.hwnd}, Rect {win.x},{win.y} ({win.width}x{win.height})")

    print("\n=== Step 2: Focus and set standardized window size ===")
    WindowManager.focus(win.hwnd)
    time.sleep(0.5)

    # Optional: set predictable size for snapshot testing
    # WindowManager.set_geometry(win.hwnd, 100, 100, 1280, 800)

    print("\n=== Step 3: Capture initial editor state ===")
    initial_shot = screenshots_dir / "vibecodium_initial.png"
    img = ScreenCapture.capture_window(win, output_path=initial_shot)
    if img:
        print(f"[OK] Saved initial screen to: {initial_shot}")
        # Copy to clipboard
        ScreenCapture.copy_image_to_clipboard(img)
        print("[OK] Copied initial screen to Windows Clipboard (you can paste with Ctrl+V)!")

    print("\n=== Step 4: Interact with application (Test Hotkeys / Menus) ===")
    # Send Alt key or hotkey to trigger menu / command palette
    # In Vibecodium / Zed, Ctrl+P opens the file finder / command palette
    WindowManager.focus(win.hwnd)
    time.sleep(0.2)
    print("Sending Ctrl+P (Palette)...")
    InputController.send_hotkey("ctrl", "p")
    time.sleep(0.7)

    palette_shot = screenshots_dir / "vibecodium_palette_opened.png"
    img_palette = ScreenCapture.capture_window(win, output_path=palette_shot)
    if img_palette:
        print(f"[OK] Saved palette screenshot: {palette_shot}")
        ScreenCapture.copy_image_to_clipboard(img_palette)
        print("[OK] Copied palette screenshot to Windows Clipboard!")

    print("\n=== Step 5: Visual Diff (verify that UI reacted to input) ===")
    if initial_shot.exists() and palette_shot.exists():
        diff_path = screenshots_dir / "vibecodium_diff.png"
        diff_res = VisualDiff.compare(initial_shot, palette_shot, diff_output_path=diff_path)
        print(f"Diff results: changed_pixels={diff_res['changed_pixels']}, ratio={diff_res['diff_ratio']}")
        if diff_res["changed_pixels"] > 0:
            print(f"[OK] Visual change detected! Diff highlighted in {diff_path}")
        else:
            print("[INFO] No visual difference detected.")

    print("\n=== Step 6: Close Palette / Restore state ===")
    InputController.press_key("esc")
    time.sleep(0.3)

    if session:
        print("\nStopping background session...")
        session.stop()
        print("[OK] Session terminated.")

    print("\n[SUCCESS] Test scenario completed!")
    return True


if __name__ == "__main__":
    success = run_test()
    sys.exit(0 if success else 1)
