# Темы Obsidian Mobile 1.15.0: устройство, настройки, установка

**Источник:** `D:\Naben\Github\rust\Aquilum-NP\Obsidian\Obsidian-1.15.0.apk` (14 511 986 байт), распакованный в `Obsidian\Obsidian-1.15.0\`.
**Что разбиралось:** `assets/public/app.js` (3 414 802 байта, одна строка), `assets/public/app.css` (593 377 байт, одна строка), `assets/public/index.html`, `assets/public/i18n.js`, `assets/capacitor.config.json`.
**Версия:** Obsidian Mobile **1.15.0**, внутренняя версия API в бандле — `Br = "1.15.0"`.
**Метод:** статический разбор минифицированного JS. Файл отформатирован esbuild в `Obsidian\_analysis\app_pretty.js` (84 797 строк) — по нему и приведены номера строк. Нативная часть (dex/so) для тем не нужна: вся логика тем находится в web-слое Capacitor. REA/jadx не потребовались.

Дополнительные артефакты разбора лежат в `Obsidian\_analysis\` (каталог не отслеживается git):

| Файл | Что внутри |
|---|---|
| `app_pretty.js` | отформатированный весь `app.js` |
| `theme_manager.js`, `theme_manager_pretty.js` | класс менеджера тем `C4` |
| `appearance_tab_pretty.js` | вкладка Appearance `KD` |
| `vars-body.md` | 899 переменных основного блока `body` |
| `vars-palette-mobile.md` | палитры `.theme-dark` / `.theme-light`, `.is-mobile` / `.is-phone` / `.is-tablet`, `:root` |
| `selector-varmap.txt` | 401 блок «селектор → объявленные переменные» |
| `community-css-themes.json`, `theme-stats.json` | реальные ответы серверов на 2026-11-15 |

---

## 0. TL;DR

- **Тема — это чистый CSS.** Два файла: `manifest.json` (метаданные) и `theme.css` (стили). Никакого JS, хуков и API у темы нет. Всё, что тема может — переопределить CSS-переменные и селекторы приложения.
- **Установка — это файлы в vault:** `<vault>/.obsidian/themes/<Имя>/manifest.json` + `theme.css`. Старый (legacy) формат — один файл `<vault>/.obsidian/themes/<Имя>.css`.
- **Включение — это конфиг vault:** `appearance.json` → `"cssTheme": "<Имя>"`. Пустая строка — тема по умолчанию.
- **Применение — это один `<style>` в `<head>`:** приложение читает `theme.css` и кладёт его текстом в `<style>` после `app.css`. Сниппеты добавляются следующими `<style>`-элементами, поэтому переопределяют тему.
- **Светлая/тёмная — это классы на `<body>`:** `theme-dark` / `theme-light`. Базовая схема хранится в `app.json` → `"theme": "obsidian" | "moonstone" | "system"`. Тема обязана сама оформить оба класса.
- **Скачивание — GitHub + прокси:** каталог `community-css-themes.json` из `obsidianmd/obsidian-releases`; файлы темы — из GitHub Releases (`/releases/download/<version>/...`) или из `raw.githubusercontent.com` (HEAD); при сетевой ошибке — через `https://releases.obsidian.md/proxy?url=...`.
- **Магазин тем встроен и на мобиле:** Appearance → Themes → Manage → модалка «Community themes» с поиском, сортировкой, скриншотами, README, установкой/обновлением/удалением.
- **Ограничения:** тема не может исполнять код; относительные `url()` в `theme.css` не указывают на папку темы; CSP разрешает внешний `@import` только с `fonts.googleapis.com`; имя папки обязано совпадать с `manifest.name`.

---

## 1. Из чего состоит мобильное приложение

Obsidian Mobile — Capacitor-приложение (`appId: md.obsidian`), весь UI — web-слой:

```
index.html
  ├── <link href="app.css">          — все дефолтные стили (593 КБ, 1228 переменных)
  └── <script src="app.js">          — вся логика (3.4 МБ)
        ├── i18n.js                  — английские строки (остальные 50 языков в i18n/*.txt)
        ├── worker.js                — воркер (поиск/индексация и пр.)
        └── lib/*                    — codemirror, mathjax, pdfjs, mermaid (pele), pixi, prism…
```

`capacitor.config.json`:

```json
{
  "appId": "md.obsidian",
  "appName": "Obsidian",
  "webDir": "mobile",
  "appendUserAgent": "obsidian/1.15.0",
  "server": { "androidScheme": "http" },
  "android": { "webContentsDebuggingEnabled": true },
  "ios": { "contentInset": "never", "scheme": "Obsidian" }
}
```

Практические следствия:

- `androidScheme: "http"` → документ живёт на `http://localhost`, поэтому относительные URL в инлайновом CSS разрешаются относительно корня web-слоя, **а не папки темы**.
- Доступ к файлам vault идёт через Capacitor Filesystem/Preferences, а не через HTTP-сервер.
- `<meta http-equiv="Content-Security-Policy" content="style-src 'unsafe-inline' 'self' https://fonts.googleapis.com">` — тему можно инлайнить (`unsafe-inline`), но внешний `@import` разрешён только с `fonts.googleapis.com` и с собственного origin. `img-src`/`font-src`/`connect-src` не заданы, поэтому картинки, шрифты и fetch на GitHub не блокируются CSP.

---

## 2. Модель данных

### 2.1 Конфиг vault: `app.json` и `appearance.json`

Каталог конфига по умолчанию — `.obsidian` (константа `ov = ".obsidian"`), может быть переопределён (валидация: имя начинается с `.`). Файлы читаются и **сливаются**:

```js
config = Object.assign({}, appearanceJson, appJson); // app.json побеждает при совпадении ключей
```

Ключи, которые пишутся в `appearance.json` (массив `aae`, строка 27043 в `app_pretty.js`):

```
accentColor, theme, cssTheme, enabledCssSnippets, showViewHeader, showRibbon,
nativeMenus, translucency, textFontFamily, interfaceFontFamily, monospaceFontFamily,
baseFontSize, baseFontSizeAction, slidingSidebar, floatingNavigation, autoFullScreen
```

**Всё остальное** (editor, files, hotkeys, tabSize и т.д.) уходит в `app.json`.

Дефолты (объект `sae`) для интересующего нас:

| Ключ | Файл | Дефолт | Смысл |
|---|---|---|---|
| `theme` | appearance.json | `"system"` | базовая схема: `obsidian` (тёмная), `moonstone` (светлая), `system` |
| `cssTheme` | appearance.json | `""` | имя активной темы; `""` — по умолчанию |
| `accentColor` | appearance.json | `""` | hex-акцент; пусто — из темы |
| `enabledCssSnippets` | appearance.json | `[]` | имена включённых сниппетов (без `.css`) |
| `translucency` | appearance.json | `false` | прозрачность окна (только desktop) |
| `interfaceFontFamily` | appearance.json | `""` | список шрифтов интерфейса |
| `textFontFamily` | appearance.json | `""` | список шрифтов редактора/чтения |
| `monospaceFontFamily` | appearance.json | `""` | список моноширинных шрифтов |
| `baseFontSize` | appearance.json | `16` | базовый размер, 10–30 |
| `baseFontSizeAction` | appearance.json | `false` | быстрый зум шрифта (Ctrl+wheel/пинч) |
| `tabSize` | app.json | `4` | размер таба → `--indent-size` |

