# Поиск в Obsidian Mobile 1.15.0

Разбор по распакованному APK:

- `Obsidian/Obsidian-1.15.0/assets/public/app.js` — минифицированный bundle;
- `Obsidian/_analysis/app_pretty.js` — его beautify-версия, по ней ниже указаны
  номера строк. Номера могут сдвинуться, если bundle пересоберут или заново
  отформатируют.

Речь о встроенном плагине **global-search**, который открывается как отдельная
панель/вкладка с типом `search`. Это не поиск в настройках и не Bases.

## 1. Где живёт поиск

- Тип view: `JE = "search"` (`app_pretty.js:58415`).
- `openGlobalSearch(query)` вызывает
  `workspace.ensureSideLeaf("search", "left", ...)`, то есть поиск живёт в левой
  боковой панели. На телефоне это левый drawer/sidebar.
- Поиск открывается из:
  - иконки поиска;
  - контекстного меню файла/папки (`path:"folder/"`);
  - поиска по выделенному тексту в редакторе;
  - тегов (`tag:...`);
  - CLI-команд.
- View хранит своё состояние в layout: `query`, `matchingCase`,
  `explainSearch`, `collapseAll`, `extraContext`, `sortOrder`
  (`getState`/`setState`, `app_pretty.js:58500+`).

## 2. Поток выполнения поиска

Упрощённо:

```text
input
  -> Ir(query, matchingCase)          // разбор запроса в matcher AST
  -> pv(files, requiredInputs, queue) // обход файлов
       -> vault.cachedRead(file)      // только если нужен content
       -> Ir.match(file, text)        // offsets совпадений
       -> dom.addResult / removeResult
  -> zc (search-results root)
       -> kf на каждый файл
            -> sl на каждую строку совпадения
```

Ключевые точки:

- `Ir` — класс запроса (`app_pretty.js:29037`). В конструкторе:
  - `AH(query)` разбирает строку на токены (`:28731`);
  - `DH(tokens, operators)` строит AST (`:28605`);
  - `requiredInputs()` сообщает, какие поля нужны: `content`, `filename`,
    `filepath`, `tag` и т.д.
- `Ir.match(file, text)` (`:29037`) возвращает объект результата для файла:
  - для `.md` учитывает `filename` и `content`;
  - для остальных файлов — `filename`/`filepath`;
  - для `.canvas` разбирает JSON и матчит текстовые узлы;
  - после матчинга массивы offset-диапазонов прогоняются через `Cc`, которая
    сортирует и **сливает пересекающиеся диапазоны** (`:24070`).
- `pv` (`:29094`) — асинхронный обход файлов:
  - пропускает ignored/unsupported файлы;
  - читает содержимое через `vault.cachedRead` только для `md`/`canvas` и только
    если запросу реально нужен `content`;
  - вызывает `query.match(file, text)` и сразу добавляет/удаляет результат в UI.
- Очередь `Sf` (`app_pretty.js:42855`) запускает обход пачками и подписывается
  на события vault/metadata. Поэтому открытый поиск обновляется при
  создании/изменении/переименовании/удалении файлов, а удалённый файл сразу
  исчезает из результатов.
- Пока идёт обход, показывается loader; общий счётчик обновляется по `onStop`
  очереди.

### Синтаксис запроса

Парсер поддерживает не только подстроку:

- `"фраза"` — точная фраза;
- `/regex/` — регулярное выражение;
- `(...)`, `OR`, `-`/`NOT` — логика;
- `file:`, `path:`, `content:`, `line:`, `block:`, `section:`;
- `task:`, `task-todo:`, `task-done:`;
- `tag:`;
- `match-case:` / `ignore-case:`;
- свойства: `[property:value]`, `[property:true]`, `[property:empty]`,
  сравнения `>`, `<`;
- `true`, `false`, `empty`.

Операторы описаны в `RH` (`app_pretty.js:29034`). Для нашей задачи важнее не
сам синтаксис, а то, что matcher возвращает **диапазоны offsets**, а не готовые
строки.

## 3. Формат результата

На один файл создаётся один объект результата. Упрощённо:

```js
{
  filename: [[start, end], ...],   // совпадения в имени
  filepath: [[start, end], ...],   // совпадения в пути
  content:  [[start, end], ...],   // совпадения в тексте
  propertyName: [[start, end], ...],
  tag: [[start, end], ...],
  properties: [                     // совпадения во frontmatter
    { key: "status", pos: [[start, end]] }
  ]
}
```

Диапазоны — это offsets в исходной строке. Пересекающиеся диапазоны уже слиты
функцией `Cc`, поэтому один диапазон может покрывать несколько визуально
совпавших участков.

## 4. Дерево DOM результата

Один файл — один `kf` (`app_pretty.js:41148`), который создаёт такую структуру:

```html
<div class="tree-item search-result">
  <div class="tree-item-self search-result-file-title is-clickable">
    <div class="tree-item-icon collapse-icon">right-triangle</div>
    <div class="tree-item-inner">Имя файла или путь</div>
    <div class="tree-item-flair-outer">
      <span class="tree-item-flair">12</span>
    </div>
  </div>

  <div class="search-result-file-matches">
    <div class="search-result-file-match tappable">
      ...контекст...
      <span class="search-result-file-matched-text">совпадение</span>
      ...контекст...
    </div>
    ...
  </div>
</div>
```

Корневой контейнер результатов — `zc` (`app_pretty.js:41468`). Он оборачивает
только список файлов; строка ввода, параметры и счётчик лежат рядом, на уровне
view:

```html
<div class="... view container ...">
  <div class="search-row">... input ...</div>
  <div class="search-info-container">...</div>
  <div class="search-params">...</div>
  <div class="search-results-info">
    <div class="search-results-result-count">42 results</div>
    <div class="dropdown">alphabetical</div>
  </div>

  <div class="search-result-container mod-global-search">
    <div class="search-results-children">
      ...набор .search-result (по одному на файл)...
    </div>
  </div>
</div>
```

CSS-классы, которые имеет смысл повторять/маппить:

- `.search-result-container`
- `.search-results-info`, `.search-results-result-count`
- `.search-results-children`
- `.search-result`
- `.search-result-file-title`
- `.search-result-file-matches`
- `.search-result-file-match`
- `.search-result-file-matched-text`
- `.tree-item-flair-outer`, `.tree-item-flair`
- `.search-empty-state`
- `.search-params`
- `.search-result-hover-button.mod-top` / `.mod-bottom`

## 5. Как считается количество совпадений

Есть две разные цифры.

### Бейдж у файла

В конструкторе `kf` (`app_pretty.js:41177`):

```js
let h = LH({ ...result, filename: undefined });
h > 0 && header
  .createDiv("tree-item-flair-outer")
  .createSpan("tree-item-flair")
  .setText(String(h));
```

`LH` (`app_pretty.js:28337`) считает:

```js
for (const field of ["filename", "filepath", "content", "propertyName", "tag"])
  if (result[field]) count += result[field].length;

if (result.properties) count += result.properties.length;
```

То есть бейдж — это количество **слитых диапазонов** плюс количество
property-совпадений. Важная деталь: в бейдже явно обнуляется `filename`,
поэтому файл, найденный только по имени, показывает результат без цифры.
Если бейдж равен нулю, DOM для него вообще не создаётся.

### Общий счётчик

`zc.getMatchCount()` (`app_pretty.js:41537`) суммирует `LH(result)` по всем
файлам, уже без исключения `filename`. Поэтому общий счётчик может быть
больше суммы бейджей: совпадения в именах входят в общий итог, но не в бейдж
конкретного файла.

### Что считается одной единицей

- `Cc` сначала сортирует диапазоны и сливает пересекающиеся, поэтому два
  наложившихся совпадения дадут один диапазон и одну единицу.
- Для content-строк `sl` дополнительно группирует диапазоны, попадающие в один
  контекст, в одну визуальную строку. Но бейдж считается по массиву диапазонов,
  а не по числу DOM-строк.
- `properties` считается по количеству записей `{key, pos}`, а не по длине
  `pos`.

## 6. Сворачивание файлов

`kf` — это tree-item с флагом `collapsible = true` и `collapsed = false` по
умолчанию. Логика:

- иконка `collapse-icon` (`right-triangle`) скрыта до первого рендера;
- после рендера она показывается только если у файла есть content/property
  совпадения (`renderContentMatches`, `app_pretty.js:41187`);
- клик по заголовку файла открывает файл;
- клик именно по иконке вызывает `onCollapseClick` → `setCollapse`
  (`app_pretty.js:41290`, `:41301`);
- `Mod+click` (Ctrl/Cmd) по иконке делает `parentDom.setCollapseAll(...)`,
  то есть сворачивает/разворачивает все файлы;
- `setCollapse` добавляет/убирает класс `is-collapsed`, анимационно меняет
  высоту (`setCollapse` → `MT`/`Ab`/`bne`, `app_pretty.js:21294`; длительность
  около 100 мс), а при сворачивании **отвязывает** `childrenEl` от DOM;
- состояние `collapsed` сохраняется при обновлении результата: если результат
  для того же файла пересоздаётся, `zc.addResult` берёт старое состояние
  (`app_pretty.js:41512`);
- глобальный `collapseAll` хранится в `zc` и в состоянии view. Тумблер
  **Collapse results** в `search-params` вызывает `setCollapseAll`
  (`app_pretty.js:58448`);
