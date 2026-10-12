# Задание: выпадающий список открытых вкладок (как в Obsidian) на Rust + GPUI

## Контекст

Я делаю аналог Obsidian (редактор markdown-заметок) на Rust и GPUI. Нужно повторить выпадающий список всех открытых вкладок, который в Obsidian открывается по шеврону справа от ряда вкладок.

Версия `gpui` и наличие крейта `ui`/`theme` (из Zed) указаны в `Cargo.toml` проекта. Перед написанием кода открой `Cargo.lock` и исходники именно этой версии `gpui` и сверь все API (см. раздел «Что нужно проверить»).

## Ключевое поведение (самое важное)

Прокрутка списка управляется **абсолютной позицией курсора** внутри списка. Колесо мыши, зажатая кнопка и перетаскивание не нужны.

- Курсор у верхнего края списка: виден начал списка.
- Курсор у нижнего края: виден конец списка.
- Между краями offset меняется пропорционально позиции курсора.
- Неподвижный курсор означает неподвижный список. Автопрокрутки «пока курсор в зоне» нет.
- Скорость прокрутки (px списка на px движения мыши) получается сама: `max_scroll / высота_списка`. Чем больше элементов и меньше высота списка, тем быстрее едет список. Отдельно скорость задавать не нужно.
- Прокрутка очень плавная: список не прыгает за курсором, а мягко догоняет целевую позицию.

### Формулы

```
usable = list_height - 2 * EDGE_PAD
t      = clamp((cursor_y - list_top - EDGE_PAD) / usable, 0, 1)
target = -(max_scroll * t)        // в GPUI offset отрицательный: 0 = верх, -max = низ
```

Сглаживание по реальному времени кадра (не по числу кадров):

```
alpha  = 1 - exp(-dt / tau)       // tau ~ 140 мс
offset = offset + (target - offset) * alpha
```

Когда `|target - offset| < 0.3 px`, зафиксировать offset на цели и остановить анимацию.

## Внешний вид (по скриншоту Obsidian)

- Список почти на всю высоту окна, если вкладок много (высота окна минус поля по 16 px сверху и снизу). Если вкладок мало, высота по содержимому.
- Скруглённое окно с тонкой рамкой и небольшими внутренними отступами. Строки по краям обрезаются, это нормально.
- Строка: иконка файла слева, название вкладки (с обрезкой по ширине), галочка справа **только у активной вкладки**.
- Подсветка при наведении: скруглённый фон с горизонтальными отступами от краёв списка. Подсветка независима от галочки.
- Крестика закрытия в строках нет.
- Ширина около 280 px.

## Дополнительные требования

1. **Первое движение мыши.** После открытия списка он показывает активную вкладку (`scroll_to_item(active_ix, Center)`). Первое событие `MouseMove` только запоминается, иначе список прыгнет из-за того, что курсор стоит над шевроном, а не над активной вкладкой.
2. **Порог дрожания.** Движения мыши меньше 1.5 px игнорируются.
3. **Клавиатура.** Стрелки вверх/вниз двигают выбор и прокручивают список (`ScrollStrategy::Nearest`), клавиатура имеет приоритет над мышью (сбрасывает цель мыши). Enter активирует выбранную вкладку, Escape закрывает список.
4. **Клик по строке** активирует вкладку и закрывает список. Закрытие по клику вне списка (`on_mouse_down_out`).
5. **Если всё помещается** (`max_scroll <= 0`), прокрутку по мыши не выполнять.
6. **Производительность.** Используй `uniform_list`: список должен оставаться быстрым при 1000+ вкладок и рендерить только видимые строки.
7. **Не дублируй состояние** прокрутки: источником правды остаётся scroll handle списка.
8. Частота кадров может быть любой (60/120/144 Гц), плавность должна выглядеть одинаково.

## Архитектура

- `TabListPopup` (view): `tabs`, `active_ix`, `scroll: UniformListScrollHandle`, `bounds: Rc<Cell<Bounds<Pixels>>>` (заполняется через невидимый `canvas` в prepaint), `target_y`, `last_mouse_y`, `last_frame`.
- События наружу через `EventEmitter`: `Activate(id)`, `Dismiss` (и при необходимости `Close(id)`).
- Позиционирование над шевроном делает родитель (`deferred(anchored()...)`), в этот компонент оно не входит.

## Что нужно проверить в своей версии gpui (не угадывать)

