# Посимвольный undo/redo по образцу AkelPad, единый для одноокна и мультиокон

## Шаг 0. Сохранить план
Записать этот план в `aquilum-gpui/TODO/undo-redo-character-level-plan.md` (по конвенции TODO-документов проекта — на русском).

## Поведение (семантика AECO_DETAILEDUNDO из AkelPad, проверено по исходникам AkelEdit.c)

- **Каждый символ — отдельный шаг undo**: Ctrl+Z после набора слова отменяет по букве (AkelPad: AE_EditChar → GroupStop при AECO_DETAILEDUNDO, AkelEdit.c:21718). Цепочка backspace/delete — тоже посимвольно (GroupStop до и после каждого удаления, :21804–21808).
- Дискретные операции — один шаг за вызов: paste, замена/удаление выделения, автоотступ Enter, табличный LineEdit (таблица атомарна). Redo симметричен.
- **Взаимоисключающие действия вычищаются** (адаптация AE_StackUndoGroupStop, AkelEdit.c:7683–7696): если сразу после шага «чистая вставка X в позиции P» идёт шаг «чистое удаление X в позиции P», оба удаляются из истории.
- **Выделение восстанавливается** при undo/redo (сейчас сбрасывается в None) — эквивалент AEUN_SETSEL/флагов каретки AkelPad, но проще: sel_before/sel_after на шаге.
- Лимит истории: **10 000 шагов, FIFO** (AkelPad: лимит по группам, выбрасывается старейшая, :7802–7827).
- Склеек и временного окна нет; Kind/merge-правила не переносятся.

## Архитектура

Новый модуль **`src/workspace/session_core/undo.rs`** — единственная реализация стека (прецедент: `ops.rs`; живой код уже импортирует типы из session_core). Ядро не знает о rope/String:

```rust
pub struct TextChange { pub at: usize, pub removed: String, pub inserted: String } // переезжает из state.rs
pub struct Selection { pub cursor: usize, pub anchor: Option<usize> }              // совпадает с TabSession gpui и CursorState session_core
pub struct Step { pub changes: Vec<TextChange>, pub sel_before: Selection, pub sel_after: Selection }
pub struct History { undo: VecDeque<Step>, redo: VecDeque<Step> } // кап 10_000 только на undo
impl History { pub fn record(&mut self, step: Step); pub fn take(&mut self, redo: bool) -> Option<Step>; }
```

- **VecDeque, а не Vec** (рекомендация принята): `push_back`/`pop_back` — стек; FIFO-выброс старейшего — `pop_front` O(1) вместо нынешнего `undo_stack.remove(0)` (document_buffer.rs:711–714, memmove всего стека при заполнении).
- `record`: если новый шаг — чистое удаление, в точности инвертирующее последний (same at, removed == prev.inserted, оба чистые) — вытолкнуть последний и не записывать. Иначе: `push_back`, очистка redo, при переполнении `pop_front`.
- Кап только на undo; redo по построению не превышает undo-историю.
- `take`: `pop_back` со своего стека, `push_back` на противоположный, возвращает шаг.

## Шаги реализации

1. **`session_core/undo.rs`** + экспорт в `session_core/mod.rs`. Unit-тесты: record чистит redo; take перекладывает между стеками; кап FIFO; взаимоисключение (точная пара уничтожается, неполная — остаётся); выделение проходит без потерь.
2. **`document_buffer.rs`**: поля → `history: History`; `record_change` без коалесценции (убрать 674–709 и локальный cap); `undo_step`/`redo_step` через `take`, наружу возвращают `Selection`; **удалить дубли** `apply_edit`/`undo`/`redo`/`update_from_active_note`/`take_content` (~200 строк, прод их не вызывает), тесты перевести на `record_change`/`undo_step`/`redo_step`; `reload` очищает историю.
3. **`view_state.rs`**: 7 вызовов `record_change` передают пары `Selection`; `undo`/`redo` восстанавливают cursor+anchor.
4. **`state.rs`**: `TextChange`/`UndoStep` переезжают в ядро (реэкспорт, старые определения удалить); сигнатуры `active_*` не меняются; тесты склейки переписаны на посимвольность («3 backspace = 3 шага»).
5. **`session_core/document.rs`**: стеки → `History`; `apply_edit` строит Step, `undo`/`redo` через `take`; «undo общий на документ» сохраняется; 15 существующих тестов проходят без изменения семантики (критерий).
6. **Новые тесты**: посимвольность («abc» тремя вставками → три undo); взаимоисключение («b» вставили и стёрли → история пуста; «b»+«c» стёрли оба → один шаг); восстановление выделения; табличный LineEdit атомарен (существующий тест зелёный).
7. **Проверка**: `cargo check` и полный `cargo test` — зелёные.

## Вне scope
- Фаза 8а плана мультиокон (единый владелец текста) — отдельно, после baseline-замеров.
- Персистентность undo между открытиями/перезапуском.
- AEUN_SETSEL как отдельные шаги; мультикурсоры; перенумерация списков (в gpui её не существует).

## Ограничения AGENTS.md
- Новых UI-компонентов нет; рендер и внешний вид не меняются.
- Комментарии в коде — по-английски; i18n не затрагивается.
- Хирургические правки: соседний код, стили, форматирование не трогаем.
- Критерии: `cargo test` зелёный; ровно одна реализация стека undo (grep `undo_stack` — только ядро и потребители); посимвольность, взаимоисключение, восстановление выделения покрыты тестами.

## Ожидаемый объём
`undo.rs` ~150 строк с тестами; правки в 5 файлах, ~350–450 строк дельты (около половины — удаление дублей и перенос тестов).