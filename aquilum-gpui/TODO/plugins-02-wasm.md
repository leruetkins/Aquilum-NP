# Система плагинов. Часть 2: WASM-плагины (wasmtime, компонентная модель)

Второй рантайм после JS (этап 9 в core). Плагины пишутся на Rust (самый зрелый путь) или на другом языке с поддержкой компонентной модели WASM, компилируются в один `main.wasm` и запускаются в `wasmtime`. Мост, права, реестр регистраций, раскладка, формат дерева UI, проверка и каталог общие с JS (`plugins-00-core.md`, «core»). Опыт Zed, на котором многое основано, описан в `plugins-03-zed-research.md` («Zed»). JS-рантайм описан в `plugins-01-js-obsidian.md` («JS»).

> Код это эскизы для проектирования, они не проверялись компиляцией. Компонентная модель, `wit-bindgen` и `wasmtime` быстро развиваются: закрепите версии и сверяйте имена типов и методов (особенно `bindgen!`, `UpdateDeadline`, `StoreLimitsBuilder`, WASI-контекст) с документацией своей версии. Всё, что опирается на Zed, я сверял с прочитанным кодом (Zed §3), остальное это мои рекомендации.

---

## 1. Когда нужен WASM, а когда JS

| | JS (QuickJS) | WASM (wasmtime) |
|---|---|---|
| Кто пишет | массовый автор, аудитория Obsidian | авторы на Rust, C, других языках |
| Порог входа | низкий: TypeScript, `esbuild` | выше: Rust, цель `wasm32-wasip2`, WIT |
| Скорость | достаточно для обычных плагинов | ближе к нативной: парсеры, граф, индексация, сжатие |
| Изоляция | кучи GC движка на C | линейная память, нет доступа к хосту кроме импортов |
| Прерывание по времени | interrupt handler | epoch interruption |
| Проверка по исходнику | бандл читаем (JS §6) | бинарник нечитаем, нужны открытый исходник и воспроизводимая сборка в каталоге |
| UI | JSON-дерево | плоский массив узлов в WIT |
| Размер и сборка | килобайты | сотни килобайт и дольше сборка |

**Рекомендация:** выпускать JS первым, WASM добавлять после заморозки API v1. Интерфейс WIT должен быть описанием **тех же** запросов и событий (`ApiRequest`, `HostEvent`), а не отдельным API, иначе придётся поддерживать два.

---

## 2. Модель выполнения

Схема повторяет Zed (Zed §3), но с одним отличием: по каналу ходят **данные**, а не замыкания.

```
UI-поток (GPUI)                      поток/задача плагина
────────────────                     ────────────────────────────────
PluginHost ── PluginCall ──────────▶ plugin_loop: Store<WasmState> + экземпляр компонента
   ▲                                       │ вызов импорта host.vault-read(...)
   │ ApiCall (через мост, core §12.2)      ▼
   └────────────────────────────────  WasmState::call_host ── HostApi::submit
HostEvent::Response ── send_event ─▶ разрешает ожидающий oneshot (плагин продолжается)
```

- Один общий `Engine` на приложение, один `Store` на плагин, одна задача на плагин (в Zed то же самое).
- `WasmRuntime` реализует `PluginRuntime` (core §12.1): `send_event`, `interrupt`, `stats`, `shutdown`.
- Запросы плагина идут через `HostApi` (core §12.14), то есть в бою через мост, в проверке через `MockHost`.
- Чтение снимка (`active-file`, `list-markdown-files`, метаданные) выполняется прямо в потоке плагина по `ArcSwap<Snapshot>` без обращения к UI-потоку (core §12.5).
- Каждый плагин получает свой `Store`, поэтому состояние и память плагинов не пересекаются.

Почему данные, а не замыкания: Zed передаёт замыкания (`ExtensionCall`), потому что вызывает свой доверенный код. Нам нужны проверяемые, сериализуемые запросы (права, лимиты, возможность позже вынести рантайм в отдельный процесс).

---

## 3. Интерфейс: WIT

Один файл `wit/plugin.wit` описывает то, что плагин может вызвать (`host`) и что он обязан реализовать (`plugin`).

