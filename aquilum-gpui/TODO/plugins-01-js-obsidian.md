# Система плагинов. Часть 1: JS/TS-плагины с API в стиле Obsidian

Основной рантайм плагинов: **JavaScript и TypeScript на QuickJS** (`rquickjs`), один рантайм и один поток на плагин. Общая часть (архитектура, манифест, права, безопасность, проверка плагинов, раскладка, формат дерева UI, этапы, каталог) описана в `plugins-00-core.md` («core»). Здесь только то, что относится к JS и к совместимости по именам с Obsidian.

Ссылки вида «core §12.2» ведут в общий документ, «WASM §3» в документ по WASM-плагинам, «Zed §3» в документ по Zed.

> Код это эскизы для проектирования, они не проверялись компиляцией. Версии `rquickjs`, парсера JS (`oxc` или `swc`) и API GPUI закрепляйте и сверяйте с документацией.

---

## 1. Решения

### 1.1 Почему JS/TS

- На нём пишут плагины Obsidian, поэтому логику легко портировать, а аудитория уже знает `onload`, `addCommand`, `vault`, `Notice`.
- `async/await` и Promise ложатся на асинхронный мост хоста (core §12.2): плагин не блокирует UI-поток.
- TypeScript даёт типы API (`.d.ts` генерируются из Rust-типов) и автодополнение у автора.
- QuickJS маленький, быстро стартует, поддерживает лимит памяти и прерывание выполнения по таймауту.

### 1.2 Почему не Lua как основной язык

Lua подходит для встраивания (Neovim, WezTerm, игровые аддоны), а Luau через `mlua` даёт встроенную песочницу и типы. Но у него меньше аудитория, нет привычного `async/await` и готовой экосистемы сборки как у TypeScript, а главное он не даёт преимущества «автору легко портировать плагин Obsidian». Если понадобится, Lua можно добавить как ещё один рантайм через `RuntimeFactory` (core §12.1) без изменения API хоста.

### 1.3 Когда лучше WASM

Для тяжёлых вычислений (разбор больших файлов, граф, индексация) и для авторов на Rust или других языках см. `plugins-02-wasm.md`. JS остаётся основным путём для обычных плагинов.

---

## 2. Публичный API v1 (в стиле Obsidian)

### 2.1 Жизненный цикл плагина

```ts
export default class MyPlugin extends Plugin {
  async onload() { /* регистрация команд, событий, UI */ }
  async onunload() { /* очистка; хост также сам снимает все регистрации */ }
}
```

### 2.2 Точки расширения

| Область | Методы (v1) |
|---|---|
| **Команды** | `addCommand({id, name, hotkeys?, callback, editorCallback?})` |
| **Vault** | `app.vault.read/modify/create/delete/rename`, `getMarkdownFiles()`, `getAbstractFileByPath(path)` |
| **Метаданные** | `app.metadataCache.getFileCache(file)` (ссылки, теги, frontmatter, заголовки) |
| **Workspace** | `app.workspace.getActiveFile()`, `app.workspace.getLeaf().openFile(file)` |
| **События** | `registerEvent(app.vault.on("modify" \| "create" \| "delete" \| "rename", cb))`, `registerEvent(app.workspace.on("file-open", cb))` |
| **Данные плагина** | `loadData()`, `saveData(obj)` |
| **Настройки** | `addSettingTab(new MySettingTab(app, this))` + `Setting` (декларативная форма, без DOM) |
| **Статус-бар** | `addStatusBarItem()` → объект с `setText()`, `onClick()` |
| **Лента (ribbon)** | `addRibbonIcon(icon, title, callback)` |
| **Панели** | `registerView(type, creator)` + `ItemView` (содержимое задаётся деревом компонентов) |
| **Markdown-рендер** | `registerMarkdownCodeBlockProcessor(lang, handler)`, `registerMarkdownPostProcessor(handler)` |
| **Редактор** | `Editor`: `getSelection/replaceSelection/getValue/setValue/getCursor/setCursor`; декорации через `registerEditorDecorations` (core, этап 6; JS §5) |
| **Уведомления** | `new Notice(text, timeout)` |
| **Сеть** | `requestUrl({url, method, headers, body})` (только разрешённые домены) |

Имена и сигнатуры повторяют Obsidian там, где поведение совпадает. Подробные правила в разделе 2.3.

Все регистрации автоматически снимаются хостом при выгрузке плагина.

### 2.3 Принцип именования: API в стиле Obsidian (без DOM)

**Решение:** публичный API повторяет понятия и названия Obsidian (`Plugin`, `onload`, `addCommand`, `app.vault`, `app.workspace`, `Notice`, `Setting` и т.д.), но не содержит DOM и CodeMirror. Автору плагина легко портировать логику, а приложение не тащит чужую архитектуру.

**Что совпадает и что отличается:**

