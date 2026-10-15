# Система плагинов. Часть 4: совместимость с плагинами Obsidian

Этот документ описывает, как довести долю запускаемых готовых плагинов Obsidian до измеримого уровня, не ослабляя модель безопасности из `plugins-00-core.md`. Он заменяет и уточняет: core §6.2, этап 5 (Micro-DOM, «Путь Б»), JS §7.4, JS §7.7, JS §7.8.

| Документ | О чём |
|---|---|
| `plugins-00a-adaptation.md` | адаптация текущего кода |
| `plugins-00-core.md` | общее ядро и дорожная карта |
| `plugins-01-js-obsidian.md` | JS-рантайм, API в стиле Obsidian |
| `plugins-04-obsidian-compat.md` (этот) | совместимость с готовыми плагинами Obsidian |

Ссылки «core §N», «JS §N», «00a §N» ведут в соответствующие документы.

> **Оговорки.**
> 1. Справочник API (раздел 5), список классов Obsidian (раздел 6) и предварительная оценка известных плагинов (раздел 11) составлены по памяти об API Obsidian и типовых бандлах. Они **не сверялись** с актуальным `obsidian.d.ts` и реальными бандлами. Первая задача этапа 0: сверить и исправить.
> 2. Колонка «частота» в справочнике пустая намеренно. Её заполняет замер (этап 0), а не оценка.
> 3. Код это эскизы, не проверялись компиляцией и запуском.
> 4. Сроки грубые, пересматриваются после этапа 0.

---

## 0. Уточнения к предыдущей версии плана

| Что | Было | Стало | Почему |
|---|---|---|---|
| Порядок Этапов 3 и 4 | сначала `Setting`/`Modal`, потом DOM-дескриптор | Этап 3 включает **скелет** виртуального элемента (`VElement`), Этап 4 расширяет его | `Setting(containerEl)` и `PluginSettingTab.containerEl` без контейнера не работают, цикличная зависимость |
| `ItemView` | Этап 8 | Базовая версия в Этапе 4 | Правая панель уже есть, а `ItemView` по сути заполнение `contentEl` тем же `VElement` |
| `require("obsidian")` | одно исключение | исключение плюс таблица «ленивых заглушек» для остальных внешних модулей | Типовой бандл Obsidian оставляет внешними `obsidian`, `electron`, `@codemirror/*`, `@lezer/*` и встроенные модули Node |
| Proxy-аудит | заглушка на любое отсутствующее свойство | заглушка только для реальных API Obsidian, которые мы не реализовали, остальное `undefined` | Плагины определяют возможности проверкой свойства, заглушка ломает такую проверку (раздел 9.3) |
| Идентичность `TFile` | не упоминалась | обязательное требование Этапа 2 | Плагины сравнивают `file === other` и используют файлы как ключи `Map` (раздел 4, Этап 2) |

---

## 1. Цели, не-цели, метрики

### 1.1 Цели

1. Довести долю корпуса плагинов, работающих **без правок**, до порога, который вы зададите после этапа 0.
2. Каждое решение принимать по данным замера, а не по предположениям.
3. Любое ограничение показывать пользователю явно: бейдж совместимости, понятная ошибка, ссылка на руководство.
4. Совместимость не ослабляет безопасность: права, лимиты, allowlist, verify, boot guard работают так же.

### 1.2 Не-цели

- Полная совместимость с Obsidian (это потребовало бы DOM, CSS-движка и CodeMirror 6).
- Запуск плагинов, которым нужны Electron, Node (`fs`, `child_process`), произвольный DOM с внешними UI-библиотеками (React, Svelte, Chart.js), canvas.
- Совместимость приватных API (`app.internalPlugins`, `app.dom`, прямая работа со `scope` и т.п.).

### 1.3 Метрики

Статус плагина определяется по трём независимым уровням:

| Уровень | Вопрос | Как проверяется |
|---|---|---|
| L1 Загрузка | Бандл загрузился, `onload` отработал без исключения, регистрации в пределах лимитов | автоматически, `plugin-verify` + корпус |
| L2 Функция | Основная функция работает (команды выполняются на тестовом vault, вкладка настроек открывается, панель рисуется) | автоматические smoke-тесты + ручной чек-лист для топ-10 |
| L3 Паритет | Внешний вид и поведение близки к Obsidian | ручная проверка, только для выборки |

Итоговая отметка плагина:
- 🟢 **Full**: L1 и L2 пройдены, деградаций нет.
- 🟡 **Partial**: L1 пройден, часть функций недоступна (список в отчёте).
- 🔴 **Incompatible**: L1 не пройден или основная функция не работает.

**Главная метрика релиза хоста:** доля корпуса в 🟢/🟡/🔴 (раздел 10). Число реализованных методов метрикой не считается.

**Статистическая оговорка.** При корпусе в 60 плагинов и доле около 50% 95%-й доверительный интервал примерно ±12 п.п. (`1.96·√(0.25/60)`). Если нужна точность лучше ±8 п.п., корпус нужен около 150. Для решений «строить слой или нет» хватает 60, для публичных заявлений о проценте нет.

---

## 2. Принципы совместимости

0. **Аксиома универсальности (никаких ad-hoc подстроек под конкретные плагины).** Хост никогда не внедряет частные «костыли», проверки по `plugin_id`, жестко захардкоженные селекторы или стили одного конкретного плагина. Все реализации API, Micro-DOM-маппинги, правила раскладки и стилизации обязаны быть системными, обобщёнными и специфицированными по дизайн-системе Obsidian (стандартные CSS-переменные `--background-*`, `--text-*`, стандартные классы `modal-*`, `setting-*`, `nav-*`, `mod-cta`, `mod-warning`, `mod-cancel`, общие flex/grid/flow layout-примитивы и семантическая типографика). Любой вызов, семантический узел или стиль, внедряемый в систему, обязан работать эквивалентно и предсказуемо для любых других плагинов из каталога.
1. **Совпадает поведение: совпадает имя и сигнатура.** Нет в хосте: явная ошибка, а не молчаливая заглушка.
2. **Нативный компонент важнее эмуляции.** `Setting`, `Modal`, `Notice` строят `UiNode` напрямую, а не «суп из `div`».
3. **Эмуляция DOM минимальна и расширяется только по данным замера.**
4. **Синхронность Obsidian сохраняется там, где она дёшева** (геттеры по снимку, чистые функции), и заменяется явной моделью там, где её нельзя сохранить (редактор, межплагинные вызовы).
5. **Права вычисляются из кода, подтверждает пользователь.** Отсутствие `permissions` в манифесте Obsidian не означает «разрешено всё».
6. **Определение возможностей должно работать как в Obsidian**: `if (app.x)`, `typeof f === "function"`, `Platform.isMobile`, `requireApiVersion(...)`.
7. **Идентичность объектов важна**: один `TFile` на путь, стабильные ключи элементов при перерисовке.
8. **Всё, что плагин не может сделать, фиксируется в отчёте** (сканер статически, Proxy в рантайме).
9. **Регрессия совместимости блокирует релиз** (CI-корпус, раздел 10).
10. **Никакого копирования кода и типов Obsidian.** Свои `.d.ts`, свои реализации. Лицензии портируемых плагинов проверяются отдельно.

---

## 3. Этап 0. Замер (≈1 неделя)

### 3.1 Что получаем на выходе

1. `corpus/` с бандлами (вне репозитория, см. 3.2).
2. `scan-results/*.json`: по плагину какие вызовы, какие внешние модули, какие библиотеки вшиты.
3. `calls.csv`: таблица «вызов → число плагинов» (раздел 3.5). Она задаёт порядок реализации и пополняет колонку «частота» в разделе 5.
4. `libs.csv`: какие плагины тянут React, Svelte, Chart.js, jQuery и т.п. (граница, за которой DOM-слой бесполезен).
5. Первое число: сколько плагинов корпуса *теоретически* покрываются планом (раздел 3.6).
6. Решение по вопросам из раздела 14.