Отдельно: **фактический размер шрифта на устройстве** хранится ещё и в `localStorage` под ключом `base-font-size`; `getBaseFontSize()` сначала читает его, потом `appearance.json`. То есть размер шрифта — локальный для устройства, а не синхронизируемый параметр.

Прочие `localStorage`-ключи, связанные с темой:

| Ключ | Смысл |
|---|---|
| `base-font-size` | фактический размер шрифта на устройстве |
| `local-themes` | имена тем, за которые уже отправлен POST-статистики |
| `communityThemeSortOrder` | сортировка магазина: `download` (по умолчанию), `release`, `alphabetical` |
| `enable-plugin-<appId>` | пройден ли первый запрос доверия к community-плагинам |

### 2.2 Манифест темы

`manifest.json` (имя файла — константа `qp = "manifest.json"`):

```json
{
  "name": "Minimal",
  "version": "9.1.4",
  "minAppVersion": "1.14.0",
  "author": "@kepano",
  "authorUrl": "https://twitter.com/kepano",
  "fundingUrl": "https://www.buymeacoffee.com/kepano"
}
```

Что реально читается в 1.15.0:

| Поле | Где используется |
|---|---|
| `name` | **обязательно**; при сканировании должно совпадать с именем папки; при установке — с именем из каталога |
| `version` | тег релиза для скачивания, сравнение версий для обновлений |
| `minAppVersion` | порог совместимости; если приложение старее — включается поиск по `versions.json` |
| `author` | подпись в магазине (если нет в каталоге) |
| `authorUrl` | ссылка на автора в карточке темы |
| `fundingUrl` | в этой версии **не отображается** |

Манифест без `name` или с `name != имени папки` молча игнорируется (`readThemes`, строки 25944–25956).

### 2.3 Каталог сообщества

URL: `https://raw.githubusercontent.com/obsidianmd/obsidian-releases/HEAD/community-css-themes.json`
(константа `j4e = wr("obsidianmd/obsidian-releases", "community-css-themes.json")`).

Это массив объектов. Реальная схема (проверено на файле от 2026-11-15, 1034 темы):

```json
{
  "name": "Atom",
  "author": "kognise",
  "repo": "kognise/obsidian-atom",
  "screenshot": "screenshot-hybrid.png",
  "modes": ["dark", "light"],
  "legacy": true
}
```

| Поле | Обяз. | Смысл |
|---|---|---|
| `name` | да | имя темы; оно же имя папки и `manifest.name` |
| `author` | да | автор (строка) |
| `repo` | да | `owner/repo` на GitHub |
| `screenshot` | да | путь к картинке относительно HEAD репозитория (может быть `.gif`) |
| `modes` | да | `["dark"]`, `["light"]`, `["dark","light"]` — только для фильтра в магазине |
| `legacy` | нет | `true` — тема старого формата (`obsidian.css`), скрывается из списка магазина |

Валидация в браузере (`Z1.loadItems`): запись пропускается, если нет строковых `author`, `name`, `repo` или массива `modes`. `legacy: true` не попадает в список карточек, но остаётся доступной для установки через CLI.

Реальная статистика каталога на 2026-11-15:

- всего 1034 темы;
- `legacy: true` — 15;
- modern — 1019;
- `modes`: `["dark","light"]` — 724, `["dark"]` — 248, `["light"]` — 46, `["light","dark"]` — 16.

### 2.4 `versions.json`

Файл в корне репозитория темы:

```json
{
  "6.0.0": "0.16.0",
  "7.0.0": "1.1.9",
  "8.0.0": "1.9.0",
  "9.0.0": "1.13.0",
  "9.1.0": "1.14.0"
}
```

Формат: `"версия релиза": "минимальная версия Obsidian"`. Логика `Xs` (строка 24295):

1. Если `minAppVersion` отсутствует или приложение **новее либо равно** ему — возвращается `manifest.version`. `versions.json` не читается.
2. Только если приложение **старее** `minAppVersion` — читается `versions.json` и выбирается **наибольший** релиз, чей `minAppVersion <= текущей версии приложения`.
3. Если `versions.json` нет/не парсится/нет совместимого релиза — возвращается `null`. Для установки это `No appropriate version found.`

Практика: на актуальном приложении `versions.json` не обязателен. Он нужен для старых клиентов. Многие современные темы (Opus, Sunflex, Vignelli) его не имеют — это нормально.

### 2.5 Статистика скачиваний

Два эндпоинта:

| Метод | URL | Ответ | Кеш |
|---|---|---|---|
| GET | `https://releases.obsidian.md/stats/theme` | `{"Имя": {"id": "Имя", "download": 12345}, ...}` | 5 мин (минимум 60 с) |
| POST | `https://releases.obsidian.md/stats/theme/<urlencoded имя>/download` | — (fire-and-forget) | — |

POST отправляется один раз на устройство для каждой темы: имя добавляется в `localStorage["local-themes"]`. Ошибка статистики игнорируется и не влияет на установку.

---

## 3. Файловая раскладка

```
<vault>/
└── .obsidian/                      # configDir (ov = ".obsidian")
    ├── app.json                    # theme, tabSize, editor, files, ...
    ├── appearance.json             # cssTheme, accentColor, enabledCssSnippets, fonts...
    ├── themes/
    │   ├── Minimal/                # modern-тема
    │   │   ├── manifest.json
    │   │   └── theme.css
    │   ├── Legacy Theme.css        # legacy-тема (один файл)
    │   └── ...
    └── snippets/
        ├── my-tweak.css            # включён/выключен в appearance.json
        └── another.css
```

### 3.1 Правила сканирования (`C4.readThemes` / `readSnippets`)

**Папки тем:**

- перебираются только папки внутри `<configDir>/themes`;
- обязателен `theme.css`; без него папка пропускается;
- `manifest.json` должен читаться как JSON;
- `manifest.name` должен **точно** совпадать с именем папки (регистрозависимо);
- в память пишется `manifest + dir`.

**Legacy-темы:**

- все файлы `*.css` прямо в `<configDir>/themes` (кроме начинающихся с `.`);
- имя темы = имя файла без `.css`;
- манифеста нет, версия считается `0.0.0`.

**Сниппеты:**

- все `*.css` прямо в `<configDir>/snippets` (кроме начинающихся с `.`);
- имя = имя файла без `.css`;
- сортируются по имени; включённые берутся из `appearance.json.enabledCssSnippets`.

Разрешение пути активной темы (`getThemePath`):

```js
if (!themes.hasOwnProperty(name) && oldThemes.contains(name))
  return `${configDir}/themes/${name}.css`;       // legacy
return `${configDir}/themes/${name}/theme.css`;   // modern
```