| Obsidian | У нас | Отличие |
|---|---|---|
| `Plugin`, `onload` / `onunload` | то же | нет |
| `this.app` (`vault`, `workspace`, `metadataCache`) | то же | нет |
| `addCommand`, `addRibbonIcon`, `addStatusBarItem`, `registerEvent`, `loadData` / `saveData` | то же | регистрации снимаются хостом автоматически |
| `Notice` | то же | нет |
| `TFile`, `TFolder`, `TAbstractFile` | то же поля: `path`, `name`, `basename`, `extension` | нет |
| `normalizePath`, `requestUrl` | то же | `requestUrl` проверяется по правам и allowlist |
| `PluginSettingTab`, `Setting` | те же имена | `display()` строит форму через `Setting`, без `containerEl` и `createEl` |
| `Modal` | то же имя | `onOpen()` возвращает дерево компонентов вместо работы с `contentEl` |
| `ItemView`, `registerView` | те же имена | вид возвращает дерево компонентов (core §5) |
| `registerMarkdownPostProcessor`, `registerMarkdownCodeBlockProcessor` | те же имена | обработчик получает объект с данными блока и возвращает дерево компонентов, а не `HTMLElement` |
| `Editor` (`getSelection`, `replaceSelection`, ...) | то же | реализован на редакторе GPUI, без CodeMirror |
| `registerEditorExtension`, `EditorView` (CM6) | **нет** | заменено на `registerEditorDecorations` (core, этап 6; JS §5) |
| `createEl`, `containerEl`, `HTMLElement`, `document` | **нет** | вызов бросает понятную ошибку |

**Правила:**

1. Поведение совпадает: то же имя и та же сигнатура. Синонимы не придумываем (`vault.modify`, а не `vault.write`).
2. Поведение отличается: другое имя, чтобы не вводить в заблуждение (`registerEditorDecorations`).
3. Не поддержано: явная ошибка вида `Not supported: DOM (createEl). See the porting guide`, а не `undefined`.
4. Синхронные геттеры (`getActiveFile`, `getMarkdownFiles`, `getAbstractFileByPath`, `metadataCache`) остаются синхронными, как в Obsidian. Рантайм плагина живёт в отдельном потоке, поэтому хост держит в нём **снимок состояния только для чтения**, который обновляется событиями. Все операции записи асинхронные.
5. Собственные `.d.ts` и собственный код. Не копировать код и типы Obsidian, лицензии проверить отдельно.
6. Общий API для плагинов Rust/WASM (см. `plugins-02-wasm.md`) строится от тех же понятий.

**Пример портирования (логика остаётся, UI заменяется):**

```ts
export default class WordCountPlugin extends Plugin {
  async onload() {
    const status = this.addStatusBarItem();
    this.registerEvent(
      this.app.workspace.on("file-open", async (file) => {
        if (!file) return;
        const text = await this.app.vault.read(file);
        status.setText(`${text.split(/\s+/).filter(Boolean).length} words`);
      })
    );
    this.addCommand({
      id: "show-count",
      name: "Show word count",
      callback: () => new Notice("Counting..."),
    });
  }
}
```

---

## 3. UI из плагина: примеры на JS

Плагин возвращает дерево компонентов, а хост рисует его и присылает события обратно (формат и правила хоста: core §5, §5.1, §5.2, §12.7). Ниже примеры в стиле Obsidian.

### 3.1 Вид с деревом компонентов

```ts
class TodoView extends ItemView {
  getViewType() { return "todo-panel"; }
  getDisplayText() { return "Tasks"; }

  render(state) {
    return {
      type: "column",
      children: [
        { type: "text", value: "Задачи", style: "heading" },
        { type: "list", items: state.tasks.map(t => ({ id: t.id, label: t.title })) },
        { type: "input", id: "new", placeholder: "Новая задача" },
        { type: "button", label: "Добавить", onClick: "add" },
      ],
    };
  }

  onEvent(event, state) { /* event.id, event.value */ }
}

// в onload():
this.registerView("todo-panel", (leaf) => new TodoView(leaf));
```

### 3.2 Модальное окно

Названия классов совпадают с Obsidian (`Modal`, `onOpen`, `open`, `close`, `setTitle`), но `onOpen` возвращает дерево, а не работает с `contentEl`.

```ts
class RenameModal extends Modal {
  value = "";

  onOpen() {
    this.setTitle("Переименовать");
    return {
      type: "column",
      children: [
        { type: "input", id: "name", placeholder: "Новое имя" },
        { type: "row", children: [
          { type: "button", id: "cancel", label: "Отмена" },
          { type: "button", id: "ok", label: "Готово", variant: "primary" },
        ]},
      ],
    };
  }

  onEvent(e) {
    if (e.id === "name") this.value = e.value;
    if (e.id === "ok") { this.onSubmit(this.value); this.close(); }
    if (e.id === "cancel") this.close();
  }
}

new RenameModal(this.app).open();
```

