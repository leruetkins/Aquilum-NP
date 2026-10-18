# Плагинная система Obsidian: разбор по бинарнику

**Источник:** `D:\Programs\Obsidian\resources\obsidian.asar` → `app.js`
**Версия сборки:** 1.7.5 (`package.json` внутри asar, совпадает с `z4="1.7.5"` в бандле)
**Объём:** 23 009 201 байт asar, 341 файл, `app.js` = 3 172 382 байта **в одну строку**, source maps отсутствуют (0 файлов)
**Метод:** статический разбор минифицированного webpack-IIFE бандла, вытащенного в `.workbuddy-ai/tmp/obsidian-app.js`. Имена идентификаторов потеряны, но **структура, строки и литералы сохранились** — этого достаточно, чтобы восстановить контракты API.

Ниже — не каталог методов, а **устройство**: какие инварианты держат систему, где проходят границы, и что из этого стоит перенести в собственный заметочник.

---

## 1. Карта слоёв

Три класса и четыре реестра — всё остальное производное.

```
┌──────────────────────────────────────────────────────────────┐
│ App (X4)                                                     │
│   vault · workspace · metadataCache · fileManager · commands │
│   setting · viewRegistry · internalPlugins · plugins         │
├──────────────────────────────────────────────────────────────┤
│ PluginManager (BZ)          │ InternalPlugins (KZ)            │
│   community-плагины         │   встроенные (core) плагины     │
│   manifests/plugins/enabled │   descriptor + wrapper (jZ)     │
├──────────────────────────────────────────────────────────────┤
│ Plugin (LZ)  ──extends──▶  Component (gA)                    │
│   app, manifest, loadData   │  _loaded/_events/_children      │
│   addCommand/registerView   │  load/unload/register*          │
├──────────────────────────────────────────────────────────────┤
│ Реестры: NK (команды) · IK (представления) · V4 (настройки)  │
│          pb (эмиттер: on/trigger/offref)                     │
└──────────────────────────────────────────────────────────────┘
```

Ключевое решение: **`Component` — это владелец ресурсов, `Plugin` — это фасад над `App`.** Плагин не «регистрирует» ничего напрямую: он вызывает метод фасада, а тот пишет в реестр и **сразу же** кладёт в очередь функцию отмены.

---

## 2. `Component` — фундамент (класс `gA`)

Полный код в бандле (читается почти без потерь):

```js
function e(){ this._loaded=!1, this._events=[], this._children=[] }

load(){ if(!this._loaded){ this._loaded=!0; this.onload();
        for(var e=0,t=this._children.slice();e<t.length;e++) t[e].load() } }

onload(){}

unload(){ if(this._loaded){ this._loaded=!1;
    for(var e=this._children, t=this._events; e.length>0;){ e.pop().unload() }
    for(; t.length>0;){ t.pop()() }
    this.onunload() } }

onunload(){}

addChild(e){ this._children.push(e); this._loaded && e.load(); return e }
removeChild(e){ var t=this._children,n=t.indexOf(e); -1!==n&&(t.splice(n,1),e.unload()); return e }

register(e){ this._events.push(e) }
registerEvent(e){ this.register(function(){ return e.e.offref(e) }) }
registerDomEvent(e,t,n,i){ e.addEventListener(t,n,i);
    this.register(function(){ return e.removeEventListener(t,n,i) }) }
registerScopeEvent(e){ this.register(function(){ return e.scope.unregister(e) }) }
registerInterval(e){ this.register(function(){ return clearInterval(e) }); return e }
```

Разбор по пунктам — здесь каждый порядок строки имеет смысл:

| Место | Смысл |
|---|---|
| `_loaded = true` **до** `onload()` | реентерабельность: `register*` внутри `onload` работает, `addChild` внутри `onload` немедленно грузит ребёнка |
| `_children.slice()` | копия — `onload` может добавлять детей во время обхода |
| дети выгружаются **до** `onunload()` | к этому моменту собственные обработчики плагина уже сняты, `onunload` видит чистое состояние |
| `_events` разбираются **LIFO** (`pop`) | отмена идёт в обратном порядке регистрации — как вложенные `try/finally` |
| `unload()` **синхронный** | асинхронный `onunload` не ожидается (см. §11) |
| `registerInterval` возвращает id | `window.setInterval(...)` остаётся в переменной плагина, но отмена уже привязана |
| `registerDomEvent(el, type, cb, opts)` | `opts` сохраняются для симметричного `removeEventListener` — иначе listener не снимется |

**Это единственный инвариант, который делает выгрузку корректной.** Плагин никогда не «забывает» снять обработчик, потому что не снимает его вручную — он только кладёт disposer в очередь. Всё остальное в системе построено вокруг этого.

---

## 3. `Plugin` — фасад (класс `LZ`)

```js
function t(t,n){ ... i.app=t, i.manifest=n, i }
```

Конструктор: `new Plugin(app, manifest)`. Ничего больше — вся инициализация в `onload`.

```js
load(){ ... this._loaded=!0; await this.onload();     // ← переопределён на async
        for(child of this._children.slice()) child.load() }
```

Единственное отличие от `Component`: `onload` может быть `async` и его ждут. Дети всё равно грузятся синхронно, после.

### 3.1 Регистрация — единый шаблон

Все методы — один и тот же трёхшаговый паттерн: **изменить реестр → записать отмену → вернуть объект**.

