# Система плагинов. Часть 0a: адаптация текущего кода

Этот документ — **обязательный подготовительный этап** перед реализацией плагинов. Он описывает, что конкретно нужно изменить в текущем коде `aquilum-gpui`, чтобы архитектура из `plugins-00-core.md` стала реализуемой.

| Документ | О чём |
|---|---|
| `plugins-00a-adaptation.md` (этот) | что изменить в текущем коде перед плагинами |
| `plugins-00-core.md` | общее ядро плагинов и дорожная карта |
| `plugins-01-js-obsidian.md` | JS/TS-плагины (QuickJS) |
| `plugins-02-wasm.md` | WASM-плагины |
| `plugins-03-zed-research.md` | что изучено в Zed |
| `plugins-04-obsidian-compat.md` | совместимость с готовыми плагинами Obsidian (E0–E8) |

---

## 0. Текущее состояние кода: диагноз

### Что есть

```
aquilum-gpui/                     ← ОДИН крейт "vibecodium"
├─ Cargo.toml                     ← нет workspace, нет подкрейтов
├─ src/
│   ├─ main.rs                    ← Application::new().run(|cx| { cx.open_window(..., AppView::new) })
│   ├─ state.rs                   ← AppState + OpenedNote + FileNode + SessionState (~1950 строк)
│   ├─ settings.rs                ← AppConfig (11 секций), SettingsManager (RwLock<AppConfig>)
│   ├─ theme.rs                   ← Theme { bg_app, bg_sidebar, bg_editor, accent, ... }
│   ├─ markdown.rs                ← VisualRow, CodeBlockState, VisualRowHeightIndex, рендеринг
│   ├─ markdown_doc.rs            ← MarkdownDocument, MarkdownBlock, Span, CursorState
│   ├─ views/
│   │   ├─ app_view.rs            ← AppView — МОНОЛИТ (~5750 строк, 80+ полей)
│   │   ├─ right_sidebar.rs       ← пустая правая панель (80 строк)
│   │   ├─ sidebar/               ← file tree, search panel, context menus, vault dialog
│   │   ├─ settings_view.rs       ← рендер настроек
│   │   ├─ tab_list.rs            ← TabListPopup (единственный вторичный Entity)
│   │   ├─ tab_context_menu.rs
│   │   └─ ...
│   └─ ...
└─ vendor/gpui/                   ← пропатченный GPUI 0.2.2
```

### Главные проблемы для плагинов

| Проблема | Почему критично | Где |
|---|---|---|
| **Монолитный `AppView`** (80+ полей, ~5750 строк) | Невозможно создать `PluginView` как отдельный Entity и вставить в раскладку. Нет трейта для «вида». | `app_view.rs` |
| **Нет крейта `core`** | Vault-логика (open_note, save, file_tree, undo/redo) живёт в `state.rs` вместе с UI-состоянием. План требует `core` без GPUI. | `state.rs` |
| **Нет шины событий** | Нет `vault.on("modify")`, `workspace.on("file-open")`. Изменения файлов обрабатываются прямо в методах `AppState`. | `state.rs`, `app_view.rs` |
| **Нет модели раскладки** | Вкладки — `Vec<PathBuf>`, панели — `bool collapsed`. Нет Dock/Split/TabGroup. | `state.rs` |
| **Нет палитры команд** | `addCommand` некуда регистрировать. Есть только goto-search. | `app_view.rs` |
| **Ribbon захардкожен** | 6 кнопок в рендере без реестра | `app_view.rs` |
| **Settings — 12 статических вкладок** | `addSettingTab` невозможен | `settings_view.rs` |
| **Только 2 типа реализуют `Render`** | `AppView` и `TabListPopup`. Нет паттерна отдельных Entity-видов | Весь `src/views/` |

### Что уже хорошо

| Аспект | Детали |
|---|---|
| **Тема на токенах** | `Theme { accent, bg_sidebar, text_primary, ... }` — совпадает с требованием «стили из токенов» |
| **Пустая правая панель** | `render_right_sidebar()` — готовое место для видов плагинов |
| **Редактор** | `OpenedNote` имеет `insert_text()`, `line_col()`, `selection_range()`, `undo/redo` — ложится на Editor API |
| **MarkdownDocument** | `apply_edit()`, `CursorState`, `MarkdownBlock` со `Span` — фундамент для хуков |
| **SessionState persistence** | JSON в `app_data_dir` — совпадает с идеей workspace.json |
| **TabListPopup + EventEmitter** | Паттерн для плагинных Entity: отдельный `Entity<T>` с `EventEmitter`, подписка через `cx.subscribe_in` |
| **version: u64 в OpenedNote** | Поле есть, нужно только инкрементировать при правках |

---

## 1. Шаг 1: Выделение VaultCore из AppState

**Цель:** отделить vault-логику от UI-состояния. `VaultCore` не зависит от GPUI и будет основой для крейта `core`.

### 1.1 Что переносится в VaultCore

Из `AppState` (state.rs, строки 811–830):

```rust
// БЫЛО: всё в AppState
pub struct AppState {
    pub settings: Arc<SettingsManager>,
    pub workspace_path: Option<PathBuf>,       // ← vault
    pub workspace_name: String,                 // ← vault
    pub file_tree: Vec<FileNode>,               // ← vault
    pub expanded_folders: HashSet<PathBuf>,      // ← UI (но нужна vault-часть)
    pub search_query: String,                    // ← UI
    pub active_note: Option<OpenedNote>,         // ← vault (содержимое) + UI (курсор, скролл)
    pub open_tabs: Vec<PathBuf>,                 // ← UI
    pub active_empty_tab: Option<PathBuf>,       // ← UI
    pub nav_history: Vec<PathBuf>,               // ← UI
    pub nav_history_index: usize,                // ← UI
    pub status_text: String,                     // ← UI
    pub session: SessionState,                   // ← UI
    pub note_states: HashMap<PathBuf, NoteSettings>, // ← UI
    pub known_vaults: Vec<PathBuf>,              // ← UI
}
```

### 1.2 Новая структура