### 3.3 Панель в боковой области

```ts
class TodoView extends ItemView {
  getViewType() { return "todo-panel"; }
  getDisplayText() { return "Tasks"; }
  getIcon() { return "check-square"; }

  onOpen() {
    this.addAction("refresh-cw", "Обновить", () => this.reload());
    return this.render();  // дерево компонентов, включая кнопки
  }
  onClose() { /* очистка */ }
  getState() { return { filter: this.filter }; }
  setState(s) { this.filter = s.filter; }
}

async onload() {
  this.registerView("todo-panel", (leaf) => new TodoView(leaf));
  this.addRibbonIcon("check-square", "Открыть задачи", () => this.activateView());
}

async activateView() {
  const leaf = this.app.workspace.getRightLeaf();
  await leaf.setViewState({ type: "todo-panel", active: true });
  this.app.workspace.revealLeaf(leaf);
}
```

---

## 4. Рантайм QuickJS: поток плагина и лимиты

Типы запросов (`ApiRequest`, `ApiResponse`, `HostEvent`, `ApiCall`) и мост на стороне GPUI общие для всех рантаймов, они в core §12.2. Здесь реализация `PluginRuntime` (core §12.1) для JS.

Поток плагина:

```rust
// plugin-runtime-js (ApiCall определён в core §12.2)
pub struct PluginHandle {
    pub id: PluginId,
    pub events_tx: std::sync::mpsc::Sender<HostEvent>,
    deadline_ms: Arc<AtomicU64>,                 // для watchdog
    join: Option<std::thread::JoinHandle<()>>,
}

pub fn spawn_plugin(
    id: PluginId,
    source: String,
    api_tx: futures::channel::mpsc::UnboundedSender<ApiCall>,
    snapshot: Arc<arc_swap::ArcSwap<Snapshot>>,
) -> PluginHandle {
    let (events_tx, events_rx) = std::sync::mpsc::channel::<HostEvent>();
    let deadline_ms = Arc::new(AtomicU64::new(u64::MAX));
    let d = deadline_ms.clone();

    let join = std::thread::Builder::new()
        .name(format!("plugin-{id}"))
        .spawn(move || {
            let rt = rquickjs::Runtime::new().unwrap();
            rt.set_memory_limit(64 * 1024 * 1024);
            rt.set_max_stack_size(512 * 1024);
            // true = прервать выполнение JS (бросается неперехватываемая ошибка)
            rt.set_interrupt_handler(Some(Box::new(move || now_ms() > d.load(Ordering::Relaxed))));
            let ctx = rquickjs::Context::full(&rt).unwrap();

            ctx.with(|ctx| {
                install_globals(&ctx, &api_tx, &snapshot);   // Plugin, Notice, app.*, console
                // main.js уже собран в один бандл; доступа к fs, net, process, require нет
                let _ = ctx.eval::<(), _>(source);
            });

            loop {
                match events_rx.recv_timeout(Duration::from_millis(20)) {
                    Ok(HostEvent::Unload) | Err(RecvTimeoutError::Disconnected) => break,
                    Ok(ev) => {
                        // бюджет на один вызов JS: событие 100 мс, команда до 1 с
                        d.store(now_ms() + budget_for(&ev), Ordering::Relaxed);
                        ctx.with(|ctx| dispatch_event(&ctx, ev));
                        // Promise-задачи тоже под тем же дедлайном
                        while rt.is_job_pending() {
                            if rt.execute_pending_job().is_err() { break; }
                        }
                        d.store(u64::MAX, Ordering::Relaxed);
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                }
            }
        })
        .unwrap();

    PluginHandle { id, events_tx, deadline_ms, join: Some(join) }
}
```

Асинхронный вызов API из JS: функция `__host_call(kind, json)` создаёт `Promise`, сохраняет `resolve` и `reject` в таблице `HashMap<RequestId, (Function, Function)>` внутри потока плагина, отправляет `ApiCall` в `api_tx`. Когда в `events_rx` приходит `HostEvent::Response { id, .. }`, поток достаёт пару из таблицы и вызывает `resolve` или `reject`. Таблица очищается по таймауту запроса (например, 30 с).

**Привязки и общий слой `HostApi`.** В коде выше `install_globals` для краткости принимает канал. В реальном коде он принимает `Arc<dyn HostApi>` (core §12.14), чтобы привязки были общими для боевого и пробного запуска: в бою `BridgeHost` отправляет `ApiCall` в мост, в проверке `MockHost` записывает вызовы и отвечает сразу. Ответ в обоих случаях приходит событием `HostEvent::Response`.

**Какие возможности движка выдаются.** Контекст собирается из явного набора встроенных объектов (в `rquickjs` для этого есть `Context::custom` с выбранными интринсиками, точный API уточните по документации вашей версии), а не `Context::full`. В набор не включают `Eval`. Тестом проверьте, что `eval(...)` и `Function("...")` недоступны, а `fetch`, `XMLHttpRequest`, `WebSocket`, `require`, `process`, `document` не определены. `JSON`, `Promise`, `Date`, `RegExp`, `Map`/`Set` и типизированные массивы подключают отдельно.