Папка с `theme.css`, но без манифеста, **не считается темой** (и наоборот). Единственное исключение — legacy-файл в корне `themes`.

---

## 4. Как тема применяется

### 4.1 Старт

`App.initializeWithAdapter` вызывает `this.customCss.load()`; `Component.load()` вызывает `CustomCss.onload()` (строки 25928–25933):

```js
this.loadData();                                  // enabledCssSnippets, cssTheme, translucency
this.registerEvent(vault.on("raw", this.boundRaw));
await Promise.all([
  this.readThemes(),
  this.readSnippets(),
  this.loadTheme(),
  this.loadSnippets()
]);
workspace.trigger("css-change");
```

`CustomCss` создаёт в конструкторе один `<style>`:

```js
this.styleEl = document.head.createEl("style", { type: "text/css" });
```

### 4.2 Слои CSS и порядок каскада

```
app.css (link в <head>)          — дефолты и все 1228 переменных
  ↓
<style> с theme.css              — тема
  ↓
<style> сниппета 1
<style> сниппета 2
...                              — сниппеты в алфавитном порядке
```

`loadSnippets` вставляет каждый следующий `<style>` **после** предыдущего (`parentNode.insertAfter`), начиная с темы. Поэтому сниппеты всегда переопределяют тему при равной специфичности, а тема переопределяет `app.css`.

Важно: `app.css` — это `<link>`, а тема — инлайновый `<style>`, но каскад решает не тип подключения, а порядок в документе и специфичность. Тема идёт позже — при равной специфичности выигрывает.

### 4.3 Чтение, кеш, дебаунс, watcher

- `loadTheme` читает файл через адаптер vault, кеширует содержимое в `Map csscache` и присваивает `styleEl.textContent`. Если текст не изменился — DOM не трогается.
- `loadSnippets` читает каждый включённый сниппет, создаёт/переиспользует `<style>`-элементы, лишние удаляет.
- Дебаунсы: тема — 100 мс, сниппеты — 100 мс, перечитывание списка тем — 2 с. Все операции сериализуются очередью `queue`.
- `vault.on("raw", path)`:
  - сбрасывает кеш для изменённого файла;
  - если изменён активный `theme.css` — перезагружает тему;
  - если изменён любой `theme.css`/`manifest.json`/`*.css` внутри `themes/` — перечитывает список тем;
  - если изменён `*.css` внутри `snippets/` — перечитывает сниппеты.
- `setTheme(name)` пишет `appearance.json.cssTheme` и планирует перезагрузку темы (100 мс).
- `setCssEnabledStatus(snippet, on)` пишет `appearance.json.enabledCssSnippets` и планирует перезагрузку сниппетов.

### 4.4 Событие `css-change`

После смены темы/сниппета/акцента/шрифта/размера приложение вызывает:

```js
workspace.trigger("css-change");
workspace.trigger("resize");
```

Это не «магия темы», а сигнал компонентам пересчитать то, что зависит от вычисленных стилей: виртуализированные списки, Canvas, graph, PDF, статус-бар iOS и т.д. Тема сама подписаться на него не может (это внутреннее событие).

### 4.5 Светлая и тёмная схема

Базовая схема (`app.json.theme`):

| Значение | Что делает |
|---|---|
| `obsidian` | тёмная; на `<body>` класс `theme-dark` |
| `moonstone` | светлая; класс `theme-light` |
| `system` | следует `prefers-color-scheme` (на desktop ещё и нативной теме ОС) |

`updateTheme` (строка 83578):

```js
let dark = config.theme === "obsidian";
if (config.theme === "system") dark = matchMedia("(prefers-color-scheme: dark)").matches;
body.toggleClass("theme-light", !dark);
body.toggleClass("theme-dark", dark);
```

`index.html` изначально содержит `<body class="theme-dark">` — до инициализации; затем классы выставляются по конфигу.

В `app.css`:

- `.theme-dark` — 61 переменная: `color-scheme: dark`, базовые `--color-*`, `--color-base-00…100`, `--mono-*`, `--color-accent*`, тени, `--background-secondary-alt`, `--interactive-*`, `--text-selection` и т.д.;
- `.theme-light` — 46 переменных: то же для светлой схемы;
- остальные ~899 семантических переменных объявлены на `body` и считаются из палитры через `var()`, `color-mix()`, `light-dark()`.

Тема может переопределять как палитру (`.theme-dark { --color-base-00: … }`), так и семантику (`body { --background-primary: … }`).

Команда `theme:toggle-light-dark`:
- при `system` — просто переключает классы `theme-dark`/`theme-light`;
- при фиксированной схеме — вызывает `changeTheme("obsidian" | "moonstone")`.

### 4.6 Акцентный цвет

`appearance.json.accentColor` — hex (`#rrggbb`). `setAccentColor` (строка 83644):

- переводит hex в HSL;
- выставляет **inline** на `<body>`: `--accent-h`, `--accent-s`, `--accent-l`;
- при светлом/тёмном акценте выставляет `--text-on-accent: var(--text-on-accent-inverted)`;
- пустое значение удаляет inline-переменные, и акцент снова берётся из темы.

Так как это inline-стиль, пользовательский акцент **сильнее** любых CSS-правил темы. Тема может задать свой дефолт через `--accent-h/s/l` или `--color-accent` — он будет использован, пока пользователь не выбрал акцент.

### 4.7 Шрифты и размер

| Настройка | Куда пишется | Что выставляет |
|---|---|---|
| `interfaceFontFamily` | appearance.json | inline `--font-interface-override` |
| `textFontFamily` | appearance.json | inline `--font-text-override`, `--font-print-override` |
| `monospaceFontFamily` | appearance.json | inline `--font-monospace-override` |
| `baseFontSize` (10–30) | localStorage `base-font-size`; `appearance.json` — стартовое значение | inline `--font-text-size: Npx` на `<body>`, `html { font-size: calc(Npx * var(--preferred-text-scale,1)) }` |
| `tabSize` | app.json | inline `--indent-size` |

Тема должна переопределять `--font-interface-theme`, `--font-text-theme`, `--font-monospace-theme` (слот темы в стеке шрифтов), а не `-override` (слот пользователя). Стек:

```css
--font-text: var(--font-text-override), var(--font-text-theme), var(--font-default);
```

### 4.8 Мобильные классы и переменные

На `<body>` в мобильном приложении добавляются:

| Класс | Когда |
|---|---|
| `is-mobile` | всегда в mobile app |
| `is-android` / `is-ios` | по платформе |
| `is-phone` / `is-tablet` | по размеру окна (`toggleClass` при ресайзе) |
| `is-focused` | окно в фокусе |
| `is-hidden-nav` | скрыта навигация на телефоне |
| `mod-toolbar-open` | открыт мобильный тулбар |
| `is-translucent` | desktop + translucency |
| `is-popout-modal` | desktop, модалка в отдельном окне |
| `mod-rtl` / `mod-system-rtl` | RTL |
| `mod-macos` / `mod-windows` / `mod-linux` | desktop |