```rust
// ─── src/vault.rs (новый файл) ───

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Информация о файле, совместимая с TFile из Obsidian API.
/// Используется в Snapshot для синхронных геттеров плагинов (core §12.5).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileInfo {
    pub path: String,      // относительный путь от корня vault
    pub name: String,      // имя файла с расширением
    pub basename: String,  // имя без расширения
    pub extension: String, // "md", "txt", ...
}

impl FileInfo {
    pub fn from_path(root: &Path, abs: &Path) -> Option<Self> {
        let rel = abs.strip_prefix(root).ok()?;
        let name = abs.file_name()?.to_string_lossy().into_owned();
        let ext = abs.extension().unwrap_or_default().to_string_lossy().into_owned();
        let basename = abs.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        Some(Self {
            path: rel.to_string_lossy().replace('\\', "/"),
            name,
            basename,
            extension: ext,
        })
    }
}

/// Ядро vault без зависимости от GPUI.
/// Позже станет частью крейта `core`.
pub struct VaultCore {
    /// Абсолютный путь к корню vault.
    pub root: PathBuf,
    /// Имя vault (последний компонент пути).
    pub name: String,
}

impl VaultCore {
    pub fn open(path: PathBuf) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Workspace".to_owned());
        Self { root: path, name }
    }

    /// Прочитать файл из vault. Путь относительный.
    pub fn read_file(&self, rel_path: &str) -> Result<String, VaultError> {
        let abs = self.resolve(rel_path)?;
        std::fs::read_to_string(&abs).map_err(|e| VaultError::Io(e.to_string()))
    }

    /// Записать файл в vault. Путь относительный.
    pub fn modify_file(&self, rel_path: &str, data: &str) -> Result<(), VaultError> {
        let abs = self.resolve(rel_path)?;
        std::fs::write(&abs, data).map_err(|e| VaultError::Io(e.to_string()))
    }

    /// Создать файл. Возвращает абсолютный путь.
    pub fn create_file(&self, rel_path: &str, data: &str) -> Result<PathBuf, VaultError> {
        let abs = self.resolve(rel_path)?;
        if abs.exists() {
            return Err(VaultError::AlreadyExists(rel_path.to_string()));
        }
        if let Some(parent) = abs.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&abs, data).map_err(|e| VaultError::Io(e.to_string()))?;
        Ok(abs)
    }

    /// Удалить файл.
    pub fn delete_file(&self, rel_path: &str) -> Result<(), VaultError> {
        let abs = self.resolve(rel_path)?;
        std::fs::remove_file(&abs).map_err(|e| VaultError::Io(e.to_string()))
    }

    /// Переименовать файл. Возвращает новый абсолютный путь.
    pub fn rename_file(&self, old_rel: &str, new_rel: &str) -> Result<PathBuf, VaultError> {
        let old_abs = self.resolve(old_rel)?;
        let new_abs = self.resolve(new_rel)?;
        if let Some(parent) = new_abs.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::rename(&old_abs, &new_abs).map_err(|e| VaultError::Io(e.to_string()))?;
        Ok(new_abs)
    }

    /// Список markdown-файлов для Snapshot.
    pub fn list_markdown_files(&self) -> Vec<FileInfo> {
        let mut result = Vec::new();
        self.walk_dir(&self.root, &mut result);
        result
    }

    fn walk_dir(&self, dir: &Path, out: &mut Vec<FileInfo>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if !name.starts_with('.') {
                    self.walk_dir(&path, out);
                }
            } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
                if let Some(info) = FileInfo::from_path(&self.root, &path) {
                    out.push(info);
                }
            }
        }
    }

    /// Безопасное разрешение пути внутри vault.
    /// Блокирует выход за пределы vault через .., симлинки и абсолютные пути.
    /// Реализация из core §12.4 (по схеме Zed writeable_path_from_extension).
    fn resolve(&self, rel: &str) -> Result<PathBuf, VaultError> {
        use std::path::Component;

        if rel.contains('\0') {
            return Err(VaultError::InvalidPath("null byte".into()));
        }
        let rel_path = Path::new(rel);
        if rel_path.is_absolute() || rel_path.has_root() {
            return Err(VaultError::InvalidPath("absolute path".into()));
        }

        // Лексическая нормализация без обращения к ФС
        let mut stack: Vec<&std::ffi::OsStr> = Vec::new();
        for c in rel_path.components() {
            match c {
                Component::Normal(s) => stack.push(s),
                Component::CurDir => {}
                Component::ParentDir => {
                    if stack.pop().is_none() {
                        return Err(VaultError::InvalidPath("escapes vault root".into()));
                    }
                }
                Component::Prefix(_) | Component::RootDir => {
                    return Err(VaultError::InvalidPath("absolute component".into()));
                }
            }
        }
        let joined = stack.iter().fold(self.root.clone(), |p, s| p.join(s));

        // Раскрыть симлинки у ближайшего существующего предка
        let mut existing = joined.as_path();
        let mut tail: Vec<std::ffi::OsString> = Vec::new();
        let canon_prefix = loop {
            match existing.canonicalize() {
                Ok(p) => break p,
                Err(_) => {
                    tail.push(
                        existing
                            .file_name()
                            .ok_or_else(|| VaultError::InvalidPath("bad component".into()))?
                            .to_owned(),
                    );
                    existing = existing
                        .parent()
                        .ok_or_else(|| VaultError::InvalidPath("no parent".into()))?;
                }
            }
        };
        let mut resolved = canon_prefix;
        for part in tail.into_iter().rev() {
            resolved.push(part);
        }

        // Канонизировать корень для сравнения
        let root_canon = self
            .root
            .canonicalize()
            .map_err(|e| VaultError::Io(e.to_string()))?;
        if !resolved.starts_with(&root_canon) {
            return Err(VaultError::InvalidPath("escapes vault (symlink)".into()));
        }

        // Запрет записи в служебную папку
        if resolved.starts_with(root_canon.join(".vault")) {
            return Err(VaultError::InvalidPath("reserved .vault path".into()));
        }

        Ok(resolved)
    }
}

#[derive(Debug, Clone)]
pub enum VaultError {
    InvalidPath(String),
    AlreadyExists(String),
    NotFound(String),
    Io(String),
}

impl std::fmt::Display for VaultError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPath(s) => write!(f, "invalid path: {s}"),
            Self::AlreadyExists(s) => write!(f, "already exists: {s}"),
            Self::NotFound(s) => write!(f, "not found: {s}"),
            Self::Io(s) => write!(f, "io error: {s}"),
        }
    }
}
```

### 1.3 Изменения в AppState

```rust
// ─── state.rs ─── (ИЗМЕНЕНИЯ)

pub struct AppState {
    pub settings: Arc<crate::settings::SettingsManager>,
    pub vault: Option<crate::vault::VaultCore>,    // ← БЫЛО: workspace_path + workspace_name
    pub file_tree: Vec<FileNode>,
    // ... остальные UI-поля без изменений
}

impl AppState {
    pub fn open_workspace(&mut self, path: PathBuf) {
        let vault = crate::vault::VaultCore::open(path);
        self.workspace_name = vault.name.clone();          // ← для совместимости с UI
        self.file_tree = Self::build_tree(&vault.root);
        self.vault = Some(vault);
    }

    /// Путь к корню vault (для совместимости с существующим кодом).
    pub fn workspace_path(&self) -> Option<&Path> {
        self.vault.as_ref().map(|v| v.root.as_path())
    }
}
```

### 1.4 Порядок миграции (инкрементальный)

1. Создать `src/vault.rs` с кодом выше
2. Добавить `pub mod vault;` в `main.rs`
3. Добавить поле `vault: Option<VaultCore>` в `AppState`, оставив `workspace_path` временно
4. Перенаправить `open_workspace()` на `VaultCore::open()`
5. Постепенно заменять `std::fs::read_to_string` на `vault.read_file()` в `open_note_internal()`
6. Постепенно заменять `std::fs::write` на `vault.modify_file()` в `save_active_note()`
7. Удалить `workspace_path` и `workspace_name` после полной миграции

---

## 2. Шаг 2: Шина событий

**Цель:** появляется возможность подписки на `vault.on("modify")`, `workspace.on("file-open")`.

### 2.1 Типы событий

```rust
// ─── src/events.rs (новый файл) ───

/// События vault (файловые операции).
/// Рассылаются всем подписчикам после каждого изменения.
#[derive(Debug, Clone)]
pub enum VaultEvent {
    /// Файл изменён (сохранение заметки).
    Modify { path: String },
    /// Файл создан.
    Create { path: String },
    /// Файл удалён.
    Delete { path: String },
    /// Файл переименован.
    Rename { old_path: String, new_path: String },
}

/// События рабочей области.
#[derive(Debug, Clone)]
pub enum WorkspaceEvent {
    /// Открыт файл (или None при закрытии последнего).
    FileOpen { path: Option<String> },
    /// Изменилась активная вкладка.
    ActiveLeafChange,
    /// Vault открыт или переключён.
    VaultOpen { path: String },
}
```

### 2.2 Шина событий

