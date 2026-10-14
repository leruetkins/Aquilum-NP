use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use futures::channel::mpsc::{unbounded, TryRecvError, UnboundedSender};
use rquickjs::{Context, Ctx, Function, Object, Runtime, Value};

use crate::plugins::api::{ApiCall, ApiRequest, HostEvent, RequestId};
use crate::plugins::manifest::PluginManifest;
use crate::state::Snapshot;

/// Maximum heap memory allocated for a single plugin instance (32 MB).
const PLUGIN_MEMORY_LIMIT: usize = 32 * 1024 * 1024;

/// Controller for communicating with and shutting down a running plugin worker thread.
pub struct PluginRuntimeHandle {
    pub manifest: PluginManifest,
    pub host_events_tx: UnboundedSender<HostEvent>,
}

/// Spawns an isolated worker thread running a QuickJS engine instance for the given plugin.
pub fn spawn_plugin_runtime(
    manifest: PluginManifest,
    script_source: String,
    api_tx: UnboundedSender<ApiCall>,
    snapshot: Arc<arc_swap::ArcSwap<Snapshot>>,
) -> Result<PluginRuntimeHandle, String> {
    manifest.validate()?;

    let (host_events_tx, mut host_events_rx) = unbounded::<HostEvent>();
    let plugin_id = manifest.id.clone();
    let thread_name = format!("plugin-worker-{}", plugin_id);

    let handle = PluginRuntimeHandle {
        manifest: manifest.clone(),
        host_events_tx,
    };

    let p_id = plugin_id.clone();
    thread::Builder::new()
        .name(thread_name)
        .spawn(move || {
            let rt = match Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("[plugin:{}] Failed to create QuickJS runtime: {}", p_id, e);
                    return;
                }
            };

            // Set memory limit to prevent memory leak DOS
            rt.set_memory_limit(PLUGIN_MEMORY_LIMIT);

            // Set execution watchdog timeout for runaway loops
            let deadline = Arc::new(AtomicU64::new(0));
            let d_clone = deadline.clone();
            rt.set_interrupt_handler(Some(Box::new(move || {
                let d = d_clone.load(Ordering::Relaxed);
                if d > 0 {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    if now > d {
                        return true; // Interrupt script execution
                    }
                }
                false
            })));

            let ctx = match Context::full(&rt) {
                Ok(ctx) => ctx,
                Err(e) => {
                    eprintln!("[plugin:{}] Failed to create QuickJS context: {}", p_id, e);
                    return;
                }
            };

            // Setup global environment and Obsidian compatibility API
            let setup_res = ctx.with(|ctx| setup_plugin_environment(&ctx, &manifest, api_tx.clone(), snapshot.clone()));
            if let Err(e) = setup_res {
                eprintln!("[plugin:{}] Environment setup error: {:?}", p_id, e);
                return;
            }

            // Execute the plugin script inside a CommonJS module wrapper with active watchdog
            let wrapped_script = format!(
                "(function(exports, require, module) {{\n{}\n}})(globalThis.module.exports, globalThis.require, globalThis.module);",
                script_source
            );
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            deadline.store(now + 5000, Ordering::Relaxed); // 5 sec deadline for initial script eval

            let eval_res = ctx.with(|ctx| {
                ctx.eval::<(), _>(wrapped_script.as_str())
            });
            deadline.store(0, Ordering::Relaxed);

            if let Err(e) = eval_res {
                let err_msg = ctx.with(|ctx| {
                    let ex = ctx.catch();
                    if let Some(obj) = ex.into_object() {
                        let msg: String = obj.get("message").unwrap_or_default();
                        let stack: String = obj.get("stack").unwrap_or_default();
                        if !stack.is_empty() {
                            format!("{}\n{}", msg, stack)
                        } else {
                            msg
                        }
                    } else {
                        format!("{:?}", e)
                    }
                });
                eprintln!("[plugin:{}] Script evaluation error: {}", p_id, err_msg);
            } else {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                deadline.store(now + 5000, Ordering::Relaxed);
                let boot_res = ctx.with(|ctx| {
                    ctx.eval::<(), _>("__bootstrap_plugin_instance()")
                });
                deadline.store(0, Ordering::Relaxed);

                if let Err(e) = boot_res {
                    eprintln!("[plugin:{}] Plugin bootstrap error: {:?}", p_id, e);
                }
                while let Ok(true) = rt.execute_pending_job() {}
            }

            // Plugin worker message loop: process incoming host events, timers, and responses
            loop {
                // Check JS timers and execute callbacks
                let timer_wait_ms: u64 = ctx.with(|ctx| {
                    let check_fn: rquickjs::Result<Function> = ctx.globals().get("__check_timers");
                    if let Ok(func) = check_fn {
                        func.call::<_, f64>(()).unwrap_or(0.0) as u64
                    } else {
                        0
                    }
                });

                match host_events_rx.try_recv() {
                    Ok(event) => match event {
                        HostEvent::Response { id, ok, result } => {
                            let _ = ctx.with(|ctx| {
                                dispatch_js_response(&ctx, id, ok, result);
                            });
                        }
                        HostEvent::Unload => {
                            let _ = ctx.with(|ctx| {
                                let _ = ctx.eval::<(), _>(r#"
                                    try {
                                        if (typeof __plugin_instance !== 'undefined' && __plugin_instance && typeof __plugin_instance.onunload === 'function') {
                                            __plugin_instance.onunload();
                                        }
                                    } catch (e) {
                                        console.error("Plugin onunload error:", e);
                                    }
                                    if (globalThis.__code_block_handlers) globalThis.__code_block_handlers.clear();
                                    if (globalThis.__command_handlers) globalThis.__command_handlers.clear();
                                    if (globalThis.__js_timers) globalThis.__js_timers.clear();
                                    if (globalThis.__velement_registry) globalThis.__velement_registry.clear();
                                "#);
                            });
                            break;
                        }
                        HostEvent::ProcessCodeBlock { language, source, block_id } => {
                            let _ = ctx.with(|ctx| {
                                dispatch_code_block_process(&ctx, language, source, block_id);
                            });
                        }
                        HostEvent::ExecuteCommand { command_id } => {
                            let _ = ctx.with(|ctx| {
                                dispatch_command_execution(&ctx, command_id);
                            });
                        }
                        HostEvent::UiEvent { view_id, node_id, event, value } => {
                            let _ = ctx.with(|ctx| {
                                dispatch_ui_interaction(&ctx, view_id, node_id, event, value);
                            });
                        }
                        _ => {
                            // Other event hooks
                        }
                    },
                    Err(TryRecvError::Empty) => {
                        let sleep_dur = if timer_wait_ms > 0 {
                            Duration::from_millis(timer_wait_ms.min(10))
                        } else {
                            Duration::from_millis(5)
                        };
                        thread::sleep(sleep_dur);
                    }
                    Err(TryRecvError::Closed) => {
                        break;
                    }
                }

                // Poll quickjs job queue for pending promise resolutions
                let _ = rt.execute_pending_job();
            }
        })
        .map_err(|e| e.to_string())?;

    Ok(handle)
}

