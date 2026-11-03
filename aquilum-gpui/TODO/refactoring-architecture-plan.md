# Генеральный план-задание по рефакторингу структуры aquilum-gpui

> **Базовое состояние проекта перед стартом:**
> - `cargo test`: **701 passed; 0 failed** (фиксированное число тестов)
> - `cargo check`: **OK**
> - `cargo clippy --all-targets`: содержит предсуществующую deny-ошибку `almost_swapped` в `src/views/image_editor.rs:418` (`let frame = editor.frame; editor.frame = frame;`). В соответствии с правилом «ничего не менять в поведении» эта строка не трогается; после каждого шага контролируется отсутствие *новых* ошибок.
> - **Паттерн модулей (Rust 2018/2021):** модуль `foo.rs` остаётся в корне `src/foo.rs`, а его подмодули размещаются в `src/foo/bar.rs`. Переименование файлов в `mod.rs` исключено, что сохраняет чистый git diff и структуру импортов.
> - **Связанность платформ:** все пары `#[cfg(windows)]` и `#[cfg(not(windows))]` остаются в единых файлах (`platform_color.rs`, `clipboard.rs`).
> - **Особый статус `plugins/runtime.rs`:** 5.7k-строчная функция `setup_plugin_environment` остаётся цельной во избежание деградации замыканий QuickJS; из файла выносятся только тесты.

---

## 0. Общие правила выполнения каждого шага

Артефакты baseline лежат в `target/refactor-baseline/`: `tests-before.txt`
(нормализованный `cargo test -- --list`, 701 имя), `check-before.log`,
`clippy-before.log`.

- **Перенос только скриптом по диапазонам строк.** Руками допускается только
  `mod` / `use` и модификаторы видимости. После каждого шага показать
  `git diff --color-moved` и подтвердить, что изменены только места переноса.
- **Видимость.** Приватные элементы, которые используются из другого подмодуля,
  получают `pub(super)`. В родителе добавляется `use self::<sub>::*;` (для
  приватных) или `pub(crate) use self::<sub>::*;` (для `pub(crate)`), чтобы
  внешние пути `crate::module::Item` не менялись. Изменения видимости выполняются
  отдельным подшагом (без коммита), затем идёт перенос. После соответствующих шагов
  проверить внешние `pub(crate)`-имена: `app_view` — 28 (`SCROLL_SETTLE_FRAMES`,
  `RowShapedLayout`, `render_central_editor`, `handle_key`, `open_find_bar`,
  `commit_title_rename_internal`, `table_lines_slice`, `select_note`, `save_note`
  и др.), `markdown` — 4 (`GrammarTagScan`, `scan_grammar_tag`,
  `parse_grammar_content`, `span_text`), `goto_search` — 13 (`GotoSearchParams`,
  `MAX_RESULTS`, `MAX_SCAN_FILES`, `parse_search_op`, `search_goto_files_opts`,
  `search_with_index`, `highlight_*`, `word_range_at` и др.).
- **Список тестов.** `cargo test -- --list` после шага нормализуется и
  сравнивается с `tests-before.txt`; имена должны совпасть полностью (701).
- **Clippy.** Baseline: 1 deny-ошибка `almost_swapped` в
  `src/views/image_editor.rs:418`; предупреждений 254 (bin) / 294 (test) на старте.
  Сравнивать отпечаток ошибки и отсутствие **новых** warning (уменьшение допустимо,
  например снятие `items after a test module` после выноса тестов `images.rs`),
  а не exit-код.
- **`cargo check`.** Baseline: 6 строк `^warning` в
  `target/refactor-baseline/check-before.log`; новых быть не должно.
- **GPUI / AGENTS.md.** Не менять существующие `id` (`note-title-panel`,
  `note-title-input`, `leaf-tab-strip-*`, `leaf-tab-close-*`, `leaf-tab-plus-*`)
  и хитбоксы; комментарии переносить дословно, новые — только по-английски.
- **Фаза D.** После шагов, меняющих `render`, `render_leaf_tab_bar_strip`,
  `render_workspace_*`, снять скриншот до/после (`tools/gui_tester`) и
  подтвердить отсутствие визуального diff.
- **Коммиты не делаются.** Изменения остаются в рабочем дереве; после шага
  остановиться и ждать «дальше». Финальный коммит делает пользователь.