Мобильный блок `.is-mobile` (105 переменных) задаёт тач-размеры и мобильный хром:

```css
--view-header-height, --view-header-action-radius;
--mobile-toolbar-height, --navbar-height, --navbar-radius, --navbar-width, --navbar-max-width;
--mobile-sidebar-width, --mobile-sidebar-width-pinned, --mobile-sidebar-min-width, --mobile-sidebar-max-width;
--ribbon-width, --input-height, --toggle-width, --checkbox-size, --slider-thumb-*;
--system-status-background; --keyboard-background;
--font-preferred-size: calc(var(--font-text-size) * var(--preferred-text-scale));
--font-ui-smaller/small/medium/large;
--icon-xs/s/m/l (с учётом preferred-text-scale);
```

**:root** содержит safe-area и масштаб текста:

```css
--safe-area-inset-top: env(safe-area-inset-top);
--safe-area-inset-bottom: env(safe-area-inset-bottom);
--safe-area-inset-left: env(safe-area-inset-left);
--safe-area-inset-right: env(safe-area-inset-right);
--preferred-text-scale: env(preferred-text-scale, 1);
--keyboard-height: 0px;
```

Android-специфика (`updateMobileFrameTheme`):

- читает вычисленное `--system-status-background`;
- если переменная отсутствует или её альфа = 0 — берёт `getComputedStyle(document.body).backgroundColor`;
- ставит нативный цвет статус-бара;
- `.is-tablet` переопределяет `--system-status-background: var(--titlebar-background)`.

То есть тема может управлять цветом системного статус-бара Android через `--system-status-background`. На iOS вместо цвета ставится стиль статус-бара (Dark/Light/Default), а также обновляется мобильная рамка.

---

## 5. Что именно стилизуется: контракт CSS

### 5.1 Масштаб

- В `app.css` **1228** уникальных custom properties.
- На `body` — **922** (899 в основном блоке + шрифтовые, анимационные и служебные переменные в отдельных `body`/`body.*` блоках).
- Остальные объявляются локально на компонентах.
- Полный список с значениями: `obsidian-mobile-themes-variables.md`.
- Индекс «селектор → переменные»: `obsidian-mobile-themes-selectors.md`.

### 5.2 Палитра и семантика

Нижний слой (`.theme-dark` / `.theme-light`) — сырые цвета:

```css
--color-red…--color-pink, --color-base-00…--color-base-100,
--mono-0, --mono-100, --color-accent, --color-accent-1, --color-accent-2,
--color-*-rgb, --color-accent-hsl, --highlight-mix-blend-mode, color-scheme
```

Средний слой (`body`) — семантика:

```css
--background-primary: var(--color-base-00);
--background-secondary: var(--color-base-20);
--background-primary-alt: var(--color-base-10);
--text-normal: var(--color-base-100);
--text-muted: var(--color-base-70);
--text-faint: var(--color-base-50);
--text-accent: var(--color-accent);
--interactive-normal: …; --interactive-hover: …;
--interactive-accent: …; --interactive-accent-hover: …;
--link-color: var(--text-accent);
--file-line-width: 700px;
--line-height-normal: 1.5;
--p-spacing: 1rem;
--radius-s/m/l, --border-width, --divider-color, --shadow-*;
```

Тема обычно переопределяет именно средний слой, оставляя палитру как есть, либо переопределяет палитру для полной смены цветов.

### 5.3 Карта областей UI

| Область | Основные селекторы | Ключевые переменные |
|---|---|---|
| Глобальный фон/текст | `body`, `.app-container` | `--background-*`, `--text-*`, `--font-*` |
| Заголовок окна | `.titlebar`, `.titlebar-button`, `.titlebar-text` | `--titlebar-*` |
| Лента (ribbon) | `.workspace-ribbon`, `.side-dock-ribbon-action` | `--ribbon-*`, `--icon-*` |
| Вкладки | `.workspace-tab-header-container`, `.workspace-tab-header`, `.workspace-tab-header-inner` | `--tab-*`, `--icon-*` |
| Проводник/файлы | `.nav-folder-title`, `.nav-file-title`, `.nav-file-tag` | `--nav-*` |
| Шапка вкладки | `.view-header`, `.view-header-title`, `.view-actions` | `--view-*`, `--header-height` |
| Редактор CM6 | `.markdown-source-view.mod-cm6`, `.cm-editor`, `.cm-scroller`, `.cm-content`, `.cm-line`, `.cm-gutters`, `.cm-activeLine`, `.cm-selectionBackground`, `.HyperMD-*` | `--code-*`, `--line-height-*`, `--indent-*`, `--caret-color`, `--text-selection` |
| Чтение | `.markdown-preview-view`, `.markdown-rendered`, `.markdown-preview-sizer` | `--file-line-width`, `--p-spacing`, `--h1..h6-*`, `--list-*`, `--blockquote-*`, `--hr-*` |
| Заголовки | `h1…h6`, `.HyperMD-header-1…6` | `--h1-size` … `--h6-size`, `--h*-weight/color/font-style/variant` |
| Код | `pre`, `code`, `.HyperMD-codeblock`, `.cm-inline-code` | `--code-*`, `--inline-code-*` |
| Callout'ы | `.callout`, `.callout[data-callout="…"]` | `--callout-*`, локальные `--callout-color`, `--callout-icon` |
| Таблицы | `table`, `thead`, `th`, `td`, `.table-wrapper`, `.cm-table-widget` | `--table-*` |
| Embed'ы | `.markdown-embed`, `.internal-embed`, `.file-embed` | `--embed-*` |
| Свойства/frontmatter | `.metadata-container`, `.metadata-property`, `.metadata-property-key/value` | `--metadata-*`, `--pill-*` |
| Теги | `.tag`, `.cm-hashtag` | `--tag-*` |
| Ссылки | `.internal-link`, `.external-link`, `.is-unresolved` | `--link-*` |
| Чекбоксы/тумблеры | `.task-list-item-checkbox`, `.checkbox-container` | `--checkbox-*`, `--toggle-*` |
| Поиск | `.search-result`, `.search-result-file-title`, `.search-result-file-match` | `--search-*` |
| Graph | `.graph-view`, `.graph-controls` | `--graph-*` |
| Canvas | `.canvas-wrapper`, `.canvas-node`, `.canvas-controls` | `--canvas-*` |
| Bases (новое ядро) | `.bases-*` (762 вхождения в `app.css`) | `--bases-*` |
| PDF | `.pdf-container`, `.pdf-viewer`, `.pdf-sidebar-container`, `.textLayer .highlight` | `--pdf-*` |
| Модалки | `.modal`, `.modal-container`, `.modal-bg`, `.modal-content` | `--modal-*` |
| Палитра команд/промпты | `.prompt`, `.prompt-input`, `.prompt-results` | `--prompt-*` |
| Подсказки | `.suggestion-container`, `.suggestion-item` | `--suggestion-*` |
| Меню | `.menu`, `.menu-item`, `.menu-separator` | `--menu-*` |
| Настройки | `.modal.mod-settings`, `.vertical-tab-*`, `.setting-item` | `--setting-*` |
| Уведомления | `.notice` | `--notice-*` |
| Статус-бар | `.status-bar`, `.status-bar-item` | `--status-bar-*` |
| Мобильный низ | `.mobile-navbar`, `.mobile-toolbar`, `.mobile-toolbar-options` | `--navbar-*`, `--mobile-toolbar-height`, `--touch-*` |
| Мобильный drawer | `.workspace-drawer`, `.workspace-drawer-inner` | `--mobile-sidebar-*`, `--safe-area-*` |
| Тултипы/поповеры | `.tooltip`, `.popover`, `.hover-popover` | `--tooltip-*`, `--popover-*` |
| Скроллбары | `::-webkit-scrollbar`, `.cm-scroller` | `--scrollbar-*` |
| Иконки | `.svg-icon`, `.clickable-icon` | `--icon-size`, `--icon-stroke`, `--icon-color*` |
| Mermaid | `.mermaid` | `--pele-*` (24 переменные) |
| Изображения | `.image-embed`, `.markdown-embed img` | `--image-*` |