```wit
package aquilum:plugin@1.0.0;

interface types {
    record file-info { path: string, name: string, basename: string, extension: string }

    variant api-error {
        denied(string),
        not-found(string),
        invalid(string),
        limit(string),
        io(string),
    }

    /// Дерево UI передаётся плоским массивом: рекурсивные типы в WIT не используются,
    /// (насколько мне известно, компонентная модель пока их не поддерживает), поэтому
    /// дочерние узлы это индексы в `nodes`.
    record ui-node {
        id: option<string>,
        kind: string,              // "column", "button", "text", ...
        props: string,             // JSON со свойствами узла (проверяется хостом)
        children: list<u32>,
    }
    record ui-tree { nodes: list<ui-node>, root: u32 }

    record ui-event { view-id: u64, node-id: string, event: string, value: option<string>, seq: u64 }

    variant host-event {
        vault-modify(string),
        file-open(option<string>),
        ui(ui-event),
        modal-closed(u64),
    }
}

interface host {
    use types.{file-info, api-error, ui-tree};

    // чтение снимка состояния (без обращения к UI-потоку)
    active-file: func() -> option<file-info>;
    list-markdown-files: func() -> list<file-info>;

    // запросы к хосту (проходят проверку прав, core §12.3)
    vault-read: func(path: string) -> result<string, api-error>;
    vault-modify: func(path: string, data: string) -> result<_, api-error>;
    http-request: func(url: string, method: string, body: option<string>) -> result<string, api-error>;

    // регистрации (попадают в реестр освобождаемых объектов, core §12.10)
    register-command: func(id: string, name: string) -> result<_, api-error>;
    register-view: func(view-type: string, title: string) -> result<u64, api-error>;
    subscribe: func(event: string) -> result<_, api-error>;

    // интерфейс
    set-view-tree: func(view-id: u64, tree: ui-tree) -> result<_, api-error>;
    open-modal: func(title: string, tree: ui-tree) -> result<u64, api-error>;
    notice: func(text: string, timeout-ms: u32);

    log: func(level: u8, message: string);
}

interface plugin {
    use types.{host-event, ui-tree};

    on-load: func() -> result<_, string>;
    on-unload: func();
    on-event: func(event: host-event);
    run-command: func(id: string) -> result<_, string>;
    render-view: func(view-id: u64) -> result<ui-tree, string>;
}

world plugin-world {
    import host;
    export plugin;
}
```

**Правила:**
- WIT-файлы генерируются или хотя бы сверяются с типами `plugin-api` тестом: каждый вариант `ApiRequest` имеет соответствие в `host`.
- Версия пакета (`@1.0.0`) совпадает с `apiVersion` манифеста.
- `HostEvent::Response` и `Unload` в WIT не попадают: ответы разрешают ожидающие вызовы внутри хоста, а выгрузка это вызов `on-unload`.
- Свойства узлов (`props`) это JSON, который хост разбирает в типизированный `UiNode` и затем пропускает через `validate_tree` (core §12.7).

Преобразование плоского дерева в `UiNode` с защитой от циклов и общих узлов:

```rust
fn tree_from_wit(t: &wit::UiTree) -> Result<UiNode, ApiError> {
    fn build(t: &wit::UiTree, idx: u32, depth: usize, seen: &mut HashSet<u32>) -> Result<UiNode, ApiError> {
        if depth > 32 { return Err(ApiError::limit("tree too deep")); }
        if !seen.insert(idx) { return Err(ApiError::invalid("node is used twice (cycle or shared node)")); }
        let n = t.nodes.get(idx as usize).ok_or_else(|| ApiError::invalid("bad node index"))?;
        let children = n.children.iter()
            .map(|&c| build(t, c, depth + 1, seen))
            .collect::<Result<Vec<_>, _>>()?;
        UiNode::from_parts(&n.kind, n.id.as_deref(), &n.props, children)   // props: JSON в типизированный узел
    }
    if t.nodes.len() > 2_000 { return Err(ApiError::limit("too many nodes")); }
    build(t, t.root, 0, &mut HashSet::new())
}
```

---

## 4. Хост на wasmtime

### 4.1 Engine

