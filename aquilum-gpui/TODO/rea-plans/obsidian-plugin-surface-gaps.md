# Поверхность плагинного API Obsidian и разрыв с Vibecodium

**Задача.** Не «описать, как устроены плагины», а получить **точный перечень того, что плагин может
вызвать**, и наложить его на `samples/vibecodium/plugins/*.rs`, чтобы список нехваток был измеренным,
а не предполагаемым.

Документ отвечает на три вопроса:

1. Что Obsidian **обязан** предоставить плагину (контракт, а не пересказ).
2. Что из этого реально вызывают настоящие плагины (частота).
3. Чего нет в хосте Vibecodium — по именам, сигнатурам и приоритетам.

---

## 1. Метод

| Источник | Что даёт | Объём |
|---|---|---|
| `obsidian.asar` → `app.js` (1.7.5) | фактическая реализация: экспорты, классы, тела методов | 3 172 382 Б, sha256 `9c3b719e…` |
| `obsidian.d.ts` (obsidianmd/obsidian-api) | подписи контракта, автогенерируется из исходников | 11 539 строк |
| Корпус плагинов | что вызывают на практике | 8 репозиториев, 1 146 `.ts` |
| `samples/vibecodium/plugins/*.rs` | текущая поверхность хоста | 9 633 строки Rust |

Корпус: `obsidian-sample-plugin` (официальный), `calendar`, `dataview`, `excalidraw`, `obsidian-git`,
`kanban`, `tasks`, `templater`. Разные по природе: настройки, рендер, редактор, файловые операции,
git-интеграция, канбан-доска.

Метод извлечения: из бандла вырезан объект экспорта `obsidian` (122 имени → локальные переменные),
затем для каждой переменной найден класс-фабрика `VAR=function(e){…}(Base)` с учётом строк и
комментариев, из тела собраны `prototype.*`-члены. Из `runtime.rs` тем же способом сняты классы и
объектные литералы (`rawVault`, `rawWorkspace`, `rawMetadataCache`, `app`), тела методов
классифицированы как `impl` / `const` / `empty`.

Проверка: распакованный `app.js` побайтово совпадает с содержимым ASAR (`same True`), так что
измерения идут по тому, что реально поставляется, а не по реконструкции.

---

## 2. Контракт жизненного цикла

Это не «как принято», а буквальный код 1.7.5. Пять инвариантов, на которых держится всё остальное.

### 2.1 `Component.load()` ставит флаг **до** `onload()`

```js
load(){ if(!this._loaded){ this._loaded=!0; this.onload();
        for(var e=0,t=this._children.slice();e<t.length;e++) t[e].load() } }
```

`_loaded=true` выставляется **перед** `onload()`. Поэтому `this.addChild(x)` внутри `onload()`
немедленно грузит ребёнка: `addChild` смотрит на `this._loaded`, а он уже истинен. Если выставить
флаг после `onload()`, `addChild` в `onload` молча не сработает — ребёнок останется неразобранным.

### 2.2 `unload()` — строго LIFO и `onunload()` **последним**

```js
unload(){ if(this._loaded){ this._loaded=!1;
  for(var e=this._children, t=this._events; e.length>0;){ e.pop().unload() }
  for(; t.length>0;){ t.pop()() }
  this.onunload() } }
```

Порядок: дети (в обратном порядке регистрации) → disposer'ы (в обратном порядке) → `onunload()`.
Не наоборот. Практический смысл: `onunload` плагина часто обращается к тому, что он создал, и
ожидает, что DOM/подписки ещё живы, а убирать за собой будет `register`.

### 2.3 Единственный примитив регистрации — `register(disposer)`

```js
register(e){ this._events.push(e) }
registerEvent(e){ this.register(function(){ return e.e.offref(e) }) }
registerDomEvent(e,t,n,i){ e.addEventListener(t,n,i),
  this.register(function(){ return e.removeEventListener(t,n,i) }) }
registerInterval(e){ this.register(function(){ return clearInterval(e) }); return e }
registerScopeEvent(e){ this.register(function(){ return e.scope.unregister(e) }) }
```

Всё остальное (`addCommand`, `registerView`, `addSettingTab`, `addRibbonIcon`, `registerEvent`,
`registerDomEvent`, `registerInterval`) — обёртки над `register`. Один инвариант вместо десяти.

### 2.4 `Plugin.load()` — асинхронный и переопределяет `Component.load()`

```js
load(){ return this._loaded ? Promise.resolve() : (this._loaded=!0,
        this.onload().then(()=>{ for(const c of this._children.slice()) c.load() })) }
```

Отличия от `Component.load()`: возвращает промис, ждёт `onload()` (он может быть `async`),
и только потом грузит детей. `_loaded` — тоже до `onload`.

### 2.5 `onLayoutReady` — не таймер, а очередь с атрибуцией

```js
onLayoutReady(e){ null===this.onLayoutReadyCallbacks ? e()
                 : this.onLayoutReadyCallbacks.push({pluginId:this.app.plugins.loadingPluginId, callback:e}) }
```

