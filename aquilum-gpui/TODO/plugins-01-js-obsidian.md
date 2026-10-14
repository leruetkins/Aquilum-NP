# Система плагинов. Часть 1: JS/TS-плагины с API в стиле Obsidian

Основной рантайм плагинов: **JavaScript и TypeScript на QuickJS** (`rquickjs`), один рантайм и один поток на плагин. Общая часть (архитектура, манифест, права, безопасность, проверка плагинов, раскладка, формат дерева UI, этапы, каталог) описана в `plugins-00-core.md` («core»). Дорожная карта запуска немодифицированных плагинов Obsidian описана в `plugins-04-obsidian-compat.md` («compat»).

| Документ | О чём |
|---|---|
| `plugins-00a-adaptation.md` | адаптация текущего кода |
| `plugins-00-core.md` | общее ядро и дорожная карта |
| `plugins-01-js-obsidian.md` (этот) | JS-рантайм, API в стиле Obsidian |
| `plugins-02-wasm.md` | WASM-плагины и Rust SDK |
| `plugins-03-zed-research.md` | исследование архитектуры Zed |
| `plugins-04-obsidian-compat.md` | совместимость с готовыми плагинами Obsidian (E0–E8) |

Ссылки вида «core §12.2» ведут в общий документ, «WASM §3» в документ по WASM-плагинам, «Zed §3» в документ по Zed, «compat §5» в спецификацию совместимости.

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

> **Примечание об архитектуре окон (Modal vs Multi-Window):**
> Модальные окна плагинов (`Modal`) отображаются как всплывающие оверлеи внутри текущего активного окна редактора над контентом. В отличие от них, системные служебные окна (такие как **Каталог сторонних плагинов** и расширенные настройки) открываются как **отдельные независимые окна операционной системы** (GPUI `cx.open_window`) с многоколоночной адаптивной сеткой карточек (core §12.15, adaptation §15). При установке или обновлении плагина через окно каталога хост выполняет горячую инициализацию потока QuickJS без прерывания сессии пользователя в окнах заметок.

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

