# 500 плагинов: уточнённые приоритеты

Расширение выборки с 100 до 500 плагинов (то же перемешивание, seed 42 — сотня входит подмножеством).
Цель — проверить, не были ли приоритеты из первого прогона случайными.

**Ответ: не были.** Ранговая корреляция Спирмена между двумя прогонами — **ρ = 0.843** на 113 общих
нехватках. Порядок не перевернулся, уточнились доли.

> **Обновлено 18.10.2026, после патчей 1–2.** Патч 1: `MarkdownView`, `Platform`,
> `workspace.getActiveViewOfType` / `activeLeaf` / `activeEditor` / `layoutReady`,
> `vault.createFolder` / `cachedRead` / `configDir`. Патч 2: `MarkdownRenderer`,
> `workspace.openLinkText` / `iterateAllLeaves` / `getMostRecentLeaf`.
> Разделы 1–4 описывают состояние **до** патчей (они остаются базой для сравнения),
> актуальные цифры — в §0.

---

## 0. Эффект патчей 1–2

Тот же корпус, тот же сканер, изменился только хост.

| Метрика | Исходно | Патч 1 | Патч 2 |
|---|---:|---:|---:|
| `ok` | 36 (7%) | 66 (13%) | **88 (18%)** |
| `degraded` | 242 (48%) | 212 (42%) | 190 (38%) |
| `blocked` | 222 (44%) | 222 (44%) | 222 (44%) |
| без жёстких блокеров | 266 (53%) | 266 (53%) | 266 (53%) |
| плагинов вообще без нехваток API | 52 | 95 | **124** |

Кривая отдачи сдвигается целиком: те же 5 функций покрывают 163 плагина (33%) вместо 84 (17%),
потому что база «уже работает» выросла с 52 до 124.

Что закрыто (было в топ-7):

| # | Нехватка | Было плагинов |
|---:|---|---:|
| 1 | `MarkdownView` | 171 |
| 2 | `workspace.getActiveViewOfType` | 148 |
| 3 | `Platform` | 112 |
| 4 | `vault.createFolder` | 112 |
| 5 | `vault.cachedRead` | 105 |
| 6 | `MarkdownRenderer` | 84 |
| 7 | `vault.configDir` | 76 |

Новый топ нехваток:

```
   41  parseYaml          40  FileSystemAdapter   24  getAllTags
   23  FileView           22  Vault               22  editorLivePreviewField
   22  stringifyYaml      18  editorInfoField     13  sanitizeHTMLToDom
   12  SettingGroup       12  getIconIds          11  requireApiVersion
   11  TextFileView        9  request              9  parseLinktext
```

**Честная оговорка про `MarkdownView`.** Он даёт `file`, `getMode()`, `getViewType()` и участвует в
`instanceof`, то есть отвечает на вопрос «какой файл открыт и в каком режиме». Но `view.editor` —
это хэндл, а не буфер: `getSelection()`/`replaceSelection()` доходят до хоста, а `editor.getValue()`,
`getCursor()`, `lineCount` — нет. Текст документа живёт в Rust, и запроса «отдай буфер» в протоколе
пока не существует. По корпусу это следующий по величине разрыв: 38 плагинов читают `editor.getValue()`,
23 — `getCursor()`, 15 — `getLine()`. Это патч 3, и он протокольный, а не JS-ный.

**Честная оговорка про `MarkdownRenderer`.** Рендерер разбирает markdown сам и выдаёт элементы для
`UiNode`, потому что `UiNode::Markdown` в хосте рисуется как обычный текст (`ui.rs:1041`). Но модель
`UiNode` знает только стили normal/muted/heading/code: жирный, курсив и маркеры списков отрисовать
нечем. Поэтому акценты сплющиваются в обычный текст — иначе `<strong>` попал бы в ветку сериализатора
«жирный без детей → заголовок» и превратил бы слово посреди абзаца в заголовок. Заголовки, код,
цитаты, списки, таблицы и `hr` рендерятся структурно.

---

## 1. Что изменилось

| Метрика | 100 плагинов | 500 плагинов |
|---|---:|---:|
| `ok` | 8% | 7% |
| `degraded` | 51% | 48% |
| `blocked` | 41% | 44% |
| без жёстких блокеров | 55% | 53% |
| медианное покрытие | 78% | 82% |

