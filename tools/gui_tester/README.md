# GUI Tester — Визуальное тестирование и автоматизация UI

Фреймворк автоматизированного визуального тестирования приложений на базе **Python 3**, **OpenCV**, **MSS / Pillow** и **Windows API**.

Особенно эффективен для **GPUI / Rust** (Vibecodium) и приложений с кастомным GPU-рендерингом, где стандартные accessibility API не всегда видят внутренние элементы интерфейса.

---

## Возможности

1. **Управление процессами и окнами**:
   - Автоматический запуск программы (`.exe` или `cargo run`) и ожидание создания окна.
   - Поиск окон по подстроке заголовка (`Vibecodium`, `Aquilum`, `Obsidian`).
   - Фокусировка (`BringToFront`), восстановление из свернутого состояния и фиксация размеров окна.
2. **Захват экрана и окон**:
   - Сверхбыстрый скриншот полного экрана (включая мультимониторные конфигурации 4K/8K).
   - Захват конкретного окна по HWND или заголовку с точным отсечением невидимых теней Windows (DWM).
   - Захват произвольного прямоугольника `(x, y, w, h)`.
3. **Прямая работа с буфером обмена Windows (Clipboard)**:
   - Автоматическое копирование любого скриншота или окна в системный буфер обмена (`CF_DIB`).
   - Изображение сразу доступно для вставки через `Ctrl+V` в графический редактор, чат, Word, Paint или среду разработки.
4. **Компьютерное зрение (Template Matching)**:
   - Поиск эталонных картинок (кнопок, иконок, вкладок, пунктов меню) на экране с помощью нормализованной кросс-корреляции OpenCV.
   - Возврат точных экранных координат, ширины, высоты и центра элемента (`center`).
   - Поиск как одного лучшего совпадения, так и всех вхождений (`find_all`).
   - Ожидание появления шаблона с таймаутом (`wait_for_template`).
5. **Симуляция ввода (Mouse & Keyboard)**:
   - Клик по координатам или напрямую по центру найденного изображения кнопки/меню.
   - Наведение мыши (`hover`) для проверки всплывающих подсказок (tooltips) и состояний наведения.
   - Ввод текста и посылка любых клавиатурных комбинаций (`Ctrl+P`, `Alt+F`, `Escape`, `Enter`).
6. **Визуальный Diff (Visual Regression Testing)**:
   - Попиксельное сравнение снимков до и после действия.
   - Подсчет коэффициента изменений (`diff_ratio`) и отрисовка красных рамок вокруг изменившихся зон.

---

## Установка зависимостей

```bash
pip install -r tools/gui_tester/requirements.txt
```

## MCP-сервер

`python tools/gui_tester/mcp_server.py` — те же возможности как MCP-инструменты (скриншоты возвращаются изображениями, координаты относительно окна, OCR, ожидания, baseline-diff). Подключение и список инструментов: см. раздел 0 в `AI_GUIDE.md`.

---

## Использование через CLI

Все команды поддерживают флаг `--json` для программного вызова и интеграции с нейросетями.

### 1. Список окон
```bash
python tools/gui_tester/cli.py list-windows --filter Vibecodium --json
```

### 2. Снятие скриншота окна и копирование в буфер обмена
```bash
python tools/gui_tester/cli.py capture --window Vibecodium --output screenshots/screen.png --to-clipboard --json
```
*(После выполнения изображение уже в буфере обмена — нажмите Ctrl+V в редакторе!)*

### 3. Поиск кнопки / иконки на экране
```bash
python tools/gui_tester/cli.py find-image --template templates/file_btn.png --threshold 0.85 --json
```
Вывод:
```json
{
  "found": true,
  "score": 0.982,
  "x": 120,
  "y": 45,
  "width": 32,
  "height": 32,
  "center": [136, 61],
  "success": true
}
```

### 4. Клик по найденной кнопке
```bash
# Клик по центру шаблона:
python tools/gui_tester/cli.py click --template templates/file_btn.png --window Vibecodium

# Или клик по точным координатам:
python tools/gui_tester/cli.py click --coords 136 61
```

### 5. Наведение мыши (Hover) для всплывающих подсказок / меню
```bash
python tools/gui_tester/cli.py hover --template templates/settings_icon.png
```

### 6. Нажатие хоткеев
```bash
python tools/gui_tester/cli.py hotkey --keys ctrl p
```

### 7. Сравнение скриншотов (Diff)
```bash
python tools/gui_tester/cli.py diff --baseline screenshots/before.png --current screenshots/after.png --diff-out screenshots/diff.png --json
```

---

## Использование в Python коде

```python
from tools.gui_tester.core import (
    WindowManager, ScreenCapture, ImageSearch, InputController, VisualDiff, AppSession
)

# 1. Запуск приложения и ожидание окна
session = AppSession("aquilum-gpui/target/debug/vibecodium.exe")
session.start(wait_secs=3.0, window_title="Vibecodium")

# 2. Поиск окна и скриншот в буфер обмена
win = WindowManager.find_window("Vibecodium")
WindowManager.focus(win.hwnd)
img = ScreenCapture.capture_window(win, output_path="screenshots/main.png")
ScreenCapture.copy_image_to_clipboard(img)

# 3. Клик по кнопке шаблона
InputController.click_template("templates/close_tab_btn.png", timeout=5.0)

# 4. Завершение
session.stop()
```