```js
addCommand(e){
  e.id   = this.manifest.id + ":" + e.id;        // неймспейсинг
  e.name = this.manifest.name + ": " + e.name;   // префикс в палитре
  this.app.commands.addCommand(e);
  this.register(function(){ return t.app.commands.removeCommand(e.id) });
  return e
}

removeCommand(e){ this.app.commands.removeCommand(this.manifest.id+":"+e) }  // принимает КОРОТКИЙ id

addRibbonIcon(e,t,n){                             // (icon, title, callback)
  var r = this.manifest.id + ":" + t;             // ← id иконки = id плагина + ЗАГОЛОВОК
  var o = i.workspace.leftRibbon.addRibbonItemButton(r,e,t,n);
  this.register(function(){ i.workspace.leftRibbon.removeRibbonAction(r); o.detach() });
  return o
}

addStatusBarItem(){
  var e = this.app.statusBar.registerStatusBarItem();
  e.addClass("plugin-" + this.manifest.id.toLowerCase().replace(/[^_a-zA-Z0-9-]/,"-"));
  this.register(function(){ return e.detach() });
  return e
}

addSettingTab(e){ this.app.setting.addSettingTab(e);
  this.register(function(){ return t.app.setting.removeSettingTab(e) }) }

registerView(e,t){                                // (viewType, creator)
  this.app.viewRegistry.registerView(e,t);
  this.register(function(){
    n.app.viewRegistry.unregisterView(e);
    n._userDisabled && n.app.workspace.detachLeavesOfType(e)   // ← см. §11
  })
}
```

### 3.2 Полная поверхность `Plugin`

| Метод | Что делает | Куда пишет |
|---|---|---|
| `addCommand(cmd)` | команда + горячие клавиши по умолчанию | `app.commands` + `app.hotkeyManager` |
| `removeCommand(shortId)` | снять команду | `app.commands` |
| `addRibbonIcon(icon,title,cb)` | кнопка в левой ленте | `app.workspace.leftRibbon` |
| `addStatusBarItem()` | элемент статус-бара | `app.statusBar` |
| `addSettingTab(tab)` | вкладка в настройках | `app.setting` |
| `registerView(type,creator)` | тип представления | `app.viewRegistry` |
| `registerExtensions(exts,type)` | расширение файла → тип | `app.viewRegistry` |
| `registerHoverLinkSource(id,info)` | источник hover-превью | `app.workspace` |
| `registerMarkdownPostProcessor(fn,sort)` | пост-обработка отрендеренного Markdown | `bL` (глобальный реестр) |
| `registerMarkdownCodeBlockProcessor(lang,fn,sort)` | свой ```lang``` | `bL` |
| `registerEditorExtension(ext)` | расширение CodeMirror 6 | `app.workspace` |
| `registerEditorSuggest(suggest)` | автодополнение в редакторе | `app.workspace.editorSuggest` |
| `registerObsidianProtocolHandler(action,fn)` | `obsidian://action` | `app.workspace` |
| `registerCodeMirror(fn)` | **пустой метод** — CM5 legacy, оставлен ради совместимости | — |
| `loadData()` | прочитать `<configDir>/plugins/<id>/data.json` | `app.vault` |
| `saveData(obj)` | записать + запомнить mtime | `app.vault` |
| `loadCSS()` | прочитать `styles.css` → `<style>` в `<head>` | DOM |
| `onUserEnable()` | хук: включил **пользователь**, не автостарт | — |
| `onExternalSettingsChange()` | хук: `data.json` изменился снаружи | — |

Плюс унаследованные `register`, `registerEvent`, `registerDomEvent`, `registerScopeEvent`, `registerInterval`, `addChild`, `removeChild`.

### 3.3 `data.json` — не просто файл, а канал синхронизации

```js
loadData(){ return this.app.vault.readPluginData(this.manifest.dir) }  // null при ENOENT
saveData(e){ var t=Date.now(); this._lastDataModifiedTime=t;
             return this.app.vault.writePluginData(this.manifest.dir,e,{mtime:t}) }

getModifiedTime(){ try { return (await adapter.stat(manifest.dir+"/data.json")).mtime } catch { return 0 } }

// debounce 50 мс
_onConfigFileChange(){ if(!this.onExternalSettingsChange) return;
  var e = await this.getModifiedTime();
  if(this._lastDataModifiedTime < e) this.onExternalSettingsChange();
  this._lastDataModifiedTime = e }
```

Менеджер слушает `vault.on("raw")` и на путь `<configDir>/plugins/<id>/data.json` дёргает `plugin.onConfigFileChange()`. Смысл: **файл может измениться не через `saveData`** — синхронизация, git, второй экземпляр приложения. Плагин получает шанс перечитать настройки, а не жить со стухшим состоянием в памяти.

Сравнение по `mtime`, а не по содержимому — дешёво, но с известной слабостью (гранулярность ФС).

---

## 4. Манифест: что реально читается

Я вытащил все обращения к `.manifest.*` в бандле. Ядро использует **семь** полей:

| Поле | Где используется | Обязательно |
|---|---|---|
| `id` | неймспейс команд, ключ в `manifests`/`plugins`, имя папки данных | да — без него манифест отбрасывается |
| `name` | префикс имени команды, заголовок вкладки настроек | да (иначе `undefined: …`) |
| `version` | сравнение версий, `versions.json` | да |
| `minAppVersion` | гейт совместимости | нет — отсутствие = «совместим со всем» |
| `isDesktopOnly` | запрет загрузки на мобильных | нет |
| `author` | `author.toLowerCase()==="obsidian"` → обнуляется (анти-подмена) | нет |
| `dir` | **выставляется загрузчиком**, не автором | — |

Схемы/валидатора в бандле нет. Валидация ровно одна:

```js
if(!(l = JSON.parse(await adapter.read(folder+"/manifest.json"))).id) continue;  // молча пропустить
```