`null` означает «лейаут уже готов». Иначе колбэк кладётся в очередь вместе с `pluginId` того плагина,
который его зарегистрировал, — чтобы при падении на разборе лейаута было видно виновника. Замена
на `setTimeout(cb, 10)` ломает оба свойства: колбэк срабатывает до появления вью и теряет авторство.

### 2.6 Побочные контракты, которые легко не заметить

| Место | Поведение |
|---|---|
| `Plugin.addCommand` | `id = manifest.id + ":" + id`, `name = manifest.name + ": " + name`; регистрирует удаление |
| `Plugin.registerView` | дубликат типа → `throw new Error('Attempting to register an existing view type "…"')` |
| `Plugin.addStatusBarItem` | `app.statusBar.registerStatusBarItem()` + класс `plugin-<id>` + `detach()` в disposer |
| `Plugin.registerMarkdownPostProcessor` | после регистрации триггерит `post-processor-change` |
| `Plugin.loadData` | читает `<configDir>/plugins/<id>/data.json`; при внешнем изменении — `onExternalSettingsChange` |
| `Plugin.onUserEnable` | вызывается при включении пользователем, не при загрузке |
| `Workspace.getActiveViewOfType` | `activeLeaf.view instanceof type ? view : null` — ровно `instanceof`, без эвристик |
| `Workspace.activeEditor` | `this._activeEditor ?? this.getActiveViewOfType(MarkdownView)` |
| `Workspace.iterateAllLeaves` | обход `rootSplit`, `leftSplit`, `rightSplit`, `floatingSplit` |
| `Workspace.getMostRecentLeaf` | лист с максимальным `activeTime` |
| `Workspace.setActiveLeaf` | `activeTime = Date.now()`, снимает/ставит `mod-active`, `_activeEditor = null` |
| `Workspace.revealLeaf` | разворачивает родителей, выбирает вкладку, фокусирует контейнер, `loadIfDeferred()` |
| `ViewRegistry.registerView` | `view-registered` / `view-unregistered` события |

---

## 3. Публичная поверхность

`obsidian` в 1.7.5 экспортирует **122 имени**: 66 классов, 56 функций и значений.

| Группа | Имена |
|---|---|
| Ядро | `App`, `Plugin`, `Component`, `Events`, `Scope`, `Keymap`, `Platform` |
| Настройки | `Setting`, `SettingTab`, `PluginSettingTab` |
| Модалки и подсказки | `Modal`, `SuggestModal`, `FuzzySuggestModal`, `PopoverSuggest`, `AbstractInputSuggest`, `EditorSuggest`, `HoverPopover` |
| Меню | `Menu`, `MenuItem`, `MenuSeparator` |
| Редактор | `Editor`, `MarkdownView`, `MarkdownRenderer`, `MarkdownRenderChild`, `MarkdownPreviewView`, `MarkdownPreviewRenderer`, `MarkdownPreviewSection` |
| Вью | `View`, `ItemView`, `FileView`, `TextFileView`, `EditableFileView` |
| Рабочая область | `Workspace`, `WorkspaceLeaf`, `WorkspaceItem`, `WorkspaceParent`, `WorkspaceSplit`, `WorkspaceTabs`, `WorkspaceRoot`, `WorkspaceSidedock`, `WorkspaceRibbon`, `WorkspaceFloating`, `WorkspaceWindow`, `WorkspaceContainer`, `ViewRegistry` |
| Файлы | `Vault`, `TAbstractFile`, `TFile`, `TFolder`, `FileManager`, `MetadataCache`, `FileSystemAdapter`, `CapacitorAdapter` |
| UI-компоненты | `BaseComponent`, `ValueComponent`, `AbstractTextComponent`, `TextComponent`, `TextAreaComponent`, `ButtonComponent`, `ExtraButtonComponent`, `ToggleComponent`, `SliderComponent`, `DropdownComponent`, `ColorComponent`, `SearchComponent`, `MomentFormatComponent`, `ProgressBarComponent`, `Notice` |
| Утилиты | `normalizePath`, `requestUrl`, `request`, `debounce`, `moment`, `setIcon`, `getIcon`, `addIcon`, `removeIcon`, `setTooltip`, `fuzzySearch`, `prepareFuzzySearch`, `prepareSimpleSearch`, `prepareQuery`, `sortSearchResults`, `renderResults`, `renderMatches`, `parseYaml`, `stringifyYaml`, `htmlToMarkdown`, `sanitizeHTMLToDom`, `getFrontMatterInfo`, `parseFrontMatterEntry`, `parseFrontMatterTags`, `parseFrontMatterAliases`, `parseFrontMatterStringArray`, `parseLinktext`, `getLinkpath`, `resolveSubpath`, `getAllTags`, `stripHeading`, `stripHeadingForLink`, `requireApiVersion`, `apiVersion`, `arrayBufferToBase64`, `base64ToArrayBuffer`, `arrayBufferToHex`, `hexToArrayBuffer`, `getBlobArrayBuffer`, `finishRenderMath`, `renderMath`, `loadMathJax`, `loadMermaid`, `loadPdfJs`, `loadPrism`, `iterateRefs`, `iterateCacheRefs`, `livePreviewState`, `editorInfoField`, `editorEditorField`, `editorLivePreviewField`, `editorViewField` |

