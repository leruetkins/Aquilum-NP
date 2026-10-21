# Vibecodium: хост плагинов Obsidian — оценка возможностей

**Что прочитано:** `samples/vibecodium/plugins/` — 9 633 строки Rust + встроенный JS-бутстрап
(~3 000 строк внутри `runtime.rs`).

| Файл | Строк | Роль |
|---|---:|---|
| `runtime.rs` | 5 221 | QuickJS-рантайм на поток, весь Obsidian-шим на JS |
| `ui.rs` | 2 189 | декларативный `UiNode`, рендер |
| `bridge.rs` | 775 | маршрутизация `ApiCall` ↔ `HostEvent`, права |
| `registry.rs` | 313 | реестр плагинов, состояние |
| `manifest.rs` | 288 | манифест, semver, права, гранты |
| `loader.rs` | 275 | дискавери и запуск |
| `compat.rs` | 247 | статический анализатор совместимости |
| `api.rs` | 234 | протокол запросов/событий |
| `code_block.rs` | 82 | обработка code block |

> **Статус на 19.10.2026.** Документ — снимок аудита; часть P0 с тех пор закрыта.
> Проверено по коду приложения (`Aquilum-NP`, коммит `f94d2a3`), снимок в REA совпадает с ним
> побайтно (`tools/mirror-check.mjs` → 10/10):
> - `vault.read` больше не подставляет `"non-empty-fallback-on-error"`, а пробрасывает ошибку
>   (`runtime.rs:3152-3163`) — закрыто.
> - `vault.trash` больше не зовёт `vault_delete`: отдельный `VaultTrash` → `vault.trash_file()`,
>   файл уезжает в `.trash/` с уникальным именем (`api.rs:20`, `bridge.rs:400`, `vault.rs:234`) — закрыто.
> - `MarkdownView` и `MarkdownRenderer` — сделаны (патчи 1–2).
> - `Editor("")` в `editorCallback` — **не сделано**, это следующая по важности работа после CSS.
>
> Ниже — исходный аудит; номера строк в нём относятся к версии 5 221 строки и не совпадают
> с текущими 5 766.

---

## 1. Вердикт коротко

**Архитектура правильная, слой совместимости — нет.**

Вы выбрали ровно тот путь, который я бы и рекомендовал: изоляция вместо общего JS-реалма,
снимок состояния для синхронных API, декларативный UI вместо DOM, права на манифесте,
лимит памяти и watchdog. По безопасности это **лучше Obsidian**, а не хуже.

Но в шиме есть **7 дефектов, которые ломают базовый инвариант жизненного цикла**, и **3 из них
ведут к потере данных пользователя**. Пока они не исправлены, «оценка возможностей» такая:
простые плагины (команды, notice, code block, настройки) — работают; всё, что делает панель,
редакторную команду или читает метаданные — либо падает, либо врёт.

Хорошая новость: это не архитектурные проблемы. Это ~200 строк правок в одном файле.

---

## 2. Что сделано хорошо

Не из вежливости — это реально сильные решения, которые стоит сохранить.

**Изоляция на поток + QuickJS.** `spawn_plugin_runtime` создаёт отдельный `Runtime` на плагин,
`set_memory_limit(256 MB)`, `set_interrupt_handler` с дедлайном. Плагин физически не может
ни прочитать чужой `data.json`, ни залезть в `window`, ни уронить хост.

**Снимок для синхронных API.** `__host_get_active_file()`, `__host_get_markdown_files()`,
`__host_get_language()` читают из `ArcSwap<Snapshot>` — синхронно, без round-trip. Это
единственно верный способ отдать плагину синхронный `getActiveFile()`, когда он в другом потоке.
Гейт по `vault:read` на этих геттерах — тоже правильно.

**Права и пиннинг бандла.** `PluginGrantsStore` с ключом `pluginId:bundleSha256` — плагин,
изменивший код, теряет грант. Этого нет в Obsidian вообще.

**`VElement` + `__velement_to_uinode`.** Эмуляция DOM с `createEl`/`createDiv`/`setText`/`addClass`/
`find`/`findAll`/`classList`/`style`/`dataset`, парсер HTML с `DANGEROUS_TAGS`, CSS-селекторы.
Это большой и аккуратный кусок — именно он даёт шанс UI-плагинам.