```rust
// ─── src/events.rs (продолжение) ───

use std::sync::{Arc, Mutex};

type VaultHandler = Box<dyn Fn(&VaultEvent) + Send + 'static>;
type WorkspaceHandler = Box<dyn Fn(&WorkspaceEvent) + Send + 'static>;

/// Шина событий. Живёт в AppState, вызывается из AppView при изменениях.
/// Позже плагины подписываются через registerEvent(app.vault.on("modify", cb)).
pub struct EventBus {
    vault_handlers: Mutex<Vec<(u64, VaultHandler)>>,
    workspace_handlers: Mutex<Vec<(u64, WorkspaceHandler)>>,
    next_id: Mutex<u64>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            vault_handlers: Mutex::new(Vec::new()),
            workspace_handlers: Mutex::new(Vec::new()),
            next_id: Mutex::new(1),
        }
    }

    fn next_id(&self) -> u64 {
        let mut id = self.next_id.lock().unwrap();
        let v = *id;
        *id += 1;
        v
    }

    /// Подписаться на события vault. Возвращает id для отписки.
    pub fn on_vault(&self, handler: VaultHandler) -> u64 {
        let id = self.next_id();
        self.vault_handlers.lock().unwrap().push((id, handler));
        id
    }

    /// Подписаться на события workspace.
    pub fn on_workspace(&self, handler: WorkspaceHandler) -> u64 {
        let id = self.next_id();
        self.workspace_handlers.lock().unwrap().push((id, handler));
        id
    }

    /// Отписаться.
    pub fn off(&self, handler_id: u64) {
        self.vault_handlers.lock().unwrap().retain(|(id, _)| *id != handler_id);
        self.workspace_handlers.lock().unwrap().retain(|(id, _)| *id != handler_id);
    }

    /// Вызывается при изменении файла в vault.
    pub fn emit_vault(&self, event: &VaultEvent) {
        let handlers = self.vault_handlers.lock().unwrap();
        for (_, handler) in handlers.iter() {
            handler(event);
        }
    }

    /// Вызывается при изменении workspace.
    pub fn emit_workspace(&self, event: &WorkspaceEvent) {
        let handlers = self.workspace_handlers.lock().unwrap();
        for (_, handler) in handlers.iter() {
            handler(event);
        }
    }
}
```

### 2.3 Врезка в существующий код

```rust
// ─── state.rs ─── (добавить поле)
pub struct AppState {
    pub events: Arc<crate::events::EventBus>,  // ← ДОБАВИТЬ
    // ...
}

impl AppState {
    pub fn new() -> Self {
        let events = Arc::new(crate::events::EventBus::new());
        // ... остальная инициализация
        Self { events, /* ... */ }
    }

    pub fn save_active_note(&mut self) {
        if let Some(note) = &self.active_note {
            if note.is_dirty {
                let path = note.path.clone();
                let content = note.content.clone();
                // ... существующий код сохранения ...

                // ДОБАВИТЬ: событие после сохранения
                let rel_path = self.vault.as_ref()
                    .and_then(|v| path.strip_prefix(&v.root).ok())
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_else(|| path.to_string_lossy().into_owned());
                self.events.emit_vault(&crate::events::VaultEvent::Modify {
                    path: rel_path,
                });
            }
        }
    }

    pub fn open_note(&mut self, path: PathBuf) {
        self.open_note_internal(path.clone());
        // ДОБАВИТЬ: событие file-open
        let rel = self.vault.as_ref()
            .and_then(|v| path.strip_prefix(&v.root).ok())
            .map(|p| p.to_string_lossy().replace('\\', "/"));
        self.events.emit_workspace(&crate::events::WorkspaceEvent::FileOpen {
            path: rel,
        });
    }
}
```

---

## 3. Шаг 3: Реестр команд и палитра команд

**Цель:** `addCommand` может зарегистрировать команду, палитра её показывает.

### 3.1 Реестр команд

```rust
// ─── src/commands.rs (новый файл) ───

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub type CommandCallback = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone)]
pub struct Command {
    pub id: String,
    pub name: String,
    /// None для встроенных команд, Some(plugin_id) для плагинных.
    pub owner: Option<String>,
    pub callback: CommandCallback,
    /// Горячая клавиша (если есть).
    pub hotkey: Option<String>,
}

/// Реестр команд. Общий для встроенных и плагинных.
pub struct CommandRegistry {
    commands: Mutex<HashMap<String, Command>>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: Mutex::new(HashMap::new()),
        }
    }

    /// Зарегистрировать команду. Возвращает id для снятия регистрации.
    pub fn register(&self, cmd: Command) -> String {
        let id = cmd.id.clone();
        self.commands.lock().unwrap().insert(id.clone(), cmd);
        id
    }

    /// Снять регистрацию.
    pub fn unregister(&self, id: &str) {
        self.commands.lock().unwrap().remove(id);
    }

    /// Снять все команды конкретного плагина.
    pub fn unregister_all_of(&self, plugin_id: &str) {
        self.commands
            .lock()
            .unwrap()
            .retain(|_, cmd| cmd.owner.as_deref() != Some(plugin_id));
    }

    /// Все команды (для палитры).
    pub fn all(&self) -> Vec<Command> {
        self.commands.lock().unwrap().values().cloned().collect()
    }

    /// Поиск по имени.
    pub fn search(&self, query: &str) -> Vec<Command> {
        let q = query.to_lowercase();
        self.commands
            .lock()
            .unwrap()
            .values()
            .filter(|cmd| cmd.name.to_lowercase().contains(&q) || cmd.id.contains(&q))
            .cloned()
            .collect()
    }

    /// Выполнить команду по id.
    pub fn execute(&self, id: &str) -> bool {
        if let Some(cmd) = self.commands.lock().unwrap().get(id) {
            (cmd.callback)();
            true
        } else {
            false
        }
    }
}
```

### 3.2 Регистрация встроенных команд

```rust
// ─── state.rs (в AppState::new) ───

pub struct AppState {
    pub commands: Arc<crate::commands::CommandRegistry>,  // ← ДОБАВИТЬ
    // ...
}

// Пример регистрации встроенных команд (вызывать в AppView::new):
fn register_builtin_commands(commands: &crate::commands::CommandRegistry) {
    // Потом в AppView::new callback'и будут замыканиями с WeakEntity<AppView>
    commands.register(crate::commands::Command {
        id: "app:new-note".into(),
        name: "New note".into(),
        owner: None,
        callback: Arc::new(|| { /* будет подключено через WeakEntity */ }),
        hotkey: Some("Ctrl+N".into()),
    });
    commands.register(crate::commands::Command {
        id: "app:open-folder".into(),
        name: "Open folder".into(),
        owner: None,
        callback: Arc::new(|| {}),
        hotkey: Some("Ctrl+O".into()),
    });
    commands.register(crate::commands::Command {
        id: "app:toggle-sidebar".into(),
        name: "Toggle sidebar".into(),
        owner: None,
        callback: Arc::new(|| {}),
        hotkey: Some("Ctrl+B".into()),
    });
    commands.register(crate::commands::Command {
        id: "app:save-note".into(),
        name: "Save note".into(),
        owner: None,
        callback: Arc::new(|| {}),
        hotkey: Some("Ctrl+S".into()),
    });
    // ... остальные встроенные команды
}
```

### 3.3 UI палитры команд

Палитра переиспользует существующий `TabListPopup` (он уже умеет показывать список с поиском и EventEmitter):

```rust
// ─── app_view.rs ─── (добавить метод)

impl AppView {
    /// Открыть палитру команд (Ctrl+Shift+P).
    pub fn open_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let commands = self.state.commands.all();
        let items: Vec<crate::views::tab_list::TabInfo> = commands
            .iter()
            .enumerate()
            .map(|(i, cmd)| crate::views::tab_list::TabInfo {
                id: i,
                label: cmd.name.clone(),
                detail: cmd.hotkey.clone(),
                is_active: false,
            })
            .collect();

        let popup = cx.new(|cx| {
            crate::views::tab_list::TabListPopup::new(
                items,
                None, // нет активного
                self.theme.clone(),
                Some("Command Palette".into()),
                None,
                None,
                cx,
            )
        });

        let cmds = commands.clone();
        let subscription = cx.subscribe_in(&popup, window, move |this, _popup, event, _window, cx| {
            match event {
                crate::views::tab_list::TabListEvent::Activate(idx) => {
                    if let Some(cmd) = cmds.get(*idx) {
                        this.state.commands.execute(&cmd.id);
                    }
                    this.tabs_popup = None;
                    this.tabs_popup_subscription = None;
                    cx.notify();
                }
                crate::views::tab_list::TabListEvent::Dismiss => {
                    this.tabs_popup = None;
                    this.tabs_popup_subscription = None;
                    cx.notify();
                }
                _ => {}
            }
        });

        self.tabs_popup = Some(popup);
        self.tabs_popup_subscription = Some(subscription);
        cx.notify();
    }
}
```