**Важно:** `app.plugins`, `app.commands`, `app.setting`, `app.internalPlugins`, `app.statusBar`,
`app.viewRegistry` в `obsidian.d.ts` **не описаны** — это внутренние API. Но плагины их используют
(см. §4), поэтому «не документировано» ≠ «не нужно».

---

## 4. Что вызывают настоящие плагины

### 4.1 Члены `app.*` — 106 различных, 981 обращение

| Член | Репозиториев | Вызовов |
|---|---:|---:|
| `workspace.getActiveViewOfType` | 6 | 113 |
| `workspace.trigger` | 6 | 54 |
| `metadataCache.getFirstLinkpathDest` | 6 | 53 |
| `workspace.on` | 6 | 44 |
| `vault.on` | 6 | 33 |
| `vault.adapter` | 4 | 51 |
| `metadataCache.getFileCache` | 4 | 40 |
| `vault.getAbstractFileByPath` | 4 | 38 |
| `vault.read` | 4 | 34 |
| `workspace.activeEditor` | 4 | 25 |
| `workspace.getLeavesOfType` | 4 | 17 |
| `workspace.onLayoutReady` | 6 | 13 |
| `workspace.getLeaf` | 5 | 27 |
| `workspace.getActiveFile` | 5 | 14 |
| `vault.getFileByPath` | 3 | 62 |
| `metadataCache.fileToLinktext` | 1 | 25 |
| `workspace.iterateAllLeaves` | 3 | 12 |
| `vault.cachedRead` | 3 | 9 |
| `vault.getConfig` | 3 | 9 |
| `fileManager.processFrontMatter` | 3 | 3 |
| `plugins.plugins` | 3 | 13 |

Полная разбивка по пространствам имён:

| Пространство | Используется | Есть в хосте | Нет |
|---|---:|---:|---:|
| `workspace` | 33 | 9 | 24 |
| `vault` | 28 | 11 | 17 |
| `metadataCache` | 10 | 4 | 6 |
| `fileManager` | 8 | 2 | 6 |
| `setting` | 5 | 0 | 5 |
| `plugins` | 4 | 0 | 4 |
| `keymap` | 3 | 0 | 3 |
| `commands` | 3 | 1 | 2 |
| `internalPlugins` | 3 | 1 | 2 |
| остальные (9 пространств) | 9 | 0 | 9 |
| **Итого** | **106** | **28** | **78** |

### 4.2 Конструкторы и импорты

| Класс | Репозиториев | `new` |
|---|---:|---:|
| `Setting` | 8 | 274 |
| `Notice` | 5 | 253 |
| `ButtonComponent` | 4 | 30 |
| `Menu` | 5 | 11 |
| `Component` | 3 | 6 |
| `TextComponent` | 3 | 5 |
| `TextAreaComponent` | 3 | 4 |

`Setting` и `Notice` — самая массовая точка контакта: 527 конструкций на 8 плагинов. Если ломается
`Setting`, ломаются настройки **всех** плагинов; если ломается `Notice`, плагин не может сообщить
об ошибке.

### 4.3 Покрытие по плагинам

| Плагин | Использует `app.*` | Покрыто | Нет | Что болит в первую очередь |
|---|---:|---:|---:|---|
| excalidraw | 77 | 20 | 57 | `getActiveViewOfType`, `fileToLinktext`, `getMostRecentLeaf` |
| templater | 38 | 19 | 19 | `activeEditor`, `plugins.plugins`, `vault.getConfig` |
| kanban | 29 | 16 | 13 | `getActiveViewOfType`, `activeEditor`, `vault.getRoot` |
| git | 26 | 12 | 14 | `workspace.offref`, `vault.configDir`, `updateOptions` |
| dataview | 16 | 6 | 10 | `openLinkText`, `plugins.plugins`, `getActiveViewOfType` |
| tasks | 13 | 10 | 3 | `activeLeaf`, `processFrontMatter`, `scope.register` |
| calendar | 8 | 5 | 3 | `layoutReady`, `activeLeaf`, `cachedRead` |
| sample (официальный) | 1 | 0 | 1 | `getActiveViewOfType` |

Даже официальный пример из документации Obsidian не работает: он вызывает
`workspace.getActiveViewOfType`, которого в хосте нет.

---

## 5. Разрыв по классам

Сопоставление 21 класса, который хост пытается эмулировать, с контрактом:

| Класс | Методов в контракте | Нет в хосте |
|---|---:|---:|
| `Editor` | 32 | **30** |
| `WorkspaceLeaf` | 16 | **13** |
| `Setting` | 22 | **10** |
| `Plugin` | 19 | **8** |
| `Menu` | 12 | **8** |
| `SuggestModal` | 8 | **5** |
| `Modal` | 8 | **4** |
| `View` | 11 | **4** |
| `PluginSettingTab` | 3 | **3** |
| `FuzzySuggestModal` | 6 | **3** |
| `AbstractInputSuggest` | 5 | **3** |
| `EditorSuggest` | 3 | **2** |
| `MenuItem` | 8 | **2** |
| `Notice` | 2 | **2** |
| `ItemView` | 1 | **1** |
| `TFolder` | 1 | **1** |
| `Component` | 10 | 0 |
| `Events` | 5 | 0 |
| `Scope` | 2 | 0 |
| `TFile` | 0 | 0 |
| `TAbstractFile` | 0 | 0 |
| **Итого** | **174** | **99** |