**`getOrCreateFallbackClass` + `obsidianProxy`.** Неизвестный класс из `require("obsidian")`
возвращается как заглушка, а не `undefined` — плагин падает на конкретном вызове, а не на
`extends undefined`. Умно.

**Честный отказ на Electron/Node.** `createLazyFailProxy` бросает внятную ошибку
«not supported in native GPUI mode». Правильно: лучше явный отказ, чем тихий no-op.

---

## 3. P0 — потеря данных

### 3.1 `vault.read` возвращает строку-заглушку при ошибке

`runtime.rs:2808-2817`

```js
read: async (path) => {
    ...
    try {
        const res = await __invoke_host("vault_read", { path: p });
        return res != null ? String(res) : "";
    } catch (e) {
        console.warn(`[plugin] vault.read error for "${p}":`, e);
        return "non-empty-fallback-on-error";   // ←
    }
},
```

Это самая опасная строка во всём проекте. Стандартный паттерн плагина:

```js
const content = await app.vault.read(file);
const updated = content.replace(/foo/g, "bar");
await app.vault.modify(file, updated);
```

Любая транзиентная ошибка (файл занят, гонка с сохранением, таймаут хоста) → `content` становится
`"non-empty-fallback-on-error"` → плагин **записывает эту строку в заметку**. Тихая потеря текста,
воспроизводимая на любом read-modify-write плагине.

`"non-empty"` в имени выдаёт, зачем это было сделано: чтобы плагин не упал на пустой строке при
отладке. Цена — порча данных.

**Исправление:** `throw new Error(...)`. Плагин, не умеющий обрабатывать ошибки, должен упасть
на своей ошибке, а не записать мусор.

### 3.2 `vault.trash` делает безвозвратное удаление

`runtime.rs:2826`

```js
trash: (path, system) => {
    ...
    return __invoke_host("vault_delete", { path: p });   // ← то же, что delete()
},
```

`trash` в Obsidian — безопасная операция (в корзину Obsidian или системную). Плагин, выбирающий
`trash` вместо `delete` именно из осторожности, получает **худший** результат. Инверсия гарантий.

**Исправление:** отдельный `ApiRequest::VaultTrash`, в хосте — перемещение в корзину, не unlink.

### 3.3 `Editor("")` + `new MarkdownView()` в editor-командах

`runtime.rs:2728-2736`

```js
else if (typeof cmd.editorCallback === "function") {
    const editor = new Editor("");       // пустой редактор
    const view = new MarkdownView();     // ← класса НЕТ в бутстрапе
    return cmd.editorCallback(editor, view);
}
```

`MarkdownView` **не определён нигде** (`grep` даёт только эти две строки использования).
То есть любая команда с `editorCallback`/`editorCheckCallback` при выполнении бросает
`ReferenceError`, который ловится в `dispatch_command_execution` и уходит в лог. Команда
молча ничего не делает.

Если бы `MarkdownView` существовал, было бы не лучше: `new Editor("")` — это `getSelection() → ""`,
`replaceSelection(x)` → подмена выделения. `editor.getValue` вообще отсутствует (`class Editor`
имеет только `getSelection` и `replaceSelection`). Плагин вида
`editor.replaceSelection(editor.getValue().toUpperCase())` падает на `getValue is not a function`.

**Исправление:** либо реальный мост к активному редактору (`editor_get_value`,
`editor_get_range`, `editor_set_cursor` через `ApiRequest`), либо честный отказ на этапе
регистрации: «editor-команды не поддерживаются» — и не регистрировать команду вовсе.

---

## 4. P0 — жизненный цикл сломан

Это главное. Весь плагинный контракт Obsidian держится на одном инварианте: **всё, что
зарегистрировано, снимается при выгрузке**. Здесь он не держится.

### 4.1 `registerEvent` — пустышка в двух местах

`runtime.rs:2191` (Component) и `2786` (Plugin)

```js
registerEvent(ref) { return ref; }
```

`registerEvent` — **самый используемый метод экосистемы**. В Obsidian он делает:

```js
registerEvent(e){ this.register(() => e.e.offref(e)) }
```

Здесь он не делает ничего. Следствия:

- `rawVault.on()` кладёт колбэк в `__vault_listeners` и возвращает `{id, unload}`;
- `registerEvent` этот `unload` не вызывает;
- после выгрузки плагина колбэк остаётся в массиве и **продолжает получать события**;
- замыкание держит объект плагина → утечка памяти на весь срок жизни хоста.

Плюс форма `EventRef` несовместима: Obsidian отдаёт `{e: Events, ...}`, здесь `{id, unload}`.
Поэтому и `app.vault.offref(ref)` из плагина не сработает.

**Исправление:** `EventRef` должен иметь `.e` (ссылку на `Events`), а `registerEvent` —
регистрировать `offref`.

### 4.2 Порядок в `Component.unload()` перевёрнут

`runtime.rs:2159`

```js
unload() {
    this._loaded = false;
    if (typeof this.onunload === "function") this.onunload();   // ← первым
    for (const c of this._children) { c.unload(); }             // ← FIFO
    this._children = [];
}
```

Obsidian (`Component.unload`, восстановлено из бандла 1.7.5):

```js
unload(){
  if(!this._loaded) return;          // guard — здесь отсутствует
  this._loaded = false;
  while(children.length) children.pop().unload();   // LIFO
  while(events.length)  events.pop()();             // LIFO
  this.onunload();                                  // последним
}
```

Три расхождения:

1. **`onunload()` вызывается первым**, а не последним. Плагин, который в `onunload` рассчитывает
   на уже снятые обработчики, получает обратное.
2. **FIFO вместо LIFO.** Отмена должна идти в обратном порядке регистрации — как вложенные
   `finally`. При FIFO `registerDomEvent` на родителе снимается раньше, чем на ребёнке, и
   возможен повторный вход.
3. **Нет guard `_loaded`.** Повторный `unload()` вызывает `onunload()` второй раз. Плагины
   пишут `onunload` без идемпотентности — это штатно для Obsidian.

Дополнительно: `register(cb)` кладёт `{unload: cb}` **в тот же массив `_children`**, что и
настоящих детей. Значит `removeChild` может «выгрузить» disposer, а `this._children.length`
(плагины так проверяют «есть ли у меня дети») считает и обработчики.

**Исправление:** два раздельных массива — `_children` и `_disposers`, оба `pop()`,
`onunload()` в конце, guard в начале.

### 4.3 `View` не грузится и не выгружается

`runtime.rs:2526` (`setViewState`) и `2575` (`detach`)

```js
const view = creator(this);
this.view = view;
if (typeof view.onOpen === "function") await view.onOpen();   // ← нет view.load()
```

```js
detach() {
    globalThis.__workspace_leaves.delete(this.id);
    if (this.view && typeof this.view.onClose === "function") {
        this.view.onClose();                                   // ← нет view.unload()
    }
    ...
}
```

Obsidian: `open()` → `containerEl.appendChild` → **`this.load()`** → `onOpen()`;
`close()` → `detach` → **`this.unload()`** → `onClose()`.

Последствия:

- `_loaded` у вью всегда `false` → `this.addChild(child)` внутри `onOpen` **не грузит ребёнка**
  (проверка `if (this._loaded)` в `addChild`). А `addChild` в `onOpen` — обычная практика.
- `detach()` не вызывает `unload()` → **все disposer'ы вью не выполняются**: подписки на
  события, интервалы, DOM-слушатели остаются жить после закрытия панели.

### 4.4 `addChild` в `onload` не загружает ребёнка

`runtime.rs:567` — `__bootstrap_plugin_instance` создаёт инстанс и вызывает `onload()`
напрямую, **не выставляя `_loaded = true`**:

```js
const inst = new exported(globalThis.app, globalThis.__plugin_manifest);
globalThis.__plugin_instance = inst;
if (typeof inst.onload === "function") await inst.onload();   // _loaded остался false
```

А `addChild` (`2169`) проверяет именно `_loaded`:

```js
addChild(child) {
    this._children.push(child);
    if (this._loaded && typeof child.load === "function") child.load();   // ← никогда
    return child;
}
```

Значит `this.addChild(new MyEditorSuggest(...))` в `onload` создаёт объект, у которого
**никогда не вызывается `onload()`**. А `EditorSuggest`, `MarkdownRenderChild` и половина
компонентов живут именно через `addChild`.

**Исправление:** одна строка — `inst._loaded = true;` перед `await inst.onload()`.

---

