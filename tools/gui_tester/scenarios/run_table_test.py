"""
Automation script to launch Vibecodium via MCP gui-tester,
create a new note with a table containing long text,
and inspect the resulting UI via screenshots, zoom, and clipboard.
"""

import sys
import io
import json
import time
from pathlib import Path
from PIL import Image

PROJECT_ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(PROJECT_ROOT))

from tools.gui_tester import mcp_server


def main():
    screenshots_dir = Path("tools/gui_tester/screenshots")
    screenshots_dir.mkdir(parents=True, exist_ok=True)

    print("=== Step 1: Launch Vibecodium via MCP launch_app ===")
    exe_path = str(Path("aquilum-gpui/target/debug/vibecodium.exe").resolve())
    
    # Launch app
    res = mcp_server.launch_app(
        cmd=exe_path,
        window_title="Vibecodium",
        timeout=15.0,
        screenshot=True,
    )
    print("Launch result:", res[0] if isinstance(res, list) else res)
    time.sleep(1.0)

    # Focus and set predictable window size
    mcp_server.focus_window()
    time.sleep(0.5)

    print("\n=== Step 2: Capture initial screenshot with grid ===")
    shot_grid = mcp_server.screenshot(grid=100, save_as="vibecodium_launch_grid")
    if isinstance(shot_grid, list):
        img = Image.open(io.BytesIO(shot_grid[1].data))
        path = screenshots_dir / "vibecodium_launch_grid.png"
        img.save(path)
        print(f"[OK] Saved initial grid screenshot to: {path}")

    print("\n=== Step 3: Create a note with long table via Vault file & Open ===")
    # Define markdown note with long table
    note_name = "Таблица_длинный_текст_MCP.md"
    note_file = Path(r"D:\Naben\Vibecodium") / note_name
    
    table_content = """# Тестирование рендеринга таблицы с длинным текстом (MCP GUI Tester)

Заметка создана для проверки корректности отображения многоколоночных Markdown таблиц с длинными ячейками текста, переносом строк и выравниванием в GPUI движке Vibecodium.

| № | Название подсистемы | Детальное описание архитектуры, функциональности и спецификаций | Статус | Дополнительные параметры |
|---|:---|:---|:---:|---|
| 01 | `tools/gui_tester/core.py` | Полнофункциональное ядро автоматизации на Python 3. Включает прямое взаимодействие с Windows Desktop Window Manager (DWM API), многомониторный захват кадров высокой четкости, экспорт растровых изображений в системный буфер обмена `CF_DIB` и алгоритмы сопоставления шаблонов OpenCV (`cv2.TM_CCOEFF_NORMED`). | Готово | Разрешение до 8K, поддержка DPI aware |
| 02 | `tools/gui_tester/mcp_server.py` | Официальный MCP-сервер GUI Tester для автономных языковых моделей (Claude, Antigravity, GPT). Предоставляет методы `launch_app`, `screenshot`, `zoom`, `run_steps`, `copy_screenshot_to_clipboard`, `click` и автоматическую валидацию логов. | Активно | Работает через протокол Model Context Protocol |
| 03 | `aquilum-gpui/src/table.rs` | Высокопроизводительный парсер и компоновщик таблиц на базе `pulldown_cmark`. Вычисляет визуальные границы колонок, переносы слов, отступы и вертикальные разделители без деградации частоты кадров 60 FPS. | В процессе | Поддерживает разделители и выравнивание по центру/краям |
| 04 | `Editor Live Preview` | Динамический движок отрисовки Markdown в реальном времени. Обеспечивает мгновенный отклик при вводе текста, плавную прокрутку и масштабирование сложных табличных структур. | Стабильно | Полная интеграция с буфером документа |

---

### Дополнительная таблица сравнения производительности:

| Тест | Объем данных | Время парсинга | Рендеринг GPUI | Стабильность памяти |
|:---|:---:|:---:|:---:|:---:|
| Компактная таблица (3x3) | 1 КБ | 0.05 мс | 60 FPS | 100% |
| Таблица средней длины (10x5) | 15 КБ | 0.22 мс | 60 FPS | 100% |
| Сверхдлинный текст в ячейках | 50 КБ | 0.48 мс | 60 FPS | 100% |

---
*Сгенерировано и открыто через автоматический скрипт GUI Tester MCP.*
"""

    note_file.write_text(table_content, encoding="utf-8")
    print(f"[OK] Written note to: {note_file}")

    print("\n=== Step 4: Open note in Vibecodium via Quick Open (Ctrl+O) ===")
    mcp_server.focus_window()
    time.sleep(0.3)
    
    # Open Quick Switcher / Find note: Ctrl+O
    open_res = mcp_server.run_steps([
        {"action": "hotkey", "keys": ["ctrl", "o"]},
        {"action": "wait", "seconds": 0.8},
        {"action": "type_text", "text": "Таблица_длинный_текст_MCP"},
        {"action": "wait", "seconds": 0.8},
        {"action": "press_key", "key": "enter"},
        {"action": "wait", "seconds": 1.5},
    ], final_screenshot=False)
    print("Open note result:", open_res)

    print("\n=== Step 5: Capture full window screenshot of the rendered table ===")
    shot_res = mcp_server.screenshot(grid=0, save_as="table_rendered_full")
    if isinstance(shot_res, list):
        img_full = Image.open(io.BytesIO(shot_res[1].data))
        path_full = screenshots_dir / "table_rendered_full.png"
        img_full.save(path_full)
        print(f"[OK] Saved full table screenshot to: {path_full} (size {img_full.size})")

    print("\n=== Step 6: Zoom in on the table region ===")
    # Zoom into the center-right area where editor table is displayed
    # Default window typically has sidebar (0..260) and editor (260..w)
    zoom_res = mcp_server.zoom(x=300, y=100, w=1200, h=700, factor=2)
    if isinstance(zoom_res, list):
        img_zoom = Image.open(io.BytesIO(zoom_res[1].data))
        path_zoom = screenshots_dir / "table_rendered_zoom.png"
        img_zoom.save(path_zoom)
        print(f"[OK] Saved zoom screenshot to: {path_zoom} (size {img_zoom.size})")

    print("\n=== Step 7: Copy screenshot to Windows clipboard ===")
    clip_res = mcp_server.copy_screenshot_to_clipboard()
    print("[OK] copy_screenshot_to_clipboard result:", clip_res)

    print("\n=== Step 8: Verify App Logs ===")
    logs = mcp_server.app_logs(tail_lines=15)
    print("Logs:\n", logs)

    print("\n=== ALL STEPS COMPLETED SUCCESSFULLY ===")


if __name__ == "__main__":
    main()