Из 99 отсутствующих **79 существуют в 1.7.5** (проверено поиском `prototype.<имя>=` по бандлу).
Остальные 11 — новее установленной версии: `registerBasesView`, `registerCliHandler`,
`getSettingDefinitions`, `getControlValue`, `setControlValue`, `addDisplayValue`, `setErrorMessage`,
`addComponent`, `onHistoryBack`, `forEvent`, `setCloseCallback`. Их можно отложить.

Отдельно: `Component` формально покрыт 11/10, но `registerEvent` там — `return ref;`, то есть
**наличие метода не означает наличие поведения**. Это тот же класс дефекта, что и отсутствие:
плагин видит функцию, вызывает её и не получает подписки.

### 5.1 Экспорты

| Статус | Имён | Что происходит |
|---|---:|---|
| Реальные | 44 | класс или функция хоста |
| Подменённые пустым классом | 28 | `Proxy` создаёт `class Name extends Component {}` по заглавной букве |
| `undefined` | 22 | утилиты не экспортированы вовсе |
| Типы (стираются при сборке) | 16 | не проблема |

Подмена пустым классом — самая коварная часть. `import { MarkdownView } from "obsidian"` **успешно
выполняется**, плагин не падает на импорте, но `instanceof` никогда не совпадёт, потому что реальные
вью не наследуют этот фальшивый класс. Ошибка всплывает позже и в другом месте.

Список подменяемых, по частоте в корпусе: `MarkdownView` (7), `Platform` (5), `MarkdownRenderer` (4),
`Workspace` (2), `FileSystemAdapter` (2), `HoverPopover` (2), `SettingPage` (2), плюс `FuzzyMatch`,
`FrontMatterCache`, `ViewStateResult`, `WorkspaceSplit`, `WorkspaceItem`, `WorkspaceWindow`,
`PluginManifest`, `FileStats`, `HeadingCache`, `ColorComponent`, `SecretComponent`, `SettingGroup`,
`ConfirmationModal`, `ConfirmationButton`, `Pos`, `Point`, `Modifier`, `Instruction`, `Stat`,
`EditorSuggestTriggerInfo`, `HoverParent`.

---

## 6. Что добавлять

Приоритет — по измеренному покрытию: P0 блокирует большинство плагинов, P3 — единичные сценарии.
Сигнатуры даны по `obsidian.d.ts`; для 1.7.5 они совпадают, кроме помеченных «новее 1.7.5».

> **Статус на 18.10.2026.** P0: пункты 2, 3, 4 сделаны (патч 1). Пункт 1 (инварианты жизненного
> цикла) и 5–7 (`adapter`, `metadataCache`, `Editor`) — **нет**. Из P1 сделаны `MarkdownRenderer`,
> `openLinkText`, `iterateAllLeaves`, `getMostRecentLeaf`, `createFolder`, `cachedRead`, `configDir`;
> остальное актуально. Подробности — в §7, «Статус».

### P0 — без этого плагины не работают

**1. Инварианты жизненного цикла** (исправление, не добавление)

- `Component.load`: `_loaded = true` **до** `onload()`.
- `Component.unload`: дети LIFO → disposer'ы LIFO → `onunload()` последним.
- `Component.registerEvent(ref)`: `this.register(() => ref.e.offref(ref))`, не `return ref`.
- `Plugin.load`: `_loaded = true` → `await onload()` → загрузка детей.
- `Plugin` в загрузчике: вызывать `inst.load()`, а не `inst.onload()` (иначе `_loaded` остаётся `false`,
  и `addChild`/`register` ведут себя как у незагруженного компонента).

**2. `MarkdownView` как настоящий класс** (7 репозиториев, 22 импорта)

```ts
class MarkdownView extends TextFileView {
  editor: Editor;
  file: TFile | null;
  getMode(): MarkdownViewModeType;          // 'source' | 'preview'
  getViewType(): string;                    // 'markdown'
  getState(): any;
  setState(state: any, result: ViewStateResult): Promise<void>;
}
```

Без него `workspace.getActiveViewOfType(MarkdownView)` всегда `null` — а это 113 обращений в 6 плагинах.

**3. `workspace.getActiveViewOfType` / `activeEditor` / `activeLeaf`**

```ts
getActiveViewOfType<T extends View>(type: Constructor<T>): T | null;
activeEditor: MarkdownFileInfo | null;
activeLeaf: WorkspaceLeaf | null;
```

Семантика ровно `instanceof`, без проверки `viewType`. `activeEditor` = `_activeEditor ?? getActiveViewOfType(MarkdownView)`.