## 5. P1 — неверная семантика API

API есть, но ведёт себя не так, как ожидает плагин. Это опаснее отсутствия: плагин не падает,
а делает не то.

| # | Место | Сейчас | Должно быть |
|---|---|---|---|
| 5.1 | `getAbstractFileByPath` `2856` | всегда `new TFile(p)`, даже для папки и несуществующего пути | `TFolder` / `TFile` / **`null`** |
| 5.2 | `getFileByPath` `2852` | всегда `TFile` | `TFile \| null` |
| 5.3 | `metadataCache.getFileCache` `2987` | `{tags:[],links:[],headings:[],frontmatter:{}}` | `null` если не посчитан |
| 5.4 | `getFirstLinkpathDest` `2993` | возвращает **строку** | `TFile \| null` |
| 5.5 | `executeCommandById` `3003` | `Promise` | `boolean` (синхронно) |
| 5.6 | `TFile.stat` `2595` | `{mtime: Date.now(), size: 0}` | реальный `stat` из снимка |
| 5.7 | `onLayoutReady` `2936` | `setTimeout(cb, 10)` | очередь + срабатывание при готовности |
| 5.8 | `debounce` `3243` | `(fn, _delay) => fn` | настоящий debounce |
| 5.9 | `addCommand` `2720` | `cmd.id` без префикса | `manifest.id + ":" + cmd.id` |
| 5.10 | `addCommand` `2720` | `callback` проверяется раньше `checkCallback` | `checkCallback` первым, `checking=true` для палитры |
| 5.11 | `addSettingTab` `2762` | `tab.display()` вызывается в `onload`; `view_id: 0` | `display()` при открытии вкладки, свой `view_id` |
| 5.12 | `addRibbonIcon` `2747` | `id = "ribbon-" + Math.random()` | `manifest.id + ":" + title` (стабильный) |

Разберу три, которые бьют сильнее всего.

**5.3 — пустой кэш хуже `null`.** Плагин делает:

```js
const cache = app.metadataCache.getFileCache(file);
if (!cache) return;                       // ← никогда не сработает
for (const h of cache.headings) ...       // ← всегда пусто
```

Плагин молча считает, что в заметке нет ни тегов, ни ссылок, ни заголовков. Любой индексатор
(Dataview-подобный, панель тегов, backlinks) выдаст **правдоподобно неверный** результат.
При этом `compat.rs` помечает `metadataCache.getFileCache` как **Supported** — то есть бейдж
обещает поддержку.

**5.5 — `Promise` вместо `boolean`.** `if (!app.commands.executeCommandById(id)) return;` —
Promise всегда truthy, проверка всегда проходит, даже если команды нет.

**5.11 — `display()` в `onload`.** Obsidian вызывает `display()` при открытии вкладки настроек.
Здесь — сразу при загрузке, до того как `onload` завершился. Комментарий в коде это признаёт
(«Some plugins rely on asynchronous data loaded after onload»). Результат: пользователь видит
настройки по умолчанию, а не сохранённые. Плюс жёсткий `view_id: 0` — коллизия с любой другой
вью с тем же id.

---

## 6. P1 — обещано, но не реализовано

`Plugin` в Obsidian имеет около двадцати методов. Здесь функциональны семь.

| Метод | Состояние |
|---|---|
| `addCommand` | есть, с дефектами 5.9–5.10 |
| `addRibbonIcon` | есть |
| `addSettingTab` | есть, с дефектом 5.11 |
| `loadData` / `saveData` | есть |
| `registerMarkdownCodeBlockProcessor` | есть, самая зрелая часть (покрыта тестом) |
| `registerView` | **только в JS-мапу**, хосту не сообщается (`2799`) |
| `registerEvent` | **no-op** (4.1) |
| `registerExtensions` | **пусто** (`2802`) |
| `registerHoverLinkSource` | **пусто** (`2803`) |
| `registerEditorExtension` | **пусто** (`2804`) |
| `removeCommand` | отсутствует |
| `addStatusBarItem` | отсутствует |
| `registerMarkdownPostProcessor` | отсутствует |
| `registerEditorSuggest` | отсутствует |
| `registerObsidianProtocolHandler` | отсутствует |
| `loadCSS` | отсутствует |