Всё остальное — «доверяй и падай позже». `description`, `authorUrl`, `fundingUrl` в ядре **не читаются** — их использует только UI каталога плагинов.

**Вывод для своего приложения:** манифест — это данные, а не код, и это правильно. Но `id` должен быть не просто «есть», а соответствовать регулярке и совпадать с именем папки — иначе получите два плагина с одним `id` и перетирание данных.

---

## 5. Дискавери и загрузка

### 5.1 Что считается плагином

```js
var kZ="manifest.json", CZ="main.js", MZ="styles.css";

function N6(e){ return "manifest.json"===e || "main.js"===e || "styles.css"===e || "data.json"===e }
```

Белый список из четырёх имён. `N6` используется классификатором хранилища: путь `plugins/<id>/<file>` с именем из списка → `community-plugin-data` (область данных плагина), всё прочее в папке плагина — не данные и не синхронизируется как настройки.

Структура на диске:

```
<vault>/.obsidian/
├── community-plugins.json      # ["dataview","templater-obsidian", …]  ← порядок загрузки
├── core-plugins.json           # { "backlink": true, "graph": false, … }
├── plugins/
│   └── <id>/
│       ├── manifest.json
│       ├── main.js             # единственная точка входа
│       ├── styles.css          # опционально
│       └── data.json           # состояние плагина, пишет он сам
```

### 5.2 Загрузка кода

```js
var code = await adapter.read(m.dir + "/" + "main.js");

if(!isDev) code = code.endsWith("/* nosourcemap */") ? code : code.replace(OZ, "");  // вырезать data-URI sourcemap

var requireShim = function(dep){
  if(AZ.hasOwnProperty(dep)) return (console.error("[CM6]["+id+'] Using a deprecated package: "'+dep+'"'), AZ[dep]);
  if(DZ.hasOwnProperty(dep)) return DZ[dep];                       // встроенные @codemirror/*
  if(document.body.hasClass("emulate-mobile"))
      return (new Notice(id+' attempted to load NodeJS package: "'+dep+'"'), console.error(...), null);
  return Xp(dep);                                                  // window.require → node
};

var module = { exports: {} };
window.eval("(function anonymous(require,module,exports){" + code + "\n})\n//# sourceURL=plugin:"
            + encodeURIComponent(id))(requireShim, module, module.exports);

var Ctor = (module.exports || module.exports.default).default || module.exports;
if(!Ctor)              throw new Error("Failed to load plugin " + id + ". No exports detected.");
var inst = new Ctor(this.app, m);
if(!(inst instanceof Plugin)) throw new Error("Failed to load plugin " + id);
this.plugins[id] = inst;
await inst.load();
await inst.loadCSS();
if(userEnable) inst.onUserEnable();
```

Что здесь важно по конструкции:

1. **Формат модуля — CommonJS с фолбэком на ESM.** `module.exports.default || module.exports` — принимает и `module.exports = class extends Plugin`, и `export default class`.
2. **`instanceof Plugin`** — не «утиная типизация», а жёсткая проверка прототипа. Плагин обязан быть наследником класса **из этого же бандла**. Это отсекает «плагин, собранный против другой версии API».
3. **`//# sourceURL=plugin:<id>`** — стектрейсы в консоли показывают `plugin:dataview`, а не `eval`. Дешёвая, но очень полезная вещь для отладки.
4. **Вырезание `sourceMappingURL`** — плагин не может подсунуть браузеру карту, которая раскроет его исходники или, что важнее, заставит DevTools читать внешний URL.
5. **`require`-мост с тремя режимами**: устаревший пакет (громко ругается) → встроенный → `window.require` (Node). На мобильном третий режим запрещён и возвращает `null` — отсюда и смысл флага `isDesktopOnly`.

**Изоляции нет.** Плагин исполняется в том же JS-реалме: общий `window`, общий `localStorage`, доступ к `window.require`, к `app` и к внутренностям других плагинов через `app.plugins.plugins`. Это осознанный выбор (производительность + полный доступ к API), но он означает: **плагин — это доверенный код**, а не песочница.

---

## 6. `PluginManager`: включение, выключение, отказ

```js
function BZ(e){
  this.manifests={}; this.plugins={}; this.enabledPlugins=new Set(); this.updates={};
  this.requestSaveConfig = debounce(this.saveConfig, 1000);
  this.app=e; this.app.vault.on("raw", this.onRaw.bind(this));
}
```

### 6.1 Старт

```js
initialize(){
  var t = await vault.readConfigJson("community-plugins");        // список включённых
  if(!t || !Array.isArray(t)) t = [];
  this.enabledPlugins = new Set(t);

  await this.loadManifests();                                     // обойти plugins/*/manifest.json
  if(Object.keys(this.manifests).length === 0) return;

  if(!this.isEnabled()){                                          // ← ГЛОБАЛЬНЫЙ рубильник
    if(localStorage.getItem("enable-plugin-"+appId) === null) new SafeModeDialog(app).open();
    return;                                                       // ни один плагин не грузится
  }

  for(const id of this.enabledPlugins){
    const timer = setTimeout(() => {                              // ← watchdog на 3 с
      notice.setContext(el => {
        el.createDiv({text: "…plugin " + id + " is taking a long time to load…"});
        el.createEl("button", {text: "Disable"}).onClickEvent(() => {
          this.enabledPlugins.delete(id); await this.saveConfig(); location.reload();
        });
      });
    }, 3000);

    await this.enablePlugin(id);
    notice.clearContext(); clearTimeout(timer);
  }

  this.requestSaveConfig();
  this.checkForDeprecations();
  setInterval(() => this.checkForDeprecations(), 43_200_000);      // раз в 12 часов
}
```