**Лимиты.** `set_memory_limit` (в бою 64 МБ, в пробном запуске 32 МБ), `set_max_stack_size`, interrupt handler с дедлайном на каждый вызов (событие 100 мс, команда до 1 с, `onload` 500 мс в пробном запуске и до 2 с в бою). Время ожидания ответа хоста на `await` не считается: пока Promise ждёт, поток простаивает в цикле событий, а дедлайн ставится только на время выполнения JS.

**Страйки.** Сработавший interrupt handler, исключение в `onload` и превышение памяти дают плагину «страйк» (логика в core §12.2). Три страйка за минуту отключают плагин.

**Выгрузка.** `shutdown(grace)` отправляет `HostEvent::Unload`, ставит дедлайн на `onunload` (500 мс), затем закрывает канал и дожидается завершения потока. Регистрации снимает хост (core §12.10), а не плагин.

---

## 5. Хуки редактора: что в первом релизе (предложение, нужно ваше подтверждение)

Хуки редактора это единственная часть API, которая лезет во внутренности вашего редактора: буфер текста, раскладку, курсор, отрисовку. Остальной API (команды, vault, события) просто вызывает ядро. Если выставить внутренности редактора наружу до того, как он стабилизировался, вы закрепите их навсегда: любая переделка редактора сломает плагины. Поэтому для первого релиза нужен минимум, который безопасен и закрывает большинство плагинов.

### 5.1 Обязательный минимум (релиз 1)

| № | Хук | Зачем |
|---|---|---|
| 1 | `Editor`: `getValue`, `setValue`, `getSelection`, `replaceSelection`, `replaceRange(text, from, to)`, `getLine`, `lineCount`, `getCursor`, `setCursor`, `setSelection` | Вставка даты и шаблонов, форматирование выделения, нумерация, счётчик слов, оглавление |
| 2 | `addCommand({ editorCallback: (editor, view) => ... })` | Команда выполняется, только когда открыта заметка в редакторе |
| 3 | Событие `editor-change` с номером версии документа | Счётчики, синхронизация, внешние интеграции |
| 4 | Группировка правок в одну операцию отмены: `editor.transact(fn)` | Несколько `replaceRange` одного вызова отменяются одним Ctrl+Z |
| 5 | `registerMarkdownCodeBlockProcessor`, `registerMarkdownPostProcessor` для Reading View и Live Preview | Самая популярная категория плагинов (диаграммы, графики, таблицы). Плагин возвращает дерево компонентов, хост показывает его в Reading View и в Live Preview, когда курсор вне блока |

### 5.2 После релиза 1

| Хук | Зачем | Почему не сейчас |
|---|---|---|
| `registerEditorDecorations` (подсветка диапазонов, стили, виджеты в строке) | Подсветка тегов, цвета, маркеры | Нужна стабильная модель позиций и раскладки |
| Подсказки при вводе (`[[`, `/`, `@`) | Автодополнение | Нужен popup и работа с курсором |
| Пункты контекстного меню редактора | Действия над выделением | Нужна стабильная модель меню |
| Свои горячие клавиши внутри редактора | Редакторные сочетания | Конфликты с хоткеями приложения |

### 5.3 Не отдавать плагинам (или очень поздно)

Скрытие и подмена разметки (то, что делает сам Live Preview), блочные виджеты на месте текста, собственные расширения со своим состоянием, новые языки. Это ядро поведения редактора, его лучше держать внутренним.

### 5.4 Что заложить в ядро уже сейчас

Даже если наружу выставлен только минимум, редактор должен иметь:
- модель позиций (смещения или строка и колонка), одинаково понятную хосту и плагину;
- **номер версии документа** и транзакции (изменение это набор операций с одной записью в истории);
- возможность пересчитать позиции через правки, случившиеся с версии N. Плагин работает в отдельном потоке, поэтому его ответ (например, декорации) придёт с задержкой, и хосту нужно «сдвинуть» диапазоны на изменения за это время (алгоритм `map_offset` в core §12.9).

### 5.5 Транзакции и асинхронность

`editor.transact(fn)` объединяет только синхронную часть вызова. Асинхронные продолжения (после `await`) в транзакцию не входят. Это нужно явно написать в документации для авторов.

---

## 6. Проверка JS-плагина перед запуском

Общая схема, политика, кэш и boot guard описаны в core §6.1 и core §12.14. Здесь части, специфичные для JS: `JsVerifier` реализует `RuntimeVerifier`.

### 6.1 Статическая проверка бандла