Выводы устойчивы. Разошлись только два пункта:

- **Canvas вырос с 15% до 18%** и сравнялся с CodeMirror. На сотне казалось, что CM6 важнее;
  на пятистах они равны.
- **Web Worker упал с 10% до 7%** — на сотне его переоценило.

Доля webview стабильна: 3% в обоих прогонах.

---

## 2. Жёсткие блокеры на 500 плагинах

| Блокер | Плагинов | Доля |
|---|---:|---:|
| Canvas 2D / WebGL | 91 | 18% |
| CodeMirror 6 | 88 | 18% |
| Node.js модули | 66 | 13% |
| Web Worker | 37 | 7% |
| Electron API | 35 | 7% |
| iframe | 26 | 5% |
| `<webview>` | 17 | 3% |
| IndexedDB | 14 | 3% |

Плюс 82 плагина (16%) содержат `process.env`/`process.platform` — это предупреждение, а не блокер:
часто мёртвый код после сборки.

Итог по вашей гипотезе: **canvas действительно первостепенен, webview — нет.** 17 плагинов из 500
(3%) — это не тот масштаб, ради которого стоит тянуть Electron.

---

## 3. Кривая отдачи

Ключевой вопрос — не «что добавить», а «сколько это даёт». Считаем: если реализовать первые *k*
нехваток из общего ранжирования, сколько плагинов окажутся покрыты полностью.

| Добавлено функций | Покрыто плагинов | Доля | Прирост |
|---:|---:|---:|---:|
| 0 (сейчас) | 52 | 10% | — |
| 5 | 84 | 17% | +32 |
| 10 | 114 | 23% | +62 |
| 20 | 188 | 38% | +136 |
| 30 | 234 | 47% | +182 |
| 50 | 309 | 62% | +257 |
| 80 | 393 | 79% | +341 |
| 100 | 426 | 85% | +374 |
| 150 | 458 | 92% | +406 |
| 200 | 478 | 96% | +426 |

«Покрыто» — плагины без единой нехватки API. Из 52 базовых 36 действительно `ok`, остальные 16
упираются в движки (canvas, CM6, Node) и методами не лечатся.

Всего различных нехваток в корпусе — **233**. То есть 200 функций закрывают 96% плагинов.

Первые 20 дают +136 плагинов, следующие 30 — ещё +123. Отдача падает, но не обрывается: до 200
функций кривая остаётся почти линейной, потому что «хвост» — это редкие узкие методы, каждый из
которых нужен своему десятку плагинов.

---

## 4. Топ-25 нехваток

| # | Что | Плагинов | Доля | Тип |
|---:|---|---:|---:|---|
| 1 | `MarkdownView` | 171 | 34% | пустой класс |
| 2 | `workspace.getActiveViewOfType` | 148 | 30% | метод |
| 3 | `Platform` | 112 | 22% | пустой класс |
| 4 | `vault.createFolder` | 112 | 22% | метод |
| 5 | `vault.cachedRead` | 105 | 21% | метод |
| 6 | `MarkdownRenderer` | 84 | 17% | пустой класс |
| 7 | `vault.configDir` | 76 | 15% | **свойство** |
| 8 | `workspace.openLinkText` | 63 | 13% | метод |
| 9 | `vault.createBinary` | 52 | 10% | метод |
| 10 | `workspace.iterateAllLeaves` | 51 | 10% | метод |
| 11 | `addStatusBarItem` | 51 | 10% | метод |
| 12 | `fileManager.trashFile` | 46 | 9% | метод |
| 13 | `registerMarkdownPostProcessor` | 46 | 9% | метод |
| 14 | `fileManager.processFrontMatter` | 43 | 9% | метод |
| 15 | `fileManager.renameFile` | 42 | 8% | метод |
| 16 | `vault.readBinary` | 42 | 8% | метод |
| 17 | `vault.getRoot` | 42 | 8% | метод |
| 18 | `workspace.setActiveLeaf` | 37 | 7% | метод |
| 19 | `vault.getAllLoadedFiles` | 36 | 7% | метод |
| 20 | `workspace.getMostRecentLeaf` | 35 | 7% | метод |
| 21 | `vault.getResourcePath` | 32 | 6% | метод |
| 22 | `vault.getName` | 28 | 6% | метод |
| 23 | `metadataCache.resolvedLinks` | 27 | 5% | свойство |
| 24 | `workspace.activeEditor` | 25 | 5% | свойство |
| 25 | `plugins.getPlugin` | 21 | 4% | метод |