`registerView` без уведомления хоста — отдельная проблема: `setViewState` работает только потому,
что плагин сам вызывает `getLeaf().setViewState()`, и `__registered_views` лежит в том же JS.
Но хост не знает о типе вью, пока плагин не создаст лист. Всё, что хост мог бы показать заранее
(иконка, «открыть как», восстановление layout при старте), невозможно.

`registerEditorExtension` — пустой. Плагины на CM6 (а это весь цвет экосистемы: Live Preview
расширения, подсветка, автодополнение) **тихо ничего не делают**. При этом в `compat.rs` они не
помечены как неподдерживаемые.

---

## 7. P2 — производительность

**7.1 Polling-цикл на каждый плагин.** `runtime.rs:196-215`

```js
match host_events_rx.try_recv() {
    ...
    Err(TryRecvError::Empty) => {
        let sleep_dur = if timer_wait_ms > 0 { ... } else { Duration::from_millis(5) };
        thread::sleep(sleep_dur);
    }
```

30 плагинов × 200 пробуждений в секунду = **6 000 пробуждений/с в простое**. Каждое — ещё и
`ctx.with(...)` с вызовом JS-функции `__check_timers`. Это заметный расход батареи на ноутбуке.

**Исправление:** блокирующий `recv_timeout` (crossbeam-channel вместо futures-mpsc) — поток спит,
пока не пришло событие или не истёк ближайший таймер.

**7.2 Watchdog не работает в главном цикле.** `deadline.store(0)` на строках `101` и `128` —
после первичного `eval` дедлайн снят и **больше не ставится**. `set_interrupt_handler` проверяет
`if d > 0`, то есть в цикле сообщений прерывание невозможно. Плагин с бесконечным циклом внутри
обработчика команды или таймера **вешает свой поток навсегда** (память ограничена, время — нет).

**Исправление:** ставить дедлайн перед каждым `dispatch_*` и `__check_timers`.

**7.3 Полная пересборка UI-дерева на каждое изменение.** `ItemView` вешает `_onModified` на
`contentEl` и `containerEl`, и любая мутация вызывает `__velement_to_uinode` по всему поддереву
+ IPC-сообщение. Для панели с сотней узлов и частым обновлением это O(n) на каждую правку.

---

## 8. `compat.rs` — анализатор, который врёт

Это отдельный разговор, потому что он формирует пользовательское ожидание.

### 8.1 Плагин без совпадений получает «100% Совместим»

`compat.rs:143-158`

```rust
} else if missing.is_empty() {
    CompatibilityReport {
        status: CompatibilityStatus::Full,
        score: 1.0,
```

Если ни один паттерн не найден, `supported` и `missing` оба пусты → `missing.is_empty()` → **`Full`**.
Проверил, изолировав эту ветку и запустив:

```
плагин вообще без совпадений (только registerEvent/registerView) -> Full  score=1
1 supported                                                     -> Full  score=1
1 supported + 1 missing                                         -> Partial(0.5)
1 supported + incompatible                                      -> Incompatible  score=0.25
```

Плагин, который делает только `this.registerEvent(...)` и `this.registerView(...)` — а это
типичный плагин с панелью — получает бейдж «100% Совместим» и падает на первом же клике.
Потому что ни `registerEvent`, ни `registerView` в списке паттернов нет.

### 8.2 Подстрочный матчинг на минифицированном бандле

```rust
if script_source.contains(pat.pattern) {
```

`main.js` плагина — это минифицированный бандл. `contains("Notice")` сработает на
`NoticeManager`, на строке в тексте, на `Notification`. `contains("Menu")` — на `MenuBar`,
`MenuItem`, `contextMenu`. Ложные срабатывания неизбежны.

Обратное тоже верно: минификатор может переименовать локальную переменную, но **свойства
объектов не переименовывает** — `app.vault.read` в бандле сохраняется. Поэтому `vault.read`
ловится надёжно, а `new Notice` — нет.

### 8.3 Счёт по числу *уникальных паттернов*, не вызовов

`supported.len() / (supported.len() + missing.len())` — плагин, вызвавший `vault.read` один раз
и `requestSaveHistory` пятьдесят раз, получает тот же балл, что и наоборот. Приоритет вызовов
не учитывается вообще.

### 8.4 `document.createElement` помечен как «несовместимо»