**4. `Platform`** (5 репозиториев)

```ts
const Platform: {
  isDesktop: boolean; isMobile: boolean;
  isDesktopApp: boolean; isMobileApp: boolean;
  isIosApp: boolean; isAndroidApp: boolean;
  isPhone: boolean; isTablet: boolean;
  isMacOS: boolean; isWin: boolean; isLinux: boolean; isSafari: boolean;
  resourcePathPrefix: string;
  canExportPdf: boolean; canPopoutWindow: boolean;
  canStackTabs: boolean; canSplit: boolean; canDisplayRibbon: boolean;
  mobileSoftKeyboardVisible: boolean;
}
```

Сейчас `Platform` попадает в фальшивый класс, `Platform.isMobile` → `undefined` → плагин уходит в
десктопную ветку.

**5. `Vault.adapter` целиком** (4 репозитория, 51 обращение)

Хост отдаёт `{ read, write, exists }`. Контракт `DataAdapter`:

```ts
exists(normalizedPath: string): Promise<boolean>;
read(normalizedPath: string): Promise<string>;
readBinary(normalizedPath: string): Promise<ArrayBuffer>;
write(normalizedPath: string, data: string, options?: DataWriteOptions): Promise<void>;
writeBinary(normalizedPath: string, data: ArrayBuffer, options?: DataWriteOptions): Promise<void>;
append(normalizedPath: string, data: string, options?: DataWriteOptions): Promise<void>;
mkdir(normalizedPath: string): Promise<void>;
rmdir(normalizedPath: string, recursive: boolean): Promise<void>;
remove(normalizedPath: string): Promise<void>;
rename(normalizedPath: string, normalizedNewPath: string): Promise<void>;
stat(normalizedPath: string): Promise<Stat | null>;
list(normalizedPath: string): Promise<ListedFiles>;
getBasePath(): string;
getResourcePath(normalizedPath: string): string;
```

**6. `metadataCache` — реальные данные** (6 репозиториев)

```ts
getFirstLinkpathDest(linkpath: string, sourcePath: string): TFile | null;
getFileCache(file: TFile): CachedMetadata | null;
getCache(path: string): CachedMetadata | null;
resolvedLinks: Record<string, Record<string, number>>;
fileToLinktext(file: TFile, sourcePath: string, omitMdExtension?: boolean): string;
```

Сейчас `getFirstLinkpathDest` возвращает строку `path` (плагин ждёт `TFile` и читает `.path`,
`.basename`, `.extension` — получает `undefined`), а `getFileCache` — пустой объект
`{tags:[],links:[],headings:[],frontmatter:{}}`, то есть «файл есть, а метаданных нет» неотличимо
от «файл пуст». Оба случая — тихая деградация, а не ошибка.

**7. `Editor`** (5 репозиториев)

30 отсутствующих методов. Минимальный рабочий набор:

```ts
getValue(): string;  setValue(content: string): void;
getLine(line: number): string;  setLine(line: number, text: string): void;
lineCount(): number;  lastLine(): number;
getCursor(string?: 'from'|'to'|'head'|'anchor'): EditorPosition;
setCursor(pos: EditorPosition | number, ch?: number): void;
getSelection(): string;
getRange(from: EditorPosition, to: EditorPosition): string;
replaceSelection(replacement: string, origin?: string): void;
replaceRange(replacement: string, from: EditorPosition, to?: EditorPosition, origin?: string): void;
listSelections(): EditorSelection[];
setSelection(anchor: EditorPosition, head?: EditorPosition): void;
setSelections(ranges: EditorSelectionOrCaret[], main?: number): void;
somethingSelected(): boolean;
focus(): void;  blur(): void;  hasFocus(): boolean;
scrollIntoView(range: EditorRange, center?: boolean): void;
undo(): void;  redo(): void;
posToOffset(pos: EditorPosition): number;  offsetToPos(offset: number): EditorPosition;
wordAt(pos: EditorPosition): EditorRange | null;
processLines(read: (line: number, lineText: string) => string | null,
             write: (line: number, lineText: string, newLineText: string) => void,
             ignoreEmpty?: boolean): void;
```

### P1 — ломает конкретные сценарии

**8. `Workspace` — 24 отсутствующих члена.** Наиболее нужные:

```ts
onLayoutReady(callback: () => any): void;          // очередь, не setTimeout
iterateAllLeaves(callback: (leaf: WorkspaceLeaf) => any): void;
iterateRootLeaves(callback: (leaf: WorkspaceLeaf) => any): void;
getMostRecentLeaf(root?: WorkspaceParent): WorkspaceLeaf | null;
setActiveLeaf(leaf: WorkspaceLeaf, params?: { focus?: boolean }): void;
setActiveLeaf(leaf: WorkspaceLeaf, pushHistory: boolean, focus: boolean): void;
revealLeaf(leaf: WorkspaceLeaf): Promise<void>;
openLinkText(linktext: string, sourcePath: string, newLeaf?: PaneType | boolean,
             openViewState?: OpenViewState): Promise<void>;
getLeafById(id: string): WorkspaceLeaf | null;
createLeafBySplit(leaf: WorkspaceLeaf, direction?: SplitDirection, before?: boolean): WorkspaceLeaf;
splitActiveLeaf(direction?: SplitDirection): WorkspaceLeaf;
updateOptions(): void;
getLayout(): Record<string, unknown>;
layoutReady: boolean;
```