---

## 4. Шаг 4: Расширяемый ribbon и status bar

### 4.1 Ribbon

```rust
// ─── src/ribbon.rs (новый файл) ───

/// Иконка в ленте (ribbon). Добавляется через addRibbonIcon.
#[derive(Clone)]
pub struct RibbonAction {
    pub id: String,
    pub icon: String,      // имя иконки из icons.rs
    pub tooltip: String,
    pub owner: Option<String>,  // None = встроенный, Some = plugin_id
    pub callback: std::sync::Arc<dyn Fn() + Send + Sync>,
}

/// Элемент статус-бара. Добавляется через addStatusBarItem.
#[derive(Clone)]
pub struct StatusBarItem {
    pub id: String,
    pub text: String,
    pub owner: Option<String>,
    pub on_click: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
}
```

### 4.2 Врезка в AppState

```rust
pub struct AppState {
    // ... существующие поля ...
    pub ribbon_actions: Vec<crate::ribbon::RibbonAction>,
    pub status_bar_items: Vec<crate::ribbon::StatusBarItem>,
}
```

При рендере ribbon в `app_view.rs` итерировать `self.state.ribbon_actions` после встроенных кнопок.
При рендере status bar — `self.state.status_bar_items` после встроенной строки.

---

## 5. Шаг 5: Инкрементировать version в OpenedNote

Сейчас `version: u64` всегда 0. Нужно для плана §12.9 (версии документа, транзакции, `map_offset`).

```rust
// ─── state.rs (изменить OpenedNote) ───

impl OpenedNote {
    pub fn insert_text(&mut self, text: &str) {
        self.snapshot_undo();
        let _ = self.delete_selection();
        // ... существующий код ...
        self.version += 1;  // ← ДОБАВИТЬ
        self.sync_cursor();
    }

    pub fn delete_backwards(&mut self) {
        // ... существующий код ...
        self.version += 1;  // ← ДОБАВИТЬ
        self.sync_cursor();
    }

    pub fn delete_forwards(&mut self) {
        // ... существующий код ...
        self.version += 1;  // ← ДОБАВИТЬ
        self.sync_cursor();
    }

    pub fn undo(&mut self) {
        // ... существующий код ...
        self.version += 1;  // ← ДОБАВИТЬ
        self.sync_cursor();
    }

    pub fn redo(&mut self) {
        // ... существующий код ...
        self.version += 1;  // ← ДОБАВИТЬ
        self.sync_cursor();
    }
}
```

---

## 6. Шаг 6: Зависимости в Cargo.toml

```toml
# ─── Cargo.toml (ДОБАВИТЬ) ───

[dependencies]
# ... существующие ...
# Подготовка к плагинам:
arc-swap = "1"              # неблокирующий снимок состояния для Snapshot (core §12.5)
semver = "1"                # версионирование API
futures = "0.3"             # каналы для моста plugin ↔ host
```

Эти зависимости лёгкие и нужны уже на подготовительном этапе. Тяжёлые зависимости (rquickjs, wasmtime, oxc, sha2) добавляются на соответствующих этапах плана.

---

## 7. Исправления в планах (plugins-00-core.md)

### 7.1 §12.7 Рендер UI: исправить имена токенов темы

В эскизе `render_node` используются несуществующие функции `theme::gap()`, `theme::text()`, `theme::button_bg()`. Настоящие токены темы:

```rust
// БЫЛО (план §12.7):
UiNode::Text { value, style } => div()
    .text_color(theme::text(*style))     // ← НЕТ такой функции
    .child(value.clone())

// НАДО (реальные токены из theme.rs):
UiNode::Text { value, style } => div()
    .text_color(match style {
        TextStyle::Default => theme.text_primary,    // ← реальные поля Theme
        TextStyle::Secondary => theme.text_secondary,
        TextStyle::Muted => theme.text_muted,
        TextStyle::Heading => theme.accent,
    })
    .child(value.clone())

// БЫЛО:
.bg(theme::button_bg(*variant, *disabled))    // ← НЕТ

// НАДО:
.bg(match (variant, disabled) {
    (_, true) => theme.bg_hover,
    (Variant::Primary, false) => theme.accent,
    (Variant::Danger, false) => gpui::rgb(0xed8796),  // Catppuccin Red
    (Variant::Default, false) => theme.bg_card,
})
```

### 7.2 §12.2 Мост: поправить GPUI API

```rust
// БЫЛО (план §12.2):
cx.spawn(async move |cx| {
    while let Some(call) = api_rx.next().await {
        if host.update(cx, |host, cx| host.handle_api(call, cx)).is_err() {

// НАДО (реальный GPUI 0.2.2 API — замыкание cx.spawn принимает |this, cx|
// но start_bridge не метод Self, поэтому используем cx.spawn без this):
cx.spawn(async move |mut cx| {
    while let Some(call) = api_rx.next().await {
        let ok = cx.update(|cx| {
            if let Some(host_entity) = host.upgrade() {
                host_entity.update(cx, |host, cx| host.handle_api(call, cx));
            }
        });
        if ok.is_err() { break; }
    }
}).detach();
```

### 7.3 §12.6 Раскладка: учесть текущее устройство вкладок

План описывает `Split`, `TabGroup`, `Dock`. Текущая реальность:
- Вкладки: `open_tabs: Vec<PathBuf>`
- Сайдбары: `sidebar_collapsed: bool`, `right_sidebar_collapsed: bool`
- Нет split-панелей

**Рекомендация:** реализовывать раскладку **инкрементально**:

1. **Фаза 1:** Заменить `Vec<PathBuf>` на `Vec<TabItem>`:

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TabItem {
    File(PathBuf),
    Empty(PathBuf),       // sentinel __new_tab__<uuid>
    PluginView {
        plugin_id: String,
        view_type: String,
        view_id: u64,
        state: serde_json::Value,
    },
}
```

2. **Фаза 2:** Заменить `sidebar_collapsed: bool` на `Dock`:

```rust
pub struct Dock {
    pub open: bool,
    pub width: f32,
    pub tabs: Vec<TabItem>,
    pub active: usize,
}
```

3. **Фаза 3:** Добавить split в центральной области (позже, не блокирует плагины).

### 7.4 §12.5 Snapshot: адаптировать к реальной модели данных (с кэшированием)

Вместо обхода диска на UI-потоке (`list_markdown_files()`) при каждой публикации снимка список файлов кэшируется в `VaultCore` и обновляется **только по событиям создания, удаления и переименования файлов** от вотчера `notify`. При обычных правках текста снимок переиспользует существующий `Arc<[FileInfo]>`:

```rust
use std::sync::Arc;
use crate::vault::FileInfo;

/// Снимок состояния для синхронных геттеров плагинов.
pub struct Snapshot {
    pub version: u64,
    pub active_file: Option<FileInfo>,
    pub files: Arc<[FileInfo]>,
    pub meta: Arc<std::collections::HashMap<String, crate::vault::FileCache>>,
}

impl Snapshot {
    pub fn empty() -> Self {
        Self {
            version: 0,
            active_file: None,
            files: Arc::new([]),
            meta: Arc::new(std::collections::HashMap::new()),
        }
    }
}

// В AppState:
pub struct AppState {
    pub snapshot: Arc<arc_swap::ArcSwap<Snapshot>>,
    cached_files: Arc<[FileInfo]>, // Обновляется только по событиям ФС
}

