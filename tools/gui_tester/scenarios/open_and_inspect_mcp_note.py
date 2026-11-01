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

    # Step 2: Click on 'Таблица_длинный_текст_MCP' at (200, 1565) relative to window
    click_x = w.x + 200
    click_y = w.y + 1565
    print(f"=== Step 2: Clicking note at ({click_x}, {click_y}) ===")
    pyautogui.click(click_x, click_y)
    time.sleep(1.5)

    # Step 3: Capture full screenshot
    img = ScreenCapture.capture_window_hwnd(w.hwnd)
    if img is None:
        img = ScreenCapture.capture_rect(w.x, w.y, w.width, w.height)

    shot_path = screenshots_dir / "mcp_note_opened_perfect.png"
    img.save(shot_path)
    print(f"[OK] Full screenshot: {shot_path}")

    # Crop table area
    crop_table = img.crop((280, 120, min(img.width, 3300), 1250))
    crop_path = screenshots_dir / "mcp_table_cropped_perfect.png"
    crop_table.save(crop_path)
    print(f"[OK] Cropped table: {crop_path}")

    # Step 4: Click into row 1 description cell (x=800, y=360)
    print("=== Step 4: Click into table cell to test inline edit mode ===")
    pyautogui.click(w.x + 800, w.y + 360)
    time.sleep(0.8)

    img_edit = ScreenCapture.capture_window_hwnd(w.hwnd)
    edit_shot = img_edit.crop((280, 120, min(img_edit.width, 3300), 1250))
    edit_path = screenshots_dir / "mcp_table_edit_mode_perfect.png"
    edit_shot.save(edit_path)
    print(f"[OK] Edit mode crop: {edit_path}")

    # Step 5: Copy to clipboard
    mcp_server.copy_screenshot_to_clipboard()
    print("[OK] Copied screenshot to Windows clipboard.")

if __name__ == "__main__":
    main()