### 3.2 Корпус

**Источники данных (проверить актуальность адресов):**
- `community-plugins.json` и `community-plugin-stats.json` в репозитории `obsidianmd/obsidian-releases`.
- Для каждого плагина: релиз на GitHub, ассеты `main.js`, `manifest.json`, `styles.css` (если есть), доступны по `https://github.com/<repo>/releases/latest/download/<файл>`.

**Состав (рекомендация, 60 плагинов):**
- 30 по числу загрузок (сверху вниз).
- 30 случайных из остальных (с фиксированным seed), чтобы не завысить результат популярными плагинами.
- Внутри выборки проверить, что представлены категории: заметки и команды, настройки и модалки, боковые панели, редактор и подсветка, code blocks и рендер, сеть и интеграции, оформление (CSS-плагины в корпус не берутся, они не про JS).

**Хранение.** Лицензии плагинов разные, поэтому сами бандлы в репозиторий проекта не коммитить. В репозитории лежит только `corpus.toml` со списком (`id`, `repo`, `tag`, `sha256` бандла) и скрипт загрузки. Скачанное лежит в игнорируемой папке или в кэше CI.

### 3.3 Скрипт статического сканирования (эскиз)

```js
// tools/compat-scan/scan.mjs
import fs from "node:fs";
import * as acorn from "acorn";
import * as walk from "acorn-walk";

const RECEIVERS = new Set(["vault", "workspace", "metadataCache", "fileManager",
  "adapter", "commands", "plugins", "setting", "keymap", "scope"]);
const DOM_METHODS = new Set(["createEl", "createDiv", "createSpan", "createSvg",
  "empty", "addClass", "removeClass", "toggleClass", "setAttr", "setAttrs", "getAttr",
  "setText", "setCssStyles", "setCssProps", "find", "findAll", "findAllSelf", "detach",
  "appendText", "insertAfter", "hasClass", "show", "hide", "isShown", "onClickEvent",
  "querySelector", "querySelectorAll", "appendChild", "insertBefore", "removeChild",
  "addEventListener", "classList", "dataset", "innerHTML", "outerHTML", "style",
  "getBoundingClientRect", "scrollIntoView", "focus", "closest", "matches"]);
const LIB_MARKERS = {
  react:   ["__SECRET_INTERNALS_DO_NOT_USE_OR_YOU_WILL_BE_FIRED", "react.element"],
  svelte:  ["svelte/internal", "$$scope", "$$props"],
  preact:  ["__PREACT", "preact/hooks"],
  vue:     ["__VUE__", "createVNode"],
  jquery:  ["jQuery.fn.init", "jquery"],
  chartjs: ["Chart.register", "chart.js"],
  d3:      ["d3-selection", "d3.select"],
  moment:  ["moment.version", "momentProperties"],
  codemirror6: ["@codemirror/view", "EditorView.decorations"],
};

export function scanBundle(src) {
  const ast = acorn.parse(src, { ecmaVersion: "latest", sourceType: "script",
    allowReturnOutsideFunction: true, allowHashBang: true });
  const obsVars = new Set();
  const destructured = new Map();
  const externals = new Map();
  const used = new Map();
  const bump = (k) => used.set(k, (used.get(k) ?? 0) + 1);
  const isReq = (n) => n?.type === "CallExpression" &&
    n.callee.type === "Identifier" && ["require", "__require"].includes(n.callee.name) &&
    n.arguments[0]?.type === "Literal";

  walk.simple(ast, {
    VariableDeclarator(n) {
      if (!isReq(n.init)) return;
      const mod = n.init.arguments[0].value;
      externals.set(mod, (externals.get(mod) ?? 0) + 1);
      if (mod === "obsidian") {
        if (n.id.type === "Identifier") obsVars.add(n.id.name);
        if (n.id.type === "ObjectPattern")
          for (const p of n.id.properties)
            if (p.key?.name) destructured.set(p.value?.name ?? p.key.name, p.key.name);
      }
    },
    CallExpression(n) {
      if (isReq(n)) {
        const mod = n.arguments[0].value;
        if (!externals.has(mod)) externals.set(mod, 1);
      }
    },
  });

  walk.simple(ast, {
    MemberExpression(n) {
      const prop = n.property.name ?? n.property.value;
      if (!prop) return;
      if (n.object.type === "Identifier" && obsVars.has(n.object.name))
        bump(`obsidian.${prop}`);
      if (n.object.type === "MemberExpression") {
        const recv = n.object.property.name;
        if (RECEIVERS.has(recv)) bump(`${recv}.${prop}`);
      }
      if (DOM_METHODS.has(prop)) bump(`dom.${prop}`);
      if (n.object.type === "Identifier" && ["document", "window", "navigator",
          "localStorage", "globalThis"].includes(n.object.name)) bump(`global.${n.object.name}.${prop}`);
    },
    Identifier(n) {
      if (destructured.has(n.name)) bump(`obsidian.${destructured.get(n.name)}`);
    },
    CallExpression(n) {
      if (n.callee.type === "Identifier" && n.callee.name === "eval") bump("danger.eval");
    },
    NewExpression(n) {
      if (n.callee.type === "Identifier" && n.callee.name === "Function") bump("danger.new_function");
    },
  });

  const libs = Object.entries(LIB_MARKERS)
    .filter(([, marks]) => marks.some((m) => src.includes(m))).map(([k]) => k);

  return { bytes: src.length, externals: Object.fromEntries(externals),
           used: Object.fromEntries(used), libs };
}

if (process.argv[1].endsWith("scan.mjs")) {
  const [, , file] = process.argv;
  console.log(JSON.stringify(scanBundle(fs.readFileSync(file, "utf8")), null, 2));
}
```

