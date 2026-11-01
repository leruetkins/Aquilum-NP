import sys
import time
from pathlib import Path
from PIL import Image

PROJECT_ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(PROJECT_ROOT))

import pyautogui
pyautogui.FAILSAFE = False

from tools.gui_tester import mcp_server
from tools.gui_tester.core import WindowManager, ScreenCapture

def main():
    screenshots_dir = Path("tools/gui_tester/screenshots")
    screenshots_dir.mkdir(parents=True, exist_ok=True)

    exe_path = str(Path("aquilum-gpui/target/debug/vibecodium.exe").resolve())
    
    # Launch app
    print("=== Step 1: Launch app ===")
    mcp_server.launch_app(cmd=exe_path, window_title="Vibecodium", timeout=15.0)
    time.sleep(1.5)

    w = mcp_server._prep("Vibecodium")
    print(f"Window: {w.title}, rect=({w.x},{w.y},{w.width},{w.height})")

    # Hover over item at x=250, y=1590
    hx = w.x + 250
    hy = w.y + 1590
    print(f"Hovering at ({hx}, {hy})...")
    pyautogui.moveTo(hx, hy)
    time.sleep(0.5)

    img_hover = ScreenCapture.capture_window_hwnd(w.hwnd)
    crop_hover = img_hover.crop((100, 1500, 450, 1800))
    crop_hover.save(screenshots_dir / "test_hover.png")

    # Click it
    print(f"Clicking at ({hx}, {hy})...")
    pyautogui.click(hx, hy)
    time.sleep(1.5)

    # Capture after click
    img_after = ScreenCapture.capture_window_hwnd(w.hwnd)
    crop_after = img_after.crop((100, 1500, 450, 1800))
    crop_after.save(screenshots_dir / "test_after_click.png")

    # Capture editor area
    crop_editor = img_after.crop((480, 120, min(img_after.width, 3300), 1250))
    crop_editor.save(screenshots_dir / "opened_note_editor.png")
    
    img_after.save(screenshots_dir / "opened_note_full.png")

    mcp_server.copy_screenshot_to_clipboard()
    print("[OK] Test completed.")

if __name__ == "__main__":
    main()