| Проверка | Как | Результат |
|---|---|---|
| Размер и кодировка | `main.js` не больше 5 МБ, UTF-8, один файл (бандл собран целиком) | ошибка |
| Внешние зависимости | нет `import`/`require` внешних модулей, нет динамического `import()` | ошибка |
| Синтаксис | компиляция QuickJS без выполнения (compile-only) | ошибка |
| Запрещённые идентификаторы | разбор AST (например, `oxc`): `eval`, `new Function`, `process`, `fetch`, `XMLHttpRequest`, `WebSocket`, `document`, `window`, `createEl`, запись в `globalThis` и в прототипы встроенных объектов | ошибка для явных обращений, предупреждение для неоднозначных |
| Соответствие прав и кода | код вызывает `requestUrl` или `vault.modify`, а в манифесте нет таких прав | предупреждение (реальный запрет всё равно делает хост) |
| Признаки обфускации | идентификаторы вида `_0x1a2b`, большие массивы строк с индексным доступом, `eval` или `Function` рядом с base64. Обычная минификация допустима | предупреждение, для каталога ошибка |

**Это линтер, а не защита.** `globalThis["ev"+"al"]` он не поймает. Поэтому опасные возможности в песочнице просто не выдаются (раздел 4): нет `fs`, `net`, `process`, `Eval`. Статическая проверка нужна, чтобы автор узнал о проблеме сразу, а не чтобы остановить злоумышленника.

### 6.2 Код

```rust
pub struct JsVerifier;

enum Severity { Error, Warning }

const FORBIDDEN: &[(&str, Severity)] = &[
    ("eval", Severity::Error),
    ("Function", Severity::Error),          // new Function(...), Function(...)
    ("process", Severity::Error),
    ("fetch", Severity::Error),
    ("XMLHttpRequest", Severity::Error),
    ("WebSocket", Severity::Error),
    ("document", Severity::Error),
    ("window", Severity::Error),
    ("createEl", Severity::Error),
    ("globalThis", Severity::Warning),      // чтение допустимо, запись это ошибка (проверяется отдельно)
];

impl RuntimeVerifier for JsVerifier {
    fn static_check(&self, manifest: &Manifest, bundle: &[u8], r: &mut VerifyReport) {
        let Ok(src) = std::str::from_utf8(bundle) else {
            return r.error("bundle.encoding", "bundle is not valid UTF-8");
        };
        if let Err(e) = compile_only(src) {                       // QuickJS без выполнения
            return r.error("syntax", e);
        }
        let program = parse_js(src);                              // oxc_parser, API уточнить по версии
        for hit in find_identifier_uses(&program, FORBIDDEN) {
            match hit.severity {
                Severity::Error   => r.error("forbidden.identifier", format!("{} (line {})", hit.name, hit.line)),
                Severity::Warning => r.warn("suspicious.identifier", format!("{} (line {})", hit.name, hit.line)),
            }
        }
        if has_dynamic_import_or_require(&program) {
            r.error("import.external", "external imports are not allowed");
        }
        // права и код
        let uses = detect_api_uses(&program);                     // requestUrl, vault.modify, ...
        if uses.network && manifest.permissions.network.is_empty() {
            r.warn("perm.network", "code uses requestUrl but no domains are declared");
        }
        if uses.vault_write && manifest.permissions.vault < VaultAccess::Write {
            r.warn("perm.vault_write", "code writes to the vault but the permission is not declared");
        }
        if looks_obfuscated(src) {
            r.warn("obfuscated", "bundle shows signs of an obfuscator");
        }
    }

    fn dry_run(&self, manifest: &Manifest, bundle: &[u8], host: Arc<MockHost>, lim: &Limits, r: &mut VerifyReport) {
        // тот же код привязок, что и в бою, но с MockHost вместо моста (раздел 4)
        let mut p = JsSandbox::spawn(bundle, host.clone(), lim.memory);

        match p.call_onload(lim.onload_ms) {
            Ok(()) => {}
            Err(CallError::Timeout) => return r.error("onload.timeout", "onload did not finish in time"),
            Err(CallError::Memory)  => return r.error("onload.memory", "memory limit exceeded"),
            Err(CallError::Js(msg)) => return r.error("onload.exception", msg),
        }

        let regs = host.registrations();
        if regs.commands.len() > lim.max_registrations { r.error("limit.commands", "too many commands"); }
        // ...то же для подписок, видов, лент

        for view in &regs.views {
            match p.call_render(view, lim.call_ms) {
                Ok(tree) => {
                    if let Err(e) = validate_tree(&tree) { r.error("view.invalid_tree", e.to_string()); continue; }
                    for id in tree.button_ids() { let _ = p.send_ui_event(view, &id, "click", None, lim.call_ms); }
                    for id in tree.input_ids()  { let _ = p.send_ui_event(view, &id, "change", Some("test"), lim.call_ms); }
                }
                Err(e) => r.error("view.render", e.to_string()),
            }
        }
        for ev in &regs.subscriptions { if let Err(e) = p.fire_sample_event(ev, lim.call_ms) { r.warn("event.exception", e.to_string()); } }
        for cmd in &regs.commands      { if let Err(e) = p.call_command(cmd, lim.call_ms)     { r.warn("command.exception", e.to_string()); } }

        p.call_onunload(lim.call_ms).ok();
        r.stats = p.stats();      // время, rt.memory_usage(), число обращений к API
    }
}
```