Три вещи, которые стоит украсть целиком:

- **Последовательная загрузка** (`await` в цикле), а не `Promise.all`. Медленнее, зато порядок из `community-plugins.json` детерминирован, а зависший плагин не блокирует остальные навсегда — его снимает watchdog.
- **Watchdog на 3 секунды с кнопкой «отключить именно этот»**. Пользователь не заперт в перезагрузке.
- **Глобальный рубильник в `localStorage`**, а не в vault. Если vault сломан плагином, настройки внутри vault читать уже нельзя — рубильник обязан жить вне его.

### 6.2 `enablePlugin` — все отказы в одном месте

```js
enablePlugin(id, userEnable=false){
  const m = this.manifests[id]; if(!m) return false;

  if(this.isDeprecated(m)) return notice("Unable to load plugin "+m.name+" v"+m.version+
      ". This version has been reported to cause issues."), false;

  if(id === "sliding-panes-obsidian")  return notice("…built-in feature called Stacked Tabs…"), false;
  if(id === "better-pdf-plugin" && m.version === "1.4.0")
      return notice("Better PDF Plugin is no longer functional."), false;

  if(!isDesktopApp && m.isDesktopOnly) return false;               // тихо, без шума

  try {
    this.loadingPluginId = id;                                     // ← ключ к атрибуции ошибок
    await this.loadPlugin(id, userEnable);
  } catch(e) {
    this.loadingPluginId = null;
    new Notice(msgFailedToLoadPlugin({plugin:id}));
    console.error("Plugin failure: " + id, e);
    return false;
  } finally {
    this.loadingPluginId = null;
    performance.mark("communityPlugins." + id);                    // ← профилировка по плагину
  }
  return true;
}
```

Порядок проверок — от дешёвых к дорогим, и **все отказы возвращают `false`, не бросая**. Вызывающий (`initialize`, `enablePluginAndSave`, UI) обрабатывает один тип результата.

### 6.3 Выключение и удаление

```js
unloadPlugin(id, userDisabled=false){
  const p = this.plugins[id]; if(!p) return;
  p._userDisabled = userDisabled;      // запоминается ДО unload — нужно для registerView
  p.unload();
  delete this.plugins[id];
}

disablePlugin(id, userDisabled){ try { await this.unloadPlugin(id,userDisabled) }
  catch(e){ new Notice("Failed to unload plugin " + id); console.error(...) } }

enablePluginAndSave(id){ if(await this.enablePlugin(id,true)){ this.enabledPlugins.add(id);
                          this.requestSaveConfig(); return true } return false }
disablePluginAndSave(id){ this.enabledPlugins.delete(id); this.requestSaveConfig();
                          await this.disablePlugin(id,true) }

uninstallPlugin(id){ await this.disablePluginAndSave(id);
  const dir = this.manifests[id]?.dir;
  if(dir && await vault.exists(dir)) await vault.adapter.rmdir(dir,true);
  delete this.manifests[id]; delete this.updates[id] }
```

Порядок в `enablePluginAndSave` важен: **сначала успешная загрузка, потом запись в конфиг.** Иначе упавший плагин останется в списке и будет падать при каждом старте. А `disablePluginAndSave` — наоборот, сначала из конфига, потом выгрузка: если выгрузка упадёт, плагин не вернётся при перезапуске.

### 6.4 Обновление и совместимость версий

```js
// versions.json в корне репозитория плагина: { "1.4.2": "0.15.0", "1.5.0": "1.0.0", … }
async function resolveVersion(repo, manifest){
  let v = manifest.version || null;
  const min = manifest.minAppVersion;
  if(!appVersion || !min || !isTooOld(appVersion, min)) return v;   // текущая версия подходит

  try {
    v = null;
    const map = await request(githubRaw(repo, "versions.json")).json();
    for(const ver in map)
      if(map.hasOwnProperty(ver) && !isTooOld(appVersion, map[ver]) && (!v || !isTooOld(v, ver)))
        v = ver;                                                     // самая новая СОВМЕСТИМАЯ
    return v;
  } catch { return null }
}
```

Сравнение версий — **числовое, без semver**:

```js
function parse(e){ if(!e||typeof e!=="string") return null;
  let ok=true; const n=e.split(".").map(x=>{ const v=parseInt(x); isNaN(v)&&(ok=false); return v });
  return ok?n:null }

function isTooOld(appVer, minVer){       // true = приложение старее требуемого
  const a=parse(appVer), b=parse(minVer);
  return !!b && (!a || less(a,b));
}
```

`less` — лексикографическое сравнение числовых компонент с добором по длине. Работает для `1.10 > 1.9` (в отличие от строкового сравнения), но **обрезает пре-релизы**: `parseInt("5-beta") === 5`, так что `1.7.5-beta` неотличим от `1.7.5`. Для своего приложения берите нормальный semver-парсер — здесь это просто историческое наследие.

---

## 7. Реестр команд (класс `NK`)

Самое интересное место системы — потому что здесь живёт не регистрация, а **политика доступности**.