/// Injects global helper functions and the Obsidian compatibility object model into QuickJS.
fn setup_plugin_environment<'js>(
    ctx: &Ctx<'js>,
    manifest: &PluginManifest,
    api_tx: UnboundedSender<ApiCall>,
    snapshot: Arc<arc_swap::ArcSwap<Snapshot>>,
) -> rquickjs::Result<()> {
    let plugin_id = &manifest.id;
    let globals = ctx.globals();

    // 1. Console logger
    let p_prefix = format!("[plugin:{}]", plugin_id);
    let log_prefix = p_prefix.clone();
    let console = Object::new(ctx.clone())?;
    let log_fn = Function::new(ctx.clone(), move |msg: String| {
        println!("{} {}", log_prefix, msg);
    })?;
    console.set("log", log_fn)?;
    console.set("warn", console.get::<_, Function>("log")?)?;
    console.set("error", console.get::<_, Function>("log")?)?;
    globals.set("console", console)?;

    // 2. Low-level host communication bridge: __raw_host_call(kind, payload_json_string)
    let p_id_str = plugin_id.to_string();
    let req_counter = Arc::new(AtomicU64::new(1));
    let r_cnt = req_counter.clone();
    let host_call_fn = Function::new(
        ctx.clone(),
        move |kind: String, payload_str: String| -> rquickjs::Result<f64> {
            let id = r_cnt.fetch_add(1, Ordering::SeqCst);
            let payload_json: serde_json::Value = serde_json::from_str(&payload_str)
                .unwrap_or(serde_json::Value::Null);
            let api_req = parse_js_api_request(&kind, payload_json);
            let _ = api_tx.unbounded_send(ApiCall {
                plugin_id: p_id_str.clone(),
                request_id: id,
                request: api_req,
            });
            Ok(id as f64)
        },
    )?;
    globals.set("__raw_host_call", host_call_fn)?;

    // 3. Snapshot synchronous getters: __host_get_active_file(), __host_get_markdown_files()
    // Enforce permission: plugins with vault:none cannot inspect workspace file tree
    let has_vault_read = manifest.permissions.vault >= crate::plugins::manifest::VaultAccess::Read;
    let snap_active = snapshot.clone();
    let get_active_file_fn = Function::new(ctx.clone(), move || -> rquickjs::Result<Option<String>> {
        if !has_vault_read {
            return Ok(None);
        }
        let snap = snap_active.load();
        Ok(snap.active_file.as_ref().map(|f| f.path.clone()))
    })?;
    globals.set("__host_get_active_file", get_active_file_fn)?;

    let snap_files = snapshot.clone();
    let get_files_fn = Function::new(ctx.clone(), move || -> rquickjs::Result<Vec<String>> {
        if !has_vault_read {
            return Ok(Vec::new());
        }
        let snap = snap_files.load();
        Ok(snap.files.iter().map(|f| f.path.clone()).collect())
    })?;
    globals.set("__host_get_markdown_files", get_files_fn)?;

    // 4. Inject manifest and CommonJS module / exports
    let manifest_json = serde_json::to_string(manifest).unwrap_or_else(|_| "{}".to_string());
    ctx.eval::<(), _>(format!(
        "globalThis.__plugin_manifest = {}; globalThis.module = {{ exports: {{}} }}; globalThis.exports = globalThis.module.exports;",
        manifest_json
    ))?;

    // 5. Inject runtime JavaScript bootstrap defining Obsidian-compatible classes & timers
    let bootstrap_js = r#"
        const __pending_promises = new Map();
        globalThis.__code_block_handlers = new Map();
        globalThis.__command_handlers = new Map();
        globalThis.__js_timers = new Map();
        globalThis.__velement_registry = new Map();
        let __next_timer_id = 1;

        function __dispatch_ui_interaction(nodeId, action, value) {
            const el = globalThis.__velement_registry.get(nodeId);
            if (el) {
                if (action === "change") {
                    const parsedVal = value === "true" ? true : (value === "false" ? false : value);
                    el.checked = !!parsedVal;
                    el.value = value;
                    el.dispatchEvent({ type: "change", value: parsedVal });
                } else {
                    el.dispatchEvent({ type: action });
                }
            }
        }

        globalThis.setTimeout = function(fn, delay = 0, ...args) {
            const id = __next_timer_id++;
            const due = Date.now() + Math.max(0, delay);
            globalThis.__js_timers.set(id, { fn, due, repeat: 0, args });
            return id;
        };

        globalThis.clearTimeout = function(id) {
            globalThis.__js_timers.delete(id);
        };

        globalThis.setInterval = function(fn, delay = 0, ...args) {
            const id = __next_timer_id++;
            const period = Math.max(10, delay);
            const due = Date.now() + period;
            globalThis.__js_timers.set(id, { fn, due, repeat: period, args });
            return id;
        };

        globalThis.clearInterval = function(id) {
            globalThis.__js_timers.delete(id);
        };

        globalThis.__check_timers = function() {
            if (globalThis.__js_timers.size === 0) return 0;
            const now = Date.now();
            const ready = [];
            let nextDue = Infinity;
            for (const [id, t] of globalThis.__js_timers.entries()) {
                if (t.due <= now) {
                    ready.push([id, t]);
                } else if (t.due < nextDue) {
                    nextDue = t.due;
                }
            }
            for (const [id, t] of ready) {
                try {
                    t.fn(...t.args);
                } catch (e) {
                    console.error("Timer callback error:", e);
                }
                if (t.repeat > 0) {
                    t.due = Date.now() + t.repeat;
                    if (t.due < nextDue) nextDue = t.due;
                } else {
                    globalThis.__js_timers.delete(id);
                }
            }
            return nextDue === Infinity ? 0 : Math.max(1, nextDue - Date.now());
        };

        async function __bootstrap_plugin_instance() {
            let exported = globalThis.module ? globalThis.module.exports : null;
            if (exported && exported.default && typeof exported !== "function") {
                exported = exported.default;
            }
            if (typeof exported === "function") {
                try {
                    const inst = new exported(globalThis.app, globalThis.__plugin_manifest);
                    globalThis.__plugin_instance = inst;
                    if (typeof inst.onload === "function") {
                        await inst.onload();
                    }
                } catch (e) {
                    console.error("Plugin instantiation or onload error:", e);
                }
            } else if (exported && typeof exported.onload === "function") {
                globalThis.__plugin_instance = exported;
                try {
                    await exported.onload();
                } catch (e) {
                    console.error("Plugin onload error:", e);
                }
            }
        }

        function __dispatch_host_response(id, ok, result) {
            const entry = __pending_promises.get(id);
            if (entry) {
                __pending_promises.delete(id);
                if (ok) {
                    entry.resolve(result);
                } else {
                    entry.reject(new Error(result?.error || "Host error"));
                }
            }
        }

        function __dispatch_command_execution(commandId) {
            const handler = globalThis.__command_handlers.get(commandId);
            if (handler) {
                try {
                    handler();
                } catch (e) {
                    console.error("Command execution error:", e);
                }
            }
        }

        function __dispatch_code_block_process(lang, source, blockId) {
            const handler = globalThis.__code_block_handlers.get(String(lang).toLowerCase());
            if (!handler) return;
            try {
                const result = handler(source);
                if (result && typeof result === "object") {
                    __invoke_host("set_code_block_view", { block_id: blockId, tree: result });
                } else if (typeof result === "string") {
                    __invoke_host("set_code_block_view", {
                        block_id: blockId,
                        tree: { type: "text", value: result }
                    });
                }
            } catch (e) {
                console.error("Code block processor error:", e);
            }
        }

        function __invoke_host(kind, payload = {}) {
            return new Promise((resolve, reject) => {
                const payloadStr = JSON.stringify(payload);
                const id = __raw_host_call(kind, payloadStr);
                __pending_promises.set(id, { resolve, reject });
            });
        }

        class Notice {
            constructor(message, duration = 4000) {
                __invoke_host("notice", { text: String(message), timeout_ms: duration });
            }
        }

        class VElement {
            constructor(tag = "div", options = {}) {
                this.tag = String(tag).toLowerCase();
                this.id = options.id || ("el-" + Math.random().toString(36).substr(2, 9));
                this.classes = new Set(options.cls ? (Array.isArray(options.cls) ? options.cls : options.cls.split(" ").filter(Boolean)) : []);
                this.text = options.text || "";
                this.attributes = Object.assign({}, options.attr || {});
                this.children = [];
                this.eventListeners = new Map();
                this.parent = null;
                this.value = options.value || "";
                this.placeholder = options.placeholder || "";
                this.disabled = !!options.disabled;
                this.checked = !!options.checked;
                this.dataset = {};
                this.style = new Proxy({}, {
                    get: (t, p) => t[p] || "",
                    set: (t, p, v) => { t[p] = v; return true; }
                });
                globalThis.__velement_registry.set(this.id, this);
            }

            createEl(tag, options = {}) {
                const el = new VElement(tag, options);
                this.appendChild(el);
                return el;
            }

            createDiv(cls = "") {
                return this.createEl("div", { cls });
            }

            createSpan(cls = "") {
                return this.createEl("span", { cls });
            }

            appendChild(child) {
                if (child instanceof VElement) {
                    child.parent = this;
                    this.children.push(child);
                }
                return child;
            }

            empty() {
                for (const c of this.children) {
                    globalThis.__velement_registry.delete(c.id);
                }
                this.children = [];
                this.text = "";
                return this;
            }

            remove() {
                globalThis.__velement_registry.delete(this.id);
                if (this.parent) {
                    const idx = this.parent.children.indexOf(this);
                    if (idx !== -1) {
                        this.parent.children.splice(idx, 1);
                    }
                    this.parent = null;
                }
            }

            detach() {
                this.remove();
            }

            addClass(...classes) {
                for (const c of classes) {
                    if (c) this.classes.add(c);
                }
                return this;
            }

            removeClass(...classes) {
                for (const c of classes) {
                    if (c) this.classes.delete(c);
                }
                return this;
            }

            hasClass(cls) {
                return this.classes.has(cls);
            }

            toggleClass(cls, force) {
                if (typeof force === "boolean") {
                    if (force) this.addClass(cls);
                    else this.removeClass(cls);
                } else {
                    if (this.classes.has(cls)) this.removeClass(cls);
                    else this.addClass(cls);
                }
                return this;
            }

            get classList() {
                return {
                    add: (...cls) => this.addClass(...cls),
                    remove: (...cls) => this.removeClass(...cls),
                    contains: (cls) => this.classes.has(cls),
                    toggle: (cls, force) => this.toggleClass(cls, force),
                    forEach: (fn) => this.classes.forEach(fn),
                };
            }

            setText(text) {
                this.text = String(text != null ? text : "");
                return this;
            }

            getText() {
                return this.text;
            }

            get textContent() {
                return this.text;
            }

            set textContent(val) {
                this.setText(val);
            }

            get innerHTML() {
                return this.text;
            }

            set innerHTML(val) {
                this.setText(val);
            }

            get innerText() {
                return this.text;
            }

            set innerText(val) {
                this.setText(val);
            }

            setAttr(name, val) {
                this.attributes[name] = val;
                return this;
            }

            getAttr(name) {
                return this.attributes[name];
            }

            setAttribute(name, val) {
                this.attributes[name] = val;
            }

            getAttribute(name) {
                return this.attributes[name];
            }

            removeAttribute(name) {
                delete this.attributes[name];
            }

            hasAttribute(name) {
                return name in this.attributes;
            }

            addEventListener(type, listener) {
                if (!this.eventListeners.has(type)) {
                    this.eventListeners.set(type, []);
                }
                this.eventListeners.get(type).push(listener);
            }

            removeEventListener(type, listener) {
                const list = this.eventListeners.get(type);
                if (list) {
                    const idx = list.indexOf(listener);
                    if (idx !== -1) list.splice(idx, 1);
                }
            }

            dispatchEvent(event) {
                const type = event?.type || "click";
                const list = this.eventListeners.get(type);
                if (list) {
                    for (const fn of list) {
                        try { fn(event); } catch (e) { console.error("Event listener error:", e); }
                    }
                }
            }

            onClick(callback) {
                this.addEventListener("click", callback);
                return this;
            }

            querySelector(sel) {
                if (sel.startsWith(".")) {
                    const c = sel.slice(1);
                    return this.children.find(ch => ch.classes && ch.classes.has(c)) || null;
                }
                return this.children.find(ch => ch.tag === sel) || null;
            }

            querySelectorAll(sel) {
                if (sel.startsWith(".")) {
                    const c = sel.slice(1);
                    return this.children.filter(ch => ch.classes && ch.classes.has(c));
                }
                return this.children.filter(ch => ch.tag === sel);
            }
        }

        class BaseComponent {
            constructor() {
                this.disabled = false;
            }
            setDisabled(disabled) {
                this.disabled = !!disabled;
                return this;
            }
        }

        class ValueComponent extends BaseComponent {
            constructor() {
                super();
                this.value = undefined;
            }
            getValue() {
                return this.value;
            }
            setValue(val) {
                this.value = val;
                return this;
            }
        }

        class ButtonComponent extends BaseComponent {
            constructor(containerEl) {
                super();
                this.buttonEl = containerEl ? containerEl.createEl("button") : new VElement("button");
                this.onClickHandler = null;
            }
            setButtonText(text) {
                this.buttonEl.setText(text);
                return this;
            }
            setIcon(icon) {
                this.buttonEl.attributes["icon"] = icon;
                return this;
            }
            setCta() {
                this.buttonEl.addClass("mod-cta");
                return this;
            }
            setWarning() {
                this.buttonEl.addClass("mod-warning");
                return this;
            }
            setDisabled(disabled) {
                super.setDisabled(disabled);
                this.buttonEl.disabled = this.disabled;
                return this;
            }
            onClick(callback) {
                this.onClickHandler = callback;
                this.buttonEl.addEventListener("click", callback);
                return this;
            }
        }

        class ExtraButtonComponent extends BaseComponent {
            constructor(containerEl) {
                super();
                this.extraSettingsEl = containerEl ? containerEl.createDiv("clickable-icon") : new VElement("div", { cls: "clickable-icon" });
            }
            setIcon(icon) {
                this.extraSettingsEl.setAttr("icon", icon);
                return this;
            }
            setTooltip(tooltip) {
                this.extraSettingsEl.setAttr("title", tooltip);
                return this;
            }
            setDisabled(disabled) {
                super.setDisabled(disabled);
                return this;
            }
            onClick(cb) {
                this.extraSettingsEl.addEventListener("click", cb);
                return this;
            }
        }

        class ToggleComponent extends ValueComponent {
            constructor(containerEl) {
                super();
                this.toggleEl = containerEl ? containerEl.createEl("input", { attr: { type: "checkbox" } }) : new VElement("input");
                this.value = false;
                this.onChangeHandler = null;
            }
            setValue(val) {
                super.setValue(!!val);
                this.toggleEl.checked = this.value;
                return this;
            }
            onChange(callback) {
                this.onChangeHandler = callback;
                this.toggleEl.addEventListener("change", (e) => {
                    this.value = e?.value !== undefined ? e.value : !this.value;
                    callback(this.value);
                });
                return this;
            }
        }

        class TextComponent extends ValueComponent {
            constructor(containerEl) {
                super();
                this.inputEl = containerEl ? containerEl.createEl("input", { attr: { type: "text" } }) : new VElement("input");
                this.value = "";
            }
            setPlaceholder(p) {
                this.inputEl.placeholder = p;
                return this;
            }
            setValue(v) {
                super.setValue(String(v != null ? v : ""));
                this.inputEl.value = this.value;
                return this;
            }
            onChange(callback) {
                this.inputEl.addEventListener("change", (e) => {
                    this.value = e?.value != null ? e.value : this.value;
                    callback(this.value);
                });
                return this;
            }
        }

        class SliderComponent extends ValueComponent {
            constructor(containerEl) {
                super();
                this.sliderEl = containerEl ? containerEl.createEl("input", { attr: { type: "range" } }) : new VElement("input");
                this.min = 0;
                this.max = 100;
                this.step = 1;
                this.value = 0;
            }
            setLimits(min, max, step) {
                this.min = min;
                this.max = max;
                this.step = step;
                return this;
            }
            setValue(v) {
                super.setValue(Number(v) || 0);
                this.sliderEl.value = this.value;
                return this;
            }
            onChange(callback) {
                this.sliderEl.addEventListener("change", (e) => {
                    this.value = Number(e?.value) || 0;
                    callback(this.value);
                });
                return this;
            }
        }

        class DropdownComponent extends ValueComponent {
            constructor(containerEl) {
                super();
                this.selectEl = containerEl ? containerEl.createEl("select") : new VElement("select");
                this.options = {};
                this.value = "";
            }
            addOption(val, display) {
                this.options[val] = display;
                if (!this.value) this.value = val;
                this.selectEl.attributes["options"] = this.options;
                return this;
            }
            addOptions(opts) {
                Object.assign(this.options, opts);
                if (!this.value && Object.keys(opts).length > 0) {
                    this.value = Object.keys(opts)[0];
                }
                this.selectEl.attributes["options"] = this.options;
                return this;
            }
            setValue(v) {
                super.setValue(String(v));
                this.selectEl.value = this.value;
                return this;
            }
            onChange(callback) {
                this.selectEl.addEventListener("change", (e) => {
                    this.value = e?.value != null ? e.value : this.value;
                    callback(this.value);
                });
                return this;
            }
        }

        class Setting {
            constructor(containerEl) {
                this.settingEl = containerEl ? containerEl.createDiv("setting-item") : new VElement("div", { cls: "setting-item" });
                this.infoEl = this.settingEl.createDiv("setting-item-info");
                this.nameEl = this.infoEl.createDiv("setting-item-name");
                this.descEl = this.infoEl.createDiv("setting-item-description");
                this.controlEl = this.settingEl.createDiv("setting-item-control");
                this.controls = [];
            }
            setName(name) {
                this.nameEl.setText(name);
                return this;
            }
            setDesc(desc) {
                this.descEl.setText(desc);
                return this;
            }
            setClass(cls) {
                this.settingEl.addClass(cls);
                return this;
            }
            setTooltip(tooltip) {
                this.settingEl.setAttr("title", tooltip);
                return this;
            }
            setHeading() {
                this.settingEl.addClass("setting-item-heading");
                return this;
            }
            addToggle(cb) {
                const toggle = new ToggleComponent(this.controlEl);
                this.controls.push(toggle);
                if (typeof cb === "function") cb(toggle);
                return this;
            }
            addButton(cb) {
                const btn = new ButtonComponent(this.controlEl);
                this.controls.push(btn);
                if (typeof cb === "function") cb(btn);
                return this;
            }
            addText(cb) {
                const txt = new TextComponent(this.controlEl);
                this.controls.push(txt);
                if (typeof cb === "function") cb(txt);
                return this;
            }
            addSlider(cb) {
                const slider = new SliderComponent(this.controlEl);
                this.controls.push(slider);
                if (typeof cb === "function") cb(slider);
                return this;
            }
            addDropdown(cb) {
                const drop = new DropdownComponent(this.controlEl);
                this.controls.push(drop);
                if (typeof cb === "function") cb(drop);
                return this;
            }
            addExtraButton(cb) {
                const extra = new ExtraButtonComponent(this.controlEl);
                this.controls.push(extra);
                if (typeof cb === "function") cb(extra);
                return this;
            }
        }

        class PluginSettingTab {
            constructor(app, plugin) {
                this.app = app;
                this.plugin = plugin;
                this.containerEl = new VElement("div", { cls: "vertical-tab-content" });
            }
            display() {}
            hide() {}
        }

        function __velement_to_uinode(el) {
            if (!el) return { type: "text", value: "" };

            if (el.classes && el.classes.has("vertical-tab-content")) {
                const children = el.children.map(__velement_to_uinode).filter(Boolean);
                return { type: "column", gap: "medium", children };
            }

            if (el.classes && el.classes.has("setting-item")) {
                const name = el.children.find(c => c.classes?.has("setting-item-info"))?.children?.find(c => c.classes?.has("setting-item-name"))?.text || el.text || "Setting";
                const desc = el.children.find(c => c.classes?.has("setting-item-info"))?.children?.find(c => c.classes?.has("setting-item-description"))?.text || undefined;
                const controlDiv = el.children.find(c => c.classes?.has("setting-item-control"));

                let controlNode = { type: "text", value: "" };
                if (controlDiv && controlDiv.children.length > 0) {
                    controlNode = __velement_to_uinode(controlDiv.children[0]);
                }

                return {
                    type: "setting_item",
                    name: String(name),
                    desc: desc ? String(desc) : undefined,
                    control: controlNode
                };
            }

            if (el.tag === "button") {
                let variant = "default";
                if (el.classes.has("mod-cta") || el.classes.has("mod-primary")) variant = "primary";
                if (el.classes.has("mod-warning") || el.classes.has("mod-danger")) variant = "danger";
                return {
                    type: "button",
                    id: el.id,
                    label: el.text || "Button",
                    variant,
                    disabled: !!el.disabled
                };
            }

            if (el.tag === "input" && (el.attributes["type"] === "checkbox" || el.checked !== undefined)) {
                return {
                    type: "toggle",
                    id: el.id,
                    value: !!el.checked
                };
            }

            if (el.tag === "input" && el.attributes["type"] === "range") {
                return {
                    type: "slider",
                    id: el.id,
                    min: Number(el.attributes["min"] || 0),
                    max: Number(el.attributes["max"] || 100),
                    step: Number(el.attributes["step"] || 1),
                    value: Number(el.value || 0)
                };
            }

            if (el.tag === "select") {
                const options = Object.entries(el.attributes["options"] || {}).map(([val, lbl]) => ({
                    value: String(val),
                    label: String(lbl)
                }));
                return {
                    type: "select",
                    id: el.id,
                    options,
                    value: String(el.value || "")
                };
            }

            if (el.tag === "input") {
                return {
                    type: "input",
                    id: el.id,
                    placeholder: el.placeholder || undefined,
                    value: el.value || undefined
                };
            }

            if (/^h[1-6]$/.test(el.tag)) {
                return {
                    type: "text",
                    style: "heading",
                    value: el.text || ""
                };
            }

            if (el.tag === "p" || el.tag === "span") {
                return {
                    type: "text",
                    style: el.classes.has("muted") ? "muted" : "normal",
                    value: el.text || ""
                };
            }

            const isRow = el.classes.has("row") || el.classes.has("flex-row");
            const children = el.children.map(__velement_to_uinode).filter(Boolean);
            return {
                type: isRow ? "row" : "column",
                gap: "medium",
                children
            };
        }

        class Component {
            constructor() {
                this._children = [];
                this._loaded = false;
            }
            load() {
                this._loaded = true;
                if (typeof this.onload === "function") this.onload();
            }
            unload() {
                this._loaded = false;
                if (typeof this.onunload === "function") this.onunload();
                for (const c of this._children) {
                    if (c && typeof c.unload === "function") c.unload();
                }
                this._children = [];
            }
            onload() {}
            onunload() {}
            addChild(child) {
                if (child) {
                    this._children.push(child);
                    if (this._loaded && typeof child.load === "function") {
                        child.load();
                    }
                }
                return child;
            }
            removeChild(child) {
                const idx = this._children.indexOf(child);
                if (idx !== -1) {
                    this._children.splice(idx, 1);
                    if (typeof child.unload === "function") child.unload();
                }
                return child;
            }
            register(cb) {
                if (typeof cb === "function") {
                    this._children.push({ unload: cb });
                }
            }
            registerEvent(ref) {
                return ref;
            }
            registerDomEvent(el, type, cb, options) {
                if (el && typeof el.addEventListener === "function") {
                    el.addEventListener(type, cb, options);
                    this.register(() => el.removeEventListener(type, cb, options));
                }
            }
            registerInterval(id) {
                this.register(() => clearInterval(id));
                return id;
            }
            registerHoverLinkSource(source, info) {}
        }

        class Events {
            constructor() {
                this._events = new Map();
            }
            on(name, cb, ctx) {
                if (!this._events.has(name)) this._events.set(name, []);
                const ref = { cb, ctx };
                this._events.get(name).push(ref);
                return ref;
            }
            off(name, cb) {
                const list = this._events.get(name);
                if (list) {
                    const idx = list.findIndex(r => r.cb === cb);
                    if (idx !== -1) list.splice(idx, 1);
                }
            }
            offref(ref) {
                for (const list of this._events.values()) {
                    const idx = list.indexOf(ref);
                    if (idx !== -1) { list.splice(idx, 1); break; }
                }
            }
            trigger(name, ...args) {
                const list = this._events.get(name);
                if (list) {
                    for (const { cb, ctx } of [...list]) {
                        try { cb.apply(ctx, args); } catch (e) { console.error(e); }
                    }
                }
            }
            tryTrigger(name, ...args) {
                this.trigger(name, ...args);
            }
        }

        class Scope {
            constructor(parent) {
                this.parent = parent;
                this.keys = [];
            }
            register(modifiers, key, func) { return {}; }
            unregister(func) {}
        }

        class View extends Component {
            constructor(leaf) {
                super();
                this.leaf = leaf;
                this.app = leaf ? leaf.app : globalThis.app;
                this.containerEl = new VElement("div", { cls: "view" });
                this.navigation = false;
            }
            getViewType() { return "view"; }
            getDisplayText() { return ""; }
            getIcon() { return "document"; }
            onOpen() {}
            onClose() {}
            getState() { return {}; }
            setState(state, result) { return Promise.resolve(); }
        }

        class ItemView extends View {
            constructor(leaf) {
                super(leaf);
                this.contentEl = this.containerEl.createDiv("item-view-content");
            }
            getViewType() { return "empty"; }
            getDisplayText() { return "Item View"; }
            async onOpen() {}
            async onClose() {}
        }

        class MarkdownRenderChild extends Component {
            constructor(containerEl) {
                super();
                this.containerEl = containerEl;
            }
        }

        class Modal extends Component {
            constructor(app) {
                super();
                this.app = app;
                this.scope = new Scope();
                this.containerEl = new VElement("div", { cls: "modal-container" });
                this.modalEl = this.containerEl.createDiv("modal");
                this.titleEl = this.modalEl.createDiv("modal-title");
                this.contentEl = this.modalEl.createDiv("modal-content");
            }
            open() {
                this.onOpen();
                const tree = __velement_to_uinode(this.containerEl);
                __invoke_host("set_view_tree", { view_id: 1, tree });
            }
            close() {
                this.onClose();
            }
            onOpen() {}
            onClose() {}
        }

        class SuggestModal extends Modal {
            constructor(app) { super(app); }
            getSuggestions(query) { return []; }
            renderSuggestion(item, el) {}
            onChooseSuggestion(item, evt) {}
        }

        class FuzzySuggestModal extends SuggestModal {
            getItems() { return []; }
            getItemText(item) { return String(item); }
            onChooseItem(item, evt) {}
        }

        class Menu {
            constructor() { this.items = []; }
            addItem(cb) { const item = new MenuItem(this); cb(item); this.items.push(item); return this; }
            addSeparator() { return this; }
            showAtPosition(pos) { return this; }
            showAtMouseEvent(evt) { return this; }
        }

        class MenuItem {
            constructor(menu) { this.menu = menu; }
            setTitle(title) { this.title = title; return this; }
            setIcon(icon) { this.icon = icon; return this; }
            setChecked(checked) { this.checked = checked; return this; }
            setDisabled(disabled) { this.disabled = disabled; return this; }
            onClick(cb) { this.callback = cb; return this; }
        }

        class WorkspaceLeaf {
            constructor(workspace) {
                this.workspace = workspace;
                this.view = null;
            }
            async setViewState(state) {
                this.viewState = state;
            }
            openFile(file) {
                return this.workspace.openFile(file);
            }
        }

        class TAbstractFile {
            constructor(path) {
                this.path = String(path);
                this.name = this.path.split("/").pop();
            }
        }
        class TFile extends TAbstractFile {
            constructor(path) {
                super(path);
                this.basename = this.name.replace(/\.[^/.]+$/, "");
                this.extension = (this.name.split(".").pop() || "").toLowerCase();
                this.stat = { mtime: Date.now(), ctime: Date.now(), size: 0 };
            }
        }
        class TFolder extends TAbstractFile {
            constructor(path) {
                super(path);
                this.children = [];
            }
        }

        function normalizePath(path) {
            if (!path) return "";
            return String(path).replace(/\\/g, "/").replace(/\/+/g, "/").replace(/^\/|\/$/g, "");
        }

        function requestUrl(options) {
            if (typeof options === "string") options = { url: options };
            return __invoke_host("http", {
                url: options.url,
                method: options.method || "GET",
                body: options.body ? (typeof options.body === "string" ? options.body : JSON.stringify(options.body)) : null
            }).then(res => {
                return {
                    status: 200,
                    text: typeof res === "string" ? res : JSON.stringify(res),
                    json: typeof res === "object" ? res : (tryJson(res) || {}),
                    arrayBuffer: new ArrayBuffer(0),
                    headers: {}
                };
            });
        }
        function tryJson(s) { try { return JSON.parse(s); } catch (e) { return null; } }

        function moment(date) {
            const d = date ? new Date(date) : new Date();
            return {
                format: (fmt = "YYYY-MM-DD") => {
                    const pad = (n) => String(n).padStart(2, '0');
                    const y = d.getFullYear();
                    const m = pad(d.getMonth() + 1);
                    const day = pad(d.getDate());
                    const h = pad(d.getHours());
                    const min = pad(d.getMinutes());
                    const s = pad(d.getSeconds());
                    return fmt
                        .replace("YYYY", y)
                        .replace("MM", m)
                        .replace("DD", day)
                        .replace("HH", h)
                        .replace("mm", min)
                        .replace("ss", s);
                },
                valueOf: () => d.getTime(),
                toDate: () => d
            };
        }

        function createLazyFailProxy(moduleName) {
            const target = function() {};
            return new Proxy(target, {
                get(_tgt, prop) {
                    if (prop === "__esModule") return true;
                    if (prop === "default") return createLazyFailProxy(moduleName);
                    if (typeof prop === "symbol" || prop === "then" || prop === "toJSON") return undefined;
                    return createLazyFailProxy(`${moduleName}.${String(prop)}`);
                },
                apply(_tgt, _thisArg, _args) {
                    throw new Error(`[Aquilum] External module call '${moduleName}()' is not supported in native GPUI mode`);
                },
                construct(_tgt, _args) {
                    throw new Error(`[Aquilum] External constructor 'new ${moduleName}()' is not supported in native GPUI mode`);
                }
            });
        }

        const SAFE_NOOP_STUBS = new Set([
            'app.workspace.requestSaveHistory',
            'app.metadataCache.trigger',
            'plugin.addRibbonIcon',
            'app.workspace.trigger',
            'app.vault.trigger'
        ]);

        const KNOWN_UNIMPLEMENTED_OBSIDIAN_METHODS = new Set([
            'app.vault.copy',
            'app.workspace.splitActiveLeaf',
            'app.fileManager.processFrontMatter',
            'app.workspace.iterateAllLeaves'
        ]);

        function makeAuditedProxy(target, targetName) {
            return new Proxy(target, {
                get(obj, prop) {
                    if (typeof prop === 'symbol' || prop === 'then' || prop === 'toJSON') {
                        return obj[prop];
                    }
                    if (prop in obj) {
                        return obj[prop];
                    }
                    const propName = String(prop);
                    const fullPath = `${targetName}.${propName}`;

                    if (SAFE_NOOP_STUBS.has(fullPath)) {
                        return (..._args) => Promise.resolve(null);
                    }

                    if (KNOWN_UNIMPLEMENTED_OBSIDIAN_METHODS.has(fullPath)) {
                        return (..._args) => {
                            throw new Error(`[Aquilum] Method ${fullPath} is not implemented`);
                        };
                    }

                    return undefined;
                }
            });
        }

        class Plugin extends Component {
            constructor(app, manifest) {
                super();
                this.app = app;
                this.manifest = manifest;
            }
            async onload() {}
            async onunload() {}
            addCommand(cmd) {
                if (cmd && cmd.id) {
                    if (cmd.callback) {
                        globalThis.__command_handlers.set(cmd.id, cmd.callback);
                    }
                    return __invoke_host("add_command", {
                        id: cmd.id,
                        name: cmd.name || cmd.id,
                        hotkey: cmd.hotkey
                    });
                }
            }
            addRibbonIcon(icon, title, callback) {
                const id = "ribbon-" + Math.random().toString(36).substr(2, 9);
                if (callback) {
                    globalThis.__command_handlers.set(id, callback);
                }
                return __invoke_host("add_ribbon_icon", {
                    id,
                    icon: String(icon),
                    title: String(title)
                });
            }
            addSettingTab(tab) {
                this.settingTab = tab;
                if (tab && typeof tab.display === "function") {
                    try {
                        tab.display();
                        const tree = __velement_to_uinode(tab.containerEl);
                        __invoke_host("set_view_tree", { view_id: 0, tree });
                    } catch (e) {
                        // Some plugins rely on asynchronous data loaded after onload.
                        // We retain this.settingTab for on-demand rendering.
                    }
                }
            }
            registerEvent(eventRef) {
                return eventRef;
            }
            async loadData() {
                return __invoke_host("load_data", {});
            }
            async saveData(data) {
                return __invoke_host("save_data", { data });
            }
            registerMarkdownCodeBlockProcessor(language, handler) {
                globalThis.__code_block_handlers.set(String(language).toLowerCase(), handler);
                return __invoke_host("register_code_block_processor", { language: String(language) });
            }
            registerView(type, viewCreator) {}
            registerExtensions(extensions, viewType) {}
        }

        const rawVault = {
            read: (path) => __invoke_host("vault_read", { path: typeof path === 'object' ? path.path : path }),
            modify: (path, data) => __invoke_host("vault_modify", { path: typeof path === 'object' ? path.path : path, data }),
            create: (path, data) => __invoke_host("vault_create", { path, data }),
            delete: (path) => __invoke_host("vault_delete", { path: typeof path === 'object' ? path.path : path }),
            process: async (file, fn) => {
                const path = typeof file === 'object' ? file.path : file;
                const content = await __invoke_host("vault_read", { path });
                const newContent = await fn(content);
                if (typeof newContent === "string") {
                    await __invoke_host("vault_modify", { path, data: newContent });
                }
                return newContent;
            },
            getMarkdownFiles: () => {
                const files = __host_get_markdown_files();
                return files.map(p => new TFile(p));
            },
            getFiles: () => {
                const files = __host_get_markdown_files();
                return files.map(p => new TFile(p));
            },
            getAbstractFileByPath: (path) => {
                return new TFile(path);
            },
            adapter: {
                read: (path) => __invoke_host("vault_read", { path }),
                write: (path, data) => __invoke_host("vault_modify", { path, data }),
                exists: async (path) => {
                    const files = __host_get_markdown_files();
                    return files.includes(String(path));
                }
            }
        };

        const rawWorkspace = {
            getActiveFile: () => {
                const p = __host_get_active_file();
                return p ? new TFile(p) : null;
            },
            openFile: (path) => __invoke_host("open_file", { path: typeof path === 'object' ? path.path : path }),
            getLeaf: () => new WorkspaceLeaf(rawWorkspace),
            getLeftLeaf: () => new WorkspaceLeaf(rawWorkspace),
            getRightLeaf: () => new WorkspaceLeaf(rawWorkspace),
            registerMarkdownCodeBlockProcessor: (language, handler) => {
                globalThis.__code_block_handlers.set(String(language).toLowerCase(), handler);
                return __invoke_host("register_code_block_processor", { language: String(language) });
            },
            on: (_event, _cb) => ({ id: "event-sub" }),
            trigger: () => {}
        };

        const rawMetadataCache = {
            getFileCache: (_file) => ({
                tags: [],
                links: [],
                headings: [],
                frontmatter: {}
            }),
            on: (_event, _cb) => ({ id: "meta-sub" }),
            trigger: () => {}
        };

        const app = {
            vault: makeAuditedProxy(rawVault, "app.vault"),
            workspace: makeAuditedProxy(rawWorkspace, "app.workspace"),
            metadataCache: makeAuditedProxy(rawMetadataCache, "app.metadataCache"),
            commands: {
                executeCommandById: (id) => __invoke_host("execute_command", { id: String(id) }),
            }
        };

        globalThis.Component = Component;
        globalThis.Plugin = Plugin;
        globalThis.PluginSettingTab = PluginSettingTab;
        globalThis.Setting = Setting;
        globalThis.Modal = Modal;
        globalThis.SuggestModal = SuggestModal;
        globalThis.FuzzySuggestModal = FuzzySuggestModal;
        globalThis.View = View;
        globalThis.ItemView = ItemView;
        globalThis.MarkdownRenderChild = MarkdownRenderChild;
        globalThis.Events = Events;
        globalThis.Scope = Scope;
        globalThis.WorkspaceLeaf = WorkspaceLeaf;
        globalThis.BaseComponent = BaseComponent;
        globalThis.ValueComponent = ValueComponent;
        globalThis.ButtonComponent = ButtonComponent;
        globalThis.ExtraButtonComponent = ExtraButtonComponent;
        globalThis.ToggleComponent = ToggleComponent;
        globalThis.TextComponent = TextComponent;
        globalThis.SliderComponent = SliderComponent;
        globalThis.DropdownComponent = DropdownComponent;
        globalThis.Menu = Menu;
        globalThis.MenuItem = MenuItem;
        globalThis.Notice = Notice;
        globalThis.TFile = TFile;
        globalThis.TFolder = TFolder;
        globalThis.TAbstractFile = TAbstractFile;
        globalThis.normalizePath = normalizePath;
        globalThis.requestUrl = requestUrl;
        globalThis.moment = moment;
        globalThis.app = makeAuditedProxy(app, "app");

        const fallbackClasses = new Map();
        function getOrCreateFallbackClass(className) {
            if (!fallbackClasses.has(className)) {
                const stub = class ObsidianClassStub extends Component {
                    constructor(...args) {
                        super(...args);
                    }
                };
                Object.defineProperty(stub, "name", { value: className });
                fallbackClasses.set(className, stub);
            }
            return fallbackClasses.get(className);
        }

        const obsidianExports = {
            Component,
            Plugin,
            PluginSettingTab,
            Setting,
            Modal,
            SuggestModal,
            FuzzySuggestModal,
            View,
            ItemView,
            MarkdownRenderChild,
            Events,
            Scope,
            WorkspaceLeaf,
            BaseComponent,
            ValueComponent,
            ButtonComponent,
            ExtraButtonComponent,
            ToggleComponent,
            TextComponent,
            SliderComponent,
            DropdownComponent,
            Menu,
            MenuItem,
            Notice,
            TFile,
            TFolder,
            TAbstractFile,
            normalizePath,
            requestUrl,
            moment,
            app: globalThis.app
        };

        const obsidianProxy = new Proxy(obsidianExports, {
            get(target, prop) {
                if (typeof prop === "symbol" || prop === "then" || prop === "toJSON") {
                    return target[prop];
                }
                if (prop in target) {
                    return target[prop];
                }
                const name = String(prop);
                if (name.length > 0 && name[0] >= 'A' && name[0] <= 'Z') {
                    return getOrCreateFallbackClass(name);
                }
                return undefined;
            }
        });

        globalThis.require = function(mod) {
            if (mod === "obsidian") {
                return obsidianProxy;
            }
            return createLazyFailProxy(mod);
        };
    "#;

    ctx.eval::<(), _>(bootstrap_js)?;
    Ok(())
}