**9. `Vault` — 17 отсутствующих.**

```ts
getFolderByPath(path: string): TFolder | null;
getRoot(): TFolder;
cachedRead(file: TFile): Promise<string>;
getConfig(key: string): any;   setConfig(key: string, value: any): void;
createFolder(path: string): Promise<TFolder>;
createBinary(path: string, data: ArrayBuffer, options?: DataWriteOptions): Promise<TFile>;
readBinary(file: TFile): Promise<ArrayBuffer>;
modifyBinary(file: TFile, data: ArrayBuffer, options?: DataWriteOptions): Promise<void>;
getResourcePath(file: TFile): string;
getAllLoadedFiles(): TAbstractFile[];
getAvailablePath(basePath: string, extension: string): string;
getAbstractFileByPathInsensitive(path: string): TAbstractFile | null;
getName(): string;
configDir: string;
```

**10. `FileManager` — 6 отсутствующих** (3 репозитория на `processFrontMatter`):

```ts
processFrontMatter(file: TFile, fn: (frontmatter: any) => void, options?: DataWriteOptions): Promise<void>;
renameFile(file: TAbstractFile, newPath: string): Promise<void>;
trashFile(file: TAbstractFile): Promise<void>;
getNewFileParent(sourcePath: string, newFilePath?: string): TFolder;
getAvailablePathForAttachment(filename: string, sourcePath?: string): Promise<string>;
createNewFile(...): Promise<TFile>;
```

`processFrontMatter` — не «записать YAML», а «прочитать, дать изменить объект, сериализовать,
записать» с сохранением остального файла.

**11. `Setting` — 10 отсутствующих** (8 репозиториев):

```ts
setDisabled(disabled: boolean): this;
addSearch(cb: (component: SearchComponent) => any): this;
addMomentFormat(cb: (component: MomentFormatComponent) => any): this;
addColorPicker(cb: (component: ColorComponent) => any): this;
addProgressBar(cb: (component: ProgressBarComponent) => any): this;
then(cb: (setting: this) => any): this;
clear(): void;
```

**12. `Menu` — 8 отсутствующих:** `hide()`, `close()`, `setNoIcon()`, `setUseNativeMenu()`,
`setParentElement()`, `onHide()`, `forEvent()`, `onHistoryBack()`.
`hide`/`close` критичны: без них меню нельзя закрыть программно.

**13. `Notice` — 2 отсутствующих:** `setMessage(message: string): this`, `hide(): void`,
плюс поля `noticeEl`, `containerEl`, `messageEl`. 253 конструкции в 5 плагинах.

**14. `Plugin` — 8 отсутствующих:**

```ts
addStatusBarItem(): HTMLElement;
removeCommand(commandId: string): void;
registerMarkdownPostProcessor(pp: MarkdownPostProcessor, sortOrder?: number): MarkdownPostProcessor;
registerObsidianProtocolHandler(action: string, handler: ObsidianProtocolHandler): void;
registerEditorSuggest(es: EditorSuggest<any>): void;
onUserEnable(): void;
```

`addStatusBarItem` требует `app.statusBar` — его в хосте нет вовсе.

**15. `MarkdownRenderer.render`** (4 репозитория)

```ts
static render(app: App, markdown: string, el: HTMLElement, sourcePath: string, component: Component): Promise<void>;
static renderMarkdown(markdown: string, el: HTMLElement, sourcePath: string, component: Component): Promise<void>;
```

Пятый аргумент обязателен: Obsidian проверяет `component instanceof Component` и ругается в консоль,
если передали не компонент.

### P2 — расширяет охват

**16. Утилиты (22 отсутствуют):**

```ts
parseYaml(yaml: string): any;
stringifyYaml(obj: any): string;
htmlToMarkdown(html: string | HTMLElement | Document | DocumentFragment): string;
sanitizeHTMLToDom(html: string): DocumentFragment;
getFrontMatterInfo(content: string): FrontMatterInfo;
parseLinktext(linktext: string): { path: string; subpath: string };
getLinkpath(linktext: string): string;
resolveSubpath(cache: CachedMetadata, subpath: string): HeadingSubpathResult | BlockSubpathResult | null;
getAllTags(cache: CachedMetadata): string[] | null;
requireApiVersion(version: string): boolean;
request(request: RequestUrlParam | string): Promise<string>;
prepareFuzzySearch(query: string): (text: string) => SearchResult | null;
sortSearchResults(results: SearchResultContainer[]): void;
getIcon(iconId: string): SVGSVGElement | null;
arrayBufferToBase64(buffer: ArrayBuffer): string;
base64ToArrayBuffer(base64: string): ArrayBuffer;
parseFrontMatterEntry / parseFrontMatterTags / parseFrontMatterAliases / parseFrontMatterStringArray
```