Настройки повторяют Zed (компонентная модель, async, epoch interruption), с двумя отличиями: тик эпохи 10 мс вместо 100 мс (дедлайны точнее) и отдельный поток для тиков (Zed в комментарии отмечает, что так безопаснее, если расширения блокируют executor).

```rust
fn wasm_engine() -> wasmtime::Engine {
    static ENGINE: OnceLock<wasmtime::Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let mut config = wasmtime::Config::new();
        config.wasm_component_model(true);
        config.async_support(true);          // как в Zed: ожидание ответа хоста без блокировки потока
        config.epoch_interruption(true);     // прерывание по времени
        config.max_wasm_stack(512 * 1024);   // защита от переполнения стека
        let engine = wasmtime::Engine::new(&config).unwrap();

        let weak = engine.weak();
        std::thread::Builder::new().name("wasm-epoch".into()).spawn(move || loop {
            std::thread::sleep(Duration::from_millis(10));
            let Some(engine) = weak.upgrade() else { break };   // выходим, когда Engine уничтожен
            engine.increment_epoch();
        }).unwrap();
        engine
    }).clone()
}
```

### 4.2 Store, лимиты и WASI

В Zed каждому расширению даётся WASI-контекст с рабочей папкой (`preopened_dir`), `inherit_stdio` и переменными окружения. Нам нужно строже: **никаких папок, переменных окружения, аргументов и сокетов**. Плагин работает с файлами только через `host.vault-read` и `host.vault-modify`, где права и пути проверяет хост.

```rust
pub struct WasmState {
    pub id: PluginId,
    pub host: Arc<dyn HostApi>,                // BridgeHost или MockHost (core §12.14)
    pub permissions: Permissions,              // core §12.3
    pub pending: PendingResponses,             // Arc<Mutex<HashMap<RequestId, oneshot::Sender<ApiResponse>>>>
    pub snapshot: Arc<arc_swap::ArcSwap<Snapshot>>,
    pub table: ResourceTable,
    pub wasi: WasiCtx,
    pub limits: StoreLimits,
    pub budget: Arc<Budget>,                   // бюджет времени текущего вызова
    next_id: u64,
}

fn build_store(engine: &Engine, spec: &SpawnSpec, host: Arc<dyn HostApi>, snapshot: Arc<ArcSwap<Snapshot>>) -> Store<WasmState> {
    let wasi = WasiCtxBuilder::new()
        // никаких preopened_dir, env, args, сокетов
        // stdout и stderr уходят в консоль плагина (реализация потока вывода зависит от версии wasmtime-wasi)
        .build();
    let limits = StoreLimitsBuilder::new()
        .memory_size(spec.limits.memory)       // предел линейной памяти
        .table_elements(100_000)
        .instances(1)
        .memories(1)
        .trap_on_grow_failure(true)            // превышение памяти завершает вызов ошибкой, а не тихим отказом
        .build();
    let mut store = Store::new(engine, WasmState {
        id: spec.id.clone(), host, permissions: spec.manifest.permissions.clone(),
        pending: Default::default(), snapshot, table: ResourceTable::new(), wasi, limits,
        budget: Arc::new(Budget::default()), next_id: 0,
    });
    store.limiter(|s| &mut s.limits);
    store
}
```

Остаются WASI-интерфейсы, которые нужны стандартной библиотеке Rust для `wasm32-wasip2` (вывод, часы, случайные числа), но без папок и сети они ничего не открывают. Что именно разрешено в импортах модуля, проверяет `WasmVerifier` (раздел 6).

### 4.3 Дедлайны: epoch interruption и время ожидания хоста

Тик эпохи происходит раз в 10 мс. Вместо того чтобы возобновлять плагин на каждом тике (как делает Zed через `epoch_deadline_async_yield_and_update`), мы на каждом тике решаем, не вышел ли бюджет, и если да, прерываем вызов.