/// Translates a JavaScript request tuple into a Rust ApiRequest.
fn parse_js_api_request(kind: &str, payload: serde_json::Value) -> ApiRequest {
    match kind {
        "notice" => {
            let text = payload
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let timeout = payload
                .get("timeout_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(4000) as u32;
            ApiRequest::Notice {
                text,
                timeout_ms: timeout,
            }
        }
        "vault_read" => {
            let path = payload
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            ApiRequest::VaultRead { path }
        }
        "vault_modify" => {
            let path = payload
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let data = payload
                .get("data")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            ApiRequest::VaultModify { path, data }
        }
        "vault_create" => {
            let path = payload
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let data = payload
                .get("data")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            ApiRequest::VaultCreate { path, data }
        }
        "vault_delete" => {
            let path = payload
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            ApiRequest::VaultDelete { path }
        }
        "open_file" => {
            let path = payload
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            ApiRequest::OpenFile { path }
        }
        "load_data" => ApiRequest::LoadData,
        "save_data" => {
            let data = payload
                .get("data")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            ApiRequest::SaveData { data }
        }
        "register_code_block_processor" => {
            let language = payload
                .get("language")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            ApiRequest::RegisterCodeBlockProcessor { language }
        }
        "set_code_block_view" => {
            let block_id = payload
                .get("block_id")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            let tree: crate::plugins::ui::UiNode = payload
                .get("tree")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_else(|| crate::plugins::ui::UiNode::Text {
                    value: "".to_string(),
                    style: crate::plugins::ui::TextStyle::default(),
                });
            ApiRequest::SetCodeBlockView { block_id, tree }
        }
        "add_command" => {
            let id = payload
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let name = payload
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(&id)
                .to_string();
            let hotkey = payload
                .get("hotkey")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            ApiRequest::AddCommand { id, name, hotkey }
        }
        "add_ribbon_icon" => {
            let id = payload
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let icon = payload
                .get("icon")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let title = payload
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            ApiRequest::AddRibbonIcon { id, icon, title }
        }
        "execute_command" => {
            let id = payload
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            ApiRequest::ExecuteCommand { id }
        }
        "set_view_tree" => {
            let view_id = payload
                .get("view_id")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let tree: crate::plugins::ui::UiNode = payload
                .get("tree")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_else(|| crate::plugins::ui::UiNode::Text {
                    value: "".to_string(),
                    style: crate::plugins::ui::TextStyle::default(),
                });
            ApiRequest::SetViewTree { view_id, tree }
        }
        "http" => {
            let url = payload.get("url").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let method = payload.get("method").and_then(|v| v.as_str()).unwrap_or("GET").to_string();
            let body = payload.get("body").and_then(|v| v.as_str()).map(|s| s.to_string());
            ApiRequest::Http { url, method, body }
        }
        _ => {
            eprintln!("[plugin] Unknown request kind: {}", kind);
            ApiRequest::Unknown {
                req_kind: kind.to_string(),
            }
        }
    }
}

/// Dispatches a UI interaction event (click, change) into QuickJS.
fn dispatch_ui_interaction<'js>(
    ctx: &Ctx<'js>,
    _view_id: u64,
    node_id: String,
    event: String,
    value: Option<String>,
) {
    let dispatch_fn: rquickjs::Result<Function> = ctx.globals().get("__dispatch_ui_interaction");
    if let Ok(func) = dispatch_fn {
        let val = value.unwrap_or_default();
        let _ = func.call::<_, ()>((node_id, event, val));
    }
}

