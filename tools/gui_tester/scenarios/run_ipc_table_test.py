import sys
import time
import subprocess
from pathlib import Path
from PIL import Image

PROJECT_ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(PROJECT_ROOT))

from tools.gui_tester import mcp_server
from tools.gui_tester.core import WindowManager, ScreenCapture, Clipboard

def main():
    screenshots_dir = Path("tools/gui_tester/screenshots")
    screenshots_dir.mkdir(parents=True, exist_ok=True)

    vault_dir = Path(r"D:\Naben\Vibecodium")
    note_file = vault_dir / "Table_Long_Text_Test.md"

    # Markdown with long tables, formatting, code, alignment, and wrapping test
    table_content = """# Тестирование рендеринга таблицы с длинным текстом в Vibecodium

Эта заметка создана для проверки визуального рендеринга таблиц движком GPUI при наличии длинных предложений, форматирования, кодовых блоков и разных типов выравнивания.

### Таблица 1: Спецификация подсистем с длинными описаниями

| № | Компонент | Подробное описание функциональности, архитектурных решений и используемых библиотек | Режим | Скорость |
|:---|:---|:---|:---:|---:|
| 01 | `Core Engine` | Высокопроизводительное ядро на Rust с использованием фреймворка GPUI. Все элементы интерфейса отрисовываются напрямую через видеокарту на частоте 120 FPS без использования веб-технологий. | Активен | 120 FPS |
| 02 | `Markdown Parser` | Инкрементальный парсер CommonMark и GitHub Flavored Markdown (GFM) на базе `pulldown_cmark`. Обеспечивает мгновенную подсветку разметки при редактировании больших файлов. | Оптимизирован | < 1 ms |
| 03 | `Table Layout System` | Специализированный компоновщик таблиц с функцией `fit_table_columns`, вычисляющий ширину столбцов (`col_widths`) по максимальной длине содержимого (`natural_widths`) с отступами ячеек `PADDING_X = 8.0` и минимальной шириной 80 px. | В разработке | O(N) строк |
| 04 | `GUI Tester MCP` | Автономный инструмент визуального тестирования интерфейса для ИИ-агентов. Поддерживает захват кадров высокой четкости, OpenCV поиск по шаблонам и прямой экспорт в системный буфер `CF_DIB`. | Готов | < 100 ms |

---

### Таблица 2: Экстремально длинный связный текст без переносов

| ID | Тип проверки | Длинный текст в ячейке для проверки переноса слов и горизонтального скроллинга | Статус |
|---|---|---|---|
| T-101 | Wrap Test | Данная колонка содержит очень длинный абзац непрерывного текста, предназначенный для проверки того, переносит ли редактор строки внутри ячеек таблицы по словам (word wrapping) или же растягивает колонку на полную ширину содержимого, вызывая горизонтальное расширение контейнера документа. | Пройдено |
| T-102 | Border Test | Вторая строка для проверки сохранения выравнивания вертикальных разделителей колонок между строками с сильно различающимся объемом содержимого ячеек. | Пройдено |

---
*Сгенерировано тестовым сценарием GUI Tester.*
"""

    note_file.write_text(table_content, encoding="utf-8")
    print(f"[OK] Wrote note to: {note_file}")

    exe_path = str(Path("aquilum-gpui/target/debug/vibecodium.exe").resolve())

    # Step 1: Launch or find Vibecodium
    w = WindowManager.find_window("Vibecodium")
    if not w:
        print("Launching Vibecodium...")
        mcp_server.launch_app(cmd=exe_path, window_title="Vibecodium", timeout=15.0)
        time.sleep(1.5)
        w = WindowManager.find_window("Vibecodium")
    else:
        print(f"Found running Vibecodium (hwnd={w.hwnd})")

    # Step 2: Send IPC command to open the test note!
    print(f"Sending IPC command to open {note_file}...")
    res = subprocess.run([exe_path, str(note_file.resolve())], capture_output=True, text=True)
    print(f"IPC process returned: code={res.returncode}")
    time.sleep(1.5)

    # Step 3: Bring window to front
    WindowManager.focus(w.hwnd)
    time.sleep(0.5)

    # Step 4: Capture screenshot
    img = ScreenCapture.capture_window_hwnd(w.hwnd)
    if img is None:
        img = ScreenCapture.capture_rect(w.x, w.y, w.width, w.height)

    full_path = screenshots_dir / "table_test_full.png"
    img.save(full_path)
    print(f"[OK] Saved full screenshot: {full_path} ({img.size})")

    # Crop the editor table area
    crop_table = img.crop((475, 120, min(img.width, 3300), 1300))
    crop_path = screenshots_dir / "table_test_crop.png"
    crop_table.save(crop_path)
    print(f"[OK] Saved cropped table: {crop_path} ({crop_table.size})")

    # High-detail crop of Table 1
    crop_t1 = img.crop((475, 200, min(img.width, 2800), 750))
    t1_path = screenshots_dir / "table_1_detail.png"
    crop_t1.save(t1_path)
    print(f"[OK] Saved Table 1 detail: {t1_path} ({crop_t1.size})")

    # High-detail crop of Table 2
    crop_t2 = img.crop((475, 750, min(img.width, 2800), 1250))
    t2_path = screenshots_dir / "table_2_detail.png"
    crop_t2.save(t2_path)
    print(f"[OK] Saved Table 2 detail: {t2_path} ({crop_t2.size})")

    # Step 5: Copy to Windows clipboard
    Clipboard.set_image_from_file(str(full_path))
    print("[OK] Copied screenshot to Windows clipboard (CF_DIB).")

    print("\n=== SUCCESSFUL COMPLETION ===")

if __name__ == "__main__":
    main()