- Как получить `ScrollHandle` из `UniformListScrollHandle` (поле `base_handle` может быть приватным, тогда ищи публичные `offset()`/`set_offset()`/`max_offset()`).
- Знак offset и границы (`0` сверху, `-max` снизу).
- Сигнатуру `track_scroll` (значение или ссылка), `cx.processor`, `uniform_list`, `scroll_to_item` и `ScrollStrategy`.
- `Window::request_animation_frame`, `canvas`, `viewport_size`.
- Наличие `cx.theme()`, `Icon`, `IconName::{File, Check}`. В чистом `gpui` замени их на свои цвета и SVG.

## Чего не делать

- Не использовать кадровый tick в стиле «скорость от края» и не считать скорость по скорости мыши. Нужна именно абсолютная позиция.
- Не хардкодить `dt = 1/60`.
- Не выводить offset за пределы `-max..=0`.
- Не добавлять в строки крестик закрытия и лишние элементы.
- Не придумывать несуществующие методы gpui. Если API нет, найди его в исходниках или предложи рабочую замену.

## Критерии приёмки

1. Код компилируется на моей версии `gpui` без предупреждений (`cargo check`, `cargo clippy`).
2. Провожу мышью сверху вниз по списку из 200 вкладок: список плавно промотан от начала до конца. С 20 вкладками на ту же дистанцию мыши список едет заметно медленнее.
3. Остановил мышь: список мягко доезжает и замирает без дрожания.
4. На мониторах 60 и 144 Гц плавность одинакова.
5. При открытии активная вкладка видна, список не прыгает до первого осмысленного движения.
6. Стрелки, Enter, Escape и клик вне списка работают.
7. Галочка только у активной вкладки, подсветка наведения отдельно.

## Что вернуть

Готовый файл `tab_list.rs`, краткий список изменений относительно эталонного кода ниже и список мест, где API пришлось адаптировать под мою версию `gpui`.

## Эталонный код (набросок, не компилировался)

Это ориентир по структуре и формулам. Адаптируй под реальный API.