### 5.4 Селекторы-состояния, на которые рассчитывает тема

- **data-атрибуты:** `[data-callout="warning"]`, `[data-task="x"]`, `[data-type="markdown"|"canvas"|"pdf"|"empty"]`, `[data-property-type="tags"]`.
- **mod-классы:** `.mod-root`, `.mod-top`, `.mod-left`, `.mod-right`, `.mod-stacked`, `.mod-active`, `.mod-cta`, `.mod-warning`, `.mod-error`, `.mod-success`, `.mod-empty-state`, `.mod-toggle`, `.mod-list`, `.mod-settings`.
- **псевдоклассы:** `:hover`, `:focus-visible`, `:focus-within`, `:active`, `:has(...)`, `:dir(rtl)`, `:not(...)`.
- **состояния тела:** `.is-focused`, `.is-mobile`, `.is-phone`, `.is-tablet`, `.is-android`, `.is-ios`, `.show-ribbon`, `.show-view-header`, `.show-inline-title`, `.is-floating-nav`, `.auto-full-screen`, `.is-grabbing`, `.is-hidden-nav`, `.mod-toolbar-open`, `.is-translucent`, `.is-popout-modal`.

### 5.5 Ограничения CSS темы

1. **Нет JS.** Тема — только CSS. Настройки темы в сообществе часто делают через отдельный плагин (например, Style Settings), который генерирует CSS; ядро для этого API не даёт.
2. **Относительные `url()` не работают как «файл рядом с theme.css».** Стили инлайнятся в документ, поэтому `url("bg.png")` разрешается относительно `http://localhost/`, а не папки темы. Используйте `data:` URI или абсолютные `https://` URL. Это важно для мобилы, где нет `app://`-схемы desktop-версии.
3. **CSP:** внешний `@import` разрешён только с `fonts.googleapis.com` и с собственного origin. `@import` с других CDN на мобиле будет заблокирован. При этом `@font-face` и `background-image` с внешних доменов не блокируются (нет `font-src`/`img-src`/`default-src`).
4. **Пользовательские настройки сильнее темы:** акцент, шрифты и размер шрифта выставляются inline на `<body>`, поэтому тема не может их перебить (кроме `!important`, но это плохая практика и всё равно проигрывает inline без `!important`).
5. **Тема не управляет `system`:** при `theme: "system"` приложение само решает, какой класс поставить; тема должна оформить оба.
6. **`manifest.name` и имя папки должны совпадать.** Иначе тема не появится в списке, даже если файлы на месте.
7. **Нет проверки подписи/песочницы.** CSS может делать сетевые запросы через `url()` (утечка факта открытия, трекинг). Доверие — на уровне каталога и пользователя.

---

## 6. UI настроек

### 6.1 Вкладка Appearance

Класс `KD` (`app_pretty.js`, строка 81762), `id = "appearance"`, иконка `palette`. Секции:

1. **Base color scheme** — dropdown: Dark / Light / Adapt to system (`R9e`: `obsidian`, `moonstone`, `system`) → `app.changeTheme(value)` → `app.json.theme`.
2. **Accent color** — color picker + кнопка сброса. Пишет `appearance.json.accentColor`.
3. **Themes** (если политика не запрещает):
   - dropdown: `Default` + все modern-темы + legacy с суффиксом `(Legacy)`;
   - выбор применяется сразу (`customCss.setTheme`), на iOS dropdown теряет фокус;
   - кнопка **Manage** → модалка магазина `Z1`.
   - На desktop дополнительно кнопка «Open themes folder».
4. **Current community themes** — сколько установлено, сколько обновлений; кнопки:
   - `Check for updates` — `customCss.checkForUpdates()`;
   - `View updates` — магазин в режиме «только обновления»;
   - `Update all` — проходит по `updates` и устанавливает каждую.
5. **CSS snippets** (если политика не запрещает):
   - список сниппетов с тумблерами (`setCssEnabledStatus`);
   - поиск, счётчик включённых, «Reload snippets»;
   - на desktop — «Open snippets folder» и контекстное меню «Show in folder»;
   - пустое состояние показывает путь `<configDir>/snippets`.
6. **Font**:
   - Interface font, Text font, Monospace font — списки имён (comma-separated), проверка наличия шрифта;
   - Font size — слайдер 10–30, дефолт 16;
   - Quick font size adjustment — toggle `baseFontSizeAction`.
7. **Advanced** (только desktop):
   - Custom app icon;
   - Translucency (`appearance.json.translucency`, класс `is-translucent`, Electron vibrancy).

На мобиле строки «Open themes folder» / «Open snippets folder» / «Custom icon» / «Translucency» отсутствуют; всё остальное есть.

### 6.2 Магазин тем (модалка `Z1`)

Класс `Z1` (`app_pretty.js`, строка 61712), CSS-класс `mod-community-theme`, заголовок «Community themes».

Возможности:

- поиск по имени и автору (fuzzy);
- фильтр «Installed only»;
- фильтр по текущему режиму: «Dark themes only» / «Light themes only» — показывается только если базовая схема не `system`; фильтрует по `modes`;
- сортировка (`localStorage["communityThemeSortOrder"]`):
  - `download` — по убыванию скачиваний (по умолчанию);
  - `release` — обратный порядок каталога (каталог приходит «свежие сверху»);
  - `alphabetical` — по имени;
- карточки: имя, автор, счётчик скачиваний, скриншот (ленивый, при >640×320 уменьшается в canvas до 512×288), бейджи `Installed`, `Legacy`, `Active`, `Update available`;
- правая панель: версия, текущая установленная версия, автор (ссылка `authorUrl`), ссылка на GitHub, README (рендерится как markdown), кнопки:
  - `Install and use` / `Use this theme` / `Stop using this theme`;
  - `Update`;
  - `Check for updates`;
  - `Uninstall`.