- клавиатура: стрелка влево сворачивает текущий файл/группу, стрелка вправо
  разворачивает.

Важно для производительности: у свернутого файла строки совпадений не просто
скрыты CSS, а физически отвязаны от DOM. Плюс сам список использует
infinite scroll (`Jp`), поэтому строки рендерятся лениво по мере прокрутки.

## 7. Строки совпадений и контекст

Каждая строка — `sl` (`app_pretty.js:41355`):

- контейнер `.search-result-file-match.tappable`;
- внутри подсвеченные участки через `.search-result-file-matched-text`;
- по краям добавляется `"..."`, если контекст обрезан;
- при наведении появляются кнопки `mod-top` / `mod-bottom` («показать больше
  контекста выше/ниже»);
- клик открывает файл с `eState.match = {content, matches}`
  (`app_pretty.js:41371`), чтобы редактор перешёл к совпадению и подсветил его;
- на мобильном вместо hover-кнопок работают те же действия через тапы и
  клавиатурные сокращения, а наведение эмулируется фокусом.

Контекст по умолчанию (`tx`, `app_pretty.js:24263`):

- берётся до 100 символов до и после совпадения;
- границы обрезаются по концам строки;
- если совпадение занимает несколько строк, диапазон расширяется.

Режим **More context** (`extraContext`) использует metadata cache
(`getMatchExtraPositions`, `app_pretty.js:41241`):

- если совпадение внутри list item — контекст расширяется до всего пункта
  списка;
- если внутри section — до всей секции;
- иначе — до абзаца (пустая строка как граница), с ограничением около 1000
  символов.

Для frontmatter-совпадений создаётся отдельный `SN`
(`app_pretty.js:41317`): строка вида `status: значение`, где подсвечивается
только нужная часть. Несколько совпадений по одному ключу группируются в одну
строку.

Перед отрисовкой диапазоны сортируются по offset, а пересекающиеся/соприкасающиеся
диапазоны объединяются в один визуальный блок (`renderContentMatches`,
`app_pretty.js:41187+`).

## 8. Сортировка и параметры поиска

Панель поиска (`eC`, `app_pretty.js:58415`) содержит:

- строку ввода;
- кнопку **Match case**;
- кнопку мобильных подсказок запроса;
- **Collapse results** — свернуть все файлы;
- **More context** — расширенный контекст;
- **Explain search term** — показать разбор запроса;
- выпадающий список сортировки;
- общий счётчик результатов.

Сортировка файлов (`Gd`, `app_pretty.js:29110`):

- `alphabetical` / `alphabeticalReverse`;
- `byModifiedTime` / `byModifiedTimeReverse`;
- `byCreatedTime` / `byCreatedTimeReverse`.

Сортировка применяется к файлам-группам, а внутри файла совпадения всегда идут
по возрастанию offset.

## 9. Что важно, если повторять это в vibecodium-mobile

Минимальная модель, которая сохраняет поведение Obsidian:

```text
SearchFileResult(
  path,
  filenameRanges,
  contentRanges,
  propertyRanges,
  ...,
)

SearchState(
  files: List<SearchFileResult>,   // по одному на файл
  collapsedPaths: Set<String>,     // независимо от результатов
  collapseAll: Boolean,
  extraContext: Boolean,
  sortOrder: ...
)
```

Практические правила:

1. Группировать результаты по файлу сразу при выдаче, а не строить плоский
   список совпадений.
2. Бейдж файла считать из нормализованных (слитых) диапазонов, а не из числа
   вхождений в тексте.
3. Решить отдельно, входит ли совпадение в имя файла в бейдж. Obsidian
   исключает `filename` из бейджа, но включает в общий счётчик.
4. Хранить `collapsedPaths` отдельно от списка результатов, чтобы обновление
   поиска не сбрасывало свёрнутость.
5. Не держать строки совпадений в UI-дереве у свёрнутых файлов — это заметно
   экономит память на большом количестве результатов.
6. Контекст по умолчанию — одна строка с обрезкой по 100 символов; расширенный
   контекст — по границам list item/section/абзаца.
7. При клике передавать в редактор не только путь, но и offsets совпадений,
   чтобы он мог перейти к нужному месту и подсветить его.

## 10. Отличия от нашего текущего поиска

В `vibecodium-mobile` сейчас есть `SearchPanel` и выдача `searchHits` в
Compose. Чтобы получить поведение Obsidian, потребуется:

- сгруппировать hits по `path`;
- добавить в состояние `collapsedPaths`, `collapseAll`, `extraContext`;
- считать count из диапазонов, а не из числа строк;
- добавить заголовок файла с шевроном и бейджем;
- при клике на строку передавать offset в нативный редактор.

Это ориентир на будущее; в рамках текущего разбора код приложения не менялся.