`JsSandbox` это тот же поток плагина из раздела 4, но с жёсткими лимитами проверки и `MockHost` вместо моста. Утечки после `onunload` проверяет общий код `verify()` (core §12.14).


---

## 7. Совместимость с плагинами Obsidian

**Коротко:** готовые плагины Obsidian напрямую не подойдут. Часть можно сделать совместимой, если очень нужно, но начинать лучше со своего API с похожими названиями (раздел 2.3).

### 7.1 Почему не подойдут

Плагины Obsidian написаны под Electron и зависят от трёх вещей, которых у вас нет:
1. **DOM.** Модальные окна, панели (`ItemView`), настройки и постпроцессоры Markdown строят интерфейс через `createEl`, `HTMLElement` и CSS. В GPUI нет DOM, и многие популярные плагины, которые рисуют таблицы или свои панели, на этом упрутся.
2. **CodeMirror 6.** Расширения редактора (подсветка, автодополнение, виджеты, скрытие разметки) пишутся как расширения CM6. У вас собственный редактор на GPUI.
3. **Node и Electron API.** Часть плагинов использует `fs`, `path`, `require` и внутренности Obsidian, которые не входят в публичный API.

Полная совместимость потребовала бы встроить webview с DOM и CM6, то есть повторить Obsidian. Это убивает смысл нативного приложения (память, скорость).

### 7.2 Варианты и выбранный путь

| Вариант | Что это | Оценка |
|---|---|---|
| 1. Свой API с похожими названиями (раздел 2) | Те же понятия (`Plugin`, `onload`, `addCommand`, `vault`, `workspace`, `Notice`, `Setting`), но без DOM. Автору легко портировать логику | **Базовый стандарт** |
| 2. **Micro-DOM эмулятор (Путь Б, раздел 7.8)** | Легковесный полифилл виртуального DOM (~350 строк JS), эмулирующий `createEl`, `contentEl`, `addEventListener`, `ButtonComponent` и на лету транслирующий дерево в `UiNode` | **Выбран для совместимости UI модалок и форм** |
| 3. Webview для UI плагинов | Даёт полноценный DOM, но приносит тяжёлый движок Chromium/WebKit и два параллельных мира UI | Не рекомендуется |

### 7.3 Совместимость на уровне данных

Важнее самих плагинов для пользователей:
- Тот же формат vault: обычные `.md`-файлы, вики-ссылки `[[...]]`, frontmatter, теги, вложения.
- Пользователи смогут открыть существующий vault без миграции.
- Часть настроек из `.obsidian` (например, горячие клавиши) можно читать, но это не обязательно.

### 7.4 Как принять решение про слой совместимости

1. Взять 10–20 популярных плагинов Obsidian разных категорий.
2. Разложить по трём группам: работают без правок, работают с небольшими правками, не работают (DOM или CM6).
3. Если доля первых двух групп мала (ориентир: меньше пятой части), слой совместимости не оправдан, лучше вложиться в руководство по портированию. Это ориентир для обсуждения, а не измеренное правило.
4. Если доля заметная, сделать модуль `obsidian` только для той части API, которая реально используется.

### 7.5 Руководство по портированию (содержание)

- Таблица соответствий и правила (раздел 2.3).
- Чем заменить DOM: дерево компонентов (core §5) и примеры до/после для панели, модального окна, статус-бара (раздел 3).
- Чем заменить CM6: хуки редактора (раздел 5).
- Что не поддерживается и какую ошибку увидит автор: `Not supported: DOM (createEl). See the porting guide`.

### 7.6 Лицензии

Не копировать код и типы Obsidian: свои `.d.ts` и свой код. Лицензии конкретных плагинов, которые вы захотите портировать или запускать, проверяйте отдельно.

### 7.7 Система автоматического анализа совместимости и Proxy-телеметрии

Для автоматической оценки совместимости любого Obsidian-плагина (`main.js`) система использует комбинацию статического сканера и рантайм-перехватчиков.

#### 1. Таблица классификации методов и API