```rust
//! Выпадающий список открытых вкладок.
//! Прокрутка = абсолютная позиция курсора внутри списка.

use std::{cell::Cell, ops::Range, rc::Rc, time::Instant};

use gpui::*;

/// Запас у верха и низа (px): первая и последняя строки достижимы без упора в границу окна.
const EDGE_PAD: f32 = 12.0;
/// Движения мыши меньше порога игнорируются.
const MIN_MOVE: f32 = 1.5;
/// Постоянная времени сглаживания (мс). 80-100 бодро, 150-200 очень плавно.
const SMOOTH_TAU_MS: f32 = 140.0;
const ROW_HEIGHT: f32 = 28.0;
const WINDOW_MARGIN: f32 = 16.0;

#[derive(Clone)]
pub struct TabEntry {
    pub id: usize,
    pub title: SharedString,
}

pub enum TabListEvent {
    Activate(usize),
    Close(usize),
    Dismiss,
}

impl EventEmitter<TabListEvent> for TabListPopup {}

pub struct TabListPopup {
    tabs: Vec<TabEntry>,
    active_ix: usize,
    scroll: UniformListScrollHandle,
    /// Границы контейнера в окне; заполняются в prepaint через canvas.
    bounds: Rc<Cell<Bounds<Pixels>>>,
    /// Желаемый offset (<= 0). None, пока мышь не двигалась.
    target_y: Option<Pixels>,
    last_mouse_y: Option<Pixels>,
    /// Время предыдущего кадра анимации (для dt).
    last_frame: Option<Instant>,
}

impl TabListPopup {
    pub fn new(tabs: Vec<TabEntry>, active_ix: usize, _cx: &mut Context<Self>) -> Self {
        let scroll = UniformListScrollHandle::new();
        scroll.scroll_to_item(active_ix, ScrollStrategy::Center);
        Self {
            tabs,
            active_ix,
            scroll,
            bounds: Rc::new(Cell::new(Bounds::default())),
            target_y: None,
            last_mouse_y: None,
            last_frame: None,
        }
    }

    fn base_handle(&self) -> ScrollHandle {
        self.scroll.0.borrow().base_handle.clone()
    }

    fn on_mouse_move(&mut self, y: Pixels, cx: &mut Context<Self>) {
        // Первое событие только запоминаем, чтобы список не прыгнул.
        let Some(prev) = self.last_mouse_y else {
            self.last_mouse_y = Some(y);
            return;
        };
        if (y - prev).abs() < px(MIN_MOVE) {
            return;
        }
        self.last_mouse_y = Some(y);

        let b = self.bounds.get();
        if b.size.height <= px(0.) {
            return;
        }
        let max = self.base_handle().max_offset().height;
        if max <= px(0.) {
            return; // всё помещается
        }

        let pad = px(EDGE_PAD);
        let usable = (b.size.height - pad * 2.).max(px(1.));
        let t = ((y - b.origin.y - pad) / usable).clamp(0.0, 1.0);

        // В gpui offset отрицательный: 0 = верх, -max = низ.
        self.target_y = Some(-(max * t));
        cx.notify();
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let n = self.tabs.len() as isize;
        if n == 0 {
            return;
        }
        self.active_ix = (self.active_ix as isize + delta).rem_euclid(n) as usize;
        self.target_y = None; // клавиатура важнее мыши
        self.scroll
            .scroll_to_item(self.active_ix, ScrollStrategy::Nearest);
        cx.notify();
    }

    /// Один шаг сглаживания; true, если нужен следующий кадр.
    fn animate(&mut self) -> bool {
        let Some(target) = self.target_y else {
            self.last_frame = None;
            return false;
        };

        // dt по реальному времени: плавность одинакова на 60/120/144 Гц.
        // Первый кадр после простоя берём как 1/60 с, чтобы не было рывка.
        let now = Instant::now();
        let dt = self
            .last_frame
            .map_or(1.0 / 60.0, |t| now.duration_since(t).as_secs_f32().min(0.05));
        self.last_frame = Some(now);

        let alpha = 1.0 - (-dt * 1000.0 / SMOOTH_TAU_MS).exp();

        let base = self.base_handle();
        let mut off = base.offset();
        let diff = target - off.y;
        if diff.abs() < px(0.3) {
            off.y = target;
            base.set_offset(off);
            self.last_frame = None;
            return false;
        }
        off.y += diff * alpha;
        base.set_offset(off);
        true
    }

    fn render_row(&self, ix: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let tab = &self.tabs[ix];
        let id = tab.id;
        let selected = ix == self.active_ix;

        div()
            .id(("tab-row", ix))
            .h(px(ROW_HEIGHT))
            .mx_1()
            .px_2()
            .gap_2()
            .flex()
            .items_center()
            .rounded_md()
            .cursor_pointer()
            .hover(|d| d.bg(cx.theme().colors().element_hover))
            .on_click(cx.listener(move |_, _, _, cx| {
                cx.emit(TabListEvent::Activate(id));
            }))
            // Иконки из крейта ui (Zed); в чистом gpui замени на svg()/свои.
            .child(Icon::new(IconName::File).size(IconSize::Small))
            .child(div().flex_1().truncate().child(tab.title.clone()))
            // Галочка у активной вкладки.
            .when(selected, |d| {
                d.child(Icon::new(IconName::Check).size(IconSize::Small))
            })
    }
}

impl Render for TabListPopup {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Продолжаем анимацию, пока не дошли до цели.
        if self.animate() {
            window.request_animation_frame();
        }

        // Почти на всю высоту окна, если вкладок много.
        let max_h = window.viewport_size().height - px(WINDOW_MARGIN * 2.);
        let height = (px(ROW_HEIGHT) * self.tabs.len() as f32 + px(8.)).min(max_h);
        let bounds = self.bounds.clone();

        div()
            .id("tab-list-popup")
            .relative()
            .w(px(280.))
            .h(height)
            .py_1()
            .rounded_md()
            .border_1()
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _w, cx| {
                this.on_mouse_move(ev.position.y, cx);
            }))
            .on_mouse_down_out(cx.listener(|_, _, _, cx| {
                cx.emit(TabListEvent::Dismiss);
            }))
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _w, cx| {
                match ev.keystroke.key.as_str() {
                    "escape" => cx.emit(TabListEvent::Dismiss),
                    "down" => this.move_selection(1, cx),
                    "up" => this.move_selection(-1, cx),
                    "enter" => {
                        if let Some(t) = this.tabs.get(this.active_ix) {
                            cx.emit(TabListEvent::Activate(t.id));
                        }
                    }
                    _ => {}
                }
            }))
            .child(
                // Невидимый слой: запоминает границы списка в окне.
                canvas(move |b, _, _| bounds.set(b), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .child(
                uniform_list(
                    "tab-list",
                    self.tabs.len(),
                    cx.processor(|this, range: Range<usize>, _w, cx| {
                        range.map(|ix| this.render_row(ix, cx)).collect::<Vec<_>>()
                    }),
                )
                .track_scroll(&self.scroll)
                .size_full(),
            )
    }
}
```