---

## Шаг 0. Baseline и фиксация плана

- [ ] **Шаг 0: Baseline и фиксация плана**
  - Сохранить `cargo test -- --list` в `target/refactor-baseline/tests-before.txt`
    (нормализованный список, 701 имя), `cargo check --all-targets` в
    `check-before.log`, `cargo clippy --all-targets` в `clippy-before.log`.
  - Коммит и тег не создаются: изменения остаются в рабочем дереве до финального
    коммита пользователя.
  - Проверка: 701 имя в списке; clippy содержит только предсуществующую
    deny-ошибку `almost_swapped`.

---

## Фаза A. Только вынос тестов (производственный код не меняется, минимальный риск)

- [ ] **Шаг 1: Вынос тестов базовых утилит и структур**
  - Модули: `document_buffer`, `editor_window`, `formatting`, `line_index`, `code_scan`, `snap`, `settings`.
  - Действие: замена `#[cfg(test)] mod tests { ... }` на `#[cfg(test)] mod tests;` с созданием `src/<module>/tests.rs`.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 2: Вынос тестов документов, таблиц и экспорта**
  - Модули: `markdown_doc`, `table`, `pdf_export`.
  - Действие: создание `src/markdown_doc/tests.rs`, `src/table/tests.rs`, `src/pdf_export/tests.rs`.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 3: Вынос тестов поисковой подсистемы**
  - Модули: `search/index`, `search/worker`, `views/find_bar`.
  - Действие: создание `src/search/index/tests.rs`, `src/search/worker/tests.rs`, `src/views/find_bar/tests.rs`.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 4: Вынос тестов подсистемы изображений**
  - Модули: `images` (1 460 строк тестов), `image_render`.
  - Действие: создание `src/images/tests.rs`, `src/image_render/tests.rs`.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 5: Вынос тестов QuickJS рантайма плагинов**
  - Модуль: `plugins/runtime` (1 540 строк тестов).
  - Действие: создание `src/plugins/runtime/tests.rs`. Производственный код функции `setup_plugin_environment` остаётся нетронутым.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 6: Вынос тестов остальных модулей плагинов**
  - Модули: `plugins/ui`, `plugins/loader`, `plugins/bridge`, `plugins/compat`, `plugins/registry`, `plugins/manifest`, `plugins/api`, `plugins/code_block`, `plugins/editor_toolbar`.
  - Действие: создание соответствующих `<module>/tests.rs`. Фикстура остаётся на месте:
    `src/plugins/tests/ui_golden_bundle.json`. В `src/plugins/ui/tests.rs` путь обязан быть
    `include_str!("../tests/ui_golden_bundle.json")`: после переноса относительный путь
    разрешается от нового файла, прежний `include_str!("tests/...")` сломает сборку.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 7: Вынос тестов доменного состояния `state.rs`**
  - Модуль: `state` (2 880 строк тестов).
  - Действие: создание `src/state/tests.rs`.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 8: Вынос тестов состояния вьюпорта воркспейса**
  - Модуль: `workspace/view_state` (772 строки тестов).
  - Действие: создание `src/workspace/view_state/tests.rs` и локального фикстура `src/workspace/view_state/test_support.rs` для `scratch_doc()`.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 9: Вынос тестов главного монолита `views/app_view.rs`**
  - Модуль: `views/app_view` (1 717 строк тестов).
  - Действие: создание `src/views/app_view/tests.rs`.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 10: Вынос тестов остальных модулей (критерий: тестовый блок ≥ 250 строк или модуль дробится в фазах B–D)**
  - Модули: `views/settings_view`, `views/plugin_catalog`, `views/goto_search`, `views/tab_list`,
    `views/sidebar/bookmarks`, `perf`, `save_manager`, `workspace/tree`.
  - `perf.rs`: `#[cfg(test)] fn measure_key_repeat` (строка 136, вне `mod tests`) переносится
    в `src/perf/tests.rs` вместе с тестами — он используется только тестами.
  - `goto_search.rs`: `#[cfg(test)] fn find_term_ranges` (строка 1298, вне `mod tests`)
    переносится в `src/views/goto_search/tests.rs`.
  - Остаются inline (тестовый блок < 250 строк и модуль не дробится):
    `views/command_palette`, `views/tabs_menu`, `views/scrollbar`, `views/text_input`,
    `views/new_tab`, `views/notification`, `views/image_ui`, `views/editor_context_menu`,
    `views/ribbon_menu`, `views/sidebar/folder_actions`, `views/sidebar/recent_files`,
    `views/sidebar/tree`, `vault`, `md_export`, `i18n`, `commands`, `commands_builtin`,
    `ipc`, `icons`, `ribbon`, `toolbar`, `search/{fold,query,tokenize,pages,locate,cache,cancel}`,
    `fs_watcher/*`, `workspace/persistence`.
  - Действие: создание соответствующих `<module>/tests.rs`.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