```rust
pub struct Budget {
    started_ms: AtomicU64,
    limit_ms: AtomicU64,           // 0 после interrupt(): ближайший тик прервёт вызов
    host_wait_ms: AtomicU64,       // время, проведённое в ожидании ответа хоста
}

fn begin_call(store: &mut Store<WasmState>, budget_ms: u64) {
    let b = store.data().budget.clone();
    b.started_ms.store(now_ms(), Ordering::Relaxed);
    b.limit_ms.store(budget_ms, Ordering::Relaxed);
    b.host_wait_ms.store(0, Ordering::Relaxed);
    store.set_epoch_deadline(1);
    store.epoch_deadline_callback(|ctx| {
        let b = &ctx.data().budget;
        let spent = now_ms()
            .saturating_sub(b.started_ms.load(Ordering::Relaxed))
            .saturating_sub(b.host_wait_ms.load(Ordering::Relaxed));      // ожидание хоста не считается
        if spent > b.limit_ms.load(Ordering::Relaxed) {
            Err(anyhow::anyhow!(wasmtime::Trap::Interrupt))               // вызов прерван (тип ошибки уточнить по версии)
        } else {
            Ok(wasmtime::UpdateDeadline::Continue(1))                     // проверить снова на следующем тике
        }
    });
}
```

Асинхронный вызов хоста из плагина не должен «съедать» бюджет, поэтому время ожидания накапливается в `host_wait_ms`:

```rust
impl WasmState {
    async fn call_host(&mut self, request: ApiRequest) -> ApiResponse {
        self.next_id += 1;
        let id = self.next_id;
        let (tx, rx) = futures::channel::oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        self.host.submit(ApiCall { plugin: self.id.clone(), id, request });

        let t0 = Instant::now();
        let resp = rx.await.unwrap_or(ApiResponse::Err { code: "closed".into(), message: "host closed".into() });
        self.budget.host_wait_ms.fetch_add(t0.elapsed().as_millis() as u64, Ordering::Relaxed);
        resp
    }
}
```

**Важно:** запрос к хосту должен иметь собственный таймаут (30 с, как в JS), иначе плагин может бесконечно ждать ответа. При `shutdown` все ожидающие `oneshot` отбрасываются, и плагин получает ошибку `closed`.

### 4.4 Импорты хоста (реализация WIT)

```rust
wasmtime::component::bindgen!({
    path: "wit",
    world: "plugin-world",
    async: true,           // в новых версиях wasmtime синтаксис другой (например, imports: { default: async }), уточните
});

impl aquilum::plugin::host::Host for WasmState {
    async fn vault_read(&mut self, path: String) -> Result<String, ApiError> {
        let req = ApiRequest::VaultRead { path };
        self.permissions.allow(&req)?;                          // core §12.3
        self.call_host(req).await.into_result()
    }
    async fn notice(&mut self, text: String, timeout_ms: u32) {
        let _ = self.call_host(ApiRequest::Notice { text, timeout_ms }).await;
    }
    fn active_file(&mut self) -> Option<FileInfo> {             // чтение снимка, без канала
        self.snapshot.load().active_file.clone().map(Into::into)
    }
    // ...остальные импорты: каждый строится как ApiRequest и проходит Permissions::allow
}
```

Правило: **каждый импорт, который что-то меняет или читает вне снимка, сначала проходит `Permissions::allow`**, и только потом уходит в `HostApi`. Это те же проверки, что и у JS, поэтому поведение прав одинаково для обоих рантаймов.

### 4.5 `WasmRuntime`: вызовы в плагин

```rust
pub enum PluginCall {
    OnLoad(oneshot::Sender<Result<(), String>>, u64),                // u64: бюджет, мс
    Event(HostEvent),
    RunCommand(String, oneshot::Sender<Result<(), String>>),
    Render(u64, oneshot::Sender<Result<UiTree, String>>),
    Unload(oneshot::Sender<()>),
}

pub struct WasmRuntime {
    tx: UnboundedSender<PluginCall>,
    pending: PendingResponses,
    budget: Arc<Budget>,
    stats: Arc<RuntimeStatsCell>,
    task: Option<std::thread::JoinHandle<()>>,
}

impl PluginRuntime for WasmRuntime {
    fn kind(&self) -> RuntimeKind { RuntimeKind::Wasm }

    fn send_event(&self, ev: HostEvent) {
        match ev {
            // ответы хоста разрешают ожидающий вызов прямо здесь, плагин продолжает выполнение
            HostEvent::Response { id, result, ok } => {
                if let Some(tx) = self.pending.lock().unwrap().remove(&id) {
                    let _ = tx.send(ApiResponse::from_parts(ok, result));
                }
            }
            other => { let _ = self.tx.unbounded_send(PluginCall::Event(other)); }
        }
    }

    fn interrupt(&self) { self.budget.limit_ms.store(0, Ordering::Relaxed); }   // ближайший тик прервёт вызов
    fn stats(&self) -> RuntimeStats { self.stats.snapshot() }

    fn shutdown(mut self: Box<Self>, grace: Duration) {
        let (tx, rx) = oneshot::channel();
        let _ = self.tx.unbounded_send(PluginCall::Unload(tx));      // onunload под бюджетом grace
        let _ = block_on_timeout(rx, grace);
        self.pending.lock().unwrap().clear();                        // отбросить ожидающие вызовы
        self.tx.close_channel();
        if let Some(t) = self.task.take() { let _ = t.join(); }
    }
}
```