/// Dispatches a command execution trigger into QuickJS.
fn dispatch_command_execution<'js>(ctx: &Ctx<'js>, command_id: String) {
    let dispatch_fn: rquickjs::Result<Function> = ctx.globals().get("__dispatch_command_execution");
    if let Ok(func) = dispatch_fn {
        let _ = func.call::<_, ()>((command_id,));
    }
}

/// Dispatches an incoming code block processing request into QuickJS.
fn dispatch_code_block_process<'js>(
    ctx: &Ctx<'js>,
    language: String,
    source: String,
    block_id: usize,
) {
    let dispatch_fn: rquickjs::Result<Function> = ctx.globals().get("__dispatch_code_block_process");
    if let Ok(func) = dispatch_fn {
        let _ = func.call::<_, ()>((language, source, block_id as f64));
    }
}

/// Resolves a pending JS Promise corresponding to a completed host RequestId.
fn dispatch_js_response<'js>(ctx: &Ctx<'js>, id: RequestId, ok: bool, result: serde_json::Value) {
    let dispatch_fn: rquickjs::Result<Function> = ctx.globals().get("__dispatch_host_response");
    if let Ok(func) = dispatch_fn {
        let json_str = serde_json::to_string(&result).unwrap_or_else(|_| "null".to_string());
        let eval_str = format!("JSON.parse({})", serde_json::to_string(&json_str).unwrap());
        let val: Value = ctx.eval(eval_str.as_str()).unwrap_or(Value::new_null(ctx.clone()));
        let _ = func.call::<_, ()>((id as f64, ok, val));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::manifest::PluginPermissions;

    #[test]
    fn test_quickjs_runtime_eval_notice() {
        let manifest = PluginManifest {
            id: "notice-plugin".into(),
            name: "Notice Test".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        let script = r#"
            new Notice("Hello from QuickJS!");
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        )
        .unwrap();

        // Give the background worker thread a moment to initialize and execute
        std::thread::sleep(Duration::from_millis(50));

        // The script should immediately emit an ApiRequest::Notice to api_rx
        let received = api_rx.try_recv().unwrap();
        assert_eq!(received.plugin_id, "notice-plugin");
        if let ApiRequest::Notice { text, .. } = received.request {
            assert_eq!(text, "Hello from QuickJS!");
        } else {
            panic!("Expected ApiRequest::Notice");
        }

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_quickjs_async_vault_read_promise_resolution() {
        let manifest = PluginManifest {
            id: "read-plugin".into(),
            name: "Read Test".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        // The plugin calls async app.vault.read, and upon promise resolution, calls Notice with the result
        let script = r#"
            (async () => {
                const content = await app.vault.read("Notes/Daily.md");
                new Notice("Received: " + content);
            })();
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        )
        .unwrap();

        std::thread::sleep(Duration::from_millis(50));

        // 1. Host receives VaultRead request
        let read_req = api_rx.try_recv().unwrap();
        assert_eq!(read_req.plugin_id, "read-plugin");
        if let ApiRequest::VaultRead { path } = read_req.request {
            assert_eq!(path, "Notes/Daily.md");
        } else {
            panic!("Expected ApiRequest::VaultRead");
        }

        // 2. Host sends Response back to plugin worker
        let _ = handle.host_events_tx.unbounded_send(HostEvent::response_ok(
            read_req.request_id,
            serde_json::json!("Plan for today"),
        ));

        std::thread::sleep(Duration::from_millis(50));

        // 3. Plugin promise resolves and sends Notice("Received: Plan for today")
        let notice_req = api_rx.try_recv().unwrap();
        if let ApiRequest::Notice { text, .. } = notice_req.request {
            assert_eq!(text, "Received: Plan for today");
        } else {
            panic!("Expected ApiRequest::Notice with resolved content");
        }

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_quickjs_load_and_save_data() {
        let manifest = PluginManifest {
            id: "data-plugin".into(),
            name: "Data Test".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        let script = r#"
            (async () => {
                const plugin = new Plugin(app, { id: "data-plugin" });
                await plugin.saveData({ theme: "dark", count: 10 });
                const loaded = await plugin.loadData();
                new Notice("Theme: " + loaded.theme);
            })();
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        )
        .unwrap();

        std::thread::sleep(Duration::from_millis(50));

        // 1. Host receives SaveData request
        let save_req = api_rx.try_recv().unwrap();
        assert_eq!(save_req.plugin_id, "data-plugin");
        if let ApiRequest::SaveData { data } = save_req.request {
            assert_eq!(data["theme"], "dark");
            assert_eq!(data["count"], 10);
        } else {
            panic!("Expected ApiRequest::SaveData");
        }

        // Fulfill SaveData
        let _ = handle.host_events_tx.unbounded_send(HostEvent::response_ok(
            save_req.request_id,
            serde_json::Value::Null,
        ));

        std::thread::sleep(Duration::from_millis(50));

        // 2. Host receives LoadData request
        let load_req = api_rx.try_recv().unwrap();
        assert_eq!(load_req.plugin_id, "data-plugin");
        assert!(matches!(load_req.request, ApiRequest::LoadData));

        // Fulfill LoadData with JSON response
        let _ = handle.host_events_tx.unbounded_send(HostEvent::response_ok(
            load_req.request_id,
            serde_json::json!({ "theme": "dark", "count": 10 }),
        ));

        std::thread::sleep(Duration::from_millis(50));

        // 3. Notice is emitted
        let notice_req = api_rx.try_recv().unwrap();
        if let ApiRequest::Notice { text, .. } = notice_req.request {
            assert_eq!(text, "Theme: dark");
        } else {
            panic!("Expected ApiRequest::Notice with loaded theme");
        }

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_quickjs_register_markdown_code_block_processor() {
        let manifest = PluginManifest {
            id: "dataview-plugin".to_string(),
            name: "Dataview".to_string(),
            version: "0.5.0".to_string(),
            min_app_version: None,
            author: "Tester".to_string(),
            description: "Dataview query processor".to_string(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        let script = r#"
            const plugin = new Plugin(app, { id: "dataview-plugin" });
            plugin.registerMarkdownCodeBlockProcessor("dataview", (source) => {
                return {
                    type: "column",
                    children: [
                        { type: "text", value: "Query result for: " + source.trim() }
                    ]
                };
            });
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        )
        .unwrap();

        std::thread::sleep(Duration::from_millis(50));

        // 1. Host receives RegisterCodeBlockProcessor
        let reg_req = api_rx.try_recv().unwrap();
        assert_eq!(reg_req.plugin_id, "dataview-plugin");
        if let ApiRequest::RegisterCodeBlockProcessor { language } = reg_req.request {
            assert_eq!(language, "dataview");
        } else {
            panic!("Expected ApiRequest::RegisterCodeBlockProcessor");
        }

        // 2. Host sends ProcessCodeBlock
        let _ = handle.host_events_tx.unbounded_send(HostEvent::ProcessCodeBlock {
            language: "dataview".to_string(),
            source: "TABLE file.name FROM #project".to_string(),
            block_id: 42,
        });

        std::thread::sleep(Duration::from_millis(50));

        // 3. Host receives SetCodeBlockView with processed UI tree
        let view_req = api_rx.try_recv().unwrap();
        assert_eq!(view_req.plugin_id, "dataview-plugin");
        if let ApiRequest::SetCodeBlockView { block_id, tree } = view_req.request {
            assert_eq!(block_id, 42);
            if let crate::plugins::ui::UiNode::Column { children, .. } = tree {
                assert_eq!(children.len(), 1);
                if let crate::plugins::ui::UiNode::Text { value, .. } = &children[0] {
                    assert_eq!(value, "Query result for: TABLE file.name FROM #project");
                } else {
                    panic!("Expected UiNode::Text child");
                }
            } else {
                panic!("Expected UiNode::Column");
            }
        } else {
            panic!("Expected ApiRequest::SetCodeBlockView");
        }

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_quickjs_class_based_obsidian_plugin_lifecycle() {
        let manifest = PluginManifest {
            id: "new-note-np".to_string(),
            name: "New Note NP".to_string(),
            version: "1.0.0".to_string(),
            min_app_version: Some("1.5.0".to_string()),
            author: "leruetkins".to_string(),
            description: "Adds a Create new note button back to the left ribbon.".to_string(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        // The exact code structure from user's plugin D:\Naben\Vibecodium\.vault\plugins\obsidian-new-note-np\main.js
        let script = r#"
            const { Plugin, Notice } = require("obsidian");

            module.exports = class NewNoteRibbonPlugin extends Plugin {
                async onload() {
                    this.addRibbonIcon("square-pen", "Новая заметка", async () => {
                        try {
                            await this.app.commands.executeCommandById("file-explorer:new-file");
                        } catch (error) {
                            new Notice("Не удалось создать заметку");
                        }
                    });
                }
            };
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        )
        .unwrap();

        std::thread::sleep(Duration::from_millis(50));

        // 1. Host receives AddRibbonIcon from plugin's onload()
        let ribbon_req = api_rx.try_recv().unwrap();
        assert_eq!(ribbon_req.plugin_id, "new-note-np");
        let (icon_id, icon_name, icon_title) = match ribbon_req.request {
            ApiRequest::AddRibbonIcon { id, icon, title } => (id, icon, title),
            other => panic!("Expected AddRibbonIcon, got {:?}", other),
        };
        assert_eq!(icon_name, "square-pen");
        assert_eq!(icon_title, "Новая заметка");

        // Fulfill AddRibbonIcon response
        let _ = handle.host_events_tx.unbounded_send(HostEvent::response_ok(
            ribbon_req.request_id,
            serde_json::Value::Null,
        ));

        // 2. Simulate ribbon icon click: host triggers ExecuteCommand with registered icon_id
        let _ = handle.host_events_tx.unbounded_send(HostEvent::ExecuteCommand {
            command_id: icon_id,
        });

        std::thread::sleep(Duration::from_millis(50));

        // 3. Plugin callback triggers executeCommandById("file-explorer:new-file")
        let exec_req = api_rx.try_recv().unwrap();
        assert_eq!(exec_req.plugin_id, "new-note-np");
        match exec_req.request {
            ApiRequest::ExecuteCommand { id } => {
                assert_eq!(id, "file-explorer:new-file");
            }
            other => panic!("Expected ExecuteCommand, got {:?}", other),
        }

        // Fulfill command execution
        let _ = handle.host_events_tx.unbounded_send(HostEvent::response_ok(
            exec_req.request_id,
            serde_json::Value::Null,
        ));

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_snapshot_permission_enforcement() {
        use crate::plugins::manifest::VaultAccess;

        let manifest = PluginManifest {
            id: "untrusted-plugin".into(),
            name: "Untrusted".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions {
                vault: VaultAccess::None, // NO vault access
                network: vec![],
            },
        };

        let (api_tx, mut api_rx) = unbounded();
        let mut snapshot_data = Snapshot::default();
        snapshot_data.files = Arc::new([crate::vault::FileInfo {
            path: "Secret/Password.md".into(),
            name: "Password.md".into(),
            basename: "Password".into(),
            extension: "md".into(),
        }]);
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(snapshot_data));

        let script = r#"
            const files = app.vault.getMarkdownFiles();
            new Notice("Files count: " + files.length);
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        )
        .unwrap();

        std::thread::sleep(Duration::from_millis(50));

        let notice_req = api_rx.try_recv().unwrap();
        if let ApiRequest::Notice { text, .. } = notice_req.request {
            // Must be 0 because vault:none cannot view snapshot files
            assert_eq!(text, "Files count: 0");
        } else {
            panic!("Expected Notice");
        }

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_obsidian_setting_tab_and_velement_flow() {
        let manifest = PluginManifest {
            id: "setting-plugin".into(),
            name: "Setting Test".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        let script = r#"
            const { Plugin, PluginSettingTab, Setting } = require("obsidian");

            class SampleSettingTab extends PluginSettingTab {
                display() {
                    const { containerEl } = this;
                    containerEl.empty();
                    new Setting(containerEl)
                        .setName("Enable Autocomplete")
                        .setDesc("Provides inline suggestions")
                        .addToggle((toggle) => toggle.setValue(true));
                }
            }

            module.exports = class TestPlugin extends Plugin {
                async onload() {
                    this.addSettingTab(new SampleSettingTab(this.app, this));
                }
            };
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        )
        .unwrap();

        std::thread::sleep(Duration::from_millis(60));

        let view_tree_call = api_rx.try_recv().expect("Expected SetViewTree call");
        if let ApiRequest::SetViewTree { tree, .. } = view_tree_call.request {
            let json = serde_json::to_string(&tree).unwrap();
            assert!(json.contains("Enable Autocomplete"));
            assert!(json.contains("Provides inline suggestions"));
            assert!(json.contains("\"type\":\"setting_item\""));
            assert!(json.contains("\"type\":\"toggle\""));
        } else {
            panic!("Expected SetViewTree request, got {:?}", view_tree_call.request);
        }

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_lazy_fail_and_audited_proxy() {
        let manifest = PluginManifest {
            id: "proxy-test-plugin".into(),
            name: "Proxy Test".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        let script = r#"
            // 1. External module require returns lazyFail proxy instead of throwing
            const cm = require("@codemirror/view");
            const hasCm = typeof cm === "function";

            // 2. Feature detection on app: missing property returns undefined
            const missing = typeof app.nonExistentFeature === "undefined";

            // 3. Known unimplemented method throws with [Aquilum] prefix
            let threwExpected = false;
            try {
                app.vault.copy("from.md", "to.md");
            } catch (e) {
                threwExpected = e.message.includes("[Aquilum] Method app.vault.copy is not implemented");
            }

            new Notice(`CM:${hasCm}, Missing:${missing}, Unimpl:${threwExpected}`);
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        )
        .unwrap();

        std::thread::sleep(Duration::from_millis(50));

        let notice_call = api_rx.try_recv().expect("Expected Notice call");
        if let ApiRequest::Notice { text, .. } = notice_call.request {
            assert_eq!(text, "CM:true, Missing:true, Unimpl:true");
        } else {
            panic!("Expected Notice request");
        }

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_obsidian_class_extension_and_fallback_stubs() {
        let manifest = PluginManifest {
            id: "class-extension-test".into(),
            name: "Class Extension Test".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        let script = r#"
            const obsidian = require("obsidian");

            // Standard Obsidian classes
            class MyView extends obsidian.View {}
            class MyComponent extends obsidian.Component {}
            class MyModal extends obsidian.Modal {}
            class MySuggestModal extends obsidian.SuggestModal {}

            // Arbitrary unmodeled Obsidian classes (like Homepage or other plugins)
            class CustomObsidianExtension extends obsidian.WorkspaceRibbon {}
            class AnotherUnknownClass extends obsidian.NonExistentClass123 {}

            const v = new MyView();
            const c = new MyComponent();
            const ext = new CustomObsidianExtension();
            const unk = new AnotherUnknownClass();

            new Notice(`SUCCESS:view=${typeof v},comp=${typeof c},ext=${typeof ext},unk=${typeof unk}`);
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        )
        .unwrap();

        std::thread::sleep(Duration::from_millis(50));

        let notice_call = api_rx.try_recv().expect("Expected Notice call");
        if let ApiRequest::Notice { text, .. } = notice_call.request {
            assert_eq!(text, "SUCCESS:view=object,comp=object,ext=object,unk=object");
        } else {
            panic!("Expected Notice request, got {:?}", notice_call.request);
        }

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }
}