---

## Фаза B. Чистые вспомогательные модули и парсеры (низкий риск)

- [ ] **Шаг 12: Выделение физики и сглаживания скролла `app_view/physics.rs`**
  - Что переносится: `WheelEase`, константы `WHEEL_*`, `WIDE_THUMB_H`, `cubic_bezier`, `wheel_glide_duration_secs`, `editor_middle_cursor_target`, `EDITOR_MIDDLE_EDGE_PAD_*`.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 13: Выделение верстки таблиц `app_view/table_layout.rs`**
  - Что переносится: `TableColResizeDrag`, `TableLayout`, `TableCellLine`, `TableCellLayout`, `TableRowLayout`, `fit_table_columns`, `compute_table_layout`, `shape_table_row`.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 14: Выделение шейпинга строк `app_view/row_shape.rs`**
  - Что переносится: `RowShapedLayout`, `LineKindTag`, `headings_cache_key`, `selection_chars_in_line`, `shape_row`.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 15: Выделение отрисовки разметки `app_view/paint/{grammar,whitespace}.rs`**
  - Что переносится: сегменты линий грамматики и точечные квады невидимых пробельных символов.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 16: Выделение отрисовки синтаксиса `app_view/paint/{syntax,live_row}.rs`**
  - Что переносится: спаны инлайн-кода, чипы кода, маркеры, `callout_icon`, `render_live_styled_row`.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 17: Выделение цветов `markdown/colors.rs` и `markdown/platform_color.rs`**
  - Что переносится: пресеты заголовков, HEX-парсинг; обе ветки `#[cfg(windows)]` / `#[cfg(not(windows))]` пипетки и системного палитрового диалога вместе в `platform_color.rs`.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 18: Выделение сканера кода и классификатора `markdown/{code_scan,classify}.rs`**
  - Что переносится: `CodeBlockKind`, `CodeScanner`, `LineKind`, `classify_line`, `TaskPrefix`, `task_*`;
    плюс `marker_prefix_len` (строка 2919) в `classify.rs` — он нужен и `line_inline_spans`,
    и `row_prefix_offset`, поэтому не должен оставаться в `quote.rs`.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 19: Выделение грамматик Markdown `markdown/grammar.rs`**
  - Диапазон: 876–1231. Переносится: `SyntaxSpan`, `GrammarKind`, `GrammarLine`,
    `GrammarSpanStyle`, `GrammarTagScan`, `scan_grammar_tag`, `grammar_paint_text`,
    `parse_grammar_content`, `push_grammar_text`, цветовые функции.
  - Важно: диапазон 876–1231 находится внутри прежнего «inline 620–1491»; границы проверены
    построчно, шаг идёт **до** inline-шага, иначе диапазоны пересекаются.
  - `parse_grammar_content` возвращает `VisibleSpan` из будущего `inline.rs`; после шага 20
    он должен резолвиться через реэкспорт родителя (`use self::inline::*;`), иначе шаг 20
    сломает `grammar.rs`.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 20: Выделение инлайн-парсинга `markdown/{inline,syntax_spans}.rs`**
  - `inline.rs`: 620–875 (`InlineSpan`, `parse_inline_spans`, backtick/url-хелперы) и
    1232–1491 (`InlineReveal`, `VisibleSpan`, `span_text`, `line_inline_spans`,
    `inline_element_ranges`, `inline_reveal`, `selection_chars_in_line`).
  - `syntax_spans.rs`: 1492–1832 (`parse_inline_syntax_spans`).
  - Границы не пересекаются с шагом 19; `SyntaxSpan` уже доступен из `grammar.rs`.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 21: Выделение виртуальных строк Markdown `markdown/visual.rs`**
  - Что переносится: `VisualRow`, `wrap_line_ranges*`, `PaintedLine*`, `VisualRowHeightIndex`.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 22: Выделение верстки табличных строк `markdown/table_rows.rs`**
  - Диапазон: 2149–2388. Переносится: `TableRowVisualKind`, `CellWrapLine`,
    `table_cell_wrap_lines`, `wrap_painted_cell`, `table_row_wrap_lines`,
    **`slice_visible_spans` (2301)**, **`slice_syntax_span` (2333)**,
    `build_visual_rows_with_table_filter`.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 23: Выделение цитат и списков `markdown/{quote,list}.rs`**
  - Что переносится: `QuotePrefix`, `CalloutKind`, `CalloutPrefix`, буллеты, нумерация списков.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 24: Выделение сегментов отрисовки `markdown/paint.rs`**
  - Что переносится: `PaintedSegment`, `PaintedRow`, `row_painted_text`, очистка ссылок, удаление тегов.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 25: Модуляризация импорта и разрешения изображений `images/{import,resolve,config}.rs`**
  - Что переносится: импорт из буфера/файлов, кэш габаритов, резолвинг путей хранилища.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 26: Модуляризация ссылок на изображения `images/{reference,notes,ref_index}.rs`**
  - `reference.rs`: `RefScan`, `references_in_text`, `references_in_rope`, `scan_references`,
    `references_in_file` (1905–1930, 2226–2248).
  - `notes.rs`: `is_note_file` (2092), `walk_markdown` (2099) — общие для `ref_index` и `trash`,
    поэтому переезжают раньше `trash`; получают `pub(super)`.
  - `ref_index.rs`: `is_same_path` (2249), `is_excluded_note` (2260), `AttachmentRefIndex` (2267),
    `REF_INDEX`, `ref_index`, `invalidate_ref_index`, `warm_ref_index`, `ensure_ref_index`,
    `update_note_references`, `remove_note_from_ref_index`, `rename_note_in_ref_index`,
    `referenced_elsewhere`, `CUT_PROTECTED`, `cut_protected_cell`, `protect_cut`, `release_cut`,
    `is_cut_protected`, `defer_removed_reference`, `reconcile_saved_references` (2267–2586).
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 27: Модуляризация аннотаций и правок `images/{annotate,edit}.rs`**
  - Что переносится: примитивы аннотаций (`Point`, `Shape`, `RectF`), геометрия, сайдкар `ImageEdit`.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 28: Модуляризация разрешенных картинок `images/resolved_image.rs`**
  - Что переносится: `ResolvedImage`, расчет высоты блока, параметры тулбара и подписей.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 29: Модуляризация буфера обмена и корзины `images/{clipboard,trash}.rs`**
  - `clipboard.rs`: `clipboard_raster`, `set_clipboard_attachment_meta`, `clipboard_attachment_meta`,
    `clipboard_file_paths`, `dib_to_bmp`, `mod win_clipboard` (1642–1904) — обе cfg-ветки вместе.
  - `trash.rs`: `trash_dir`, `unique_path`, `trash_pair`, `restore_if_trashed`,
    `is_image_reference_at` (2124), `note_references_attachment` (2167), `move_note_attachments`,
    `trash_note_attachments`, `unused_attachments` (2022–2166, 2587–2783).
    Использует `super::notes::{is_note_file, walk_markdown}` из шага 26.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 30: Модуляризация экспорта PDF `pdf_export/{world,typst,export}.rs`**
  - Что переносится: `MemoryWorld` (typst::World), трансляция Markdown в Typst, компиляция в PDF.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 31: Модуляризация UI-движка плагинов `plugins/ui/{style,tree,render}.rs`**
  - Что переносится: парсер стилей `NodeStyle`, AST `UiNode`, рендерер GPUI `UiSink`.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 32a: Выделение wire-утилит `plugins/bridge/wire.rs`**
  - Что переносится: `validate_network_url`, `base64_encode`, `base64_decode` и связанные константы.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 32b: Выделение чистого поиска `views/goto_search/query.rs`**
  - Что переносится: `SearchOp`, `parse_search_op`, `GotoSearchParams`, `search_goto_files_opts`,
    `search_with_index`, `match_*`, `split_sections`, `make_snippet`, `highlight_*` (547–1343).
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 32c: Выделение сканера `views/find_bar/scan.rs`**
  - Что переносится: `stream_rope`, `scan_rope`, `replace_rope`, `patch_matches`,
    `delete_marked_lines_replacement`, `floor_boundary`.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 32d: Модуляризация `views/tab_list/{model,popup,render,app}.rs`**
  - Что переносится: константы/`TabEntry`/`TabListEvent`/`TabListHeader`/геометрия,
    `impl TabListPopup`, `impl Render`, `impl AppView`.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 33: Модуляризация закладок `views/sidebar/bookmarks/{data,ui}.rs`**
  - Что переносится: модель данных `BookmarksData`, сериализация JSON отдельно от UI-рендеринга.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