Цикл задачи плагина (каждый плагин на своём потоке с маленьким `tokio` current-thread рантаймом, потому что `wasmtime-wasi` использует tokio, как это видно в Zed):

```rust
async fn plugin_loop(mut store: Store<WasmState>, world: PluginWorld, mut rx: UnboundedReceiver<PluginCall>) {
    while let Some(call) = rx.next().await {
        match call {
            PluginCall::OnLoad(reply, budget_ms) => {
                begin_call(&mut store, budget_ms);
                let r = world.aquilum_plugin_plugin().call_on_load(&mut store).await;
                let _ = reply.send(handle(&mut store, r));
            }
            PluginCall::Event(ev) => {
                begin_call(&mut store, 100);
                let r = world.aquilum_plugin_plugin().call_on_event(&mut store, &to_wit(ev)).await;
                let _ = handle(&mut store, r);
            }
            // RunCommand, Render, Unload устроены так же
            _ => {}
        }
    }
}

/// Ошибка вызова это страйк. Таймаут и trap считаются отдельно.
fn handle<T>(store: &mut Store<WasmState>, r: wasmtime::Result<T>) -> Result<T, String> {
    match r {
        Ok(v) => Ok(v),
        Err(e) => {
            store.data().stats.add_strike(classify_trap(&e));       // Timeout, Memory, Stack, Other
            Err(e.to_string())
        }
    }
}
```

**После trap экземпляр компонента нельзя использовать повторно** (насколько мне известно, в компонентной модели он помечается как непригодный для новых вызовов). Поэтому trap это не просто страйк: хост перезапускает плагин (новый `Store` и экземпляр, заново `on-load`) или отключает его, если страйков уже три за минуту (core §12.2).

### 4.6 Кэш компиляции

Компиляция больших модулей занимает время. Zed хранит артефакты инкрементальной компиляции в кэше `moka` размером до 32 МБ. Мы можем сохранять результат `Engine::precompile_component` в папке данных приложения по ключу «`sha256` бандла + версия `wasmtime`».

**Безопасность:** загрузка предкомпилированного артефакта (`Component::deserialize`) небезопасна, потому что это готовый машинный код. Загружать можно только артефакты, созданные самим приложением и лежащие вне vault и вне папки плагина. Артефакт никогда не берётся из каталога или из архива плагина.

---

## 5. Rust SDK

Крейт `aquilum_plugin` (по образцу `zed_extension_api` с макросом `register_extension!`, Zed §2):

```
sdk-rust/
├─ Cargo.toml          # crate aquilum_plugin
├─ wit/plugin.wit      # те же WIT-файлы
└─ src/lib.rs          # wit_bindgen::generate!, trait Plugin, register_plugin!, ui-билдер
```

```rust
// aquilum_plugin/src/lib.rs
wit_bindgen::generate!({ path: "wit", world: "plugin-world", pub_export_macro: true });

pub use aquilum::plugin::host::*;          // vault_read, notice, register_command, ...

pub trait Plugin {
    fn new() -> Self where Self: Sized;
    fn on_load(&mut self) -> Result<(), String> { Ok(()) }
    fn on_unload(&mut self) {}
    fn on_event(&mut self, _event: HostEvent) {}
    fn run_command(&mut self, _id: &str) -> Result<(), String> { Ok(()) }
    fn render_view(&mut self, _view_id: u64) -> Result<UiTree, String> { Err("no views".into()) }
}

#[macro_export]
macro_rules! register_plugin {
    ($t:ty) => {
        // 1. статический экземпляр плагина и реализация экспортируемого интерфейса `plugin`
        // 2. export!(Component);
        // 3. версия API в custom section модуля (идея Zed: секция `zed:api-version`, 6 байт, major/minor/patch как u16 big-endian)
        #[unsafe(link_section = "aquilum:api-version")]      // в редакции 2021: #[link_section = "..."]
        pub static API_VERSION: [u8; 6] = [0, 1, 0, 0, 0, 0]; // 1.0.0
    };
}
```

