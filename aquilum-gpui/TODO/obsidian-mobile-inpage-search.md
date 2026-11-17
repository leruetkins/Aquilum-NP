# Поиск внутри заметки в Obsidian Mobile 1.15.0

Это не глобальный поиск по vault. Речь о **Find / Search current file**: панель
поиска внутри открытой заметки. В Obsidian есть две разные реализации:

- **reading / preview mode** — поиск по уже отрендеренному DOM;
- **source / live-preview mode** — поиск по тексту CodeMirror.

Обе используют один и тот же внешний вид панели, но алгоритм и подсветка
различаются. Ниже ссылки на beautify-версию bundle:
`Obsidian/_analysis/app_pretty.js`.

## 1. Как открывается

Команды (`app_pretty.js:40904`, `:40909`):

- `editor:open-search` — «Search current file», горячая клавиша `Mod+F`
  (Ctrl/Cmd+F);
- `editor:open-search-replace` — «Search & replace current file»,
  `Mod+Alt+F` или `Mod+H`; доступна только в source-режиме.

`MarkdownView.showSearch(replace)` делегирует текущему режиму
(`app_pretty.js:66992`):

- preview/reader → `xf.showSearch()` (`:43183`);
- source/live-preview → `G3.show(replace)` (`:56161`).

При открытии:

- если в редакторе есть выделение, оно подставляется в поле поиска;
- поле получает фокус и выделяется;
- панель вставляется в начало контейнера заметки (`prepend`);
- в source-режиме можно сразу открыть режим replace.

## 2. Как выглядит

DOM-структура (`xf` и `wE`/`G3`):

```html
<div class="document-search-container">
  <div class="document-search">
    <div class="search-input-container document-search-input">
      <input type="text" placeholder="Find...">
      <div class="document-search-count">3 / 17</div>
    </div>

    <div class="document-search-buttons">
      <button class="clickable-icon document-search-button">↑</button>
      <button class="clickable-icon document-search-button">↓</button>
      <div class="clickable-icon document-search-close-button">×</div>
    </div>
  </div>

  <!-- Только в source/replace mode -->
  <div class="document-replace">
    <input class="document-replace-input" placeholder="Replace...">
    <div class="document-replace-buttons">
      <button>replace</button>
      <button>replace all</button>
    </div>
  </div>
</div>
```

Стили (`app.css`):

- `.document-search-container` — flex-колонка, `z-index` popover; на мобильном
  растянута на всю ширину, имеет нижнюю границу и безопасные отступы;
- `.document-search` — строка: input растягивается, кнопки справа;
- input — скруглённое поле поиска (pill);
- `.document-search-count` — мелкий muted-текст, абсолютно справа внутри поля,
  `font-variant-numeric: tabular-nums`;
- кнопки — muted-иконки `arrow-up`, `arrow-down`, `x`;
- при отсутствии совпадений input получает класс `mod-no-match` и красноватый
  фон (20% error-color);
- replace-строка скрыта, пока контейнер не получит `mod-replace-mode`;
- на телефоне кнопки справа, gap меньше, input занимает всё остальное место.

Внешне это не отдельное модальное окно, а полоса сверху заметки. В source-mode
она стоит над CodeMirror, в reading-mode — над preview.

## 3. Reading / preview mode

Класс `xf` (`app_pretty.js:41555`).

Пайплайн:

```text
input (debounce 150 ms)
  -> updateQuery()
       -> для каждой section:
            собрать textContent текстовых узлов
            lower-case
            indexOf(query) в цикле
       -> сохранить ranges в section.highlightRanges
       -> queueRender()
  -> renderHighlights()
       -> только для видимых sections
       -> DOM Range -> getClientRects()
       -> абсолютно позиционированные div.search-highlight
```

Особенности:

- поиск идёт по **тексту отрендеренного DOM**, а не по markdown-исходнику;
- регистр игнорируется: и запрос, и текст приводятся к lower-case;
- учитываются все вхождения, а не только первое;
- `lastText`/`lastQuery` кэшируются: повторный `findNext`/`findPrevious` не
  пересканирует документ;
- поиск перезапускается после `onRenderComplete`, если текст перерисовался;
- подсветка — не изменение текста, а overlay-слои:
  `.search-highlight > div` с `position:absolute`, `pointer-events:none`,
  `box-shadow`, `opacity:.3`; активное совпадение — `is-active` с accent
  box-shadow и `opacity:1`;
- `selectRange` прокручивает совпадение через
  `scrollIntoView({block:"center"})`, то есть найденная строка оказывается
  примерно по центру;
- счётчик точный, без ограничения сверху;
- replace в reading-mode недоступен.

Стоимость:

- один полный проход по тексту всех sections на каждый изменённый запрос:
  `O(n)` по символам + `O(m)` по совпадениям;
- lower-case создаёт копию текста;
- DOM Range и `getClientRects` считаются только для видимых секций, но при
  большом числе совпадений overlay-элементов может быть много;
- виртуализация reading-режима ограничивает число отрисованных секций, поэтому
  поиск остаётся практичным для больших заметок, хотя память под ranges и
  overlay растёт линейно.

## 4. Source / live-preview mode

Классы `wE` (общая панель, `app_pretty.js:45639`) и `G3`
(source-поиск, `:45677`).

Пайплайн:

```text
input
  -> editor.searchCursor(query)
       -> CodeMirror SearchCursor, streaming по Text-итератору
  -> cursor.findNext() / findPrevious()
  -> editor.addHighlights([match], "obsidian-search-match-highlight")
  -> editor.scrollIntoView(match)
```

`Editor.searchCursor` (`app_pretty.js:55523`) оборачивает `CodeMirror`
`SearchCursor`:

- поиск всегда case-insensitive через lower-case нормализацию;
- это **literal substring**, без regex и без переключателя «match case»;
- `nextOverlapping()` позволяет находить перекрывающиеся вхождения;
- `findNext()` ищет от текущего конца совпадения и заворачивается в начало;
- `findPrevious()` идёт назад блоками по 10 000 символов;
- `findAll()` проходит документ целиком и возвращает все диапазоны;
- `getIndexAndCount()` считает `current / total`, проходя документ от начала
  до текущего совпадения и останавливаясь на 9999 совпадениях;
- `replace()` заменяет текущее совпадение, `replaceAll()` делает одну
  транзакцию со всеми заменами.

Поведение панели:

- при вводе сначала подсвечивается первое совпадение;
- `findNext`/`findPrevious` двигают активное совпадение;
- кнопка «Find all» (`lucide-text-select`, Alt+Enter) подсвечивает все
  совпадения;
- replace/replace-all доступны только в source-mode;
- `obsidian-search-match-highlight` рисует box-shadow вокруг диапазона:
  `0 0 0 3px var(--text-accent)`;
- при закрытии панели снимаются подсветки, курсор возвращается к текущему
  совпадению, фокус уходит в редактор;
- при отсутствии совпадения input получает `mod-no-match`;
- счётчик обновляется с debounce 20 мс.

Стоимость:

- потоковый поиск `O(n)` по документу на первый запрос/переход; без построения
  индекса и без копии всего документа;
- `findNext` от текущей позиции обычно дешёвый, но при завороте в начало снова
  идёт по документу;
- `getIndexAndCount` — отдельный проход от начала до текущего совпадения,
  поэтому на очень больших документах и при частом вводе именно счётчик может
  быть дороже самого поиска; он ограничен 9999 совпадениями;
- `findAll` и `replaceAll` линейны по документу и создают массив диапазонов;
- память — O(query + active matches), без хранения индекса.

## 5. Горячие клавиши и мобильное поведение

Source/reading search scope (`xf`/`G3`):

- `F3`, `Mod+G` — следующее;
- `Shift+F3`, `Mod+Shift+G` — предыдущее;
- `Enter` — следующее; `Shift+Enter` — предыдущее;
- `Escape` — закрыть;
- `Alt+Enter` — подсветить все (Find all);
- `Mod+Alt+Enter` — replace all в source-mode.

На телефоне:

- физической клавиатуры обычно нет, поэтому `Enter` в поле поиска скрывает
  клавиатуру (`ot()`), а навигация идёт кнопками ↑/↓;
- панель занимает всю ширину, кнопки справа, input растягивается;
- replace-режим на телефоне доступен только в source-режиме.

## 6. Насколько эффективен алгоритм

Для одной открытой заметки Obsidian сознательно **не использует индекс**:

- текст меняется при каждом нажатии, и индексировать его на лету дороже, чем
  один линейный проход;
- линейный поиск не требует хранения постings и не устаревает после правки;
- типичная заметка в десятки–сотни килобайт полностью сканируется за один
  проход, а нативный `indexOf`/CodeMirror-итератор работают быстро.

Практически:

- **source-mode** эффективнее по памяти и лучше подходит для больших файлов:
  поиск потоковый, без копии всего текста; основной риск — повторный полный
  проход для счётчика `current / total` на очень больших документах;
- **reading-mode** проще по логике, но делает lower-case копию текста и строит
  DOM-диапазоны/overlay для совпадений; на заметке с тысячами совпадений это
  заметно дороже, чем source-поиск;
- для типичных заметок обе реализации быстрые; для экстремально больших
  документов узкое место — не сам поиск, а подсветка и подсчёт;
- ограничение счётчика в source-mode (`9999`) защищает от бесконечного прохода
  при огромном числе совпадений, но после 9999 цифра перестаёт быть точной.

## 7. Что из этого полезно для vibecodium-mobile

Если делать in-page поиск в нативном редакторе:

- не нужен полнотекстовый индекс: достаточно линейного прохода по тексту
  текущей заметки;
- хранить только диапазоны текущего запроса и активный индекс;
- искать без учёта регистра, literal substring, как Obsidian;
- следующий/предыдущий переход с заворотом;
- счётчик `current / total`, но считать его лениво/с ограничением, чтобы не
  сканировать большой документ на каждый символ;
- подсветку делать либо диапазонами в рендерере, либо отдельным overlay-слоем;
- активное совпадение центрировать по экрану;
- панель — строка сверху заметки: поле, `n / total`, ↑, ↓, закрытие;
- replace — только в source-режиме, с отдельной строкой;
- на телефоне Enter скрывает клавиатуру, навигация кнопками.

## 8. Оговорки

- Номера строк относятся к `_analysis/app_pretty.js`; после пересборки или
  повторного beautify они могут сдвинуться.
- В bundle также лежит стандартный CodeMirror search panel (`cm-search`,
  `SearchQuery`, `RegExpCursor`), но Obsidian's custom `document-search` панель
  для in-page поиска использует собственный UI и literal `SearchCursor`, а не
  regex-панель CodeMirror.
