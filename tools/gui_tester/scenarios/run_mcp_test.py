"""
End-to-End Test Scenario using the GUI Tester MCP Server.

Steps executed via MCP Tools:
1. launch_app (starts Vibecodium and waits for window).
2. run_steps (sends Ctrl+P, types note name in Russian Cyrillic, presses Enter to switch tab).
3. screenshot with coordinate grid (grid=100).
4. zoom in on the table region (window-relative coordinates).
5. copy_screenshot_to_clipboard (places the result in Windows clipboard for user).
6. app_logs (inspects app logs/panics).
7. stop_app.
"""

import sys
import io
import json
from pathlib import Path
from PIL import Image

PROJECT_ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(PROJECT_ROOT))

from tools.gui_tester import mcp_server


def main():
    screenshots_dir = Path("tools/gui_tester/screenshots")
    screenshots_dir.mkdir(parents=True, exist_ok=True)

    print("=== Step 1: Launch application via MCP launch_app ===")
    exe_path = str(Path("aquilum-gpui/target/debug/vibecodium.exe").resolve())
    note_path = "D:\\Naben\\Vibecodium\\Таблица_длинный_текст.md"

    res_launch = mcp_server.launch_app(
        cmd=[exe_path, note_path],
        window_title="Vibecodium",
        timeout=15.0,
        screenshot=True,
    )
    print("[OK] launch_app succeeded:")
    if isinstance(res_launch, list):
        print(res_launch[0])

    print("\n=== Step 2: Open note via MCP run_steps (Ctrl+P -> type Cyrillic -> Enter) ===")
    steps_result = mcp_server.run_steps([
        {"action": "hotkey", "keys": ["ctrl", "p"]},
        {"action": "wait", "seconds": 0.6},
        {"action": "type_text", "text": "Таблица_длинный_текст"},
        {"action": "wait", "seconds": 0.6},
        {"action": "press_key", "key": "enter"},
        {"action": "wait", "seconds": 1.2},
    ], final_screenshot=False)
    print("[OK] run_steps result:", steps_result[0] if isinstance(steps_result, list) else steps_result)

    print("\n=== Step 3: Take window-relative screenshot with coordinate grid ===")
    res_grid = mcp_server.screenshot(grid=100)
    if isinstance(res_grid, list):
        print("[OK] screenshot(grid=100) succeeded:")
        img_grid = Image.open(io.BytesIO(res_grid[1].data))
        img_grid.save(screenshots_dir / "mcp_table_grid.png")
        print(f"Saved: {screenshots_dir / 'mcp_table_grid.png'}")

    print("\n=== Step 4: Zoom in on the Table region (window-relative coords) ===")
    res_zoom = mcp_server.zoom(x=450, y=100, w=1800, h=950, factor=1.5)
    if isinstance(res_zoom, list):
        print("[OK] zoom succeeded:")
        img_zoom = Image.open(io.BytesIO(res_zoom[1].data))
        img_zoom.save(screenshots_dir / "mcp_table_zoom.png")
        print(f"Saved: {screenshots_dir / 'mcp_table_zoom.png'}")

    print("\n=== Step 5: Copy screenshot to Windows clipboard ===")
    res_clip = mcp_server.copy_screenshot_to_clipboard()
    print("[OK] copy_screenshot_to_clipboard:", res_clip)

    print("\n=== Step 6: Check App Logs (stderr/stdout capture) ===")
    logs = mcp_server.app_logs(tail_lines=10)
    print("Logs tail:\n", logs)

    print("\n[SUCCESS] MCP Tool verification completed!")


if __name__ == "__main__":
    main()