Это отсекает огромный класс живых UI-плагинов. При этом `VElement` **умеет** работать с
созданными элементами — проблема не в `document`, а в том, что вы не эмулируете `document.createElement`.
Кстати, `__doc_body`/`__doc_head` в бутстрапе есть, то есть `document` частично эмулируется —
значит пометка «несовместимо» просто неверна.

### 8.5 Два источника правды

Список паттернов в `compat.rs` поддерживается вручную и не связан с тем, что реально
реализовано в `runtime.rs`. Уже сейчас они расходятся: `metadataCache.getFileCache` помечен
Supported, а возвращает пустой фейк (§5.3).

### 8.6 Что делать вместо этого

Инфраструктура для честного ответа **уже есть**, но не используется:
`makeAuditedProxy` (`2685`) назван «audited», однако ничего не аудитирует — просто возвращает
`undefined` для неизвестных свойств.

```js
function makeAuditedProxy(target, targetName) {
    return new Proxy(target, {
        get(obj, prop) {
            ...
            return undefined;      // ← и всё
        }
    });
}
```

**Предложение:** заменить статический анализ на **рантайм-аудит**:

1. В `makeAuditedProxy` и `obsidianProxy` при промахе писать в `__audit_log`:
   `{ path: "app.vault.copy", kind: "undefined" }`.
2. В конце `onload` (+ через 2 с) отправлять лог хосту одним сообщением `AuditReport`.
3. Хост строит отчёт: «плагин вызвал 14 API, 11 работают, 2 заглушки, 1 отсутствует».
4. Статический анализ оставить как **предварительную** оценку до первого запуска, но честно
   помечать «оценка по строкам, не проверено».

Это даёт ответ на настоящий вопрос пользователя — «заведётся ли *этот* плагин» — вместо
«сколько ключевых слов совпало».

---

## 9. Что реально заведётся

По коду, а не по бейджу.

| Класс плагина | Состояние | Причина |
|---|---|---|
| Команда + `Notice` + `vault.read/modify` | **работает** | базовый путь цел |
| Code block processor | **работает** | самая зрелая часть, есть интеграционный тест |
| Модалки (`Modal`, `SuggestModal`, `FuzzySuggestModal`) | **работает** | реализованы полноценно |
| Настройки (`Setting` + `PluginSettingTab`) | **частично** | `display()` в `onload`, `view_id: 0` (§5.11) |
| Плагин с панелью (`ItemView`) | **не работает** | нет `load()`/`unload()` (§4.3), `registerView` не сообщает хосту |
| Editor-команды | **падает** | `MarkdownView` не определён (§3.3) |
| CM6-расширения | **тихо не работает** | стаб (§6) |
| Markdown post-processors | **нет** | не реализовано |
| Индексаторы (`metadataCache`) | **ложно работает** | пустой кэш (§5.3) |
| Плагины с `registerEvent` | **течёт** | no-op (§4.1) |
| Electron / Node / `child_process` | **честный отказ** | правильно |

Грубая оценка: из плагинов, не требующих CM6 и Electron, **сейчас заведётся примерно треть**.
После исправления P0 + §5.1–5.5 — **две трети**. Оставшаяся треть — это CM6, post-processors
и `MarkdownRenderer` (последнего нет вообще: `grep MarkdownRenderer` даёт 0).

---

## 10. План: 12 правок по приоритету

Порядок — по «предотвращённый ущерб / строки кода».

| # | Правка | Файл:строка | Объём |
|---|---|---|---|
| 1 | `vault.read` → `throw` вместо строки-заглушки | `runtime.rs:2816` | 1 строка |
| 2 | `trash` → отдельный `VaultTrash`, не `delete` | `runtime.rs:2826` + `api.rs` | ~20 |
| 3 | `registerEvent` → `this.register(() => ref.e.offref(ref))`; `EventRef.e` | `runtime.rs:2191`, `2786` | ~10 |
| 4 | `Component.unload` — guard, LIFO, `onunload` последним, раздельные массивы | `runtime.rs:2159` | ~25 |
| 5 | `View`: `load()` перед `onOpen()`, `unload()` в `detach()` | `runtime.rs:2526`, `2575` | ~6 |
| 6 | `inst._loaded = true` перед `await onload()` | `runtime.rs:567` | 1 строка |
| 7 | `MarkdownView` — определить либо честно отказать в регистрации | `runtime.rs:2730` | ~30 или 3 |
| 8 | `addCommand` — неймспейс `pluginId:`, `checkCallback(checking)` | `runtime.rs:2720` | ~25 |
| 9 | `metadataCache.getFileCache` → `null`; `getAbstractFileByPath` → `null` | `runtime.rs:2987`, `2856` | ~15 |
| 10 | `executeCommandById` → `boolean`; `getFirstLinkpathDest` → `TFile\|null` | `runtime.rs:3003`, `2993` | ~20 |
| 11 | `debounce` — настоящий debounce | `runtime.rs:3243` | ~10 |
| 12 | `compat.rs` — пустой случай → `Unknown`, не `Full` | `compat.rs:143` | ~15 |