Скриншот грузится с `https://raw.githubusercontent.com/<repo>/HEAD/<screenshot>`, README — с `.../HEAD/README.md`. Для темы по умолчанию используются `dark.png`/`light.png` из `obsidianmd/obsidian-releases`.

### 6.3 Быстрое переключение темы

Команда `theme:switch` открывает suggest-модалку `S4` (`app_pretty.js`, строка 26170):

- список: `Default` + установленные modern + legacy `(Legacy)`;
- при выборе с клавиатуры — live preview через `setTheme`;
- `Escape` возвращает исходную тему (`initialTheme`);
- активная тема помечается бейджем `Active`.

Команда `theme:toggle-light-dark` переключает тёмную/светлую схему.

### 6.4 Внутренний CLI

В бандле есть `window.handleCli(argv)` (`app_pretty.js`, строка ~32760) — нативный хост может вызывать команды, в том числе:

```
theme:set name=<имя>            # применить установленную тему ("" — дефолт)
theme:install name=<имя> [enable]  # установить из каталога
theme:uninstall name=<имя>
snippet:enable name=<имя>
snippet:disable name=<имя>
```

Это не пользовательский API темы, но полезно знать для автоматизации/отладки на устройстве.

---

## 7. Как тема скачивается

### 7.1 Карта URL

| Назначение | URL | Примечание |
|---|---|---|
| Каталог | `https://raw.githubusercontent.com/obsidianmd/obsidian-releases/HEAD/community-css-themes.json` | кеш 5 мин |
| Скриншот | `https://raw.githubusercontent.com/<repo>/HEAD/<screenshot>` | путь из каталога |
| README | `https://raw.githubusercontent.com/<repo>/HEAD/README.md` | для карточки |
| Манифест (HEAD) | `https://raw.githubusercontent.com/<repo>/HEAD/manifest.json` | проверка версии/обновлений |
| Версии (HEAD) | `https://raw.githubusercontent.com/<repo>/HEAD/versions.json` | только для старого приложения |
| Релиз: манифест | `https://github.com/<repo>/releases/download/<version>/manifest.json` | тег = `version` |
| Релиз: стили | `https://github.com/<repo>/releases/download/<version>/theme.css` | имя ассета фиксировано |
| Legacy CSS | `https://raw.githubusercontent.com/<repo>/HEAD/obsidian.css` | старый формат |
| Прокси | `https://releases.obsidian.md/proxy?url=<urlencoded URL>` | fallback при сетевой ошибке |
| Статистика | `https://releases.obsidian.md/stats/theme` | GET/POST |

Вспомогательные функции (`app_pretty.js`, строки 23895–24009):

```js
const l4 = "https://github.com/";
const FO = "https://raw.githubusercontent.com/";
wr(repo, file, ref = "HEAD") => `${FO}${repo}/${ref}/${file}`;
xg(repo, version, file)     => `${l4}${repo}/releases/download/${version}/${file}`;
hr(url)                     => fetch с ленивыми .text/.json и прокси-fallback;
```

Прокси включается, только если URL начинается с `https://github.com/` или `https://raw.githubusercontent.com/`, и ошибка **не** 404. 404 пробрасывается как есть — на нём построен fallback «релиз → HEAD».

### 7.2 Выбор версии

```
resolveVersion(repo, manifest):
  if no appVersion or no minAppVersion or appVersion >= minAppVersion:
      return manifest.version
  versions = GET raw/<repo>/HEAD/versions.json
  choose highest release R where versions[R] <= appVersion
  return R or null
```

`appVersion` здесь — внутренняя `Br = "1.15.0"` (в диагностике называется «API version»), а не обязательно маркетинговая версия сборки.

### 7.3 Установка modern-темы

Упрощённый порядок `installTheme(info, version)` (`app_pretty.js`, строка 26116):

1. Проверка политики: `$n(name)` (см. §8.5). Если нельзя — выход.
2. Один раз на устройство: POST статистики, запись имени в `localStorage["local-themes"]`.
3. Если `version == null` → legacy-путь (§7.4).
4. Скачать `manifest.json` и `theme.css` из релиза:
   `GET github.com/<repo>/releases/download/<version>/{manifest.json,theme.css}` как текст.
5. Если первый запрос упал с 404 и манифест ещё не получен — fallback:
   `GET raw/<repo>/HEAD/{manifest.json,theme.css}`.
   Это спасает темы без GitHub Releases (например, Sunflex/Opus: релиз без ассетов → HEAD).
6. Распарсить манифест и проверить `manifest.name === имя из каталога`; иначе ошибка `Theme name mismatch.`
7. Создать `<configDir>/themes` при необходимости; создать `<configDir>/themes/<Name>`.
8. Записать `manifest.json` (как пришёл) и `theme.css`.
9. Удалить legacy-файл `<configDir>/themes/<Name>.css`, если он был.
10. Показать тост «Successfully installed theme …», убрать запись из `updates`, перечитать список тем.

Тост/ошибка живут 3 секунды, классы `is-loading`, `mod-success`, `mod-error`.

Особенности:

- Запись не атомарна: сначала манифест, потом CSS. При обрыве может остаться частичная папка; следующая установка перезапишет.
- `theme.css` из релиза не проверяется на размер/содержимое — просто текст.
- Манифест из релиза проверяется только на `name`; `version` из него для установки не используется (версия уже выбрана).
- Папка называется по имени из **каталога**, а не по `manifest.name` (после проверки они совпадают).

### 7.4 Legacy-темы

Если `version == null` (нет `manifest.json` в HEAD, то есть modern-версия недоступна):

1. `GET raw/<repo>/HEAD/obsidian.css`.
2. Создать `<configDir>/themes` и `<configDir>/themes/<Name>`.
3. Записать `theme.css` = содержимое `obsidian.css`.
4. Сгенерировать манифест:

```json
{
  "name": "<Name>",
  "version": "0.0.0",
  "minAppVersion": "0.16.0",
  "author": "<author из каталога>"
}
```

5. Удалить `<Name>.css`, если он был.
6. Перечитать список тем.

Поэтому после установки legacy-тема выглядит как modern-папка с синтетическим манифестом версии `0.0.0`. Ручная legacy-тема (один `.css` в `themes/`) остаётся legacy до переустановки.

### 7.5 Валидация и ошибки

| Ситуация | Поведение |
|---|---|
| Тема не разрешена политикой | `installTheme` молча выходит; в UI тема отфильтрована |
| Нет `manifest.json` в HEAD | legacy-путь (`obsidian.css`) |
| Релиз 404 | fallback на HEAD `manifest.json` + `theme.css` |
| HEAD тоже 404 | тост `Failed to install theme “<name>”.` |
| `manifest.name` не совпал | тост `Failed to install theme … Theme name mismatch.` |
| Старое приложение, нет совместимой версии | `No appropriate version found.` |
| Сеть/прокси | ошибка → тост, установка не начинается |