```js
addCommand(cmd){
  if(cmd.mobileOnly && !isMobile) return;                    // фильтр на входе

  // ── если плагин дал editorCallback/editorCheckCallback, оборачиваем в checkCallback
  if(cmd.editorCallback || cmd.editorCheckCallback){
    cmd.checkCallback = function(checking){
      const ed = app.workspace.activeEditor;
      if(!ed) return null;                                   // нет редактора → команда недоступна
      if(!cmd.allowPreview && ed.getMode() === "preview") return;   // режим чтения

      if(ed instanceof MarkdownView){
        if(ed.inlineTitleEl.isActiveElement()) return;        // фокус в заголовке заметки
        if(ed.titleEl.isActiveElement())       return;        // фокус в заголовке вкладки
        if(!cmd.allowProperties && activeDocument.activeElement.closest(".metadata-container")) return;
      }
      return cmd.editorCheckCallback
        ? cmd.editorCheckCallback(checking, ed.editor, ed)
        : (checking || cmd.editorCallback(ed.editor, ed), true);
    };
  }

  if(cmd.showOnMobileToolbar) this.editorCommands[cmd.id] = cmd;
  this.commands[cmd.id] = cmd;
  if(cmd.hotkeys) app.hotkeyManager.addDefaultHotkeys(cmd.id, cmd.hotkeys);
}

listCommands(){
  return Object.values(this.commands).filter(c => {
    if(!c.checkCallback) return true;
    try { return c.checkCallback(true) }                   // checking = true → «только спросить»
    catch(e){ console.log("Command failed to execute: ", c.id); console.error(e); return false }
  });
}

executeCommandById(id, evt){ const c = this.findCommand(id); return !!c && this.executeCommand(c, evt) }
executeCommand(cmd, evt){
  app.lastEvent = evt || null;                              // ← команда знает, чем её вызвали
  try { OK(cmd) } catch(e){ console.log("Command failed to execute: ", cmd.id); console.error(e); return false }
  return true;
}

function OK(cmd){                                            // одна точка исполнения
  if(cmd.checkCallback) cmd.checkCallback(false);
  else if(cmd.callback)  cmd.callback();
  else console.error("Command " + cmd + " did not provide a callback");
}
```

### Контракт команды (по фактическому чтению полей)

```ts
interface Command {
  id: string;                    // автоматически → "<pluginId>:<localId>"
  name: string;                  // автоматически → "<Plugin Name>: <localName>"
  icon?: string;                 // lucide-имя
  callback?: () => any;
  checkCallback?: (checking: boolean) => boolean | void;
  editorCallback?: (editor: Editor, ctx: MarkdownView | MarkdownFileInfo) => any;
  editorCheckCallback?: (checking: boolean, editor: Editor, ctx: …) => boolean | void;
  hotkeys?: Hotkey[];            // [{ modifiers: ["Mod"], key: "P" }]
  mobileOnly?: boolean;
  showOnMobileToolbar?: boolean;
  allowPreview?: boolean;        // разрешить в режиме чтения
  allowProperties?: boolean;     // разрешить при фокусе в свойствах (frontmatter)
}
```

Три идеи, которые стоят переноса:

1. **`checkCallback(checking)` — двухфазный.** `checking=true` — «можно ли?» (без побочных эффектов, вызывается при каждом открытии палитры). `checking=false` — «выполняй». Одна функция вместо двух.
2. **Палитра фильтрует по `checkCallback`.** Команда, недоступная в текущем контексте, не появляется в списке — вместо серого пункта или ошибки при клике.
3. **Обёртка для editor-команд** снимает с автора плагина пять типовых проверок (нет редактора / режим чтения / фокус в заголовке / фокус в свойствах). Автор пишет бизнес-логику, каркас решает, уместна ли она.

Обратная сторона: `addCommand` **мутирует переданный объект** (`cmd.id`, `cmd.name`), а `removeCommand` принимает короткий id и снова добавляет префикс. Если сохранить ссылку на исходный объект и передать его в `removeCommand` — получите `plugin:plugin:cmd`.

---

## 8. Реестр представлений (класс `IK`)

```js
function IK(){
  this.viewByType = {};        // "my-view" → (leaf) => new MyView(leaf)
  this.typeByExtension = {};   // "canvas" → "canvas"
  this.registerViewWithExtensions(["canvas"], "canvas", leaf => new CanvasView(leaf));
  this.registerViewWithExtensions(["pdf"],    "pdf",    leaf => new PdfView(leaf));
  …
}

registerView(type, creator){
  if(this.viewByType.hasOwnProperty(type))
    throw new Error('Attempting to register an existing view type "' + type + '"');
  this.viewByType[type] = creator;
  this.trigger("view-registered", type);
}

registerExtensions(exts, type){
  for(const e of exts) if(this.typeByExtension.hasOwnProperty(e))
    throw new Error('Attempting to register an existing file extension "' + e + '"');
  for(const e of exts) this.typeByExtension[e] = type;
  this.trigger("extensions-updated");
}

getViewCreatorByType(type){ return this.viewByType[type] }
getTypeByExtension(ext){ return this.typeByExtension[ext] }
isExtensionRegistered(ext){ return this.typeByExtension.hasOwnProperty(ext) }
```

Дубликаты **бросают исключение**, а не перетирают. Это правильно: молчаливое перетирание чужого типа представления — неотлаживаемая ошибка. Но цена — исключение летит из `registerView` в `onload` плагина, и если плагин его не поймал, `enablePlugin` поймает и покажет «failed to load plugin». Один конфликтующий плагин не ломает остальные.

События `view-registered` / `view-unregistered` / `extensions-updated` позволяют UI (палитра «открыть как», иконки файлов) перестраиваться без поллинга.

### База `View` / `ItemView`

```js
function View(app, leaf){
  this.icon = "lucide-file"; this.navigation = false;
  this.app = app; this.leaf = leaf;
  this.containerEl = leaf.containerEl.createDiv("workspace-leaf-content");
  this.containerEl.setAttribute("data-type", this.getViewType());   // ← CSS-хук по типу
}

open(parentEl){ parentEl.appendChild(this.containerEl); this.load(); await this.onOpen() }
close(){ this.containerEl.detach(); this.unload(); await this.onClose() }
```