---

## 4. Этапы реализации (E1…E8)

### E1. Фундамент загрузки (≈1–2 недели)
- Реестр модулей `require` с ленивыми заглушками (`lazyFail`) для `@codemirror/*`, `electron` и др.
- Политика `verify` для режима совместимости (`compat: "obsidian"`).
- Поддержка оригинального формата `manifest.json`.
- Единый крейт `plugin-api`.
- Базовые классы `Component`, `Events`, `Plugin`, стандартные глобальные объекты.

### E2. Ядро данных и событий (≈2–3 недели)
- `Vault` с гарантией идентичности `TFile`/`TFolder` (`Map<path, TFile>`).
- Изоляция снимка: плагин без `vault: read` не видит файлов и метаданных.
- Атомарное сохранение `vault.process` и сохранение открытых файлов через редактор.
- Инкрементальный кэш метаданных `MetadataCache`.
- `Workspace` (без UI-вьюх), команды с префиксом `pluginid:`.
- `loadData`/`saveData`, безопасный `fetch` как обёртка над `requestUrl`.

### E3. Нативные компоненты UI (≈3–4 недели)
- Скелет виртуального элемента `VElement` (`containerEl`, `createEl/Div/Span`, `empty()`, `setText`, `addClass`).
- Реализация `Setting` и его контролов (`Text`, `Toggle`, `Select`, `Slider`, `Button`, `TextArea`).
- Интеграция `PluginSettingTab` в настройки хоста.
- Модальные окна `Modal`, `SuggestModal`, `FuzzySuggestModal`, `Notice`.
- Контекстные меню `Menu`, `MenuItem`, каскадные подменю `setSubmenu()` и событие `workspace.on("editor-menu")` (прямой кейс `obsidian-amount-in-words-np`).
- Семантические узлы `UiNode` (`SettingItem`, `Toggle`, `Select`, `Slider`, `Heading`, `Callout`, `Table`).

### E4. Расширенный DOM и `ItemView` (≈2–3 недели)
- Расширение `VElement`: обход дерева, классы, атрибуты, селекторы `querySelector`/`closest`.
- Обработка `innerHTML` через безопасный HTML-парсер хоста.
- Базовая поддержка `ItemView` (боковые панели: `.view-header`, `.view-content`).
- Защита от утечек через разметку (`img[src]` только allowlist/vault, блокировка `iframe`/`script`).