Пример плагина (счётчик слов):

```rust
use aquilum_plugin as aq;

struct WordCount;

impl aq::Plugin for WordCount {
    fn new() -> Self { WordCount }

    fn on_load(&mut self) -> Result<(), String> {
        aq::register_command("show-count", "Show word count").map_err(|e| format!("{e:?}"))?;
        Ok(())
    }

    fn run_command(&mut self, id: &str) -> Result<(), String> {
        if id == "show-count" {
            if let Some(file) = aq::active_file() {
                let text = aq::vault_read(&file.path).map_err(|e| format!("{e:?}"))?;
                aq::notice(&format!("{} words", text.split_whitespace().count()), 3000);
            }
        }
        Ok(())
    }
}

aq::register_plugin!(WordCount);
```

```toml
# Cargo.toml плагина
[lib]
crate-type = ["cdylib"]          # как у расширений Zed

[dependencies]
aquilum_plugin = "1"
```

Сборка (Zed использует ту же цель для своих расширений): `cargo build --release --target wasm32-wasip2`. Результат копируется как `main.wasm`, в манифесте `"runtime": "wasm"`, `"main": "main.wasm"`.

Билдер дерева UI в SDK скрывает плоский массив: автор пишет `ui::column([ui::text("Задачи").heading(), ui::button("add", "Добавить").primary()])`, а билдер сам раскладывает узлы по массиву и расставляет индексы детей.

---

## 6. Проверка WASM-плагина перед запуском

Общая схема, политика, кэш и boot guard описаны в core §6.1 и core §12.14. Здесь `WasmVerifier`.

### 6.1 Статическая проверка модуля

| Проверка | Как | Результат |
|---|---|---|
| Размер | `main.wasm` не больше 20 МБ | ошибка |
| Структура | `wasmparser::Validator` целиком до использования `wasmtime` (Zed тоже разбирает весь модуль заранее, чтобы невалидные байты давали ошибку, а не панику внутри `wasmtime`) | ошибка |
| Версия API | custom section `aquilum:api-version` есть и совпадает с `apiVersion` манифеста | ошибка |
| Компиляция | `Component::from_binary` в отдельном потоке с ограничением времени (враждебный модуль может компилироваться долго) | ошибка |
| Экспорты | есть интерфейс `aquilum:plugin/plugin@1.x`, нет лишних экспортов | ошибка |
| Импорты | только `aquilum:plugin/host@1.x` и разрешённые WASI-интерфейсы: `wasi:cli`, `wasi:clocks`, `wasi:random`, `wasi:io`. `wasi:sockets` и `wasi:http` запрещены. `wasi:filesystem` даёт предупреждение (без папок он ничего не открывает, но плагин, который его использует, скорее всего сломан) | ошибка или предупреждение |
| Память | если объявленный минимум памяти встроенного модуля удаётся прочитать и он больше лимита | ошибка |

### 6.2 Код