`open`/`close` — это `mount`/`unmount`: **сначала вставить в DOM, потом `load()`** (чтобы `onOpen` работал с уже живым деревом), и **сначала вынуть из DOM, потом `unload()`**.

Обязательные к реализации: `getViewType()`, `getDisplayText()`. Опциональные: `getIcon()`, `onOpen()`, `onClose()`, `getState()`/`setState()` (постоянное состояние), `getEphemeralState()`/`setEphemeralState()` (временное, не сохраняется в layout), `onResize()`, `onPaneMenu()`, `onHeaderMenu()`, `onTabMenu()`, `handleCut/Copy/Paste()`.

`ItemView` добавляет готовую шапку: `headerEl`, `titleEl`, `titleParentEl` (хлебные крошки), `actionsEl` + `addAction(icon, title, cb)`, `contentEl` — контейнер для содержимого.

Разделение `getState` / `getEphemeralState` — хорошая идея: layout хранит только то, что нужно для восстановления при старте, а «прокрутка на 40%» живёт в памяти сессии.

---

## 9. Настройки: `PluginSettingTab` + `Setting`

```js
// база
function SettingTab(app, setting){ this.app=app; this.setting=setting;
  this.containerEl = createDiv("vertical-tab-content") }
SettingTab.prototype.hide = function(){}

// вкладка плагина
function PluginSettingTab(app, plugin){ SettingTab.call(this, app, app.setting);
  this.plugin = plugin; this.name = plugin.manifest.name; this.id = plugin.manifest.id }
```

Автор плагина реализует `display()`, заполняя `this.containerEl`. Всё остальное — декларативный билдер:

```js
new Setting(containerEl)
  .setName("Интервал синхронизации")
  .setDesc("В секундах")
  .addSlider(s => s.setLimits(5,300,5).setValue(30).setDynamicTooltip()
                  .onChange(v => { this.settings.interval = v; this.save() }))
  .addExtraButton(b => b.setIcon("rotate-ccw").setTooltip("Сбросить")
                        .onClick(() => this.reset()));
```

Полный набор: `setName`, `setDesc`, `setClass`, `setTooltip`, `setHeading`, `setDisabled`, `setNoInfo`, `setVisibility`, `clear`, `then`; контролы — `addButton`, `addExtraButton`, `addToggle`, `addText`, `addSearch`, `addTextArea`, `addMomentFormat`, `addDropdown`, `addColorPicker`, `addProgressBar`, `addSlider`.

Каждый контрол наследует `setDisabled` / `then`, а `registerOptionListener(host, key)` вешает двустороннюю привязку к свойству — единственный кусок, который экономит реальный код.

Регистрация: `this.addSettingTab(new MyTab(this.app, this))` → `app.setting.addSettingTab` + disposer. Открытие программно: `app.setting.open(); app.setting.openTabById("<pluginId>")`.

---

## 10. Изоляция ошибок — то, что чаще всего делают плохо

Четыре разных механизма, у каждого своя задача.

**(1) Ошибка загрузки плагина** — ловится в `enablePlugin`, превращается в Notice + `console.error`, плагин не попадает в `plugins`. Остальные грузятся.

**(2) Ошибка в команде** — ловится в `executeCommand`:
```js
try { OK(cmd) } catch(e){ console.log("Command failed to execute: ", cmd.id); console.error(e); return false }
```
Команда падает — приложение живёт. Но `checkCallback` при открытии палитры тоже обёрнут в `try`, иначе один битый плагин ломал бы палитру целиком.

**(3) Атрибуция через `loadingPluginId`.** Пока плагин грузится, менеджер держит его id в поле. Всё, что регистрируется в этот момент, может узнать виновника:

```js
onLayoutReady(cb){
  if(this.onLayoutReadyCallbacks === null) cb();                    // layout уже готов — сразу
  else this.onLayoutReadyCallbacks.push({ pluginId: this.app.plugins.loadingPluginId, callback: cb });
}
```

**(4) Отложенные колбэки с изоляцией.** При `layout-ready` очередь разбирается:

```js
trigger("layout-ready");
const queue = this.onLayoutReadyCallbacks;
this.onLayoutReadyCallbacks = null;
for(const { pluginId, callback } of queue){
  await sleep(0);                       // ← уступить event loop между плагинами
  try { callback() }
  catch(e){ console.error(e); if(pluginId) new Notice(msgPluginError({plugin: pluginId})) }
}
```

`await sleep(0)` между колбэками — не косметика: один тяжёлый синхронный колбэк не превращает старт приложения в фриз, а ошибка одного плагина подписана его именем в Notice. Пользователь видит «плагин X сломался», а не «что-то пошло не так».

---

## 11. Слабые места — что не копировать

Честный список; половина из этого — цена обратной совместимости, половина — просто недоделки.