---

## Фаза C. Доменное состояние и сессии (средний риск)

- [ ] **Шаг 34: Выделение состояния окна и сессии `state/{window,session}.rs`**
  - Что переносится: `WindowState`, загрузка/сохранение координат, дефолты сессий.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 35: Выделение активных документов `state/documents.rs`**
  - Что переносится: активные документы и вьюхи, диспетчеризация событий редактирования.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 36: Выделение жизненного цикла документов `state/doc_lifecycle.rs`**
  - Что переносится: LRU-кэш, открытие/вытеснение/очистка документов, снимки памяти.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 37: Выделение закрытия вкладок `state/close.rs`**
  - Что переносится: закрытие листьев воркспейса, `CloseEffect`, подтверждения сохранения.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 38: Выделение событий файловой системы `state/fs.rs`**
  - Что переносится: реакция на удаление/переименование/изменение файлов из fs_watcher.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 39: Выделение файлового дерева воркспейса `state/workspace.rs`**
  - Что переносится: открытие хранилища, фильтрация дерева, скрытие вложений, ресканирование.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 40: Выделение навигации и создания заметок `state/nav.rs`**
  - Что переносится: создание безымянных заметок, история назад/вперед, хлебные крошки.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 41: Выделение персистентности хранилища `state/persist.rs`**
  - Что переносится: сохранение грязных документов, недавние файлы, экспорт/импорт сессии v2.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 42: Выделение заголовка и кэшей вьюпорта `workspace/view_state/{title,cache}.rs`**
  - Что переносится: `TitleInput`, состояние выделения таблиц сеткой, кэши вьюпорта.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 43: Выделение редактирования и виртуализации `workspace/view_state/{edit,visual}.rs`**
  - Что переносится: ввод текста, замена диапазона, автоотступ, расчет экранных Y-координат.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