```rust
pub struct WasmVerifier { engine: Engine }

const ALLOWED_WASI: &[&str] = &["wasi:cli/", "wasi:clocks/", "wasi:random/", "wasi:io/"];
const DENIED_WASI:  &[&str] = &["wasi:sockets/", "wasi:http/"];

impl RuntimeVerifier for WasmVerifier {
    fn static_check(&self, manifest: &Manifest, bundle: &[u8], r: &mut VerifyReport) {
        // 1. структура модуля без выполнения
        if let Err(e) = wasmparser::Validator::new().validate_all(bundle) {
            return r.error("wasm.invalid", e);
        }
        // 2. версия API из custom section должна совпасть с манифестом
        match read_api_version_section(bundle) {
            Some(v) if v == manifest.api_version => {}
            Some(v) => r.error("api.mismatch", format!("section {v}, manifest {}", manifest.api_version)),
            None => return r.error("api.missing", "no aquilum:api-version section"),
        }
        // 3. компиляция и разбор импортов и экспортов
        let component = match compile_with_timeout(&self.engine, bundle, Duration::from_secs(10)) {
            Ok(c) => c,
            Err(e) => return r.error("wasm.compile", e),
        };
        let ty = component.component_type();                       // имена типов и методов уточнить по версии
        for (name, _) in ty.imports(&self.engine) {
            if name.starts_with("aquilum:plugin/host") { continue; }
            if DENIED_WASI.iter().any(|p| name.starts_with(p)) { r.error("import.denied", name); }
            else if name.starts_with("wasi:filesystem/") { r.warn("import.filesystem", name); }
            else if !ALLOWED_WASI.iter().any(|p| name.starts_with(p)) { r.error("import.unknown", name); }
        }
        if !ty.exports(&self.engine).any(|(n, _)| n.starts_with("aquilum:plugin/plugin")) {
            r.error("export.missing", "no aquilum:plugin/plugin export");
        }
    }

    fn dry_run(&self, manifest: &Manifest, bundle: &[u8], host: Arc<MockHost>, lim: &Limits, r: &mut VerifyReport) {
        // тот же Engine, Store, лимиты и импорты, что и в бою (раздел 4), но host = MockHost
        let mut p = WasmSandbox::spawn(&self.engine, bundle, host.clone(), lim);

        if let Err(e) = p.call_on_load(lim.onload_ms) {
            return r.error("onload.failed", e);      // таймаут, нехватка памяти, trap, ошибка плагина
        }
        let regs = host.registrations();
        if regs.commands.len() > lim.max_registrations { r.error("limit.commands", "too many commands"); }

        for view in &regs.views {
            match p.call_render(view, lim.call_ms).and_then(|t| tree_from_wit(&t).map_err(|e| e.to_string())) {
                Ok(tree) => {
                    if let Err(e) = validate_tree(&tree) { r.error("view.invalid_tree", e.to_string()); continue; }
                    for id in tree.button_ids() { let _ = p.send_ui_event(view, &id, "click", None, lim.call_ms); }
                    for id in tree.input_ids()  { let _ = p.send_ui_event(view, &id, "change", Some("test"), lim.call_ms); }
                }
                Err(e) => r.error("view.render", e),
            }
        }
        for ev in &regs.subscriptions { if let Err(e) = p.fire_sample_event(ev, lim.call_ms) { r.warn("event.failed", e); } }
        for cmd in &regs.commands      { if let Err(e) = p.call_command(cmd, lim.call_ms)     { r.warn("command.failed", e); } }

        p.call_on_unload(lim.call_ms).ok();
        r.stats = p.stats();
    }
}
```

**Разница с JS:** в WASM нет «подозрительных идентификаторов» и обфускации в привычном смысле. Статическая проверка опирается на структуру модуля и импорты, а читаемость исходника обеспечивают правила каталога (раздел 7).

---

## 7. Безопасность

| Вопрос | JS | WASM |
|---|---|---|
| Доступ к файлам | нет `fs`, только `vault.*` | нет WASI-папок, только `host.vault-*` |
| Сеть | нет `fetch`, только `requestUrl` по allowlist | нет сокетов, только `host.http-request` по allowlist |
| Динамический код | `Eval` не выдаётся | не применимо (модуль уже скомпилирован) |
| Память | `set_memory_limit` | `StoreLimits`, `trap_on_grow_failure` |
| Время | interrupt handler | epoch interruption |
| Стек | `set_max_stack_size` | `max_wasm_stack` |
| Баги движка | QuickJS на C | `wasmtime` на Rust, но не безошибочен |
| Анализ исходника | бандл читаем | бинарник нечитаем |

**Требования к каталогу для WASM-плагинов:**
- ссылка на открытый исходник и лицензию;
- воспроизводимая сборка: релиз собирается в CI репозитория, а `sha256` артефакта записан в каталоге (core, этап 8);
- проверка в CI каталога тем же `plugin-verify` (`WasmVerifier`);
- запрет предкомпилированных артефактов в релизе (раздел 4.6).