---

## 8. Установка, включение, обновление, удаление

### 8.1 Включение темы

```js
setTheme(name):
  this.theme = name
  vault.setConfig("cssTheme", name)   // appearance.json
  requestLoadTheme()                  // debounce 100 ms
```

`loadTheme`:

```js
if (!policyAllows(name) || disabled) name = "";  // CSS не применяется
if (name) css = read(getThemePath(name));        // theme.css или <name>.css
styleEl.textContent = css;
workspace.trigger("css-change");
```

Пустая строка — дефолтная тема (CSS темы отсутствует, остаётся `app.css`).

### 8.2 Обновления

`checkForUpdate(info)`:

- если тема не установлена — выход;
- `installed = themes[name]?.version`;
- `GET raw/<repo>/HEAD/manifest.json`;
- если манифеста нет → legacy: скачать `obsidian.css` и сравнить текст с установленным файлом; если отличается — обновление;
- если манифест есть → `latest = resolveVersion(repo, manifest)`; обновление, если `!installed || installed < latest` (semver-сравнение `Hr(a,b)` = «a старее b»);
- результат складывается в `updates[name] = {themeInfo, version}`.

`checkForUpdates()` проходит по всему каталогу (с учётом политики) и показывает тост «Found N themes to update» / «No theme updates found».

`Update all` в настройках просто вызывает `installTheme(updates[name].themeInfo, updates[name].version)` для каждой записи.

Автопроверки при старте нет — только по кнопке/команде.

### 8.3 Удаление

`removeTheme(name)`:

- `rmdir(<configDir>/themes/<Name>, recursive)` если папка есть;
- если это legacy — удалить `<Name>.css`;
- перечитать темы;
- если удалённая тема активна — `setTheme("")`.

### 8.4 Синхронизация

Obsidian Sync различает категории файлов. Для тем/сниппетов:

- `.obsidian/themes/<Name>/theme.css`
- `.obsidian/themes/<Name>/manifest.json`
- `.obsidian/snippets/*.css`

все относятся к категории **`appearance-data`**;
`.obsidian/appearance.json` — категория **`appearance`**;
`app.json`/`types.json` — **`app`**.

То есть темы и сниппеты синхронизируются отдельным фильтром «Appearance», а не как обычные файлы заметок. При отключённой категории они не уедут на другое устройство.

### 8.5 Политики организации

Объект политики (`wn = Roe.policy`) может содержать:

- `themes: true` — разрешены все;
- `themes: ["Name1", "Name2"]` — только перечисленные;
- `themes: false` — запрещены все.

Функция `$n(name)` разрешает пустое имя всегда, иначе `UO(policy.themes, name)`. Она применяется:

- при загрузке CSS (`loadTheme`) — запрещённая тема не применяется;
- в dropdown и магазине — запрещённые скрываются;
- при установке/удалении/CLI.

`wn.themes !== false` управляет видимостью строки Themes; `wn.snippets` — страницы CSS snippets.

Отдельный нюанс: при первом запуске с установленными community-плагинами приложение показывает запрос доверия (`enable-plugin-<appId>`) и на это время вызывает `customCss.globallyDisable()` — тема и сниппеты **не применяются**, пока пользователь не закроет диалог (при закрытии `globallyEnable()`).

---

## 9. Мобильная специфика

1. **Тот же web-слой, что и desktop, но другие классы.** Темы, написанные под desktop (`.titlebar`, `.workspace-ribbon`, `.is-hidden-frameless`), на мобиле часто не имеют эффекта или требуют `.is-mobile`/`.is-phone`.
2. **Статус-бар Android** управляется переменной `--system-status-background`; тема может её переопределить. Если переменная прозрачная/отсутствует — берётся фон `body`.
3. **Safe area** уже учтена в `:root`; мобильные модалки/панели используют `--safe-area-inset-*`. Переопределять их в теме обычно не нужно.
4. **Масштаб текста** приходит из нативной настройки через `env(preferred-text-scale)`; от него зависят `--font-preferred-size`, `--font-ui-*`, иконки. Тема должна использовать эти переменные, а не жёсткие px, иначе не будет адаптации.
5. **Мобильный хром:** `--navbar-height`, `--navbar-radius`, `--navbar-width/max-width`, `--mobile-sidebar-*`, `--mobile-toolbar-height`, `--touch-size-*`, `--touch-radius-*`. Всё это в `.is-mobile`.
6. **Нет кнопок «открыть папку»** и контекстного меню «Show in folder» — они desktop-only. На мобиле установка идёт через магазин или ручное копирование файлов.
7. **Ручная установка** на мобиле: скопировать папку темы в `<vault>/.obsidian/themes/<Name>/` (или legacy `.css` в `themes/`). После копирования нужно переоткрыть vault/приложение: список тем читается при старте и по событиям `raw` от адаптера; внешнее копирование через файловый менеджер может не дать события в реальном времени.
8. **Импорта темы из файла в 1.15 нет.** В UI есть только каталог. Альтернативы — ручное копирование файлов в vault и внутренний CLI `theme:install`, который тоже ставит тему из каталога (не из произвольного файла).
9. **CSP и ассеты:** см. §5.5. Для мобилы особенно важно не использовать относительные пути к картинкам/шрифтам в папке темы.
10. **WebView debugging** включён в `capacitor.config.json` (`android.webContentsDebuggingEnabled: true`) — можно инспектировать DOM и вычисленные переменные через `chrome://inspect`, если нужно отлаживать тему на устройстве.

---

## 10. Подводные камни и что не копировать

| Проблема | Причина | Как правильно |
|---|---|---|
| Тема не появилась в списке | имя папки ≠ `manifest.name`, нет `theme.css`, битый JSON | папка = `manifest.name`; оба файла на месте |
| Modern-тема не установилась | нет релиза и нет `theme.css` в HEAD; нет `versions.json` при старом клиенте | публиковать `manifest.json` + `theme.css` в HEAD и/или релиз с ассетами `manifest.json`/`theme.css` |
| Legacy-тема не обновилась | у неё синтетический манифест `0.0.0`; обновление сравнивает содержимое `obsidian.css` | для обновлений лучше modern-формат |
| Тема «ломает» чужой акцент | тема пытается перебить inline `--accent-*` | не трогать `--accent-h/s/l`, если пользователь выбрал акцент; оформлять через `--color-accent*` |
| Шрифт темы не применяется | тема пишет в `--font-text-override` | писать в `--font-text-theme` (и аналогично для interface/monospace) |
| Картинки темы не грузятся на мобиле | относительный `url()` разрешается от `http://localhost` | `data:` URI или абсолютный `https://` |
| Внешний `@import` не работает | CSP `style-src` | только `fonts.googleapis.com` (и same-origin) |
| Тема выглядит только в одном режиме | оформлен только `.theme-dark` или только `.theme-light` | оформить оба класса |
| Обновление не приходит | `version` не менялся / нет манифеста в HEAD | поднимать `version` и публиковать `manifest.json` |
| Тема не применяется после первого запуска плагинов | `globallyDisable()` на время запроса доверия | закрыть диалог доверия |
| Организационная политика блокирует | `policy.themes` = `false`/массив без имени | добавить имя в allowlist |