`requireApiVersion` — единственный способ плагина проверить версию хоста; без него плагины либо
падают на отсутствующем методе, либо обкладываются `try/catch`.

**17. Внутренние пространства имён** (используются, но не документированы):

```ts
app.plugins.plugins: Record<string, Plugin>;        // 3 репозитория
app.plugins.manifests: Record<string, PluginManifest>;
app.plugins.getPlugin(id: string): Plugin | undefined;
app.plugins.enabledPlugins: Set<string>;
app.commands.commands: Record<string, Command>;
app.commands.listCommands(): Command[];
app.keymap.pushScope(scope: Scope): void;  popScope(scope: Scope): void;  getRootScope(): Scope;
app.setting.open(): void;  openTabById(id: string): void;
```

**18. Подсказки и модалки:**

```ts
SuggestModal: setPlaceholder(text: string); setInstructions(inst: Instruction[]);
              onNoSuggestion(); selectSuggestion(value, evt); selectActiveSuggestion(evt);
FuzzySuggestModal: getSuggestions(query): FuzzyMatch<T>[]; renderSuggestion(item, el);
                   onChooseSuggestion(item, evt);
AbstractInputSuggest: setValue(value: string); getValue(): string; onSelect(value, evt);
EditorSuggest: setInstructions(inst: Instruction[]); onTrigger(context, editor): EditorSuggestTriggerInfo | null;
Modal: setTitle(title: string); setContent(content: string); setCloseCallback(cb: () => any);
```

**19. `WorkspaceLeaf` — 13 отсутствующих:** `open(file, openViewState?)`, `getViewState()`,
`getEphemeralState()`, `setEphemeralState(state)`, `loadIfDeferred()`, `setPinned(pinned)`,
`togglePinned()`, `getIcon()`, `getDisplayText()`, `onResize()`, `on(...)`, `setGroup()`, `setGroupMember()`.

### P3 — единичные сценарии

`app.secretStorage` (`getSecret`/`setSecret`), `app.internalPlugins.plugins`,
`app.dragManager.draggable`, `app.mobileToolbar.update`, `app.metadataTypeManager.setType`,
`app.hotkeyManager.addDefaultHotkeys`, `app.viewRegistry.getTypeByExtension`,
`app.embedRegistry.embedByExtension`, `app.scope.register`, `Scope.handleKey`,
`MenuItem.setWarning/setSection`, `TFolder.isRoot`, `ItemView.addAction`.

---

## 7. Порядок внедрения

Порядок выбран так, чтобы каждый шаг **разблокировал проверяемый сценарий**, а не «добавлял методы».

| Шаг | Что делаем | Критерий приёмки |
|---|---|---|
| 1 | Инварианты `Component`/`Plugin` (§6.1) | тест: плагин с `registerEvent` в `onload` после `unload` не получает событий; `onunload` видит живой DOM |
| 2 | `MarkdownView` + `TextFileView` + `FileView` | `getActiveViewOfType(MarkdownView)` возвращает вью для открытого `.md` |
| 3 | `workspace.getActiveViewOfType` / `activeEditor` / `activeLeaf` | официальный `obsidian-sample-plugin` проходит свой сценарий |
| 4 | `Platform` | `Platform.isMobile === false`, `isDesktopApp === true` в десктопной сборке |
| 5 | `metadataCache` на реальных данных | `getFirstLinkpathDest` возвращает `TFile`, `getFileCache` — реальный frontmatter |
| 6 | `Vault.adapter` целиком | плагин, читающий `adapter.stat`/`list`, работает без `undefined` |
| 7 | `Editor` (30 методов) | команда `editorCallback` правит документ и ставит курсор |
| 8 | `Setting` + `PluginSettingTab.display` по требованию | настройки 8/8 плагинов корпуса открываются |
| 9 | `Notice.setMessage/hide`, `Menu.hide/close` | уведомление можно обновить и закрыть |
| 10 | `Workspace` P1 + `Vault` P1 | `onLayoutReady` срабатывает после готовности лейаута, `iterateAllLeaves` видит листы |
| 11 | Утилиты P2 | `parseYaml`/`stringifyYaml`/`htmlToMarkdown` не падают на типовых входах |
| 12 | `app.plugins`/`commands`/`keymap` | `app.plugins.getPlugin("dataview")` возвращает экземпляр |

**Статус на 18.10.2026.** Шаги 2–4 закрыты патчем 1 в `runtime.rs` — с одной оговоркой: текст
документа `MarkdownView.editor` не отдаёт (это шаг 7, нужен запрос к хосту за буфером). Дополнительно
закрыты `vault.createFolder` (новый запрос `vault_create_folder` в `api.rs` + `bridge.rs`),
`vault.cachedRead` и `vault.configDir`. Патч 2 закрыл `MarkdownRenderer`, `workspace.openLinkText`,
`iterateAllLeaves` и `getMostRecentLeaf`. Эффект на корпусе 500 плагинов: без нехваток API было 52,
стало 124; `ok` 36 → 88 (`plugin-compat-500.md` §0).