---

## 8. Цикл разработки автора

- `aquilum-plugin create --lang rust` создаёт шаблон (манифест, `Cargo.toml`, `src/lib.rs`, WIT).
- `cargo build --release --target wasm32-wasip2`, затем `aquilum-plugin check` (проверка из раздела 6).
- `aquilum-plugin dev`: сборка при изменении и установка в тестовый vault. В приложении можно «Установить плагин для разработки» из папки: идея из Zed (Zed §2, «Install Dev Extension»), установленная версия временно перекрывает опубликованную.
- Панику плагина хост превращает в trap с сообщением в консоли плагина, а не в падение приложения.
- Горячая перезагрузка по изменению `main.wasm` (core §12.10).

---

## 9. Тесты

| Тест | Ожидание |
|---|---|
| Бесконечный цикл в `on-load` и в `on-event` | Прервано по бюджету за время порядка тика (10 мс) плюс бюджет, интерфейс не страдает, страйк |
| Рост памяти до лимита | Вызов завершён ошибкой, приложение живо |
| Переполнение стека | trap, страйк |
| Ожидание ответа хоста дольше бюджета | Плагин не прерывается по времени из-за ожидания хоста (`host_wait_ms`), но запрос завершается по таймауту 30 с |
| Попытка импортировать `wasi:sockets` | `plugin check` отклоняет модуль |
| Модуль с неверной или отсутствующей `aquilum:api-version` | Отклонён |
| Дерево с циклом или общим узлом, глубина больше 32, больше 2000 узлов | Отклонено |
| `shutdown` во время ожидания хоста | Ожидающие `oneshot` отброшены, поток завершён |
| Повторный вызов после trap | Не выполняется, плагин перезапущен или отключён |
| Права: `vault-modify` без права `write` | `denied`, как у JS |
| Одинаковое поведение JS и WASM на эталонных плагинах | Те же регистрации, те же запросы к хосту, те же отказы по правам |

---

## 10. Этап 9: задачи

- [ ] Заморозить API v1 и сгенерировать WIT из типов `plugin-api` (или тест соответствия)
- [ ] `plugin-runtime-wasm`: `WasmRuntimeFactory`, `WasmRuntime`, цикл плагина, `wasmtime` с эпохами и лимитами
- [ ] Импорты `host` с проверкой прав, общий `HostApi`
- [ ] Преобразование `ui-tree` в `UiNode`, проверка циклов и лимитов
- [ ] `WasmVerifier`: статическая проверка и пробный запуск
- [ ] Rust SDK `aquilum_plugin`: макрос, секция версии API, билдер UI
- [ ] CLI: `create --lang rust`, `check`, `dev`
- [ ] 2–3 эталонных плагина (счётчик слов, тяжёлая обработка, панель) и сравнение поведения с JS
- [ ] Документация: «Первый WASM-плагин за 30 минут»
- [ ] Правила каталога для WASM (исходник, воспроизводимая сборка, `sha256`)

**Готово, когда:** плагин на Rust добавляет команду и панель, проходит `plugin check`, ведёт себя так же, как JS-плагин с теми же правами, а бесконечный цикл в нём не вешает приложение.

---

## 11. Риски и открытые вопросы

- **Стабильность WIT и инструментов.** Компонентная модель и `wit-bindgen` развиваются, имена и синтаксис меняются между версиями. Закрепить версии и иметь тест, который собирает эталонный плагин.
- **Асинхронные функции в компонентной модели.** Сейчас асинхронность реализована на стороне хоста (async host functions, как в Zed). Поведение и синтаксис в новых версиях `wasmtime` могут измениться.
- **Размер и время сборки** для авторов: объяснить в документации, кому WASM действительно нужен.
- **Другие языки.** Rust самый зрелый путь, остальные языки зависят от состояния их инструментов для компонентной модели. Заявлять поддержку только после проверки на практике.
- **Нужен ли WASM вообще в первой версии.** Если нет конкретных плагинов, которым не хватает JS, этап 9 можно отложить: контракт `PluginRuntime` (core §12.1) уже оставляет для него место.