impl AppState {
    /// Быстрое обновление активной заметки и версии (без обращения к диску)
    pub fn publish_snapshot(&self) {
        let active = self.active_note.as_ref().and_then(|note| {
            self.vault.as_ref().and_then(|v| {
                FileInfo::from_path(&v.root, &note.path)
            })
        });

        let current = self.snapshot.load();
        self.snapshot.store(Arc::new(Snapshot {
            version: self.active_note.as_ref().map(|n| n.version).unwrap_or(0),
            active_file: active,
            files: self.cached_files.clone(), // Дешёвое клонирование Arc
            meta: current.meta.clone(),
        }));
    }

    /// Обновление списка файлов (вызывается из вотчера notify в фоне)
    pub fn update_files_cache(&mut self, new_files: Vec<FileInfo>) {
        self.cached_files = new_files.into();
        self.publish_snapshot();
    }
}
```

---

## 8. Порядок выполнения

| Шаг | Что | Блокирует | Оценка |
|---|---|---|---|
| 1 | `vault.rs` — VaultCore с resolve_in_vault | Этап 1 плана | 2-3 дня |
| 2 | `events.rs` — шина событий + врезка в save/open | Этап 2 плана | 1-2 дня |
| 3 | `commands.rs` — реестр + палитра команд | Этап 2 плана | 2-3 дня |
| 4 | `ribbon.rs` — расширяемый ribbon + status bar | Этап 5 плана | 1 день |
| 5 | Инкремент version в OpenedNote | Этап 6 плана | 30 мин |
| 6 | Зависимости (arc-swap, semver, futures) | Этап 1 плана | 10 мин |
| 7 | Snapshot + ArcSwap | Этап 2 плана | 1-2 дня |
| 8 | TabItem enum вместо Vec<PathBuf> | Этап 5 плана | 2-3 дня |
| **Итого** | | | **~2 недели** |

После шагов 1–8 можно начинать **Этап 1** основного плана (core §7): типы `ApiRequest`, `HostEvent`, `PluginRuntime`, контракт рантайма, с последующим развёртыванием этапов запуска готовых плагинов Obsidian E0–E8 (`plugins-04-obsidian-compat.md`).

---

## 9. Что НЕ нужно делать сейчас

- **Полная декомпозиция AppView на Entity** — это дорого и не блокирует первые этапы плагинов. `PluginView` может быть Entity, вставленным в правую панель `AppView`, без полной декомпозиции.
- **Переход на workspace с подкрейтами** — можно начать с модулей внутри одного крейта (`vault.rs`, `events.rs`, `commands.rs`), а крейты выделить позже.
- **Модальный слой (ModalLayer)** — нужен только на этапе 5 плана. До этого момента модалки плагинов можно показывать через существующий if-механизм в `AppView::render`.
- **WASM-рантайм** — этап 9, зависит от заморозки API v1.

---

## 10. Детальный дизайн: GPUI 0.2.2 Bridge и безопасная обработка вызовов

В планах `plugins-00-core.md §12.2` код моста использовал абстрактные сигнатуры. Ниже приведена рабочая реализация для нашего vendored GPUI 0.2.2, учитывающая потокобезопасность (`Send + Sync`), модель обновления UI через `WeakEntity`, ограниченную очередь (`bounded channel`) и валидацию прав хостом:

```rust
// ─── src/plugins/bridge.rs (новый файл) ───

use std::sync::Arc;
use futures::channel::mpsc::{Receiver, Sender};
use futures::StreamExt;
use gpui::{App, AsyncApp, WeakEntity, Context};
use crate::views::app_view::AppView;
use crate::vault::resolve_in_vault;

pub type RequestId = u64;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ApiRequest {
    VaultRead { path: String },
    VaultModify { path: String, data: String },
    Notice { text: String, timeout_ms: u32 },
    OpenFile { path: String },
    SetViewTree { view_id: u64, tree: crate::plugins::ui::UiNode },
    Http { url: String, method: String, body: Option<String> },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApiCall {
    pub plugin_id: String,     // Заполняется хостом при чтении из канала плагина
    pub request_id: RequestId,
    pub request: ApiRequest,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HostEvent {
    Response {
        id: RequestId,
        ok: bool,
        result: serde_json::Value,
    },
    VaultModify { path: String },
    FileOpen { path: Option<String> },
    UiEvent {
        view_id: u64,
        node_id: String,
        event: String,
        value: Option<String>,
    },
    Unload,
}

/// Запуск фонового моста с ограниченной очередью сообщений (bounded channel: 128)
pub fn start_plugin_bridge(
    weak_app: WeakEntity<AppView>,
    mut api_rx: Receiver<ApiCall>,
    cx: &mut App,
) {
    cx.spawn(async move |mut cx| {
        while let Some(call) = api_rx.next().await {
            let handle_result = cx.update(|cx| {
                if let Some(app) = weak_app.upgrade() {
                    app.update(cx, |this, cx| {
                        this.handle_plugin_call(call, cx);
                    });
                    true
                } else {
                    false
                }
            });

            if matches!(handle_result, Ok(false) | Err(_)) {
                break; // Окно закрыто или AppView уничтожен
            }
        }
    })
    .detach();
}

impl AppView {
    /// Обработка запроса плагина на главном потоке GPUI с проверкой прав
    pub fn handle_plugin_call(&mut self, call: ApiCall, cx: &mut Context<Self>) {
        // 1. Проверка прав плагина
        if let Some(perms) = self.plugin_manager.get_permissions(&call.plugin_id) {
            if let Err(e) = perms.allow(&call.request) {
                self.send_plugin_error(&call.plugin_id, call.request_id, e);
                return;
            }
        } else {
            return;
        }

        // 2. Диспетчеризация проверенного вызова
        match call.request {
            ApiRequest::Notice { text, timeout_ms: _ } => {
                // Token bucket: не более 10 вызовов в секунду
                if self.notice_rate_limiter.check(&call.plugin_id) {
                    self.state.status_text = format!("[{}] {}", call.plugin_id, text);
                    cx.notify();
                }
            }
            ApiRequest::OpenFile { path } => {
                // Безопасное открытие файла: строгая проверка через resolve_in_vault и vault >= read
                if let Some(vault) = &self.state.vault {
                    match resolve_in_vault(vault.root(), &path) {
                        Ok(safe_path) => self.select_note(safe_path, cx),
                        Err(_) => self.send_plugin_error(&call.plugin_id, call.request_id, "path escapes vault"),
                    }
                }
            }
            ApiRequest::VaultRead { path } => {
                // Чтение выполняется в background пуле, не блокируя UI
                if let Some(vault) = &self.state.vault {
                    let content = vault.read_file(&path);
                    self.send_plugin_response(&call.plugin_id, call.request_id, content);
                }
            }
            ApiRequest::Http { url, method, body } => {
                // Сетевой экран: проверка прав network, блокировка SSRF (localhost, RFC 1918)
                self.handle_plugin_http(&call.plugin_id, call.request_id, url, method, body, cx);
            }
            ApiRequest::SetViewTree { view_id, tree } => {
                self.update_plugin_view(&call.plugin_id, view_id, tree, cx);
            }
            _ => {}
        }
    }
}
```

---

## 11. Монтирование PluginView в правую панель (`right_sidebar.rs`)

В текущем `src/views/right_sidebar.rs` правая панель полностью пустая:
```rust
div().id("right-sidebar")...
```
Адаптируем её под отображение вкладок плагинов и рендеринг декларативного дерева `UiNode`.

```rust
// ─── src/plugins/ui.rs (дерево UI и рендер) ───

use gpui::{div, prelude::*, px, rgb, AnyElement, Context, ElementId, IntoElement, SharedString};
use crate::theme::Theme;
use crate::views::app_view::AppView;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiNode {
    Column { gap: Option<f32>, children: Vec<UiNode> },
    Row { gap: Option<f32>, children: Vec<UiNode> },
    Text { value: String, style: Option<String> },
    Button { id: String, label: String, variant: Option<String> },
    Input { id: String, placeholder: Option<String>, value: Option<String> },
    Separator,
}

pub fn render_ui_node(
    node: &UiNode,
    plugin_id: &str,
    view_id: u64,
    theme: &Theme,
    cx: &Context<AppView>,
) -> AnyElement {
    match node {
        UiNode::Column { gap, children } => {
            let mut el = div().flex().flex_col();
            if let Some(g) = gap {
                el = el.gap(px(*g));
            }
            el.children(children.iter().map(|c| render_ui_node(c, plugin_id, view_id, theme, cx)))
                .into_any_element()
        }
        UiNode::Row { gap, children } => {
            let mut el = div().flex().flex_row().items_center();
            if let Some(g) = gap {
                el = el.gap(px(*g));
            }
            el.children(children.iter().map(|c| render_ui_node(c, plugin_id, view_id, theme, cx)))
                .into_any_element()
        }
        UiNode::Text { value, style } => {
            let color = match style.as_deref() {
                Some("muted") => theme.text_muted,
                Some("accent") => theme.accent,
                _ => theme.text_primary,
            };
            div()
                .text_color(color)
                .text_size(px(13.0))
                .child(value.clone())
                .into_any_element()
        }
        UiNode::Button { id, label, variant } => {
            let bg = match variant.as_deref() {
                Some("primary") => theme.accent,
                Some("danger") => rgb(0xed8796),
                _ => theme.bg_card,
            };
            let text_color = match variant.as_deref() {
                Some("primary") => theme.text_on_accent,
                _ => theme.text_primary,
            };
            let btn_id = id.clone();
            let p_id = plugin_id.to_string();

            div()
                .id(ElementId::NamedInteger(btn_id.clone().into(), view_id as usize))
                .px(px(8.0))
                .py(px(4.0))
                .rounded(px(4.0))
                .bg(bg)
                .text_color(text_color)
                .cursor(gpui::CursorStyle::PointingHand)
                .hover(|s| s.bg(theme.bg_hover))
                .child(label.clone())
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |this, _, _, cx| {
                    this.dispatch_plugin_ui_event(&p_id, view_id, &btn_id, "click", None, cx);
                }))
                .into_any_element()
        }
        UiNode::Separator => {
            div().w_full().h(px(1.0)).bg(theme.border_subtle).my(px(4.0)).into_any_element()
        }
        UiNode::Input { id, placeholder, value } => {
            div()
                .px(px(6.0))
                .py(px(3.0))
                .border_1()
                .border_color(theme.border)
                .rounded(px(3.0))
                .bg(theme.bg_editor)
                .text_color(theme.text_primary)
                .child(value.clone().unwrap_or_else(|| placeholder.clone().unwrap_or_default()))
                .into_any_element()
        }
    }
}
```

А в [`src/views/right_sidebar.rs`](file:///D:/Naben/Github/rust/Aquilum-NP/aquilum-gpui/src/views/right_sidebar.rs) добавляем отрисовку зарегистрированных видов:

```rust
// ─── Изменение в src/views/right_sidebar.rs ───