Отдельно: **не копировать** подход «тема = произвольный HTML/JS». В Obsidian тема — это только CSS, и это осознанное ограничение безопасности. Если в своём приложении делать темы, стоит сохранить этот контракт: CSS-файл + манифест, без исполнения кода, с явным списком переменных.

---

## 11. Чек-лист реализации (контракт для собственного приложения)

Минимальный набор, чтобы повторить поведение Obsidian Mobile:

### 11.1 Данные

```
ThemeManifest {
  name: string            // = имя папки
  version: string
  minAppVersion?: string
  author?: string
  authorUrl?: string
}

ThemeEntry {              // каталог
  name: string
  author: string
  repo: string
  screenshot: string
  modes: ("dark"|"light")[]
  legacy?: boolean
}

AppearanceConfig {
  theme: "obsidian"|"moonstone"|"system"
  cssTheme: string
  accentColor: string
  enabledCssSnippets: string[]
  interfaceFontFamily/textFontFamily/monospaceFontFamily: string
  baseFontSize: number
}
```

### 11.2 Сканирование

```
scanThemes(configDir):
  for folder in list(configDir/themes):
    if !exists(folder/theme.css): continue
    manifest = parse(folder/manifest.json) or continue
    if manifest.name != basename(folder): continue
    themes[manifest.name] = manifest + dir
  for file in list(configDir/themes):
    if file.endsWith(".css") && !basename(file).startsWith("."):
      legacy[basename(file, ".css")] = file
```

### 11.3 Применение

```
applyTheme(name):
  css = name == "" ? "" : read(themePath(name))
  themeStyle.textContent = css

applySnippets(names):
  for i, name in enabledSnippets:
    style[i].textContent = read(configDir/snippets/name.css)
  remove extra style nodes

themePath(name):
  return (!themes.contains(name) && legacy.contains(name))
           ? configDir/themes/name.css
           : configDir/themes/name/theme.css
```

Порядок узлов: `appStyles → themeStyle → snippetStyle[0..n]`.

### 11.4 Скачивание

```
catalog = GET raw.githubusercontent.com/obsidianmd/obsidian-releases/HEAD/community-css-themes.json

resolveVersion(repo, manifest):
  if !manifest.minAppVersion || currentVersion >= manifest.minAppVersion:
    return manifest.version
  versions = GET raw/<repo>/HEAD/versions.json
  return max { r | versions[r] <= currentVersion } or null

install(entry):
  if !policyAllows(entry.name): return
  version = resolveVersion(entry.repo, manifestHead)
  if version == null: installLegacy(entry)
  else:
    try release assets; on 404 fallback to HEAD
    verify manifest.name == entry.name
    write themes/<name>/{manifest.json,theme.css}
    remove themes/<name>.css
  rescan()
```

### 11.5 Что должно быть в CSS-слое для совместимости

Если цель — запускать существующие темы, нужно предоставить как минимум:

- палитру `.theme-dark`/`.theme-light` и `color-scheme`;
- семантические переменные `body` (полный список — в приложении A), в первую очередь:
  `--background-*`, `--text-*`, `--interactive-*`, `--color-*`, `--font-*`,
  `--file-line-width`, `--line-height-normal`, `--p-spacing`, `--radius-*`,
  `--titlebar-*`, `--status-bar-*`, `--ribbon-*`, `--nav-*`, `--tab-*`,
  `--modal-*`, `--prompt-*`, `--menu-*`, `--suggestion-*`, `--code-*`,
  `--callout-*`, `--tag-*`, `--table-*`, `--embed-*`, `--metadata-*`,
  `--checkbox-*`, `--toggle-*`, `--slider-*`, `--dropdown-*`, `--icon-*`;
- мобильные переменные `.is-mobile`: `--navbar-*`, `--mobile-sidebar-*`,
  `--touch-size-*`, `--system-status-background`, `--font-preferred-size`;
- классы на корне: `theme-dark`/`theme-light`, `is-mobile`, `is-phone`/`is-tablet`,
  `is-android`/`is-ios`;
- inline-слоты пользователя: `--accent-h/s/l`, `--text-on-accent`,
  `--font-*-override`, `--font-text-size`, `--indent-size`.

### 11.6 Отличия, которые стоит сохранить

| Решение Obsidian | Зачем |
|---|---|
| тема = CSS + manifest, без JS | безопасность, предсказуемость, лёгкая синхронизация |
| папка = имя темы | однозначная идентификация без реестра |
| `cssTheme` в конфиге vault | тема переносится вместе с vault |
| тема и сниппеты отдельными `<style>` | сниппеты могут переопределять тему без её правки |
| inline-акцент/шрифты пользователя | пользовательские настройки сильнее темы |
| release → HEAD fallback | темы без релизов всё равно устанавливаются |
| прокси для GitHub | обход сетевых/CORS-ограничений в WebView |
| legacy `.css` | обратная совместимость со старым каталогом |
| policy allowlist | корпоративное управление |

---

## 12. Приложение

- **A. CSS-переменные:** `obsidian-mobile-themes-variables.md` — 899 переменных основного `body`, шрифтовые переменные, палитры `.theme-dark`/`.theme-light`, `.is-mobile`/`.is-phone`/`.is-tablet`, `:root`.
- **B. Карта селекторов:** `obsidian-mobile-themes-selectors.md` — 401 блок «селектор → объявленные переменные».

Воспроизвести разбор:

```bash
# из корня репозитория
node "D:/Naben/Github/REA/node_modules/esbuild/bin/esbuild" \
  "Obsidian/Obsidian-1.15.0/assets/public/app.js" \
  --format=esm --minify=false \
  --outfile="Obsidian/_analysis/app_pretty.js"
```

Ключевые места в `app_pretty.js`:

| Что | Строки |
|---|---|
| Константы URL и `wr`/`xg`/`hr` | 23894–24009 |
| `Hr` (semver), `UO`/`$n` (политика) | 24043, 24401 |
| `Xs` (выбор версии) | 24295 |
| Константы `theme.css`/`manifest.json`/каталога/статистики | 25902–25910 |
| Менеджер тем `C4` | 25910–26169 |
| Быстрый выбор темы `S4` | 26170–26205 |
| Конфиг vault `Bn` (`app.json` + `appearance.json`) | 27169–27270 |
| Ключи appearance и дефолты `aae`/`sae` | 27043 |
| Магазин тем `Z1` | 61712–61910 |
| Вкладка Appearance `KD` | 81762–82103 |
| Версия API `Br`, размер шрифта `iU` | 83104 |
| `updateTheme` / `updateMobileFrameTheme` | 83578–83608 |
| `updateFontFamily` / `updateFontSize` | 83609–83631 |
| `setAccentColor` | 83644–83660 |