| Место | Проблема |
|---|---|
| `unload()` синхронный | `async onunload()` не ожидается. Плагин, которому нужно дописать файл при выгрузке, этого не сделает — процесс может умереть раньше. Асинхронная очистка требует `await` по всей цепочке. |
| `app` как god-object | Плагин получает весь `App`: `vault`, `workspace`, `metadataCache`, `fileManager`, `commands`, `setting`, `plugins`, `internalPlugins`, `keymap`, `customCss`, `dragManager`, `embedRegistry`. Нет ни одного барьера между «плагин читает заметки» и «плагин переписывает layout». |
| Нет модели разрешений | Плагин делает что хочет: сеть, `window.require("fs")`, чтение чужих `data.json`, патчинг прототипов. Единственная защита — «пользователь поставил сам». |
| Общий JS-реалм | Один плагин может переопределить `Component.prototype.register` и сломать всем выгрузку. Нет ни `Proxy`, ни `vm`, ни worker-изоляции. |
| `_userDisabled` — приватный флаг | Используется внутри disposer'а `registerView`, чтобы решить, сносить ли открытые вкладки. Флаг ставится в `unloadPlugin` **до** `unload()`. Работает, но это протекающая абстракция: «почему закрылись мои вкладки» зависит от того, кто выключил плагин. |
| id ribbon-кнопки из заголовка | `id = pluginId + ":" + title`. Переименовал кнопку — получил новый id; старый остаётся в раскладке. |
| `addCommand` мутирует аргумент | `cmd.id` и `cmd.name` перезаписываются префиксами. Повторная передача того же объекта даёт `plugin:plugin:cmd`. |
| Версии — `parseInt` | `1.7.5-beta` ≡ `1.7.5`. Нет pre-release, нет build-metadata, нет диапазонов (`^1.2`). |
| `mtime` как признак изменения | Гранулярность ФС + правки в ту же секунду = пропущенное изменение `data.json`. |
| Нет лимитов | Ни по памяти, ни по времени CPU, ни по размеру `data.json`. Один плагин может съесть всё. |
| Каталог плагинов — глобальный GitHub | Установка только из `raw.githubusercontent.com`/`releases/download`. Нет локального реестра, нет подписи, нет хеш-проверки скачанного. |

---

## 12. Порядок загрузки приложения (для сверки со своей архитектурой)

Восстановлен по `initializeWithAdapter`; `PY(...)` — метки `performance.mark`, которыми Obsidian сам себя профилирует.

```
1  vault = new Vault(adapter)
2  vault.setupConfig()                          → PY("vault.setup")
3  workspace, fileManager, statusBar,
   metadataCache, metadataTypeManager,
   setting, foldManager                         → PY("workspace.components")
4  internalPlugins = new InternalPlugins(app)   ← core-плагины
5  plugins        = new PluginManager(app)      ← community-плагины (пока только конструктор)
6  тема, шрифты, customCss                      → PY("workspace.appearance")
7  app.registerCommands()                        ← встроенные команды
8  internalPlugins.loadPlugin(...) × ~30         ← регистрация дескрипторов
9  await internalPlugins.enable()                → PY("corePlugins")
10 await plugins.initialize()                    → PY("communityPlugins"), PY("communityPlugins.<id>")
11 metadataCache.preload(); await vault.load()
12 await metadataCache.initialize()              → PY("vault")
13 await workspace.loadLayout()                  → PY("workspace.layout")
14 trigger("layout-ready") → разбор onLayoutReadyCallbacks
15 update-checks, release notes                  → PY("workspace.extra")
```

Обратите внимание на позицию 10: **community-плагины грузятся до того, как прочитан vault и построен layout.** Отсюда и существование `onLayoutReady` — плагин не может работать с файлами в `onload`, потому что индекс ещё не построен. Это осознанный компромисс в пользу скорости старта: код плагинов читается параллельно с диском.

В своём приложении это стоит перевернуть: **сначала данные, потом плагины**. Медленнее на старте, зато `onload` получает готовый мир и не нужен отдельный «второй вход».

---

## 13. Что взять в собственный заметочник

Минимальный набор, который даёт 90% пользы при 10% сложности.

### 13.1 Контракт плагина

```ts
// host.ts — то, что видит плагин. НЕ весь App.
export interface PluginHost {
  readonly manifest: Readonly<PluginManifest>;
  readonly version: string;                    // версия приложения

  commands: CommandRegistry;
  views: ViewRegistry;
  settings: SettingsRegistry;
  events: EventBus;
  storage: PluginStorage;                      // namespaced, только своя папка
  ui: { notice(msg: string): void; ribbon(icon: string, title: string, cb: () => void): Disposable };
}
```

`app` целиком не отдавать. Каждая возможность — отдельный интерфейс: это и документация, и граница для будущих разрешений (`storage.read` vs `vault.write`).

### 13.2 Базовый класс

```ts
export abstract class Component implements Disposable {
  #loaded = false;
  #children: Component[] = [];
  #disposers: Array<() => void> = [];

  get loaded() { return this.#loaded; }

  load(): void {
    if (this.#loaded) return;
    this.#loaded = true;                 // до onload — реентерабельность
    this.onload();
    for (const c of this.#children.slice()) c.load();
  }

  unload(): void {
    if (!this.#loaded) return;
    this.#loaded = false;
    while (this.#children.length) this.#children.pop()!.unload();   // LIFO
    while (this.#disposers.length) this.#disposers.pop()!();        // LIFO
    this.onunload();
  }

  protected onload(): void {}
  protected onunload(): void {}

  addChild<T extends Component>(c: T): T {
    this.#children.push(c);
    if (this.#loaded) c.load();
    return c;
  }

  /** Единственный способ зарегистрировать что-либо. */
  protected register(disposer: () => void): void { this.#disposers.push(disposer) }

  protected interval(ms: number, fn: () => void): number {
    const id = window.setInterval(fn, ms);
    this.register(() => clearInterval(id));
    return id;
  }

  protected listen<K extends string>(bus: EventBus, evt: K, fn: (p: any) => void): void {
    const off = bus.on(evt, fn);
    this.register(off);
  }
}

export abstract class Plugin extends Component {
  constructor(readonly host: PluginHost) { super() }
  async onload(): Promise<void> {}      // разрешаем async
  async onunload(): Promise<void> {}
}
```