### E5. Markdown и code blocks (≈3 недели)
- `registerMarkdownCodeBlockProcessor`: асинхронный запуск, кэш по хэшу, пересчет `VisualRowHeightIndex`.
- `MarkdownRenderer.render` в нативные узлы хоста.
- Постпроцессоры `registerMarkdownPostProcessor` принимаются как заглушки до E8b.

### E6. Редактор (≈3–4 недели)
- UTF-16 кодовые единицы для всех позиций и диапазонов (`EditorPosition`).
- Модель «локальное зеркало редактора (`EditorSession`) + пакет правок по версии (`ApplyEdits`)».
- Атомарное применение группы правок одной транзакцией истории хоста (`Ctrl+Z`).
- Взаимодействие с контекстным меню: `editor.getSelection()`, `editor.replaceSelection()` с атомарной транзакцией Undo.
- `editor.cm` возвращает `undefined` для совместимости с проверками CM6.

### E7. Инструменты совместимости (параллельно E1–E6)
- Единый машинный реестр API хоста (`compat_registry.rs`).
- Статический сканер бандлов на Rust (`oxc`).
- Точный Proxy-аудит (раздел 9.3): возврат `undefined` для несуществующих в Obsidian свойств, явная ошибка для нереализованных, запрет перехвата `then`/`Symbol`.
- Модель `CompatibilityReport` и бейджи совместимости в каталоге.

### E8. Расширения по данным (по результатам E0)
- E8a: Подмножество CSS из `styles.css`.
- E8b: HTML-представление для Markdown постпроцессоров.
- E8c: `EditorSuggest`.
- E8d: Межплагинные вызовы через сериализуемый RPC.
- E8f: Импорт настроек и хоткеев из `.obsidian/`.

---

## 5. Справочник API

*(Полный справочник соответствует разделу 5 спецификации: экспорты `obsidian`, методы `Plugin`, `Vault`, `Workspace`, `Editor`, `Setting`, `Modal`, `MetadataCache` с присвоенными уровнями поддержки ✅, 🟡, 🔴, ⛔).*

---

## 6. Правила трансляции DOM → `UiNode` (Обобщённый Micro-DOM)

- **Семантическое сопоставление тегов**: стандартные HTML-элементы (`button`, `input[type=checkbox]`, `select`, `textarea`, `h1`–`h6`, `p`, `span`, `hr`) преобразуются в соответствующие варианты `UiNode`.
- **Обобщённая классификация контейнеров (Row vs Column)**:
  - Элементы классифицируются как `Row` при наличии CSS-стилей `display: flex; flex-direction: row`, классов `row`, `flex-row`, либо семантических суффиксов дизайн-системы Obsidian: `*-item`, `*-row`, `*-controls`, `*-footer`, `*-stats`, `*-buttons`, `modal-button-container`, `tree-item-self`, `nav-file-title`.
  - Элементы классифицируются как `Column` для вертикальных списков, форм и карточек: `*-list`, `*-info`, `modal-content`, `modal-container`, `vertical-tab-content`.
- **Обобщённая семантика кнопок и контролов**:
  - `mod-cta`, `mod-primary`, `*-primary` $\to$ `ButtonVariant::Primary` (акцентный цвет темы, `bg_accent`).
  - `mod-warning`, `mod-danger`, `*-danger` $\to$ `ButtonVariant::Danger` (красный акцент темы; с учётом `disabled` / `--active` состояний).
  - `mod-cancel`, `*-ghost`, `*-secondary` $\to$ `ButtonVariant::Secondary` (нейтральная рамка, фон `bg_sidebar`).
- **Обобщённая типографика**:
  - Суффиксы `*-title`, `*-name`, `*-header` $\to$ основной шрифт средней насыщенности (`text_sm`, `FontWeight::MEDIUM`).
  - Суффиксы `*-subtitle`, `*-desc`, `*-path`, `*-muted` $\to$ уменьшенный приглушённый шрифт (`text_xs`, `text_muted`).
  - Суффиксы `*-count`, `*-badge`, `*-stats` $\to$ компактный полужирный шрифт верхнего регистра (`text_xs`, `FontWeight::SEMIBOLD`).