---

## Фаза D. UI бог-объект и представления (высокий риск, строго по одному)

- [ ] **Шаг 44: Вынос корневого рендеринга `app_view/render.rs`**
  - Что переносится: метод `render` трейта `gpui::Render for AppView` (окно CSD, Ribbon, оверлеи).
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 45: Вынос жизненного цикла и мигания каретки `app_view/{doc_lifecycle,blink}.rs`**
  - Что переносится: таймеры автосохранения, сброс кареток, наблюдатели закрытия окна.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 46: Вынос геометрии и табличных операций `app_view/{geometry,table_ops}.rs`**
  - Что переносится: расчет ширин панелей, отступов, коммит ячеек таблиц.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 47: Вынос скролла и навигации курсора `app_view/scroll.rs`**
  - Что переносится: скролл колесом, автоскролл средней кнопкой, перетаскивание скроллбара, навигация каретки.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 48: Вынос мыши, HEX-инпута и заголовка `app_view/{mouse,hex,title}.rs`**
  - Что переносится: клики/выделение в редакторе, ввод HEX-цвета, инпут имени файла (`#note-title-input`).
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 49: Вынос панели поиска и замены `app_view/find.rs`**
  - Что переносится: логика поиска, замена всех, удаление строк, отрисовка `#find-bar`.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 50: Вынос навигации и операций с файлами `app_view/{nav,notes}.rs`**
  - Что переносится: клики по ссылкам, экспорт PDF, создание заметок и папок.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 51: Вынос интеграции плагинов `app_view/plugins.rs`**
  - Что переносится: модальные окна плагинов, боковые панели плагинов, хоткеи инпутов плагинов.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 52: Вынос вкладок и полосы табов `app_view/{tabs,tab_bar}.rs`**
  - Что переносится: `render_leaf_tab_bar_strip`, узлы сплитов `render_workspace_node/leaf`, переключение вкладок.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 53: Вынос диспетчеризации клавиатуры `app_view/keys.rs`**
  - Что переносится: глобальный обработчик `handle_key` и сопутствующие сопоставления хоткеев.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 54: Модуляризация ядра настроек `settings_view/{tab,window,actions,widgets}.rs`**
  - Что переносится: `SettingsTab`, `SettingsWindowManager`, общие UI-компоненты настроек.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 55: Модуляризация вкладок настроек `settings_view/tabs/*`**
  - Что переносится: индивидуальные функции рендеринга 15 вкладок настроек по группам файлов.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 56: Модуляризация инпута плагинов и тулбара настроек `settings_view/{plugin_input,toolbar_settings,render}.rs`**
  - Что переносится: реализация `UiSink` для настроек, настройки панелей инструментов, `Render`.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 57: Модуляризация состояния каталога плагинов `plugin_catalog/{state,format}.rs`**
  - Что переносится: парсинг реестра, форматирование скачиваний и дат, оконный менеджер.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 58: Модуляризация представления каталога `plugin_catalog/{view,actions}.rs`**
  - Что переносится: `PluginCatalogView`, методы установки плагинов в `AppView`.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 59: Опциональная модуляризация редактора картинок `views/image_editor/*`**
  - Что переносится: разделение `views/image_editor.rs` на `{state,crop,annotate,keys,render,caption}.rs`.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