| Категория | Статус / Цвет | Примеры вызовов | Поведение хоста |
|---|---|---|---|
| **Ядро и Поддержанные** | 🟢 **Зелёный (Full)** | `new Notice()`, `plugin.addCommand()`, `plugin.registerMarkdownCodeBlockProcessor()`, `app.vault.read()`, `app.vault.modify()`, `app.vault.create()`, `plugin.loadData()`, `plugin.saveData()`, `app.workspace.getActiveFile()` | Полное нативное выполнение в хосте. |
| **Второстепенные / Заглушки / Легко добавить** | 🟡 **Жёлтый (Partial)** | `app.vault.getMarkdownFiles()`, `app.vault.getFiles()`, `app.vault.getAbstractFileByPath()`, `plugin.addRibbonIcon()`, `plugin.addStatusBarItem()`, `app.workspace.on()`, `app.metadataCache.getFileCache()`, `normalizePath()` | Либо возвращается безопасный дефолт (fallback), либо метод быстро дописывается в `runtime.rs` / `bridge.rs`. |
| **Критические несовместимости** | 🔴 **Красный (Incompatible)** | `require('electron')`, `require('child_process')`, сырой `fs`, `document.createElement()`, `containerEl.appendChild()`, `innerHTML`, `jQuery`, CM6 `EditorView`, `app.internalPlugins` | Плагин помечается как несовместимый. При попытке вызова выдаётся понятное предупреждение в UI/логах. |

#### 2. Архитектура JS Proxy для аудита в реальном времени

В `src/plugins/runtime.rs` глобальные объекты `app`, `app.vault`, `app.workspace` оборачиваются в `Proxy`:

```javascript
function makeAuditedProxy(target, targetName, hostBridge) {
    return new Proxy(target, {
        get(obj, prop) {
            if (prop in obj) {
                return obj[prop];
            }
            // Метод ещё не реализован в хосте
            const methodName = String(prop);
            hostBridge.auditApiCall(targetName, methodName);
            console.warn(`[API Audit] Plugin called unimplemented API: ${targetName}.${methodName}`);
            
            // Возвращаем безопасную заглушку, предотвращающую моментальный краш
            return (...args) => {
                return Promise.resolve(null);
            };
        }
    });
}
```