**Статус на 21.10.2026.** Шаг 7 (Editor) закрыт. Чтение буфера синхронное: `Snapshot` несёт
`active_doc` (rope + курсор/якорь в (line, ch)), публикуется из единого выхода `handle_key`,
клика по документу, drag-выделения и `dispatch_edit_event`; JS-класс `Editor` читает его через
`__host_get_active_buffer()` — `getValue`, `getLine`, `setLine`, `lineCount`, `lastLine`,
`getCursor`/`setCursor` (в т.ч. `"anchor"`), `getRange`, `getSelection`, `replaceSelection`,
`replaceRange`, `setSelection`, `listSelections`, `somethingSelected` + no-op `focus`/`blur`/
`hasFocus`. Запись идёт запросами `editor_set_buffer`/`editor_replace_range`/`editor_set_cursor`/
`editor_set_selection` (api.rs, bridge.rs). Модель `getFirstLinkpathDest`/`getFileCache` из §6.6
осталась как описано. Ограничения: `getSelection()` использует async-канал; мультикурсорности нет
(одна выборка); позиции не валидируются после чужих правок между чтением и записью. Харнесс:
125/125 (16 новых тестов Editor). Коммит: буфер редактора.

**Статус на 21.10.2026, этап 2.** Шаг 11 (утилиты P2) закрыт частично: `parseYaml`/`stringifyYaml`
(YAML-парсер в бутстрапе, frontmatter-подмножество: вложенные карты, списки объектов, inline-flow,
block-скаляры, комментарии; якоря/теги/многодокументность не поддерживаются и падают), `getAllTags`,
`requireApiVersion` (хост = 1.7.5), `parseLinktext`, `getLinkpath`,
`arrayBufferToBase64`/`base64ToArrayBuffer`. Из пачки 1 (`plugin-compat-500.md` §6) закрыт
`fileManager.processFrontMatter` (чтение → мутация объекта → ре-сериализация YAML → запись файла
с нетронутым телом), `trashFile` (переиспользует `vault_trash`), `renameFile`; `vault.getRoot`/
`getAllLoadedFiles`/`getName`/`getResourcePath`. Эффект на корпусе 500: `ok` 91 → **123**,
`degraded` 187 → 155. Харнесс 146/146. Не закрыты из шага 11: `sanitizeHTMLToDom`,
`getFrontMatterInfo`, `resolveSubpath`, `prepareFuzzySearch`, `request`.

### 7.1 Что проверить на каждом шаге

- **Загрузка → выгрузка → повторная загрузка** одного и того же плагина: реестры, подписки, таймеры,
  DOM и `<style>` возвращаются к базовому состоянию.
- **Ошибка в `onload`**: плагин помечается выключенным, остальные грузятся, сообщение называет id.
- **`registerEvent` после `unload`**: обработчик не вызывается (сейчас вызывается всегда, потому что
  подписки не снимаются).
- **`instanceof` для всех экспортируемых классов**: плагин, делающий `view instanceof MarkdownView`,
  получает `true` на реальной вью.

---

## 8. Чего в этом документе нет

- **Частоты по методам классов.** Корпус даёт `new Setting(…)` и `import { … }`, но не «сколько раз
  вызван `Setting.addSearch`». Для этого нужен AST-проход по `.ts` с типами — отдельная работа.
- **Поведения CodeMirror 6.** `registerEditorExtension` и `editorInfoField` требуют настоящего CM6;
  заглушка не даст ничего, кроме тихой пустоты.
- **Мобильных сценариев.** `Platform.isMobileApp`/`canPopoutWindow` в десктопной сборке можно
  зафиксировать константами, но проверять нечем.

---

## 9. Как воспроизвести

```bash
# 1. Распаковать app.js из asar (pickle: u32=4, u32=payload, u32=json_len, JSON at 16)
python - <<'PY'
import json,struct
d=open('obsidian.asar','rb').read()
jlen=struct.unpack('<I',d[12:16])[0]
hdr=json.loads(d[16:16+jlen].decode())
base=(16+jlen+3)//4*4
n=hdr['files']['app.js']; open('app.js','wb').write(d[base+int(n['offset']):base+int(n['offset'])+int(n['size'])])
PY

# 2. Найти карту экспортов obsidian: n.d(d,{Component:()=>gA, Plugin:()=>LZ, ...})
#    и вытащить 122 имени → локальные переменные.

# 3. Для каждой переменной найти фабрику VAR=function(base){…}(Base) с учётом строк,
#    собрать prototype-члены, разрешить наследование по аргументу IIFE.

# 4. Сравнить с samples/vibecodium/plugins/runtime.rs:
#    классы — по prototype-членам и полям из конструктора;
#    объекты rawVault/rawWorkspace/rawMetadataCache/app — по ключам верхнего уровня.

# 5. Корпус: 8 репозиториев, импорты из "obsidian", обращения app.<ns>.<member>, new <Class>(.
```

Артефакты измерений: `api-exports.json`, `api-classes.json`, `host-classes.json`,
`host-objects.json`, `corpus-stats.json`, `gap-summary.json`, `class-gap.json`.