Далее по частоте идут `parseYaml` (41), `FileSystemAdapter` (40), `getAllTags` (24), `FileView` (23),
`stringifyYaml` (22), `Vault` (22), `editorLivePreviewField` (22), `editorInfoField` (18),
`registerObsidianProtocolHandler` (19), `registerEditorSuggest` (14), `removeCommand` (9).

---

## 5. Двести плагинов в пределах двух функций

**200 из 500 (40%)** не хватает одной-двух вещей. Это не «почти работает» — это «работает, если
добавить одну строку». Примеры из отчёта:

| Плагин | Чего не хватает |
|---|---|
| `13th-age-statblocks` | `parseYaml` |
| `accounting-journal-ledger` | `vault.cachedRead` |
| `achmage-slides-ultra` | `FileSystemAdapter` |
| `alpha-bullet` | `sanitizeHTMLToDom` |
| `ancient-poetry-printer` | `vault.createFolder` |
| `ai-bot`, `art-de-one` | `MarkdownView` + `getActiveViewOfType` |
| `auto-glossary` | `vault.getRoot` + `vault.cachedRead` |
| `ara` | `vault.rename` |

Отдельно стоит посмотреть на пару случаев, где **ноль нехваток API, но вердикт `blocked`**:

| Плагин | Блокер | Что это значит |
|---|---|---|
| `advanced-formatting` | CodeMirror 6 | весь функционал — расширение редактора, API не трогает |
| `ai-commit` | Node.js модули | работает с git через `fs`/`child_process` |

Это важный класс: плагин идеально ложится на нашу API-поверхность и всё равно не работает, потому
что живёт в движке, которого нет. Такие нельзя «дописать методами» — только тянуть зависимость.

Для контраста: `aql-syntax-highlighting` и `auto-reveal-in-explorer` — просто `ok`, без блокеров
и без нехваток.

---

## 6. Порядок работ, уточнённый

**Корпус 1500, 21.10.2026 — расширение и повтор всех проверок.** Скачано 1497/1500
(3 репозитория мертвы: manifest 404 — `colecao-livros-quadrinhos`, `hugo-sync`, `magneto`).
Корпус: `D:\Naben\zcode-plugin-probe\corpus1500` (500 прежних + 997 новых).

Статический doctor: **`ok` 525, `degraded` 292, `blocked` 680** (движки: process 229,
codemirror 277, canvas 259, node 188, electron 121, worker 100, iframe 76, indexeddb 61,
webview 45). Без жёстких блокеров — 780. Кривая отдачи: 813 плагинов уже без нехваток API;
5 функций → 59% покрытия, 30 → 78%, 100 → 92%. Топ отсутствующих членов `app.*`:
`workspace.containerEl` (54 плагина), `plugins.plugins` (50), `setting.open/openTabById`
(50/49), `vault.getAllFolders` (49), `vault.rename` (48), `secretStorage` (43/29),
`workspace.updateOptions` (42), `requestSaveLayout` (40), `vault.append` (26),
`metadataCache.unresolvedLinks` (16).

Динамический прогон (probe, 780 без блокеров): **777/780 загружаются**; три — честные
движковые отказы (`node:util.promisify` ×2, `os.homedir` ×1). Закрыты по ходу корпуса:
краш пробника на зависших плагинах (unref'нутый wall-clock таймер позволял node молча
выйти с кодом 0 до записи отчёта — убран `.unref()`), глобалы `File`/`Blob`/`AbortController`
(инертные конструкторы), `DocumentFragment` (класс над VElement с `append`),
`document.getElementsByTagName` → `[]` (проба KaTeX), `CodeMirror` (CM5-пространство
регистраций: defineMode/registerHelper/Pos и т.д.), экспорт `htmlToMarkdown` (текстовый
обход дерева через существующий HTML-парсер). Дыра контракта: `SettingItem.control` может
прийти `null` (my-spaces: строка с одной иконкой) — теперь и JS (фолбэк на пустой text),
и serde (`deserialize_with`) прощают это.