Отдельно, не срочно, но важно:

- `VaultAccess::default()` → `None` (сейчас **`Write`**: плагин без объявленных прав получает
  запись — `manifest.rs:23`).
- `is_compatible_with_host` (`manifest.rs:112`) написан, но **нигде не вызывается** —
  `load_installed_plugins` делает только `validate()`. Гейт `minAppVersion` мёртв.
- `isDesktopOnly` в манифесте отсутствует как поле — плагины с этим флагом грузятся молча.
- `makeAuditedProxy` — реализовать аудит (§8.6).
- Polling → `recv_timeout` (§7.1), watchdog в цикле (§7.2).

---

## 11. Тесты, которых не хватает

У вас есть хорошие интеграционные тесты (`load_and_run_installed_plugin_full_lifecycle`,
тест `setViewState` → `SetLeafView`). Но нет ни одного теста **на инвариант выгрузки**.
Именно поэтому §4 дожил до этого состояния.

Минимальный набор, который стоит добавить:

**Тест 1 — утечка подписок.**
```js
// плагин
onload() { this.registerEvent(this.app.vault.on("modify", () => this.count++)); }
```
Загрузить, выгрузить, отправить `VaultModify`. Ожидание: счётчик не изменился.
Сейчас: изменится.

**Тест 2 — порядок выгрузки.**
```js
onload() { this.order = []; this.register(() => this.order.push("a"));
           this.register(() => this.order.push("b")); }
onunload() { this.order.push("onunload"); }
```
Ожидание: `["b", "a", "onunload"]`. Сейчас: `["onunload", "a", "b"]`.

**Тест 3 — двойной `unload`.** Вызвать `unload()` дважды, ожидание: `onunload` вызван один раз.

**Тест 4 — `addChild` в `onload`.**
```js
onload() { this.addChild({ onload() { this.loaded = true; }, unload() {} }); }
```
Ожидание: `child.loaded === true`. Сейчас: `undefined`.

**Тест 5 — закрытие панели.** `setViewState` → `leaf.detach()` → проверить, что disposer'ы вью
выполнены. Сейчас: не выполнены.

**Тест 6 — `vault.read` при ошибке.** Хост отвечает `ok: false`. Ожидание: промис отклонён.
Сейчас: резолвится строкой `"non-empty-fallback-on-error"`.

Шесть тестов, ~150 строк, и §3.1, §4.1–4.4 больше не смогут вернуться незамеченными.

---

## 12. Итог

Задача, которую вы взяли, объективно тяжёлая: QuickJS без DOM, асинхронный мост при синхронном
API, декларативный UI вместо императивного. То, что уже написано — `VElement`, снимок состояния,
права с пиннингом бандла, декларативные `UiNode` — это не «набросок», а рабочая основа, и по
изоляции она **обгоняет оригинал**.

Проблема в одном: **жизненный цикл не держит инвариант**. `registerEvent` — no-op, `View` не
вызывает `load`/`unload`, `Component.unload` перевёрнут, `addChild` не грузит детей. Четыре
дефекта в одном месте, и все четыре бьют по одному и тому же — по обещанию «что зарегистрировал,
то и сниму».

Это чинится за один вечер, ~100 строк. И три строки из этого списка (`vault.read`, `trash`,
`Editor("")`) сейчас могут испортить пользовательские заметки — их стоит поправить первыми,
до любого другого улучшения.

Покажите `ui.rs` и `bridge.rs` целиком, если хотите разбор хостовой стороны — там я пока смотрел
только маршрутизацию и проверку прав.