#### 3. Модель данных отчёта совместимости (`src/plugins/compat.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompatibilityStatus {
    Full,         // 🟢 100% поддерживается
    Partial(f32), // 🟡 Частично (например, 75%)
    Incompatible, // 🔴 Требует Electron / DOM
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityReport {
    pub status: CompatibilityStatus,
    pub score: f32, // 0.0 .. 1.0
    pub supported_calls: Vec<String>,
    pub missing_calls: Vec<String>,
    pub incompatible_calls: Vec<String>,
```

### 7.8 Архитектура Micro-DOM эмулятора для Obsidian UI (Путь Б)

Большинство утилитарных плагинов Obsidian (очистка заметок, поиск сирот, генераторы шаблонов, диалоги подтверждения) не требуют сложного CSS или Canvas, а используют стандартный набор методов Obsidian для построения интерфейсов модальных окон и настроек. Для запуска таких плагинов без переписывания создаётся легковесный полифилл виртуального DOM (~350 строк JS внутри QuickJS).

#### 1. Поддерживаемое подмножество DOM API
- Создание элементов: `element.createEl(tag, options)`, `element.createDiv(cls)`, `element.createSpan(cls)`.
- Дерево элементов: `element.empty()`, `element.remove()`, `element.appendChild(child)`.
- Стили и классы: `element.addClass(cls)`, `element.removeClass(cls)`, `element.toggleClass(cls, bool)`.
- Содержимое и свойства: `element.textContent`, `element.innerHTML` (текстовый парсинг), `element.disabled`, `element.checked`.
- События: `element.addEventListener(type, listener)`, `element.dispatchEvent(event)`.
- Селекторы: `element.querySelector(selector)`, `element.querySelectorAll(selector)`, `document.getElementById(id)`.
- Компоненты Obsidian: `ButtonComponent` (`setButtonText`, `setCta`, `onClick`), `ToggleComponent` (`setValue`, `onChange`), `TextComponent` (`setPlaceholder`, `onChange`).

#### 2. Трансляция виртуального DOM в нативные `UiNode` GPUI
При закрытии сборки модального окна (`Modal.open()`) или панели, корневой виртуальный элемент `contentEl` рекурсивно транслируется в нативное дерево `UiNode`:

| Виртуальный тег / компонент | Трансляция в `UiNode` | Особенности |
|---|---|---|
| `button` / `ButtonComponent` | `UiNode::Button` | Класс `mod-cta` / `ef-btn-danger` мапится в `variant: ButtonVariant::Primary / Danger`. Свойство `disabled` поддерживается нативно. |
| `input[type=checkbox]` / `ToggleComponent` | `UiNode::Checkbox` | Состояние `checked` и событие `change`. |
| `input[type=text]` / `TextComponent` | `UiNode::Input` | Поле ввода с `placeholder` и `value`. |
| `h1`, `h2`, `h3`, `h4` | `UiNode::Text { style: Heading }` | Заголовки разделов. |
| `p`, `span`, текстовые ноды | `UiNode::Text` | Текстовые блоки. Классы `muted` / `subtitle` мапятся в `style: Muted`. |
| `div` (контейнер) | `UiNode::Column` / `UiNode::Row` | По умолчанию вертикальный стек (`Column`). Если задан flex-row или класс кнопочной панели — горизонтальный ряд (`Row`). |

#### 3. Маршрутизация событий кликов и ввода
- Каждый виртуальный элемент получает уникальный идентификатор (`ElementId`).
- При рендере в GPUI кнопкам и полям назначается этот идентификатор.
- Когда пользователь кликает по кнопке или переключает чекбокс в окне приложения, хост передает в QuickJS:
  `HostEvent::UiAction { element_id, action: "click" / "change", value: Option<String> }`.
- JS-рантайм находит виртуальный элемент и синхронно вызывает привязанные обработчики `addEventListener("click", ...)` с синтетическим объектом `Event`.
- После завершения обработчика, если дерево изменилось (например, добавлен список удаленных файлов или обновлен текст кнопки), хост получает обновленное дерево `SetViewTree` и перерисовывает окно.

---

## 8. SDK и цикл разработки автора

### 8.1 Состав SDK

```
sdk/
├─ types/            # .d.ts: сгенерированные из Rust (ApiRequest, UiNode, события) и рукописные (Plugin, Modal, ItemView, Notice, Setting)
├─ template/         # шаблон плагина: manifest.json, src/main.ts, tsconfig.json, esbuild-конфиг
└─ cli/              # aquilum-plugin: create, check, build, dev, pack
```

### 8.2 Типы из Rust

Типы запросов, событий и дерева UI живут в `plugin-api` (core §12.2, §12.7). Из них `ts-rs` или `specta` генерируют `.d.ts`, чтобы TypeScript и Rust не расходились. Классы (`Plugin`, `Modal`, `ItemView`, `Notice`, `Setting`) это JS-прототипы, которые создаёт `install_globals` (раздел 4), для них `.d.ts` пишутся вручную и проверяются тестом на соответствие реальному поведению.

### 8.3 Сборка

Плагин собирается в один файл без внешних зависимостей. Хост перед выполнением бандла определяет глобальные `module` и `exports`, после выполнения берёт `module.exports.default` (или `module.exports`) и создаёт экземпляр класса.

```bash
esbuild src/main.ts --bundle --format=cjs --platform=neutral --target=es2020 --outfile=main.js
```

Цель `es2020` выбрана осторожно: QuickJS поддерживает современные возможности языка, но проверяйте на реальном рантайме, какие из них доступны в вашей версии. Обычная минификация допустима, обфускация нет (раздел 6.1).

### 8.4 CLI

| Команда | Что делает |
|---|---|
| `aquilum-plugin create` | создаёт плагин из шаблона |
| `aquilum-plugin check` | запускает `verify()` (core §12.14) и печатает отчёт |
| `aquilum-plugin build` | собирает `main.js` |
| `aquilum-plugin dev` | собирает при изменении файлов и ставит плагин в тестовый vault, приложение перезагружает его (core §12.10) |
| `aquilum-plugin pack` | собирает ассеты релиза и считает `sha256` для каталога (core, этап 8) |

### 8.5 Цель: 15 минут до работающего плагина

Новый разработчик ставит CLI, выполняет `create`, правит `src/main.ts`, запускает `dev` и видит результат в приложении. Проверка этого критерия: взять человека, не знающего проект, и засечь время по документации.

---

## 9. Тесты JS-рантайма

| Тест | Ожидание |
|---|---|
| `while(true){}` в `onload` и в обработчике события | Прервано по дедлайну, интерфейс не теряет кадры, после трёх страйков плагин отключён |
| `new Array(1e9)` или рост строки | Ошибка памяти внутри плагина, приложение живо |
| `eval("...")`, `Function("...")` | Недоступны в рантайме, и `plugin check` сообщает об этом |
| `fetch`, `require`, `process`, `document`, `createEl` | Не определены, понятная ошибка для `createEl` |
| Promise, который никогда не разрешается | Запись в таблице удалена по таймауту запроса (30 с), плагин не копит память |
| Исключение в обработчике события | Страйк, остальные плагины не затронуты |
| Выгрузка плагина | Не осталось команд, подписок, видов, окон, потока (core §12.10) |
| Синхронные геттеры (`getActiveFile`, `getMarkdownFiles`) во время записи нового снимка | Всегда согласованное состояние (core §12.5) |

---

## 10. Какие этапы core затрагивает этот документ

| Этап core | Что здесь |
|---|---|
| 2. Минимальный хост | раздел 4 (поток QuickJS, привязки), раздел 2 (первые методы API) |
| 3. Права и лимиты | разделы 4 и 6 (лимиты, страйки, проверка) |
| 5. Декларативный UI | раздел 3 (примеры) |
| 6. Редактор и Markdown | раздел 5 (хуки редактора) |
| 7. SDK и документация | разделы 7 и 8 (совместимость, SDK) |