UI-телеметрия: **566 плагинов строят деревья, 1546 деревьев** (aggregated: `column` 7511,
`text` 5142, `setting_item` 4225, `button` 3474, `input` 1902, `row` 1404, `toggle` 960,
`select` 694, `textarea` 226, `styled` 188, `slider` 93, `checkbox` 84, `separator` 62).
Inline-стили по-прежнему редки, но не экзотика: `width` 74, `display` 57, `height` 36,
`borderRadius` 27, `margin*` ~45, `color`/`backgroundColor` 24/24, `padding` 9.
Реестр: `out1500/ui-pattern-registry.json`.

Golden-бандл расширен: 566 деревьев / 9853 узла / 1.04 МБ (`ui_golden_bundle.json`),
`--per-plugin=2 --max-bytes=48K`. Пороги в `golden_corpus_translation_coverage` подняты
под новую базу; новый ассерт: rendered 99.7% узлов (0.3% — осознанный `display:none`).
cargo test 497/497, харнесс 193/193, зеркало 10/10.

**Golden-пайплайн 21.10.2026 — телеметрия → тесты.** Деревья `set_view_tree` теперь сохраняются
пробником на диск (`out/trees/<id>.json`, рядом с компактным отчётом) и собираются в golden-бандл
для Rust-тестов: `node make-fixtures.cjs <probe-out> <fixtures-dir>` (tools/plugin-probe) берёт до
3 деревьев на плагин (≤64КБ каждое) и пишет `fixtures/_bundle.json`; бандл хранится в репозитории
(`aquilum-gpui/src/plugins/tests/ui_golden_bundle.json`, 192 дерева / 3003 узла / 338КБ).
Тесты в `ui.rs` (`plugins::ui::tests::golden_*`): `golden_corpus_parses_and_validates` — каждое
живое дерево разбирается serde'ом, переживает round-trip и проходит `validate_tree`;
`golden_corpus_translation_coverage` — чистая функция `tree_translation_stats` считает покрытие
трансляции (узлы по видам, разобранные inline-стили, layout-стили, текстовые байты) и утверждает
пороги базы. База на корпусе 2026-10: `setting_item` 1186 (39.5%), `input` 405, `column` 326,
`text` 388, `toggle` 298, `select` 148, `button` 116, `textarea` 45, `slider` 33, `row` 17,
`checkbox` 11, `separator` 29, `styled` 1; inline-стилей почти нет (width ×1) — оформление идёт
классами и styles.css. Тест падает при регрессии: serde-дрейф (bootstrap ↔ Rust), отказ
`validate_tree` на живом дереве, потеря видов узлов или стилевых свойств. cargo test 497/497.

**Статус на 19.10.2026.** Пачка 1 закрыта целиком, из пачки 2 — пять пунктов, плюс оба P0.
Итог по корпусу 500: `ok` 91, `degraded` 187, `blocked` 222 (без жёстких блокеров 266, 53%).

**Динамический прогон 21.10.2026 (probe) — фаза активации.** После добавления фазы активации
(дёргать до 12 зарегистрированных команд, тикать `__check_timers`, триггерить
`active-leaf-change`) телеметрия вскрыла ленивые плагины: **265/266 загружаются**, UI-деревьев
465 (было 191), кнопок 814 (было 111). `appCalls` (через Proxy на `globalThis.app`, т.к. плагины
берут app из конструктора) дали точную карту вызовов; сверка с хостом закрыла последние методы:
`vault.getFolderByPath` (реальная проверка префикса), `vault.getConfig` → `null`,
`app.keymap` (push/pop/getRootScope), `app.hotkeyManager`, `app.secretStorage` (getSecret → null),
`workspace.rootSplit/leftSplit/rightSplit` → сам workspace (единый лейаут), `request` (promise-of-text
над `requestUrl`). После фиксов: `ok` 175 → **183**, `degraded` 95. Реальные missing-экспорты на
корпусе: только `loadMermaid` (2) и `getIconIds` (1) — уже добавлены в экспорты; `__esModule` —
шум трансляции. Единственный незагруженный — `attachment-uploader` (`node:util.promisify`,
движковый блокер вне JS-компетенции). Реестр обновлён: `out/ui-pattern-registry.json`.

