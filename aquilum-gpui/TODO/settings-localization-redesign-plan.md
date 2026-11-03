# План редизайна локализации окна настроек (`aquilum-gpui`)

Документ — продолжение `TODO/LOCALIZATION_PLAN.md` (этап 2 «Экстракция строк»).
Здесь закрывается остаток: подсистема плагинов, редактор панели инструментов и
поисковый реестр настроек, которые появились после первого прохода i18n и остались
на `if lang.starts_with("ru") { … } else { … }`.

Цель: **любая пользовательская строка окна настроек берётся из `locales/*.json`**.
Русский и английский перестают быть единственными языками; остальные 18 локалей
получают переведённые подписи, а не английский фолбэк.

---

## 1. Границы и инварианты

В scope:

- `src/views/settings_view/{tab,tabs,ui,plugin_toolbar}.rs`
- `src/settings.rs` — `SettingDescriptor` и `SETTINGS` (поиск по настройкам)
- `locales/*.json` — 20 файлов
- тесты: `src/views/settings_view/tests.rs`, `src/settings/tests.rs`, `src/i18n.rs` (parity)

Вне scope (фиксируем, но не трогаем):

- `is_ru` в остальных модулях (`ribbon_menu`, `command_palette`, `more_options_menu`,
  `editor_context_menu`, `plugin_catalog`, `app_view/*` — ещё ~22 места). Отдельная задача.
- `src/settings.rs:427` — дефолт `caption_template: "Рис. {n}. {text}"` и пресеты
  подписей в `tabs.rs:1353`. Это пользовательский контент документа, а не подпись UI.
- Логи `eprintln!("[settings] …")` — не пользовательские строки.
- Имена шрифтов (`Segoe UI`, `Consolas`), обозначения клавиш (`Ctrl + O`),
  имена файлов (`README.md`), hex-цвета, идентификаторы элементов (`btn-…`, `tb-…`).

Инварианты (AGENTS.md):

- Новые компоненты не создаются: меняются только строковые литералы на вызовы `t()/t_fmt()`.
- `id`, размеры, отступы, порядок элементов не меняются — внешний вид окна настроек
  остаётся прежним (проверяется скриншот-диффом в ru и en).
- Новых `render_*` не появляется; правки идут внутри существующих функций.
- Комментарии в коде — по-английски; пользовательские строки — только через i18n.

---

## 2. Целевая модель

### 2.1. API

Ничего нового в `i18n.rs` не требуется, используем существующее:

- `crate::i18n::t(key, lang)` — строка;
- `crate::i18n::t_fmt(key, lang, &["…"])` — позиционные `{}`;
- `crate::i18n::t_args(key, lang, &[("name", "…")])` — именованные `{name}`
  (для `Core plugin '{plugin}' updated`);
- на `SettingsView` уже есть `self.t()` и `self.t_fmt()` (`ui.rs:101–108`) — используем их.

`if is_ru` в модуле настроек не остаётся ни одного (сейчас 16 мест).

### 2.2. Схема ключей

Новые namespace-ветки в `locales/en.json`:

```jsonc
{
  "settings": {
    "tabs": {
      "core_plugins": "Core plugins",
      "community_plugins": "Community plugins",
      "plugin_settings": "Plugin settings",
      "editor_toolbar": "Editor Toolbar"
    },

    "nav": {
      "search_placeholder": "Search settings...",
      "search_empty": "No settings found"
    },

    "units": {
      "px": "{} px",
      "ch": "{} ch",
      "percent": "{}%",
      "mb": "{} MB"
    },

    "ui": {
      "font_weight_semibold": "Semibold",
      "font_weight_regular": "Regular"
    },

    "plugins": {
      "core": {
        "title": "Application Core Plugins",
        "status_updated": "Core plugin '{plugin}' updated",
        "command_palette": { "name": "Command palette", "desc": "Quickly search and execute application actions (Ctrl+Shift+P)" },
        "word_counter": { "name": "Word counter", "desc": "Displays character and word count in the status bar" },
        "editor_toolbar": { "name": "Editor toolbar", "desc": "Markdown formatting toolbar above the editor with customizable buttons" },
        "file_recovery": { "name": "File recovery", "desc": "Automatic local snapshotting to protect against data loss" }
      },

      "community": {
        "title": "Community Plugins",
        "safe_mode": "Safe Mode",
        "safe_mode_desc": "Blocks community plugins execution for system stability and data protection",
        "browse": "Browse community plugins",
        "catalog": "Community plugins catalog",
        "catalog_desc": "Discover, install and update community-created plugins",
        "compat_full": "100% Compatible",
        "compat_score": "{}% Compatible",
        "incompatible": "Incompatible",
        "compat_unknown": "Compatibility unknown",
        "installed": "Installed plugins ({})"
      },

      "detail": {
        "back": "Back to plugins",
        "enable": "Enable",
        "disable": "Disable",
        "reload": "Reload",
        "open_folder": "Open folder",
        "enabled": "Enabled",
        "disabled": "Disabled",
        "author": "By {}"
      },

      "metadata": {
        "title": "Metadata",
        "name": "Plugin Name",
        "id": "Plugin ID",
        "version": "Version",
        "min_app_version": "Minimum App Version",
        "author": "Author",
        "author_url": "Author URL",
        "desktop_only": "Desktop Only",
        "desktop_only_true": "true (desktop only)",
        "desktop_only_false": "false (cross-platform)",
        "install_path": "Installation path"
      },

      "readme": {
        "title": "PLUGIN INFORMATION",
        "missing": "No README.md file found",
        "hint": "No documentation file exists in plugin directory. Add a README.md file to display instructions here."
      },

      "config": {
        "title": "PLUGIN SETTINGS",
        "store": "Configuration store (.vault/plugins/<id>/data.json)",
        "card": "Plugin settings (data.json)",
        "card_short": "Settings"
      },

      "edit_title": "Edit"
    },

    "toolbar": {
      "back": "Back to core plugins",
      "title": "Editor Toolbar",
      "desc": "Customize formatting buttons, nested dropdown groups, and separators above the editor.",
      "add_button": "+ Button",
      "add_dropdown": "+ Dropdown ▾",
      "add_separator": "+ Separator",
      "reset": "Reset to defaults",
      "export": "Export JSON",
      "import": "Import JSON",
      "separator": "──────── (separator)",
      "dropdown_fallback": "Dropdown",
      "add_inside": "+ add inside",
      "items": "{} items",
      "search": "Search...",
      "select_command": "Select command...",
      "select_icon": "Select icon...",
      "step_command": "1. Command:",
      "step_icon": "2. Icon:",
      "save_button": "Save Button",
      "add_formatting_button": "Add Formatting Button",
      "create_group_title": "Create Dropdown Group",
      "select_group_icon": "Select group icon:",
      "create_group": "Create Group",
      "default_group": "New group",
      "status": {
        "reset_done": "Reset to defaults",
        "copied": "Configuration copied to clipboard",
        "clipboard_empty": "Clipboard is empty",
        "import_done": "Configuration imported successfully!",
        "import_error": "Error: failed to parse JSON"
      }
    },

    "registry": {
      "ui.theme": { "title": "Appearance Theme", "desc": "Color theme for the user interface: system, dark, or light" }
      // … по одной записи на каждый id из SETTINGS (23 шт.)
    }
  }
}
```

Переиспользуем существующее вместо новых ключей:

- `common.cancel` — «Отмена» в `plugin_toolbar.rs:1381,1517`;
- `common.delete` — «Удалить» в `tabs.rs:2928`;
- `settings.title` — заголовок окна: `format!("{} — Vibecodium", self.t("settings.title"))`
  (сейчас `settings.title` в локалях есть, но не используется);
- `editor.live_tabs_unit`, `editor.debounce_unit` — уже существующие единицы.

Итого: **~88 новых ключей в `settings.*` + 46 ключей `settings.registry.*`**.

### 2.3. Поисковый реестр

`SettingDescriptor` (`src/settings.rs:632–657`) меняет представление:

```rust
pub struct SettingDescriptor {
    pub id: &'static str,
    pub tab_id: &'static str,
    pub title_key: &'static str,
    pub desc_key: &'static str,
    pub keywords: &'static [&'static str],
    pub aliases: &'static [&'static str],
    pub setting_type: SettingType,
}

impl SettingDescriptor {
    pub fn title(&self, lang: &str) -> String { crate::i18n::t(self.title_key, lang) }
    pub fn description(&self, lang: &str) -> String { crate::i18n::t(self.desc_key, lang) }
}
```

`search_settings(query, lang)` (`settings.rs:964`) матчит по:

1. `id` и `aliases`;
2. `title_key/desc_key`, разрешённым для **текущего языка, `en` и `ru`** —
   это сохраняет сегодняшнее поведение, когда при английском UI запрос «перенос»
   находит настройку;
3. `keywords` — остаются в Rust: это поисковые синонимы, а не подписи интерфейса.
   Переводы синонимов в JSON потребовали бы поддержки массивов в `flatten_json`
   (сейчас массивы отбрасываются) — отдельное решение, не блокирует редизайн.

Вызовы: `ui.rs:1313`, `ui.rs:1734` — передать `&self.config.ui.language`;
`ui.rs:1823` — `.title(&lang)` вместо `.title(is_ru)`.