impl AppView {
    pub fn update_plugin_view(
        &mut self,
        plugin_id: String,
        view_id: u64,
        tree: crate::plugins::ui::UiNode,
        cx: &mut Context<Self>,
    ) {
        self.state.plugin_views.insert((plugin_id, view_id), tree);
        if self.right_sidebar_collapsed {
            self.right_sidebar_collapsed = false; // Автоматически раскрываем правую панель
        }
        cx.notify();
    }

    pub fn render_right_sidebar(&self, theme: &Theme, cx: &Context<Self>) -> impl IntoElement {
        let active_views: Vec<_> = self.state.plugin_views.iter().collect();

        div()
            .id("right-sidebar")
            .flex()
            .flex_col()
            .w(px(self.right_sidebar_width.clamp(160.0, 600.0)))
            .flex_shrink_0()
            .overflow_hidden()
            .h_full()
            .bg(theme.bg_sidebar)
            .border_l_1()
            .border_color(theme.border)
            .when(active_views.is_empty(), |this| {
                this.p_4().text_color(theme.text_muted).child("No active plugin panels")
            })
            .children(active_views.into_iter().map(|((p_id, v_id), tree)| {
                div()
                    .p_3()
                    .border_b_1()
                    .border_color(theme.border_subtle)
                    .child(crate::plugins::ui::render_ui_node(tree, p_id, *v_id, theme, cx))
            }))
    }
}
```

---

## 12. Хуки Markdown: асинхронные `registerMarkdownCodeBlockProcessor`

В Obsidian один из самых востребованных API — это кастомный рендеринг блоков кода:
```ts
this.registerMarkdownCodeBlockProcessor("dataview", (source, el, ctx) => { ... });
```
В текущем коде [`src/markdown.rs`](file:///D:/Naben/Github/rust/Aquilum-NP/aquilum-gpui/src/markdown.rs) распознаются стандартные fenced-блоки через `CodeBlockState`.

### Асинхронный пайплайн рендеринга (без зависания UI-потока):
1. **Синхронный вызов из рендерера строк недопустим**, так как поток плагина отвечает с асинхронной задержкой.
2. **Кэш по хэшу содержимого (Content-Hash Cache):**
   - Хост хранит кэш готовых деревьев: `code_block_cache: HashMap<u64, Arc<UiNode>>`, где ключ — `sha256(lang + source)`.
   - Если дерево есть в кэше — оно мгновенно рендерится в строке без задержки.
   - Если кэша нет — рендерится аккуратный скелетон/заглушка с высотой по умолчанию, а в поток плагина отправляется асинхронный запрос `RenderCodeBlock { lang, source }`.
3. **Обновление высоты строк (`VisualRowHeightIndex`):**
   - При получении готового `UiNode` кэш обновляется, и вызывается точечный пересчёт индекса высот `row_height_index.update_row(...)`, исключая прыжки скролла и дрожание текста при прокрутке.

---

## 13. Сохранение данных и настроек плагина (`loadData` / `saveData`)

В Obsidian каждый плагин сохраняет настройки в `.vault/plugins/<id>/data.json`.
Чтобы избежать прямого доступа к файловой системе, операции выполняются через `VaultCore` с контролем лимитов и безопасности:

```rust
// ─── src/vault.rs (дополнение к VaultCore) ───

impl VaultCore {
    /// Путь к файлу данных плагина: <vault>/.vault/plugins/<id>/data.json
    pub fn plugin_data_path(&self, plugin_id: &str) -> std::path::PathBuf {
        self.root.join(".vault").join("plugins").join(plugin_id).join("data.json")
    }