**Динамический прогон 21.10.2026 (probe) — финал.** Инструмент `plugin-probe` (REA:
`.workbuddy-ai/tools/plugin-probe/`, данные и реестр: `D:\Naben\zcode-plugin-probe\out\`,
README внутри) исполняет реальный `main.js` 266 плагинов без движковых блокеров в изолированном
vm-контексте с настоящим bootstrap'ом. После добавления найденных прогоном пробелов:
**264/266 загружаются** (единственный оставшийся — `attachment-uploader`, тянет `node:util.promisify`
— это движковый блокер, который статика не видела). Найденные и закрытые пробником дыры:
`Buffer`/`global`/`speechSynthesis`/`HTMLElement`/`Image`/`self`/`crypto.getRandomValues`/
`window.addEventListener`/`document.addEventListener`/`TransformStream`-семейство — стенд-ины
бутстрапа; `loadMermaid`/`loadPrism` → `null`, `getIconIds` → `[]`, `getIcon` → `null`;
`parseFrontMatterEntry`/`parseFrontMatterTags`/`parseFrontMatterAliases`/`parseFrontMatterStringArray`
— настоящие экспорты над frontmatter-объектами. После фиксов: `ok` 170 → **175**.

UI-телеметрия (185 плагинов строят деревья): `setting_item` 1178, `text` 474, `input` 407,
`column` 378, `toggle` 296, `select` 145, `button` 120, `textarea` 43, `slider` 33 — **90%+ UI
плагинов — Setting-формы**, все типы поддержаны; inline-стили почти не используются (2 свойства
на весь корпус) — оформление идёт классами и styles.css. Кнопки: default/cta/warning.
Реестр паттернов для GPUI-трансляции: `out/ui-pattern-registry.json`. Хост-каналы: `add_command`
1316, `load_data` 411, `set_view_tree` 362, `add_ribbon_icon` 185, `register_code_block_processor`
62, `save_data` 52, `create_folder` 34, `add_status_bar_item` 34 (работает), `notice` 22.
Дальше пробником: интерактивные сценарии (ленивые колбэки 45+ плагинов, пока без вызовов).

**Динамический прогон 21.10.2026 (probe) — первый заход.** Новый инструмент `plugin-probe` (REA:
`.workbuddy-ai/tools/plugin-probe/`, данные: `D:\Naben\zcode-plugin-probe\out\`) исполняет
реальный `main.js` 266 плагинов без движковых блокеров в изолированном vm-контексте с настоящим
bootstrap'ом. Результаты первого прогона: **243/266 загрузились**; не хватает API почти ничего —
`loadMermaid` (2), `getIconIds` (1), остальное — Node/browser-глобалы (`Buffer`, `global`,
`Image`, `speechSynthesis.getVoices`, `node:util.promisify`, `window.addEventListener`).
`Buffer`/`global`/`speechSynthesis` добавлены в бутстрап (минимальные стенд-ины), `loadMermaid`/
`loadPrism` → `null` (честная ветка «библиотеки нет»), `getIconIds` → `[]`, `getIcon` → `null`.
После фиксов: `ok` 170 → **175**. UI-телеметрия (165 плагинов строят деревья, 2 869 узлов):
`setting_item` 1075, `text` 445, `input` 377, `column` 353, `toggle` 262, `select` 138,
`button` 111, `textarea` 37, `slider` 29 — т.е. **90%+ UI плагинов — это Setting-формы**, все
типы которых уже поддержаны; inline-стили почти не используются (1 свойство на весь корпус) —
оформление идёт классами и styles.css. Кнопки: default 84 / cta 25 / warning 2. Реестр паттернов:
`out/ui-pattern-registry.json`. Хост-каналы: `add_command` 609, `load_data` 196, `set_view_tree` 171,
`add_ribbon_icon` 89, `register_code_block_processor` 30, `save_data` 24, `create_folder` 17,
`add_status_bar_item` 16 (уже работает), `notice` 10. 45 плагинов не сделали ни одного вызова —
их активность ленивая (колбэки, таймеры), для них следующий шаг — интерактивные сценарии пробника.

**Статус на 21.10.2026, этап 5.** Утилиты: `sanitizeHTMLToDom` (переиспользует sanitize+parse
пайплайн, отдаёт фрагмент с `.children`/`appendTo`), `getFrontMatterInfo` (exists/from/to/
frontmatter/frontmatterPos), `resolveSubpath` (heading-совпадения, блок-референсы честно null —
кэша блоков нет), `prepareFuzzySearch`/`prepareSimpleSearch` (подмножество: contiguity и boundary
бонусы, без `matches`-координат). Классы `FileView`/`TextFileView` (`data`, `setContent` →
`onSetData`, `getDoc`), экспортированы; `instanceof`-цепочка `TextFileView → FileView → ItemView`
работает. `app.plugins` (`.manifests`/`.plugins`/`.enabledPlugins`/`getPlugin`) и `app.commands`
(`.commands`/`listCommands`) — зеркала реестров через snapshot (в `Snapshot` добавлены
`commands: [CommandInfo]` и `plugins: [PluginInfo]`; плагинный список публикуется при
загрузке/выгрузке плагинов, командный — из `CommandRegistry` при каждом `publish_snapshot`).
`adapter.list` ходит в новый запрос `vault_list_dir` (реальный листинг каталога, скрытые
исключены, отсутствующий каталог → пустой список). **Статус-бар сделан по-настоящему**:
`addStatusBarItem` регистрирует слот в `StatusBarRegistry` хоста (запрос `add_status_bar_item`),
`el.setText()` обновляет текст (`update_status_bar_item`); рендер — существующий цикл статус-бара
(`app_view.rs`). Итог по корпусу 500: **`ok` 170, `degraded` 108, `blocked` 222**.
Харнесс 189/189 (16 новых тестов). Осталось из хост-слотов этапа 3: вызов
`registerMarkdownPostProcessor`-ов при рендере и протокольных хендлеров при `obsidian://`-переходе.

**Статус на 21.10.2026, этап 4.** Пачка 2 закрыта: бинарный канал base64-в-JSON —
`vault.readBinary`/`createBinary`/`modifyBinary` через запросы `vault_read_binary`/
`vault_create_binary`/`vault_modify_binary` (api.rs; `vault.rs::read_file_binary`/
`create_file_binary`/`modify_file_binary` с лимитом 50 МБ; бридж кодирует/декодирует base64,
права vault:read/write сохранены). `adapter.readBinary`/`writeBinary` больше не бросают —
пишут через create-с-фолбэком-на-modify. `FileSystemAdapter` — статические методы через
vault-канал (без абсолютных путей диска: песочница). Итог по корпусу 500:
**`ok` 168, `degraded` 110, `blocked` 222**. Харнесс 173/173 (8 новых тестов).

**Статус на 21.10.2026, этап 3.** Закрыты `addStatusBarItem` (JS-слот, рендер в статус-бар — TODO),
`registerMarkdownPostProcessor` (регистрация и вызов при рендере — TODO хост-хук), `removeCommand`
(снимает и prefixed-id в реестре команд хоста), `registerObsidianProtocolHandler`,
`registerEditorSuggest`, `Notice.setMessage`/`hide` (запросы `notice_update`/`notice_hide`),
`Menu.hide`/`close`/`onHide`/`setNoIcon`/`setUseNativeMenu`/`setParentElement`, `Setting.setDisabled`
/`addSearch`/`addMomentFormat`/`addColorPicker`/`addProgressBar`/`then`/`clear` + классы
`SearchComponent`/`MomentFormatComponent`/`ColorComponent`/`ProgressBarComponent`.
Итог по корпусу 500: **`ok` 148, `degraded` 130, `blocked` 222**. Харнесс 165/165 (19 новых тестов).

**Статус на 21.10.2026.** Пачка 1 закрыта целиком (YAML-парсер в бутстрапе покрывает
frontmatter-подмножество: плоские ключи, вложенные карты, списки скаляров и объектов,
inline-flow, block-скаляры, комментарии; полный YAML — якоря, теги, многодокументность —
за пределами и честно падает). Закрыты: `parseYaml`, `stringifyYaml`, `getAllTags`,
`requireApiVersion` (хост = 1.7.5), `parseLinktext`, `getLinkpath`,
`arrayBufferToBase64`/`base64ToArrayBuffer` (своя base64 — btoa/atob в rquickjs нет),
`fileManager.processFrontMatter`/`trashFile`/`renameFile`, `vault.getRoot`/`getAllLoadedFiles`/
`getName`/`getResourcePath`. Итог по корпусу 500: **`ok` 123, `degraded` 155, `blocked` 222**.
Харнесс 146/146 (21 новый тест: 11 YAML, 7 FileManager/vault, 3 утилиты).

> **P0 закрыты** (проверено по коду приложения, коммит `f94d2a3`): `vault.read` пробрасывает ошибку
> вместо строки `"non-empty-fallback-on-error"` (`runtime.rs:3152-3163`); `vault.trash` идёт в
> отдельный `VaultTrash` → `vault.trash_file()`, файл уезжает в `.trash/` с уникальным именем
> (`api.rs:20`, `bridge.rs:400`, `vault.rs:234`). Возвращаться к ним не нужно — регрессия здесь
> означает откат исправления потери данных.

| Сделано | Где |
|---|---|
| `MarkdownView`, `workspace.getActiveViewOfType` / `activeLeaf` / `activeEditor` / `layoutReady` / `setActiveLeaf`, `Platform`, `vault.createFolder` / `cachedRead` / `configDir` | патч 1, `runtime.rs` + `VaultCreateFolder` в `api.rs`/`bridge.rs` |
| `MarkdownRenderer`, `workspace.openLinkText` / `iterateAllLeaves` / `getMostRecentLeaf` | патч 2, `runtime.rs` |

**Осталось — по измеренной цене на 19.10.2026** (сколько плагинов станет полностью без нехваток,
если закрыть ровно эту пачку; `blocked` исключены, база — 278 плагинов, из них 91 уже чисты):

| # | Пачка | Плагинов | Чем ограничена |
|---:|---|---:|---|
| 1 | `parseYaml`, `stringifyYaml`, `fileManager.processFrontMatter`, `renameFile`, `trashFile`, `vault.getRoot` | **22** | JS + существующие запросы; риск — YAML |
| 2 | `vault.readBinary` / `createBinary` / `modifyBinary` / `getResourcePath`, `getRoot`, `getAllLoadedFiles` | **16** | нужен бинарный канал в бридже |
| 3 | `addStatusBarItem`, `registerMarkdownPostProcessor` | **14** | нужен UI-слот в хосте |
| 4 | `renameFile` / `trashFile` / `getRoot` без YAML | 13 | JS |
| 5 | только `vault.rename` / `vault.trash` | 1 | JS |

Цифры до патчей 1–2 были 29 / 25 / 22 / 16 / 7 (в прежнем порядке строк) — они устарели: база
выросла, часть плагинов уже чиста. Пересчёт: `doctor.mjs plan tmp/corpus500` (жадный план по
одиночным символам: 60 символов закрывают 239 из 278, дальше остаются 39 плагинов с ≥2 нехватками).

Вне этого счёта — две вещи, которые счётчик нехваток не видит:

- **CSS — V1 и V2 сделаны 19.10.** Две части. V1: inline-стили больше не теряются
  (`VElement.style` → `__style_to_uinode` → `UiNode::Styled` → `apply_style`), `setCssProps`
  реализован и не падает — **76% присваиваний (3 640 из 4 768)**, целиком покрыто 82 плагина
  из 184. V2: `styles.css` плагина разбирается и применяется — селекторы по тегу/классу/атрибуту,
  потомок и `>`, каскад по специфичности, `var()` с `:root`/темой. Из 53 таблиц (1.4 MB)
  **7 545 селекторов из 9 583 (79%)** переживают разбор и дают **18 595 деклараций** по белому
  списку — вчетверо больше, чем inline-присваиваний.
  Чего нет и почему: псевдоклассы (`:hover`, `:focus`, `:first-child` — 1 347 селекторов),
  `@media` (201 блок), `+`/`~` — это состояния и условия, которых у нас не существует.
  Свойства без эквивалента в GPUI отбрасываются в JS: `transform` 116, `cursor` 92, `zIndex` 82,
  `gap` 40, `clipPath` 37, `lineHeight` 35, `transition` 31, `border` 29, `fontFamily` 28,
  `pointerEvents` 27. Остаётся: `<style>` заблокирован в `DANGEROUS_TAGS` (динамический CSS),
  `z-index`/`fixed` без прямого эквивалента, **CSS nesting** (`.a { .b { … } }`) не разворачивается.
  Отдельное правило: `display: none` не применяется, если тот же элемент показывается правилом
  с `:hover`/`:focus` — состояния у нас нет, а навсегда спрятанная кнопка хуже лишней видимой.
- **Буфер редактора — сделан 21.10.** `editor.getValue` 38, `getCursor` 23, `getLine` 15.
  Чтение синхронное: `Snapshot.active_doc` (rope + курсор/якорь в (line, ch)) публикуется из
  единого выхода `handle_key`, клика по документу, drag-выделения и `dispatch_edit_event`; JS-`Editor`
  читает через `__host_get_active_buffer()`. Запись — запросы `editor_set_buffer` /
  `editor_replace_range` / `editor_set_cursor` / `editor_set_selection` (api.rs + bridge.rs).
  Харнесс 125/125 (16 новых тестов Editor). `ok` счётчика не меняется: сканер не видит вызовы
  методов классов (§7), но блокировка macro-plugin-plan снята — команды `editorCallback` теперь
  правят реальный документ, а не пустую строку.

**Движки.** Canvas 91, CodeMirror 88, Node 66, Worker 37 — отдельное продуктовое решение.
После пачек 1–3 плагины, которым нужен только API, работают; остальные упираются в платформу.

---

## 7. Оговорки, которые остаются

- **Выборка случайная, не взвешенная по установкам.** 500 из 9 087. Плагин с миллионом установок
  весит столько же, сколько заброшенный. Для «что чинить первым» это правильно, для «что сломается
  у большинства» — нет.
- **Статический анализ не доказывает работу.** Он доказывает нехватку. Обратное требует запуска.
- **Методы ловятся только для `this.<method>(` и статических вызовов через алиас.** Вызов через
  переменную не виден — значит, реальных нехваток может быть больше, но не меньше.
- **Шум в экспортах отфильтрован** по правилу «строчное имя вне контракта — не Obsidian»
  (`createEl`, `length`, `push` ушли). Заглавные имена вне контракта оставлены и помечены
  «вне контракта» — там попадаются реальные новинки вроде `BasesView`.
- **Контракт снят с 1.7.5.** 29% плагинов требуют `minAppVersion` выше — часть их запросов
  относится к API, которого в 1.7.5 ещё не было.
- **CSS по-прежнему не измеряется сканером.** Он видит методы и экспорты, но не видит, что плагин
  стилизует свою вью. Inline-стили пишут **184 плагина (37%)**, `setCssProps` вызывают 59,
  `styles.css` есть у **88%** (53 из 60 проверенных), тег `<style>` инжектят 55 — и вот он
  по-прежнему не работает. Поддержано подмножество (76% inline-присваиваний, 79% селекторов
  из таблиц), но `ok` по API всё ещё не означает «выглядит правильно»: неподдержанное
  (`transform`, `zIndex`, `clipPath`, псевдоклассы) отбрасывается молча.

---

## 8. Воспроизведение

```bash
cd .workbuddy-ai/tools/plugin-doctor
node doctor.mjs sync
node doctor.mjs fetch --count=500 --out=../../tmp/corpus500 --conc=2 --delay=1200
node doctor.mjs report ../../tmp/corpus500 --json=../../tmp/corpus500-report.json
```

`--conc=2 --delay=1200` — из-за rate limit GitHub: при 8 параллельных запросах 72 из 500 получили 429,
при двух с задержкой — ноль ошибок.

Машинный отчёт: `.workbuddy-ai/tmp/corpus500-report.json`.
Предыдущий прогон на 100: `plugin-compat-100.md` (оставлен для сравнения).