`get_setting_by_id` и `ipc.rs:38` не меняются: им нужны только `id`/`tab_id`.

---

## 3. Этапы

Порядок выбран так, чтобы каждый этап компилировался и **не менял внешний вид**:
сначала аддитивно добавляются ключи, затем литералы заменяются один-в-один,
и только последним этапом идут переводы.

### Этап 0. Ключи и синхронизация локалей

1. Добавить новые ключи в `locales/en.json` (эталон) и `locales/ru.json`
   (русские значения берутся из текущих литералов — формулировки не меняем).
2. Во все остальные 18 локалей записать **английские значения**.
   Это не «фейковый перевод», а сохранение сегодняшнего поведения: вне `ru`
   все эти экраны и так показываются по-английски. Благодаря этому первый проход
   не меняет ни одну строку на экране, а parity-тест остаётся зелёным.
3. Проверка: `cargo test test_all_locales_have_full_key_parity_with_en`.

### Этап 1. Вкладки и окно — `tab.rs`, `ui.rs`

- `tab.rs:86–115`: убрать ветки `is_ru`; `title()` → `crate::i18n::t(self.key(), lang)`.
  Для `PluginSettings("editor-toolbar")` — ключ `settings.tabs.editor_toolbar`;
  для остальных `PluginSettings(name)` — имя плагина (данные, не UI).
- `ui.rs:1021–1025`: `format!("{} — Vibecodium", self.t("settings.title"))`.
- `ui.rs:1481`: `self.t("settings.nav.search_placeholder")`.
- `ui.rs:1775`: `self.t("settings.nav.search_empty")`.
- `ui.rs:841–853`: фолбэк заголовка модалки → `self.t("settings.plugins.edit_title")`.
- Тест `views/settings_view/tests.rs:4–12` переписать: ожидания берутся из `t()`,
  добавляется проверка фолбэка для не-ru/en языка (например `de` → английский текст).

Проверка: `cargo test settings`, окно настроек в ru/en/de — заголовки совпадают с прежними.

### Этап 2. Плагины — `tabs.rs`

Функции: `render_core_plugins_tab`, `render_community_plugins_tab`, карточка плагина.
Замены по списку из анализа:

- core: 1858–1889 (заголовок + 4 модуля), 1928 (`t_args` со статусом);
- community: 1970–2026, 2108–2123, 2211–2213, 2697;
- detail: 2827–2928, 2989–3000 (`format!("{} {}", t("…author"), author)` — лучше
  `t_args("settings.plugins.detail.author", &[("author", &author_str)])`);
- metadata: 3015–3072;
- readme/config: 3120–3252.

Статус `app.state.status_text = format!("Core plugin '{}' updated", tid)` считается
**до** `app.update(...)`, чтобы был доступ к языку из `self.config`.

Проверка: в исходнике не остаётся `is_ru`/кириллицы (кроме однобуквенных клавиш
раскладки); ru/en/de на всех трёх экранах.

### Этап 3. Панель инструментов — `plugin_toolbar.rs`

32 литерала (список в анализе): 304–1517. Отдельно:

- 615 `"──────── (разделитель)"` → `self.t("settings.toolbar.separator")`;
- 698 `"Dropdown"` → `self.t("settings.toolbar.dropdown_fallback")`;
- 1496 `Some("Новая группа")` → `Some(self.t("settings.toolbar.default_group"))`.
  Имя сохраняется в конфиг в момент создания, поэтому уже созданные группы
  не переведутся при смене языка — это пользовательские данные, не UI;
- 1634–1676: пять статусов → `self.t("settings.toolbar.status.*")`.

Проверка: пройти сценарии «добавить кнопку / группу / разделитель», «сбросить»,
«экспорт/импорт» в ru и en; строки совпадают с прежними.

### Этап 4. Реестр поиска — `settings.rs`

- Поля `title_ru/title_en/desc_ru/desc_en` → `title_key/desc_key` (23 записи).
- Методы `title(lang)`, `description(lang)`.
- `search_settings(query, lang)` + обновление трёх вызовов в `ui.rs`.
- `src/settings/tests.rs:42–63`: передавать язык; те же запросы («перенос», «wrap»,
  «шрифт размер») должны находить те же настройки; добавить кейс `de`.

Проверка: `cargo test settings`, ручной поиск в настройках на ru/en/de.

### Этап 5. Guard-тесты (защита от регрессии)

В `src/views/settings_view/tests.rs` добавить два теста по исходникам модуля
(`include_str!("tabs.rs")` и т.д.):

