# 100 плагинов Obsidian против хоста Vibecodium

> **Обновлено:** расширенная выборка на 500 плагинах — `plugin-compat-500.md`. Ранжирование
> подтвердилось (ρ = 0.843), доли уточнились: canvas вырос с 15% до 18%, Worker упал с 10% до 7%.
> Этот файл оставлен как первый прогон и для сравнения.

Прогон `plugin-doctor` по случайной выборке из `community-plugins.json` (seed 42, 100 плагинов,
0 ошибок загрузки). Ниже — что показал статический анализ и что из этого следует.

---

## 1. Инструмент

`D:\Naben\Github\REA\.workbuddy-ai\tools\plugin-doctor\` — Node, без зависимостей.

```bash
node doctor.mjs sync                              # перечитать runtime.rs → capabilities
node doctor.mjs scan <plugin-dir>                 # один плагин
node doctor.mjs fetch --count=100 --out=corpus    # скачать выборку
node doctor.mjs report corpus --json=report.json  # сводка
```

Что делает:

1. Читает `runtime.rs` и извлекает **фактическую** поверхность хоста: 50 классов, 44 экспорта,
   члены `rawVault`/`rawWorkspace`/`rawMetadataCache`/`app`. Ничего не пишется руками — `sync`
   пересобирает `data/vibecodium-capabilities.json` из кода, поэтому отчёт не расходится с реальностью.
2. Разбирает `main.js` плагина: алиасы `require("obsidian")`, деструктуризацию, обращения
   `app.<ns>.<member>`, `this.<метод>(`, статические `alias.Class.method`, жёсткие зависимости.
3. Сопоставляет с контрактом Obsidian 1.7.5 (122 экспорта, члены `app.*` — объединение
   `obsidian.d.ts` и фактического бандла).
4. Выдаёт вердикт: `ok` / `degraded` / `blocked`, покрытие и список конкретных нехваток.

Сознательные ограничения: методы ловятся только для `this.<method>(` из известного списка и для
статических вызовов через алиас; вызов через переменную (`const p = this; p.addCommand(...)`) не виден.
`import` типов стирается при сборке — такие имена в бандле отсутствуют и не считаются.

---

## 2. Результат в одну строку

| Вердикт | Плагинов | Что значит |
|---|---:|---|
| `ok` | **8** | всё, что трогает, в хосте есть |
| `degraded` | **51** | загрузится, но часть вызовов упадёт или вернёт пустоту |
| `blocked` | **41** | упирается в то, чего в хосте нет в принципе |

**Без жёстких блокеров — 55 из 100.** Медианное покрытие — 78%.

То есть главная проблема не в движках. Больше половины плагинов не требуют ни webview, ни canvas,
ни CodeMirror — им просто не хватает методов API. Это хорошая новость: чинится дешевле.

---

## 3. Про webview и canvas — гипотеза подтвердилась лишь частично

| Блокер | Плагинов | Из них с *единственным* блокером |
|---|---:|---:|
| CodeMirror 6 | 16 | **10** |
| Canvas 2D / WebGL | 15 | 4 |
| Node.js модули (`fs`, `child_process`) | 10 | 5 |
| Web Worker | 10 | 2 |
| Electron API | 5 | 0 |
| iframe | 5 | 3 |
| IndexedDB | 4 | 0 |
| `<webview>` | **3** | 1 |

Вы правы, что webview и canvas без зависимостей не поедут. Но масштаб другой, чем ожидалось:

- **webview — всего 3 плагина из 100.** Это редкая экзотика (в основном встраивание сторонних
  сайтов). Тянуть Electron ради трёх плагинов вряд ли оправдано.
- **canvas — 15 из 100**, и это в основном графики и подсветка, а не «вся программа». Excalidraw
  в выборку не попал, но он бы добавил ещё один.
- **CodeMirror 6 — 16 из 100 и 10 из них упираются *только* в него.** Это самый дорогой и самый
  частый блокер: без настоящего CM6 не работают расширения редактора, подсветка, автодополнение,
  live-preview-виджеты. Дешевле, чем webview, но требует полноценного CM6-слоя.

Порядок по соотношению «плагинов на единицу работы»: **CM6 → canvas → Worker → Node**.
webview — в самый конец, если вообще.

---

## 4. Что важнее движков: API

### 4.1 Отсутствующие экспорты

| Имя | Плагинов | Сейчас в хосте |
|---|---:|---|
| `MarkdownView` | **38** | пустой класс-заглушка |
| `Platform` | 17 | пустой класс-заглушка |
| `MarkdownRenderer` | 17 | пустой класс-заглушка |
| `parseYaml` | 10 | `undefined` |
| `request` | 6 | `undefined` |
| `Vault` | 5 | пустой класс-заглушка |
| `FileSystemAdapter` | 5 | пустой класс-заглушка |
| `stringifyYaml` | 5 | `undefined` |
| `getAllTags` | 4 | `undefined` |
| `editorLivePreviewField` | 4 | `undefined` |
| `FileView` | 4 | пустой класс-заглушка |

`MarkdownView` — треть выборки. Это не «одна из функций», это точка входа почти любого плагина,
который работает с текущей заметкой.

### 4.2 Отсутствующие члены `app.*`

| Член | Плагинов | Вызовов |
|---|---:|---:|
| `workspace.getActiveViewOfType` | **32** | 70 |
| `vault.cachedRead` | **25** | 54 |
| `vault.createFolder` | **23** | 46 |
| `vault.configDir` | 15 | 40 |
| `workspace.openLinkText` | 11 | 38 |
| `workspace.iterateAllLeaves` | 8 | 30 |
| `vault.readBinary` | 8 | 19 |
| `fileManager.processFrontMatter` | 8 | 15 |
| `vault.createBinary` | 8 | 9 |
| `vault.getAllLoadedFiles` | 8 | 8 |
| `fileManager.renameFile` | 7 | 30 |
| `vault.getConfig` | 7 | 11 |
| `plugins.getPlugin` | 6 | 26 |
| `fileManager.trashFile` | 6 | 16 |
| `vault.getName` | 6 | 14 |

Обратите внимание на `vault.configDir` — это **свойство**, а не метод. Стоит одной строки, а нужно
15 плагинам из 100.

### 4.3 Отсутствующие методы `Plugin`/`Component`

| Метод | Плагинов |
|---|---:|
| `registerMarkdownPostProcessor` | 11 |
| `registerObsidianProtocolHandler` | 6 |
| `addStatusBarItem` | 5 |
| `registerEditorSuggest` | 1 |

---

## 5. Симуляция: сколько даёт один рывок

Если добавить **58 функций** (26 экспортов + 26 членов `app.*` + 6 методов):

| Было | Стало |
|---|---|
| 8 `ok` | **39 `ok`** |
| 51 `degraded` | 20 `degraded` |
| 41 `blocked` | 41 `blocked` (блокеры не трогаем) |

То есть **31 плагин из 51 переходит в «работает»** без единой строчки CM6/canvas.

Оставшиеся 20 `degraded` просят по мелочи:

| Что | Плагинов |
|---|---:|
| `app.internalPlugins.plugins` | 4 |
| `app.metadataCache.getTags` | 3 |
| `app.workspace.offref`, `activeLeaf` | по 2 |
| `app.metadataCache.resolvedLinks` | 2 |
| `app.workspace.containerEl` | 2 |
| `app.setting.open/openTabById/close` | по 1 |
| `TextFileView`, `apiVersion`, `parseFrontMatterAliases` | по 1 |

---

## 6. Что делать, по порядку

**Шаг 1 — API-рывок (58 функций).** Разблокирует 31 плагин. Внутри — тремя пачками:

1. `MarkdownView` + `workspace.getActiveViewOfType` + `activeEditor` + `Platform` — это трогает
   38+32+17 плагинов и является предпосылкой для остального.
2. `vault`: `cachedRead`, `createFolder`, `configDir`, `readBinary`, `createBinary`,
   `getAllLoadedFiles`, `getConfig`, `getName`, `getResourcePath`, `getFolderByPath`, `getRoot`.
3. `fileManager`: `processFrontMatter`, `renameFile`, `trashFile`, `getNewFileParent`;
   `workspace.openLinkText`, `iterateAllLeaves`, `layoutReady`, `updateOptions`, `setActiveLeaf`;
   `plugins.getPlugin`; утилиты `parseYaml`/`stringifyYaml`/`request`/`getAllTags`.

**Шаг 2 — CM6.** 16 плагинов, из них 10 упираются только в него. Самый дорогой шаг; начинать
имеет смысл с `registerEditorExtension` + `editorInfoField` + `editorLivePreviewField`, а не со
всего CM6 сразу.

**Шаг 3 — canvas.** 15 плагинов, но только 4 упираются исключительно в него. Реалистичный минимум —
2D-контекст с `drawImage`/`fillRect`/`measureText`; WebGL — отдельная история.

**Шаг 4 — Worker / Node.** По 10 плагинов. Worker в QuickJS не появится; Node-модули можно
эмулировать через хост-мост (`fs` → vault, `child_process` — вряд ли).

**Шаг 5 — webview.** 3 плагина. Решение продуктовое, не техническое.

---

## 7. Оговорки

- **Выборка случайная, не по популярности.** 100 из 9 087. Смещение возможно: редкие плагины
  попадают наравне с массовыми. Для приоритетов «что чинить первым» это скорее плюс (видно
  разнообразие), для «что сломается у большинства пользователей» — нужен вес по установкам,
  которого у нас нет.
- **29 плагинов из 100 требуют `minAppVersion` выше 1.7.5.** Контракт снят с 1.7.5; часть
  новых имён помечена «вне контракта» и не считается нехваткой хоста.
- **`process.env`/`process.platform`** помечены как предупреждение, а не блокер: часто это мёртвый
  код после сборки.
- **`app.<ns>` фильтруется по известным пространствам имён.** Без этого в отчёт попадали
  `app.classList`, `app.foldManager`, `app.customCss` — это локальные переменные плагинов, а не Obsidian.
- **Статический анализ не доказывает работоспособность.** Он доказывает нехватку. Обратное
  («всё есть → заработает») требует запуска.

---

## 8. Воспроизведение

```bash
cd .workbuddy-ai/tools/plugin-doctor
node doctor.mjs sync
node doctor.mjs fetch --count=100 --out=../../tmp/corpus100
node doctor.mjs report ../../tmp/corpus100 --json=../../tmp/corpus100-report.json
```

Машинный отчёт по этому прогону: `.workbuddy-ai/tmp/corpus100-report.json`.
Сам корпус (`tmp/corpus100`) не хранится — снесён при уборке 18.10, восстанавливается командой `fetch` выше.
(по каждому плагину: вердикт, покрытие, блокеры, списки нехваток).