- [ ] **Шаг 60: Опциональная модуляризация просмотрщика картинок `views/image_ui/*`**
  - Что переносится: разделение `views/image_ui.rs` на `{block,viewer,actions,popover}.rs`.
  - Проверка: общий чек-лист §0 (check, список тестов = 701, clippy fingerprint, diff).

## Шаг 11 (опционально): общий `test_support` — выполнять в самом конце и только по отдельному «ок»

- [ ] **Шаг 11: Общий модуль тестовых хелперов `src/test_support.rs`**
  - Это уже не чистый перенос: меняются тела тестовых хелперов, поэтому шаг
    выполняется последним и только по отдельному подтверждению.
  - Точный список:
    - `unique_temp_dir(prefix: &str, tag: &str)` — все 7 мест:
      `src/fs_watcher/mod.rs` (`vibe_fs_watch_*`), `src/search/cache.rs`,
      `src/search/locate.rs`, `src/search/index.rs` (`vibecodium_search_index_*`),
      `src/search/query.rs`, `src/search/worker.rs`
      (`vibecodium_search_worker_*`), `src/views/goto_search.rs`.
    - `write_tmp(dir, name, content)` — `src/search/index.rs`, `src/search/worker.rs`.
    - `png(w, h)`, `canvas(w, h)`, `approx(left, right)` — **не дубликаты**;
      переносятся в общий test-only модуль как есть.
  - Проверка: общий чек-лист §0.

---

## Итоговый чек-лист сдачи
- [ ] Ровно **701 тест пройден**, список имён совпадает с `tests-before.txt`.
- [ ] `cargo check` завершается без ошибок; новых `^warning` нет (baseline — 6 строк).
- [ ] `cargo clippy --all-targets`: отпечаток единственной deny-ошибки `almost_swapped`
      не изменился; на старте 254 (bin) / 294 (test), новых нет (уменьшение допустимо).
- [ ] Внешние пути импорта сохранены 1-в-1; проверены 28 `app_view` + 4 `markdown` +
      13 `goto_search` `pub(crate)`-имён.
- [ ] `git diff --color-moved` показывает только перенос, `mod`/`use` и видимость.
- [ ] Все существующие комментарии, имена и функционал сохранены целиком без сокращений;
      новые комментарии — по-английски.
- [ ] Идентификаторы элементов и хитбоксы GPUI полностью соответствуют `AGENTS.md`
      (`note-title-panel`, `note-title-input`, `leaf-tab-strip-*`, `leaf-tab-close-*`,
      `leaf-tab-plus-*`).
- [ ] Для фазы D снят скриншот до/после и визуального diff нет.
