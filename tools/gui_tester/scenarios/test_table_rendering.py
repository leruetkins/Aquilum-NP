"""
Comprehensive test script for table rendering in Vibecodium:
1. Launches Vibecodium via MCP gui_tester
2. Creates a markdown note with a rich table containing long text
3. Opens the note via Quick Switcher (Ctrl+O)
4. Captures full screenshot and zoomed screenshots of the rendered table
5. Clicks inside the table to observe edit mode vs preview mode
6. Verifies visual appearance and logs
"""

import sys
import io
import json
import time
from pathlib import Path
from PIL import Image

PROJECT_ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(PROJECT_ROOT))

# Ensure stdout is utf-8
if sys.stdout:
    try:
        sys.stdout.reconfigure(encoding="utf-8")
    except Exception:
        pass

import pyautogui
pyautogui.FAILSAFE = False

from tools.gui_tester import mcp_server
from tools.gui_tester.core import WindowManager, ScreenCapture, InputController

def main():
    screenshots_dir = Path("tools/gui_tester/screenshots")
    screenshots_dir.mkdir(parents=True, exist_ok=True)

    print("=== Step 1: Create Note in Vault ===")
    note_name = "Table_Long_Text_Inspection.md"
    vault_dir = Path(r"D:\Naben\Vibecodium")
    note_file = vault_dir / note_name

    table_md = """# Тестирование рендеринга таблицы с длинным текстом

В этой заметке исследуется поведение компонента таблицы в редакторе Vibecodium при наличии длинных предложений, форматирования и кодовых блоков.

| ID | Компонент | Описание функциональности и спецификация архитектуры | Режим работы | Производительность |
|:---|:---|:---|:---:|---:|
| T-01 | `Core Engine` | Полнофункциональный движок на базе Rust и GPUI, обеспечивающий реактивный рендеринг пользовательского интерфейса с аппаратным ускорением на видеокарте через DirectX/Metal. | Активный | 120 FPS |
| T-02 | `Markdown Parser` | Инкрементальный лексер на основе `pulldown_cmark`, поддерживающий GitHub Flavored Markdown (GFM), таблицы, списки задач и подсветку синтаксиса на лету. | Оптимизирован | < 0.5 ms |
| T-03 | `Table Layout System` | Специализированный компоновщик ячеек с алгоритмом `fit_table_columns`, вычисляющий естественную ширину столбцов (`natural_widths`) с учетом отступов `PADDING_X = 8.0` и ограничений ширины `MIN_COL_WIDTH = 80.0`. | В разработке | O(N) строк |
| T-04 | `Live Preview Mode` | Гибридный режим отображения: таблица отображается в виде стилизованной графической сетки с рамками, а при переходе курсора внутрь переключается на редактирование. | Стабильный | Мгновенно |

---

### Таблица с экстремально длинным абзацем текста:

| Номер | Короткое поле | Экстремально длинный текст в ячейке для проверки переноса слов и горизонтального скроллинга | Примечание |
|---|---|---|---|
| 1 | Тест 1 | Данная колонка содержит очень длинный связный текст без разрывов строк, предназначенный для проверки того, происходит ли перенос текста внутри ячейки таблицы (word wrapping) или же колонка расширяется на всю длину текста, вызывая горизонтальную прокрутку контейнера редактора. В настольных редакторах поведение таблиц при длинных строках является одной из ключевых UX-характеристик. | Завершено |
| 2 | Тест 2 | Вторая строка с еще одним длинным описанием: проверка сохранения выравнивания границ колонок по вертикали между разными строками таблицы при неодинаковой длине содержимого. | Успешно |

---
*Создано для визуального анализа через gui_tester.*
"""

    note_file.write_text(table_md, encoding="utf-8")
    print(f"[OK] Note written to: {note_file}")

    print("\n=== Step 2: Launch Vibecodium via MCP ===")
    exe_path = str(Path("aquilum-gpui/target/debug/vibecodium.exe").resolve())
    
    res = mcp_server.launch_app(
        cmd=exe_path,
        window_title="Vibecodium",
        timeout=15.0,
        screenshot=True,
    )
    print("Launch result:", res[0] if isinstance(res, list) else res)
    time.sleep(1.5)

    # Bring to front
    w = mcp_server._prep("Vibecodium")
    print(f"Window handle: {w.hwnd}, geometry: {w.x},{w.y} {w.width}x{w.height}")
    time.sleep(0.5)

    print("\n=== Step 3: Open the note via Quick Switcher (Ctrl+O) ===")
    # Move cursor safely into the window center first to avoid 0,0 failsafe
    cx, cy = w.x + w.width // 2, w.y + w.height // 2
    pyautogui.moveTo(cx, cy)
    time.sleep(0.3)

    # Open switcher
    pyautogui.hotkey("ctrl", "o")
    time.sleep(0.8)
    
    # Type note name
    pyautogui.write("Table_Long_Text_Inspection", interval=0.03)
    time.sleep(0.8)
    
    # Press Enter
    pyautogui.press("enter")
    time.sleep(1.5)

    print("\n=== Step 4: Capture Full Screenshot of Opened Note ===")
    w = mcp_server._prep("Vibecodium")
    img_full = ScreenCapture.capture_window_hwnd(w.hwnd)
    if img_full is None:
        img_full = ScreenCapture.capture_rect(w.x, w.y, w.width, w.height)
    
    shot_path = screenshots_dir / "table_inspection_full.png"
    img_full.save(shot_path)
    print(f"[OK] Full screenshot saved to: {shot_path} ({img_full.size})")

    print("\n=== Step 5: Capture Zoomed Views of Tables ===")
    # Editor starts around x=260 (sidebar is ~260px wide).
    # Table 1 is located around y=200 to y=600
    # Let's crop and save Table 1
    # Scale from logical window coords
    scale_x = img_full.width / w.width
    scale_y = img_full.height / w.height

    crop_t1 = (
        int(260 * scale_x),
        int(140 * scale_y),
        int(min(w.width, 2200) * scale_x),
        int(650 * scale_y)
    )
    img_t1 = img_full.crop(crop_t1)
    t1_path = screenshots_dir / "table_1_zoom.png"
    img_t1.save(t1_path)
    print(f"[OK] Table 1 zoom saved to: {t1_path} ({img_t1.size})")

    # Table 2 (extreme long text)
    crop_t2 = (
        int(260 * scale_x),
        int(600 * scale_y),
        int(min(w.width, 2400) * scale_x),
        int(1100 * scale_y)
    )
    img_t2 = img_full.crop(crop_t2)
    t2_path = screenshots_dir / "table_2_zoom.png"
    img_t2.save(t2_path)
    print(f"[OK] Table 2 zoom saved to: {t2_path} ({img_t2.size})")

    print("\n=== Step 6: Test Clicking Inside Table (Edit Mode) ===")
    # Click inside the third column of Table 1 (approx x = 600, y = 350 relative to window)
    click_x = w.x + int(600 * scale_x)
    click_y = w.y + int(360 * scale_y)
    pyautogui.click(click_x, click_y)
    time.sleep(0.8)

    img_edit = ScreenCapture.capture_window_hwnd(w.hwnd)
    edit_path = screenshots_dir / "table_edit_mode.png"
    img_edit.save(edit_path)
    print(f"[OK] Edit mode screenshot saved to: {edit_path}")

    # Crop the edit mode area
    img_edit_zoom = img_edit.crop(crop_t1)
    edit_zoom_path = screenshots_dir / "table_edit_mode_zoom.png"
    img_edit_zoom.save(edit_zoom_path)
    print(f"[OK] Edit mode zoom saved to: {edit_zoom_path}")

    print("\n=== Step 7: Click Outside Table (Return to Preview Mode) ===")
    # Click on the title or empty line above
    pyautogui.click(w.x + int(400 * scale_x), w.y + int(180 * scale_y))
    time.sleep(0.8)

    img_returned = ScreenCapture.capture_window_hwnd(w.hwnd)
    returned_path = screenshots_dir / "table_returned_preview.png"
    img_returned.save(returned_path)
    print(f"[OK] Returned preview screenshot saved to: {returned_path}")

    print("\n=== Step 8: Copy Final Screenshot to Clipboard ===")
    mcp_server.copy_screenshot_to_clipboard()
    print("[OK] Copied screenshot to Windows clipboard (CF_DIB).")

    print("\n=== Step 9: App Logs ===")
    logs = mcp_server.app_logs(tail_lines=15)
    print("Logs:\n", logs)

    print("\n=== TEST COMPLETED SUCCESSFULLY ===")

if __name__ == "__main__":
    main()
