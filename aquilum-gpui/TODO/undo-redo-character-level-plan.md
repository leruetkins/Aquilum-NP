# Посимвольный undo/redo по образцу AkelPad, единый для одноокна и мультиокон

Ревизия 1. Одобрено 2026-10-20.

## Поведение (семантика AECO_DETAILEDUNDO из AkelPad, проверено по исходникам AkelEdit.c)

- **Каждый символ — отдельный шаг undo**: Ctrl+Z после набора слова отменяет по букве (AkelPad: `AE_EditChar` → `AE_StackUndoGroupStop` при `AECO_DETAILEDUNDO`, AkelEdit.c:21718). Цепочка backspace/delete — тоже посимвольно (в AkelPad это так в обоих режимах: GroupStop до и после каждого удаления, AkelEdit.c:21804–21808).
- Дискретные операции — один шаг за вызов: paste, замена/удаление выделения, автоотступ Enter, табличный LineEdit (таблица атомарна). Redo симметричен.
- **Взаимоисключающие действия вычищаются** (адаптация `AE_StackUndoGroupStop`, AkelEdit.c:7683–7696): если сразу после шага «чистая вставка X в позиции P» идёт шаг «чистое удаление X в позиции P», оба удаляются из истории — напечатал и тут стёр = ничего не было. В нашей модели delete→insert-вариант уже слит в один TextChange, поэтому нужна только пара insert→delete.
- **Выделение восстанавливается** при undo/redo (раньше сбрасывалось в None). AkelPad решает это элементами `AEUN_SETSEL` и флагами каретки (`AEUN_BACKSPACEKEY`/`AEUN_DELETEKEY`/`AEUN_CARETATSTART`, AkelEdit.c:21206–21215); у нас эквивалентно и проще: `sel_before`/`sel_after` на каждом шаге.
- Лимит истории: **10 000 шагов, FIFO — выбрасывается старейший** (AkelPad: лимит считается группами, выбрасывается старейшая группа, AkelEdit.c:7802–7827). Прежние 1000 шагов для посимвольного undo слишком мало.
- Склеек и временного окна нет; Kind-классификация (Typing/Deleting/Other/Block) и merge-правила не переносятся — при посимвольной гранулярности нечего склеивать.

## Архитектура

Новый модуль **`src/workspace/session_core/undo.rs`** — единственная реализация стека (прецедент: `ops.rs` — общее ядро без GPUI, живой код уже импортирует оттуда типы). Ядро не знает о rope/String:

```rust
pub struct TextChange { pub at: usize, pub removed: String, pub inserted: String } // переезжает из state.rs
pub struct Selection { pub cursor: usize, pub anchor: Option<usize> }              // совпадает с TabSession gpui и CursorState session_core
pub struct Step { pub changes: Vec<TextChange>, pub sel_before: Selection, pub sel_after: Selection }
pub struct History { undo: VecDeque<Step>, redo: VecDeque<Step> } // кап 10_000 только на undo
impl History { pub fn record(&mut self, step: Step); pub fn take(&mut self, redo: bool) -> Option<Step>; }
```

- **VecDeque, а не Vec**: `push_back`/`pop_back` — стековая семантика; FIFO-выброс старейшего шага — `pop_front` O(1) вместо прежнего `undo_stack.remove(0)` (document_buffer.rs:711–714), который при заполненном стеке делал memmove всего стека на каждой правке.
- `record`: если новый шаг — чистое удаление, в точности инвертирующее последний шаг (same at, removed == prev.inserted, оба чистые) — вытолкнуть последний и не записывать новый (взаимоисключение). Иначе: `push_back`, очистка redo, при переполнении `pop_front`.
- Кап только на undo-стек; redo по построению не превышает undo-историю (шаг попадает в redo только через take).
- `take`: `pop_back` со своего стека, `push_back` на противоположный, возвращает шаг.

## Шаги реализации

1. **`session_core/undo.rs`** + экспорт в `session_core/mod.rs`. Unit-тесты: record чистит redo; take перекладывает между стеками; кап FIFO; взаимоисключение (точная пара уничтожается, неполная — остаётся); выделение проходит через шаг без потерь.
2. **`document_buffer.rs`**: поля `undo_stack`/`redo_stack` → `history: History`; `record_change` без коалесценции (убраны строки 674–709 и локальный cap); `undo_step`/`redo_step` через `take`, наружу возвращают `Selection`; **удалён дублирующий механизм** (прод-код не вызывал — только тесты): `apply_edit`, `undo`, `redo`, мёртвые `update_from_active_note`, `take_content`. Тесты переведены на `record_change`/`undo_step`/`redo_step`. `reload` продолжает очищать историю (в AkelPad то же — `AE_EmptyUndoBuffer` на загрузку).
3. **`view_state.rs`**: 7 вызовов `record_change` передают пары `Selection` (cursor_offset + selection_anchor до/после); `undo`/`redo` восстанавливают cursor и anchor.
4. **`state.rs`**: `TextChange`/`UndoStep` переезжают в ядро (реэкспорт, старые определения удалены); сигнатуры `active_insert_text`/`active_replace_range` не меняются; тесты склейки переписаны на посимвольность («3 backspace = 3 шага»).
5. **`session_core/document.rs`**: стеки → `History`; `apply_edit` строит `Step` (selection из `cursor_after`), `undo`/`redo` через `take` + применение `TextChange` к String. Правило «undo общий на документ» сохраняется. Критерий: 15 существующих тестов session_core проходят без изменения семантики.
6. **Новые тесты**: посимвольность («abc» тремя вставками → три undo возвращают по символу, обе реализации); взаимоисключение («b» вставили и стёрли → история пуста; «b», затем «c», стёрли оба → один шаг); восстановление выделения; табличный LineEdit + undo атомарен (существующий тест зелёный).
7. **Проверка**: `cargo check` и полный `cargo test` — зелёные.

## Вне scope

- Фаза 8а плана мультиокон (единый владелец текста через DocumentRegistry) — отдельно, после baseline-замеров (TODO/multi-document-workspace-plan.md).
- Персистентность undo между открытиями/перезапуском — вне scope обоих планов.
- Элементы `AEUN_SETSEL` как отдельные шаги (undo выделения без текста) — AkelPad-специфика, у нас sel на шаге.
- Мультикурсоры, перенумерация нумерованных списков (в gpui перенумерации не существует — переносить нечего).

## Ограничения AGENTS.md

- Новых UI-компонентов нет — изменения только в состояниях/методах существующих структур; рендер и внешний вид не меняются.
- Комментарии в коде — по-английски; новых пользовательских строк нет (i18n не затрагивается).
- Хирургические правки: соседний код, стили и форматирование не трогаем.
- Критерии готовности: `cargo test` зелёный; в репо ровно одна реализация стека undo (grep `undo_stack` находит только ядро и его потребителей); посимвольность, взаимоисключение и восстановление выделения покрыты тестами.

## Ожидаемый объём

`undo.rs` ~150 строк с тестами; правки в 5 существующих файлах, ~350–450 строк дельты (около половины — удаление дублей и перенос тестов).