1. `settings_module_has_no_hardcoded_cyrillic` — падает, если в строковом литерале
   длиннее одного символа есть кириллица (однобуквенные `"ф"`, `"с"`, `"м"`, `"ч"`,
   `"б"` — клавиши раскладки — разрешены; строки-исключения помечаются
   комментарием `// i18n-allow`).
2. `settings_module_has_no_is_ru_switch` — падает на `starts_with("ru")` в модуле.

Проверка: тесты зелёные; при попытке вернуть литерал — красные.

### Этап 6. Переводы 18 локалей

Сейчас ключи содержат английский текст. Дальше — пакетная работа переводчика
(или машинный проход + вычитка) по языкам: `pl, de, fr, es, it, pt, zh-CN, zh-TW,
ja, ko, tr, nl, cs, sv, uk, ar, vi, hi`.

Приоритет: `de, fr, es, pl, uk, zh-CN` (самые вероятные пользователи), затем остальные.
Parity-тест заставляет добавить ключи, но не проверяет качество — вычитка нужна
отдельно. Объём: ~134 ключа × 18 локалей.

### Этап 7. Финальная проверка и мелочи

- `tabs.rs:359` «Semibold/Regular» → `settings.ui.font_weight_*`.
- Единицы `"{} px"`, `"{} ch"`, `"{} MB"`, `"{}%"` → `settings.units.*` через `t_fmt`
  (низкий приоритет: символы нейтральны, но прецедент `editor.*_unit` уже есть).
- `tabs.rs:1770` `"v0.1.0 GPUI Native DirectX 120 FPS"` → версия из
  `env!("CARGO_PKG_VERSION")` (не локализация, но захардкоженная версия).
- Удалить временно ненужные `is_ru`-переменные, которые остались после замен.
- `cargo check`, `cargo test`, `cargo clippy`.

---

## 4. Матрица проверок

| Что | Как |
|---|---|
| Ключи есть во всех 20 локалях | `cargo test test_all_locales_have_full_key_parity_with_en` |
| Поиск настроек работает на ru/en/de | `cargo test test_search_settings` + ручной ввод «перенос», «wrap», «шрифт» |
| Заголовки вкладок | `cargo test test_settings_tab_keys_and_titles` |
| Нет литералов в модуле настроек | guard-тесты этапа 5 |
| Внешний вид не изменился | скриншот-дифф (`tools/gui_tester`) окна настроек в ru и en до/после; все 14 вкладок + 4 экрана плагинов |
| Сборка | `cargo check`, `cargo clippy`, `cargo test` |

---

## 5. Риски и принятые решения

1. **Объём переводов.** 134 ключа × 18 языков. Решение: первый проход только переносит
   строки в JSON с английскими значениями вне ru — поведение на экране не меняется,
   переводы идут отдельными порциями.
2. **Смысл поиска.** Сейчас поиск матчит ru и en одновременно. Решение: матчить
   текущий язык + `en` + `ru`; это сохраняет привычку искать по-русски при английском UI.
3. **`keywords` остаются в Rust.** Это не подписи, а синонимы поиска; JSON-загрузчик
   не поддерживает массивы. Если нужна их локализация — расширять `flatten_json`
   и формат ключей отдельной задачей.
4. **Плюрализация.** `{} items` / «{} элементов» — одна форма. Полноценные формы
   для ru/pl/uk (1/2–4/5+) — отдельная фича `i18n`, в этот редизайн не входит.
5. **Смена языка после создания группы toolbar.** Имя группы хранится в конфиге
   и не переводится задним числом — это данные пользователя, не chrome.
6. **Формулировки.** Тексты переносятся 1-в-1 из текущих литералов. Любая правка
   формулировок — отдельным пакетом, чтобы диффы перевода и поведения не смешивались.
7. **AGENTS: «внешний вид не меняется».** Никаких изменений `id`, ширин и отступов;
   проверка — скриншот-дифф, а не «на глаз».

---

## 6. Логические пакеты изменений

Разбиение на пакеты — чтобы каждый шаг был проверяемым и не смешивал перенос
строк с переводом. Коммиты и их сообщения — на усмотрение владельца репозитория.

1. Только JSON: новые ключи `settings.*` в `en.json` и `ru.json`, английские
   значения в остальных 18 локалях.
2. Вкладки и заголовки окна (`tab.rs`, `ui.rs`).
3. Вкладки core/community и карточка плагина (`tabs.rs`).
4. Настройки панели инструментов (`plugin_toolbar.rs`).
5. Реестр поиска на ключах (`settings.rs` + вызовы в `ui.rs`).
6. Guard-тесты против литералов в модуле настроек.
7. Переводы 18 локалей — порциями по языкам.

После пакета 6 модуль настроек полностью на i18n; пакет 7 может идти параллельно
с другими задачами.