    pub fn load_plugin_data(&self, plugin_id: &str) -> Option<serde_json::Value> {
        let path = self.plugin_data_path(plugin_id);
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Сохранение с лимитом размера (макс. 5 МБ) и дебаунсом записи
    pub fn save_plugin_data(&self, plugin_id: &str, data: &serde_json::Value) -> Result<(), String> {
        let json = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
        
        // Лимит размера: не более 5 МБ на плагин
        if json.len() > 5 * 1024 * 1024 {
            return Err("Data size exceeds 5MB limit".into());
        }

        let path = self.plugin_data_path(plugin_id);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        
        // Атомарная запись через временный файл
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }
}
```

### Защита конфиденциальных данных (Секретов):
Чувствительные токены (токены GitHub, API-ключи OpenAI/Anthropic, пароли) **не должны сохраняться в `.vault/`**, так как хранилище синхронизируется через Git/облако.
Для них предоставляется отдельный API хоста:
`app.secretStorage.get(key) / set(key, value)`, сохраняющий данные в защищённом хранилище приложения в `AppData` пользователя.

---

## 14. Адаптация меню настроек (`settings_view.rs`) под Core & Community плагины

Текущий диалог настроек (`src/views/settings_view.rs`) оперирует фиксированным перечислением `SettingsTab` из 12 вкладок.
Для полноценной поддержки плагинов расширяем его по модели Obsidian.

### 14.1 Архитектура категорий и вкладок
Расширяем модель навигации:
```rust
// ─── src/views/settings_view.rs ───

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsTab {
    // Базовые настройки приложения
    Ui,
    Editor,
    Reader,
    Search,
    Templates,
    Files,
    Analysis,
    Mcp,
    History,
    Trash,
    System,
    Shortcuts,
    // Разделы управления плагинами
    CorePlugins,
    CommunityPlugins,
    // Динамические вкладки настроек активных плагинов
    PluginSettings(String), // plugin_id
}
```

### 14.2 Конфигурация активных плагинов в Vault
Состояние активности плагинов сохраняется в служебных файлах хранилища:
1. `<vault>/.vault/core-plugins.json` — список включённых встроенных модулей (например, `["command-palette", "word-count", "file-recovery"]`).
2. `<vault>/.vault/community-plugins.json` — список включённых сторонних плагинов (например, `["dataview", "calendar"]`).
При запуске `load_installed_plugins_for_vault` считывает этот файл и запускает изолированные потоки QuickJS **только для активированных плагинов**.

### 14.3 Интерфейс вкладки Community Plugins
- **Глобальный Safe Mode (Безопасный режим):** предупреждение о выполнении стороннего JS-кода.
- **Кнопка вызова каталога:** `[ Browse Community Plugins ]` (открывает модальное окно каталога).
- **Список установленных плагинов:**
  - Карточка каждого плагина: Название, Версия, Автор, Краткое описание.
  - Тумблер переключения активности (при клике вызывает `spawn_plugin_runtime` или `HostEvent::Unload`).
  - Кнопка настроек `[ ⚙ ]` (переключает активную вкладку на `SettingsTab::PluginSettings(id)`).
  - Кнопка удаления `[ 🗑 ]` (выгружает плагин и удаляет каталог из `.vault/plugins/<id>/`).

### 14.4 Декларативные настройки плагина (`PluginSettingTab`)
В рантайме плагин вызывает:
```ts
plugin.addSettingTab({
  render: () => ({
    type: "column",
    children: [
      { type: "text", value: "Параметры плагина", style: "heading" },
      { type: "input", id: "api_key", placeholder: "Введите токен", value: this.settings.apiKey },
      { type: "checkbox", id: "enable_sync", label: "Включить синхронизацию", checked: this.settings.sync }
    ]
  })
});
```
Хост рендерит это дерево внутри `render_settings_content` через `render_ui_node`, транслируя события изменения значений обратно в поток плагина.

---

---

## 15. Отдельное окно каталога плагинов (Community Plugins Window & Responsive Grid)

### 15.1 Архитектурный переход: отдельное окно ОС и жизненный цикл
Каталог плагинов выносится из модального оверлея в **полноценное отдельное окно операционной системы** на базе GPUI (`cx.open_window`):

1. **Обоснование и решение проблемы потери контекста:**
   - Чтение документации и конфигурация плагинов происходят рядом с заметками без блокировки редактора и перекрытия рабочего пространства.
   - Заголовок окна явно привязан к активному хранилищу: `«Сторонние плагины — {vault_name} — Aquilum»`.
   - Жизненный цикл строго синхронизирован: при закрытии главного окна приложения или переключении активного Vault окно каталога автоматически закрывается.
2. **Синглтон окна:**
   - Дескриптор `catalog_window: Option<gpui::WindowHandle<PluginCatalogView>>` хранится в `AppView`.
   - Повторный вызов каталога (из настроек, палитры команд или глубокой ссылки) не открывает дубликат, а активирует существующее окно и выводит его на передний план (`cx.activate_window()`).
   - При закрытии окна дескриптор очищается.
3. **Геометрия и мультимониторность:**
   - Фиксированный размер по умолчанию: **`1120 × 760 px`** (минимальный: `640 × 480 px`).
   - Позиция `(x, y)` и размеры сохраняются в `catalog-window.json`.
   - При открытии сохранённые координаты обязательно валидируются против текущих подключённых дисплеев (`cx.displays()`). Если второй монитор отключили, и сохранённое окно попадает за пределы видимости, оно автоматически центрируется на основном дисплее.
4. **Запасной вариант (Fallback для тайлинговых WM):**
   - В настройках приложения предусмотрен параметр `catalog_display_mode: Window | Overlay`. Пользователи Linux с тайлинговыми WM (i3, sway, Hyprland) могут переключить каталог в режим модального оверлея.
5. **Изолированный модальный слой:**
   - Все вторичные диалоги (подтверждение прав, Safe Mode, отчёты об ошибках) открываются **внутри окна каталога**, не затрагивая главное окно заметок.

### 15.2 Источник данных каталога и статистика (без телеметрии)
Каталог использует двухфайловую схему, аналогичную Obsidian:
1. `community-plugins.json` (метаданные плагинов):
   ```json
   [
     {
       "id": "dataview",
       "name": "Dataview",
       "author": "blacksmithgu",
       "description": "Complex data queries and table views over your vault",
       "repo": "blacksmithgu/obsidian-dataview",
       "branch": "master",
       "tags": ["query", "table", "index"]
     }
   ]
   ```
2. `community-plugin-stats.json` (статистика релизов):
   ```json
   {
     "dataview": {
       "downloads": 5120000,
       "updated_at": 1712000000,
       "created_at": 1610000000
     }
   }
   ```
   - **Сбор статистики:** файл генерируется в репозитории каталога автоматическим GitHub Action раз в 6–12 часов через GitHub API (подсчёт скачиваний релизных ассетов).
   - В самом приложении **полностью отсутствует телеметрия**.
   - **Офлайн и скелетоны:** при загрузке отображаются анимированные скелетоны карточек. При отсутствии интернета загружается локальный кэш `.vault/catalog_cache.json` с пометкой «Данные от {date}» и кнопкой «Повторить попытку».

### 15.3 Адаптивная сетка карточек и виртуализация (Responsive Grid)
Свыше 3000 плагинов требуют строгой производительности (60/120 FPS) без скачков геометрии:

1. **Динамический расчёт колонок по минимальной ширине карточки:**
   Вместо жестких порогов брейкпоинтов используется формула с плавающим числом колонок:
   ```rust
   let min_card: f32 = 280.0;
   let gap: f32 = 12.0;
   let cols = ((available_width + gap) / (min_card + gap)).floor().max(1.0) as usize;
   let card_width = (available_width - gap * (cols - 1) as f32) / cols as f32;
   ```
   - При ширине 1120 px: 3 колонки.
   - При ширине 1600+ px (4K/Ultrawide): 4–5 колонок (как на скриншоте Obsidian).
   - При компактном окне (700 px): 2 колонки; при < 580 px: 1 колонка.
2. **Виртуализация строк (`uniform_list`):**
   - Высота карточки строго зафиксирована: **`170 px`**.
   - Описание плагина ограничивается 2–3 строками (`line_clamp(3)` + `text_ellipsis()`), что гарантирует идентичную высоту рядов и исключает дрожание скролла.
   - Полное описание без срезов и ограничений отображается на странице деталей.

### 15.4 Вкладки верхнего уровня и расширенные фильтры
Панель управления каталога включает:
1. **Вкладки первого уровня:**
   - `Все плагины` (полная база каталога).
   - `Установленные` (плагины текущего хранилища с быстрыми тумблерами включения/настроек).
   - `Обновления (N)` — список плагинов с доступными новыми версиями и кнопкой массового обновления **«Обновить все»**.
2. **Интеллектуальный поиск:**
   - Полноценный инпут с поддержкой выделения мышью, клавиатурных комбинаций (Ctrl+A/C/X/V, Shift+Стрелки), мигающего курсора и очистки `[ ✕ ]`.
   - Мультитерминовый поиск с приоритизацией совпадений в названии перед описанием и подсветкой терминов (`highlight_terms`).
3. **Фильтры безопасности и совместимости:**
   - По совместимости API: 🟢 Полная (100%) / 🟡 Частичная / 🔴 Несовместим.
   - По правам: «Без доступа к сети (Offline only)», «Только чтение Vault».
   - По рантайму: JS (QuickJS) / WASM.
   - По тегам: категории из записи каталога.
4. **Сортировка:**
   - По популярности (загрузкам из `stats.json`), по дате обновления, по названию.

### 15.5 Анатомия карточки и страница деталей (Drill-Down)

#### Карточка в сетке:
- **Шапка:** название плагина, компактный цветной индикатор совместимости (🟢/🟡/🔴), бейдж статуса (`УСТАНОВЛЕН` / `ОБНОВЛЕНИЕ`).
- **Свойства:** `Автор: <name>`, иконка сети `ICON_GLOBE` (если плагин запрашивает сеть), счётчик загрузок (`8.5M`) и дата релиза.
- **Описание:** лаконичный блок текста (до 3 строк) с подсветкой совпадений поиска.
- **Интерактивность:** подсветка рамки при наведении, переход в детали по клику или нажатию `Enter`.

#### Страница детального просмотра (Detail View):
- Кнопка возврата `[ ← Сторонние плагины ]` (сохраняет поисковую строку, выбранные фильтры и позицию скролла в сетке).
- Заголовочный блок: название, версия, автор, ссылка на GitHub-репозиторий и баг-трекер (Issues), лицензия.
- **Панель версий и Changelog:** текст последнего релиза из GitHub Releases, дата первого и последнего релизов, предупреждение, если плагин не обновлялся более 12 месяцев.
- **Панель безопасности (Permissions Card):** список запрашиваемых прав (`vault:read`, `vault:write`, внешние сетевые домены) и сводка отчёта `plugin-verify`.
- **Встроенный ридер README.md:**
  - Парсинг и рендеринг Markdown.
  - Безопасная загрузка изображений: кэширование, лимит размера 5 МБ.
  - Внешние ссылки открываются в системном браузере по умолчанию с предупреждением о переходе.

### 15.6 Сквозной поток безопасной установки
1. **Проверка Safe Mode:**
   - По умолчанию сторонние плагины заблокированы безопасным режимом.
   - При попытке первой установки открывается модальный диалог с разъяснением рисков исполнения стороннего JS-кода. Плагины загружаются только после явного подтверждения.
2. **Диалог прав перед установкой (с поддержкой diff):**
   - При первой установке: полный список запрашиваемых прав.
   - При обновлении плагина: диалог показывает только **разницу прав** («Новые права: сетевой доступ к api.github.com»). Если права не менялись — обновление происходит без лишних вопросов.
3. **Прогресс и валидация:**
   - Скачивание `manifest.json` и `main.js` во временную папку `.<id>.tmp`.
   - Проверка контрольной суммы SHA-256 и запуск экспресс-проверки `plugin-verify`.
   - При успехе: бэкап текущей версии в `.<id>.old` и атомарный перенос новой версии в `.vault/plugins/<id>/`.
4. **Завершение:**
   - Кнопка действия в окне каталога мгновенно меняется на `[ Включить сейчас ]`.
   - В случае ошибок новой версии доступна кнопка отката `[ Откатить на предыдущую версию ]`.

### 15.7 Клавиатурная навигация, доступность и входные точки
- `Ctrl/Cmd + F`: моментальный перевод фокуса в строку поиска.
- `Стрелки (← ↑ → ↓)`: перемещение фокуса по карточкам сетки.
- `Enter`: вход в карточку плагина (drill-down).
- `Alt + ←` или `Backspace` (когда поиск не активен): возврат назад к сетке карточек с восстановлением фокуса на той же карточке.
- `Ctrl/Cmd + W`: закрытие окна каталога.
- **Входные точки:**
  - Кнопка в окне настроек `[ Обзор плагинов ]`.
  - Команда палитры: `«Плагины: Обзор сторонних плагинов»`.
  - Глубокие ссылки URL-схемы: `aquilum://show-plugin?id={id}` (автоматически открывает каталог сразу на странице выбранного плагина).

### 15.8 Межоконное взаимодействие и синхронизация (GPUI 0.2)
1. Окно каталога связывается с родительским окном через `WeakEntity<AppView>`.
2. Окно подписывается на события смены темы оформления (`ThemeChanged`) и языка (`LocaleChanged`).
3. При установке/активации плагина в шину событий `EventBus` отправляется событие, и `AppView` горячим способом поднимает поток QuickJS без перезапуска всего приложения.

### 15.9 Нативная панель заголовка в стиле Obsidian (Custom Titlebar, Window Controls & Resizing)
Верхняя часть окна каталога плагинов оформляется в точном визуальном и функциональном стиле Obsidian (согласно предоставленному референсу):

1. **Анатомия и геометрия панели заголовка (Titlebar):**
   - **Фиксированная высота:** `36 px` (в уровень с вкладками редактора основного окна).
   - **Фон и граница:** фон `theme.bg_editor`, нижняя разделительная граница толщиной `1 px` (`theme.border_subtle`).
   - **Центрированный заголовок:** название `«Сторонние плагины — {vault_name} — Aquilum»` (в стиле Obsidian `«Настройки - Obsidian - Obsidian 1.14.4»`) центрируется по горизонтали (`text_sm`, `text_color(theme.text_secondary)`, `font_weight(FontWeight::NORMAL)`).
   - **Область перетаскивания (Window Drag):** вся свободная область панели помечается `.window_control_area(WindowControlArea::Drag)` для бесшовного перемещения окна курсором по экрану и между мониторами.
   - **Двойной клик:** двойное нажатие по панели заголовка переключает состояние развёрнутости окна (Maximize / Restore).

2. **Блок системных кнопок управления окном (Window Controls — правый угол):**
   - **Кнопка «Свернуть» (`win-ctrl-min`):**
     - Пиктограмма `—` (`icons::ICON_MINIMIZE`, 12 px).
     - Размер кликабельной области: `46 × 36 px`.
     - Hover: подсветка фоном `theme.bg_hover`.
     - Хит-тест: `.window_control_area(WindowControlArea::Min)` и вызов `window.minimize_window()`.
   - **Кнопка «Развернуть / Уменьшить размер» (`win-ctrl-max`):**
     - Динамическая иконка: `icons::ICON_RESTORE` (два наложенных квадрата), когда окно развёрнуто (`window.is_maximized()`), либо `icons::ICON_MAXIMIZE` (один квадрат), когда окно в плавающем режиме.
     - Размер кликабельной области: `46 × 36 px`.
     - Hover: подсветка фоном `theme.bg_hover`.
     - Хит-тест: `.window_control_area(WindowControlArea::Max)` и вызов `window.zoom_window()`.
   - **Кнопка «Закрыть» (`win-ctrl-close`):**
     - Пиктограмма `✕` (`icons::ICON_WINDOW_CLOSE`, 12 px).
     - Размер кликабельной области: `46 × 36 px`.
     - Hover: классический красный системный акцент `rgb(0xe81123)` и белый цвет иконки `rgb(0xffffff)`.
     - Хит-тест: `.window_control_area(WindowControlArea::Close)` и вызов `window.remove_window()`. При закрытии текущие координаты и состояние развёрнутости сохраняются в `catalog-window.json`.

3. **Свободное изменение размера (Растягивание / Window Resizing):**
   - **Минимальный размер:** `window_min_size: Some(size(px(640.0), px(480.0)))` предотвращает искажение интерфейса при сильном сжатии.
   - **Нативный ресайз граней:** поддержка захвата курсором мыши за все 4 грани (левую, правую, верхнюю, нижнюю) и 4 угла с отображением системных стрелок изменения размера.
   - **Адаптивность внутреннего содержимого (Responsive Flow):** при растягивании окна сетка карточек плагинов динамически пересчитывает число колонок (от 1 колонки при ширине < 580 px, 2 при 700 px, 3 при 1120 px, до 4–5 на Ultrawide/4K мониторах), гарантируя плавные 120 FPS без скачков геометрии.
   - **Персистентность:** состояние развёрнутости (`maximized`), позиция `(x, y)` и размеры `(width, height)` сохраняются при закрытии окна и восстанавливаются при следующем открытии.