Если `onunload` должен быть асинхронным — либо делайте `unload()` возвращающим `Promise` и **ждите его везде**, либо не разрешайте `async onunload` вообще. Obsidian выбрал третье (разрешил и не ждёт) — это худший вариант.

### 13.3 Загрузчик

```ts
async function loadPlugin(dir: string, host: PluginHost): Promise<Plugin> {
  const manifest = JSON.parse(await fs.readFile(join(dir, "manifest.json"), "utf8"));
  assertManifest(manifest);                       // id/name/version + regexp на id

  let code = await fs.readFile(join(dir, manifest.main ?? "main.js"), "utf8");
  code = code.replace(/^\/\/[#@]\s*sourceMappingURL=.*$/gm, "");

  const module = { exports: {} as any };
  const factory = new Function("require", "module", "exports", code + `\n//# sourceURL=plugin:${manifest.id}`);
  factory(makeRequire(manifest.id), module, module.exports);

  const Ctor = module.exports?.default ?? module.exports;
  if (typeof Ctor !== "function") throw new PluginLoadError(manifest.id, "no export");
  const inst = new Ctor(host);
  if (!(inst instanceof Plugin)) throw new PluginLoadError(manifest.id, "not a Plugin");
  return inst;
}
```

`new Function` вместо `eval` — тот же эффект, но без доступа к лексическому окружению загрузчика. `//# sourceURL` обязателен, иначе отладка невозможна.

### 13.4 Реестр команд — минимальная версия

Здесь я **сознательно отступаю от Obsidian**: у него `checkCallback(checking)` — одна функция, которая при `checking=false` сама выполняет действие. Это экономит одну функцию, но легко забыть ветку и получить «команда есть, а ничего не делает». Ниже `check` — только предикат, `run` — только действие.

```ts
class CommandRegistry {
  #byId = new Map<string, Command>();

  add(cmd: Command): Disposable {
    if (this.#byId.has(cmd.id)) throw new Error(`Duplicate command: ${cmd.id}`);
    this.#byId.set(cmd.id, cmd);
    return { dispose: () => { this.#byId.delete(cmd.id) } };
  }

  list(): Command[] {
    return [...this.#byId.values()].filter(c => {
      if (!c.check) return true;
      try { return c.check() } catch { return false }
    });
  }

  async run(id: string, evt?: Event): Promise<boolean> {
    const c = this.#byId.get(id);
    if (!c) return false;
    try {
      if (c.check && !c.check()) return false;
      await c.run?.(evt);
      return true;
    } catch (e) {
      this.host.log.error(`[${c.pluginId}] command ${id} failed`, e);   // ← атрибуция
      return false;
    }
  }
}
```

### 13.5 Чек-лист перед тем, как считать систему готовой

- [ ] Плагин не может зарегистрировать ничего, кроме как через `register(disposer)` — иначе выгрузка неполна.
- [ ] Дубликат id команды / типа представления / расширения → **исключение**, не перетирание.
- [ ] Загрузка одного плагина не мешает остальным: `try/catch` на каждом, `Promise.allSettled` или последовательно.
- [ ] Ошибка в `onload` не оставляет полусостояние: плагин не попадает в активные.
- [ ] Есть глобальный «безопасный режим», хранящийся **вне** пользовательских данных.
- [ ] Watchdog на зависший `onload` с кнопкой «отключить именно этот».
- [ ] `minAppVersion` проверяется **до** исполнения кода, а не после.
- [ ] Есть `versions.json`-аналог: при несовместимости ставится последняя совместимая версия, а не отказ.
- [ ] `data.json` пишется атомарно (temp + rename) и перечитывается при внешнем изменении.
- [ ] Каждая ошибка подписана id плагина.
- [ ] `unload` дожидается асинхронной очистки (или `onunload` синхронный по контракту).
- [ ] Есть лимит на размер `data.json` и время исполнения `onload`.

---

## 14. Резюме в пяти строках

1. **`Component` + `register(disposer)`** — 80% корректности плагинной системы. Всё остальное надстраивается.
2. **Фасад вместо god-object**: плагин видит `addCommand/registerView/addSettingTab`, а не весь `App`.
3. **Реестры с запретом дубликатов** и событиями `*-registered` — UI перестраивается без поллинга.
4. **Двухфазный `checkCallback(checking)`** — команда сама объясняет, уместна ли она в текущем контексте; палитра не показывает мёртвых пунктов.
5. **Атрибуция ошибок через `loadingPluginId`** + изоляция каждого плагина в своём `try/catch` — пользователь видит имя виновника, а не «что-то сломалось».

---

### Как перепроверить

```bash
# бандл уже вытащен
ls -l .workbuddy-ai/tmp/obsidian-app.js

# найти класс Component
python -c "s=open('.workbuddy-ai/tmp/obsidian-app.js',encoding='utf-8').read(); i=s.find('_loaded'); print(s[i-200:i+1400])"

# найти Plugin
python -c "s=open('.workbuddy-ai/tmp/obsidian-app.js',encoding='utf-8').read(); i=s.find('addRibbonIcon=function'); print(s[i-600:i+2500])"

# найти PluginManager
python -c "s=open('.workbuddy-ai/tmp/obsidian-app.js',encoding='utf-8').read(); i=s.find('loadManifests=function'); print(s[i-1200:i+3000])"
```

Повторная распаковка asar: заголовок — Chrome-pickle (`u32` = 4, затем размер pickle, затем длина JSON), не наивные `size` + JSON. Надёжнее искать `{"files"` и парсить через `json.JSONDecoder().raw_decode`, затем выровнять базу данных на 4 байта.