/// Политика Safe Mode (Безопасный режим):
/// По умолчанию safe_mode = true. Ни один поток QuickJS не запускается до явного
/// отключения безопасного режима пользователем через диалог в окне плагинов или настройках.
pub fn spawn_plugin(
    id: PluginId,
    source: String,
    api_tx: futures::channel::mpsc::Sender<ApiCall>,
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
            
            // Watchdog interrupt handler: прерывает выполнение при превышении дедлайна
            rt.set_interrupt_handler(Some(Box::new(move || now_ms() > d.load(Ordering::Relaxed))));
            let ctx = rquickjs::Context::full(&rt).unwrap();

            // 1. Дедлайн выставляется ДО запуска верхнеуровневого скрипта (защита от while(true))
            d.store(now_ms() + 2000, Ordering::Relaxed);
            ctx.with(|ctx| {
                install_globals(&ctx, &api_tx, &snapshot); // Plugin, Notice, app.*, таймеры
                // Поддержка require("obsidian") через виртуальный модуль хоста
                install_obsidian_shim_module(&ctx);
                let _ = ctx.eval::<(), _>(source);
            });
            d.store(u64::MAX, Ordering::Relaxed);

            // 2. Цикл событий с учётом таймеров хоста (без холостого пробуждения 20 мс)
            loop {
                let timeout = get_time_until_next_timer();
                let next_event = match timeout {
                    Some(duration) => events_rx.recv_timeout(duration),
                    None => events_rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
                };

                match next_event {
                    Ok(HostEvent::Unload) => {
                        // Вежливый вызов onunload() плагина перед выходом
                        d.store(now_ms() + 500, Ordering::Relaxed);
                        ctx.with(|ctx| call_plugin_onunload(&ctx));
                        d.store(u64::MAX, Ordering::Relaxed);
                        break;
                    }
                    Err(RecvTimeoutError::Disconnected) => break,
                    Ok(ev) => {
                        d.store(now_ms() + budget_for(&ev), Ordering::Relaxed);
                        ctx.with(|ctx| dispatch_event(&ctx, ev));
                        while rt.is_job_pending() {
                            if rt.execute_pending_job().is_err() { break; }
                        }
                        d.store(u64::MAX, Ordering::Relaxed);
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        // Сработал таймер (setTimeout / setInterval)
                        d.store(now_ms() + 100, Ordering::Relaxed);
                        ctx.with(|ctx| trigger_expired_timers(&ctx));
                        d.store(u64::MAX, Ordering::Relaxed);
                    }
                }
            }
        })
        .unwrap();

    PluginHandle { id, events_tx, deadline_ms, join: Some(join) }
}
```

**Таймеры хоста (`setTimeout`, `setInterval`):**
В QuickJS нет встроенных таймеров ОС. Хост реализует `setTimeout(fn, ms)`, `clearTimeout(id)`, `setInterval(fn, ms)`, `clearInterval(id)`. Запланированные таймеры хранятся в очереди с приоритетом (`BinaryHeap<ScheduledTimer>`) в потоке плагина. Поток ожидает ровно столько миллисекунд, сколько осталось до ближайшего таймера, устраняя 100% паразитной нагрузки на CPU. Метод `plugin.registerInterval(id)` автоматически заносит таймер в реестр очистки при выгрузке.

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
| 6 | Событие `workspace.on("editor-menu", (menu, editor, view) => ...)` и классы `Menu`, `MenuItem` (включая `setSubmenu()`) | Контекстное меню мыши для действий над выделением и каскадных подменю (напр. `obsidian-amount-in-words-np`) |

### 5.2 После релиза 1

| Хук | Зачем | Почему не сейчас |
|---|---|---|
| `registerEditorDecorations` (подсветка диапазонов, стили, виджеты в строке) | Подсветка тегов, цвета, маркеры | Нужна стабильная модель позиций и раскладки |
| Подсказки при вводе (`[[`, `/`, `@`) | Автодополнение | Нужен popup и работа с курсором |
| Свои горячие клавиши внутри редактора | Редакторные сочетания | Конфликты с хоткеями приложения |

### 5.3 Не отдавать плагинам (или очень поздно)

Скрытие и подмена разметки (то, что делает сам Live Preview), блочные виджеты на месте текста, собственные расширения со своим состоянием, новые языки. Это ядро поведения редактора, его лучше держать внутренним.

### 5.4 Что заложить в ядро уже сейчас

Даже если наружу выставлен только минимум, редактор должен иметь:
- **Кодирование позиций (UTF-16 $\leftrightarrow$ UTF-8):** JS оперирует 16-битными кодовыми единицами (`String.length`, `substring`, позиции курсора в Obsidian API), тогда как Rust в GPUI оперирует байтовыми смещениями UTF-8. Хост и плагин обязаны конвертировать смещения на границе сообщений моста, гарантируя сохранение эмодзи и суррогатных пар без смещения позиций.
- **Версионированное зеркало редактора (`EditorSession`):** локальная копия текста и выделения живёт в потоке JS-плагина, обновляясь по событиям хоста. Плагин читает позиции мгновенно синхронно.
- **Номер версии документа** и транзакции (изменение это набор операций с одной записью в истории).
- Возможность пересчитать позиции через правки, случившиеся с версии N. Плагин работает в отдельном потоке, поэтому его ответ (например, декорации или замена текста) придёт с задержкой, и хосту нужно «сдвинуть» диапазоны на изменения за это время (алгоритм `map_offset` в core §12.9).
- **Пакетное применение правок (`ApplyEdits`):** несколько вызовов `replaceSelection` / `replaceRange` в рамках одной функции склеиваются в единую атомарную транзакцию Undo хоста.

### 5.5 Транзакции и асинхронность

`editor.transact(fn)` объединяет только синхронную часть вызова. Асинхронные продолжения (после `await`) в транзакцию не входят. Это нужно явно написать в документации для авторов. Детальная архитектура синхронизации редактора вынесена в `plugins-04-obsidian-compat.md §7`.

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

> **Исключение для режима совместимости с Obsidian (`compat: "obsidian"`):**
> Для немодифицированных плагинов из экосистемы Obsidian (подробно в `plugins-04-obsidian-compat.md §3`):
> 1. `require("obsidian")` разрешён и перехватывается хостовым виртуальным модулем-shim.
> 2. Внешние CJS/ESM импорты (`@codemirror/*`, `electron`) не блокируют установку, а разрешаются в `lazyFail` Proxy: ошибка бросается только при попытке их вызова во время исполнения.
> 3. Идентификаторы `createEl`, `document`, `window` обслуживаются полифиллом `VElement` и не считаются ошибкой статического анализа, если плагин помечен для запуска в режиме совместимости.

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
| 1. Свой API с похожими названиями (раздел 2) | Те же понятия (`Plugin`, `onload`, `addCommand`, `vault`, `workspace`, `Notice`, `Setting`), но без DOM. Автору легко портировать логику | **Базовый стандарт для новых плагинов** |
| 2. **Слой совместимости с Obsidian (документ `plugins-04-obsidian-compat.md`)** | Полифилл `require("obsidian")`, виртуальный элемент `VElement` для `Setting` и `Modal` с прямой трансляцией в `UiNode`, зеркало редактора `EditorSession` (UTF-16) | **Выбран для запуска готовых плагинов без переписывания** |
| 3. Webview для UI плагинов | Даёт полноценный DOM, но приносит тяжёлый движок Chromium/WebKit и два параллельных мира UI | Не рекомендуется |

### 7.3 Совместимость на уровне данных

Важнее самих плагинов для пользователей:
- Тот же формат vault: обычные `.md`-файлы, вики-ссылки `[[...]]`, frontmatter, теги, вложения.
- Пользователи смогут открыть существующий vault без миграции.
- Часть настроек из `.obsidian` (например, горячие клавиши) можно читать, но это не обязательно.

### 7.4 Дорожная карта совместимости

Детальный пошаговый план запуска готовых плагинов, каталог API Obsidian и метрики готовности описаны в **`plugins-04-obsidian-compat.md`**:
- **Этап E0:** Аудит и сбор тестового корпуса из 20 плагинов.
- **Этапы E1–E2:** Shim `require("obsidian")`, `lazyFail` для внешних пакетов, реализация `App` и `Vault`.
- **Этапы E3–E4:** Скелет `VElement` для настроек и модалок, интеграция `ItemView` в сайдбар.
- **Этап E5:** Зеркало редактора и конвертация смещений UTF-16 $\leftrightarrow$ UTF-8.
- **Этапы E6–E8:** Блоки кода, автоматический CI-гейт и массовая валидация каталога.

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
// Набор известных методов Obsidian, для которых безопасен no-op stub
const SAFE_NOOP_STUBS = new Set([
    'app.workspace.requestSaveHistory',
    'app.metadataCache.trigger',
    'plugin.addRibbonIcon'
]);

// Набор известных методов Obsidian, ещё не реализованных в Aquilum
const KNOWN_UNIMPLEMENTED_OBSIDIAN_METHODS = new Set([
    'app.vault.copy',
    'app.workspace.splitActiveLeaf',
    'app.fileManager.processFrontMatter'
]);

function makeAuditedProxy(target, targetName, hostBridge) {
    return new Proxy(target, {
        get(obj, prop) {
            // 1. КРИТИЧНО: Защита thenable-проверок в async/await, символов и сериализации
            if (typeof prop === 'symbol' || prop === 'then' || prop === 'toJSON') {
                return obj[prop];
            }
            if (prop in obj) {
                return obj[prop];
            }
            const propName = String(prop);
            const fullPath = `${targetName}.${propName}`;

            // 2. Если вызывается метод из белого списка безопасных заглушек
            if (SAFE_NOOP_STUBS.has(fullPath)) {
                hostBridge.auditApiCall(targetName, propName, "stub");
                return (..._args) => Promise.resolve(null);
            }

            // 3. Если вызывается известный метод Obsidian, пока не реализованный у нас
            if (KNOWN_UNIMPLEMENTED_OBSIDIAN_METHODS.has(fullPath)) {
                hostBridge.auditApiCall(targetName, propName, "unimplemented");
                return (..._args) => {
                    throw new Error(`[Aquilum] Method ${fullPath} is not implemented`);
                };
            }

            // 4. Если плагин проверяет наличие свойства (feature detection: if (app.foo))
            // Возвращаем undefined, а не бросаем исключение и не возвращаем truthy-объект!
            return undefined;
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
    pub warnings: Vec<String>,
```

### 7.8 Архитектура `VElement` и семантическая трансляция UI (Путь Б)

Большинство плагинов Obsidian строят интерфейс настроек и модальных окон не через сырой HTML, а через класс `Setting(containerEl)` и вспомогательные компоненты (`ButtonComponent`, `ToggleComponent`, `SliderComponent`, `DropdownComponent`).
Поэтому вместо попытки построить полноценный эмулятор браузерного DOM (что невозможно без движка стилей и лэйаута), вводится **скелет виртуального элемента `VElement`** (~250 строк JS), ориентированный на семантическое связывание с GPUI `UiNode`.

Подробная реализация `VElement`, маппинг компонентов и правила рендеринга специфицированы в **`plugins-04-obsidian-compat.md §5, §6`**.

#### 1. Поддерживаемое подмножество API `VElement`
- Создание и обход элементов: `element.createEl(tag, options)`, `element.createDiv(cls)`, `element.createSpan(cls)`, `element.empty()`, `element.appendChild(child)`.
- Стили и классы: `element.addClass(cls)`, `element.removeClass(cls)`, `element.toggleClass(cls, bool)`.
- Текст и свойства: `element.setText(text)`, `element.textContent`, `element.disabled`, `element.value`.
- События: `element.addEventListener(type, listener)`, `element.removeEventListener()`.
- Obsidian UI компоненты: `Setting` (`setName`, `setDesc`, `addToggle`, `addText`, `addSlider`, `addDropdown`, `addButton`), `ButtonComponent`, `ToggleComponent`, `TextComponent`, `SliderComponent`, `DropdownComponent`.

#### 2. Трансляция `VElement` $\to$ нативные `UiNode` GPUI
При закрытии сборки модального окна (`Modal.open()`) или вызове `display()` в `PluginSettingTab`, дерево `containerEl` / `contentEl` сериализуется в семантические `UiNode`:

| Компонент / Элемент Obsidian | Целевой `UiNode` | Семантика в GPUI |
|---|---|---|
| `Setting` | `UiNode::SettingItem` | Горизонтальный ряд: имя + описание слева, контрол справа |
| `ButtonComponent` / `<button>` | `UiNode::Button` | Кнопка с вариантом `primary`, `danger` или `default` |
| `ToggleComponent` / `<input type="checkbox">` | `UiNode::Toggle` | Переключатель (акцентный цвет темы) |
| `SliderComponent` / `<input type="range">` | `UiNode::Slider` | Ползунок с диапазоном `min..max` и шагом `step` |
| `DropdownComponent` / `<select>` | `UiNode::Select` | Выпадающий список опций |
| `TextComponent` / `<input type="text">` | `UiNode::Input` | Поле ввода текста с debounce |
| `<h1>` .. `<h6>` | `UiNode::Text` | Заголовок (`heading`) |
| `<p>`, `<span>`, текстовый узел | `UiNode::Text` | Текстовый блок / подпись |
| `<div>` | `UiNode::Column` / `UiNode::Row` | Контейнер компоновки элементов |

#### 3. Маршрутизация событий кликов и ввода
- Каждый виртуальный элемент получает детерминированный `ElementId`.
- Хост пересылает события пользователя: `HostEvent::UiAction { element_id, action: "click" | "change", value: Option<String> }`.
- JS-рантайм находит виртуальный элемент и синхронно вызывает зарегистрированные обработчики `addEventListener` / `onChange`.
- После завершения коллбэка изменённое дерево сериализуется и отправляется в хост (`SetViewTree`), вызывая перерисовку окна.

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
| 8. Каталог и распространение | раздел 3.2, 8.4 (ассеты для каталога, горячая загрузка из окна плагинов) |