- **Преобразование стилей и CSS-переменных**:
  - Токены Obsidian (`--background-primary`, `--background-secondary`, `--background-modifier-border`, `--background-modifier-hover`, `--text-normal`, `--text-muted`, `--interactive-accent`) транслируются в соответствующие токены темы хоста (`bg_card`, `bg_sidebar`, `border`, `bg_hover`, `text_primary`, `text_muted`, `accent`).
  - Жестко захардкоженные стили под один конкретный плагин запрещены. Все правила работают единообразно для любых плагинов экосистемы Obsidian.
- **Безопасность разметки**: `img[src]` строго проверяется по allowlist доменов плагина, `javascript:` и `file:` запрещены, внешние ссылки требуют явного клика и подтверждения пользователя.

---

## 7. Протокол событий и идентичность элементов

- События UI передаются со снимком значений всех полей (`values: Vec<(String, String)>`), предотвращая чтение устаревших значений в `click`.
- Детерминированный расчет ключей элементов:
  $$\text{key}(el) = \text{hash}(\text{parent.key}, \text{tag}, \text{explicit\_key}, \text{sibling\_index})$$
  позволяет сохранять фокус и курсор при перерисовках через `containerEl.empty()`.

---

## 8. Права, одобрения и импорт

- Вычисление прав по статическому анализу бандла (верхняя граница).
- Хранение выданных пользователем прав в `%APPDATA%/Aquilum/plugin_grants.json` по ключу `(vault_id, plugin_id, sha256_bundle)`.
- Мастер безопасного импорта из `.obsidian/plugins/` с показом отчёта совместимости до активации.

---

## 9. Сканер, Proxy-аудит, отчёт

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Verdict { Full, Partial, Incompatible }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityReport {
    pub verdict: Verdict,
    pub score: f32,
    pub supported: Vec<ApiFinding>,
    pub partial: Vec<ApiFinding>,
    pub missing: Vec<ApiFinding>,
    pub incompatible: Vec<ApiFinding>,
    pub libs: Vec<String>,
    pub externals: Vec<String>,
    pub inferred_permissions: Permissions,
    pub reasons: Vec<String>,
    pub scanned_with: String,
}
```

Proxy-аудит:
- Не перехватывает `then`, `Symbol`, `toJSON`, `constructor`.
- Возвращает `undefined` для свойств, отсутствующих в спецификации Obsidian.
- Возвращает функцию-ошибку для известных, но не реализованных в хосте методов.
- Возвращает безопасную заглушку для методов из списка `Support::Stub`.

---

## 10. Регрессионный корпус и CI

- Фиксация тестового корпуса плагинов в `corpus/corpus.toml` с контрольными суммами SHA-256.
- Автоматизированный smoke-тест (L1 + L2) в CI.
- Фиксация эталонных деревьев `UiNode` для 10–15 ключевых плагинов с блокировкой регрессий рендера.

---

## 11. Принятые архитектурные решения

1. **Режим совместимости:** По умолчанию выключен, активируется только для импортируемых/каталожных Obsidian плагинов.
2. **Внешние импорты:** Принимается таблица E1.1 (разрешен `obsidian`, ленивые `lazyFail Proxy` для `@codemirror/*`, `electron`).
3. **Постпроцессоры Markdown:** На этапе E5 принимаются как no-op (🟡 `Stub`), переход к E8b — только по данным E0.
4. **Межплагинный доступ:** Прямой доступ к памяти чужих плагинов заблокирован (🔴), взаимодействие только через асинхронные события.
5. **Переводы строк:** Нормализованный `\n` в редакторе, сохранение оригинального формата (CRLF/LF) при записи на диск.
6. **Версия эмулируемого API:** `obsidianCompat = "1.5.12"`.
7. **Телеметрия:** Исключительно локальный аудит в консоли хоста.
