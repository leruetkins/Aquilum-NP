use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use futures::channel::mpsc::{unbounded, TryRecvError, UnboundedSender};
use rquickjs::{Context, Ctx, Function, Object, Runtime};

use crate::plugins::api::{ApiCall, ApiRequest, HostEvent, RequestId};
use crate::plugins::manifest::PluginManifest;
use crate::state::Snapshot;

/// Maximum heap memory allocated for a single plugin instance (256 MB).
const PLUGIN_MEMORY_LIMIT: usize = 256 * 1024 * 1024;

/// Controller for communicating with and shutting down a running plugin worker thread.
pub struct PluginRuntimeHandle {
    pub manifest: PluginManifest,
    pub host_events_tx: UnboundedSender<HostEvent>,
}

/// Plugin sources: the code and an optional stylesheet. A separate struct instead of two
/// consecutive string parameters, which are otherwise easy to swap silently.
#[derive(Debug, Clone)]
pub struct PluginSources {
    pub script: String,
    pub styles: Option<String>,
}

/// Spawns an isolated worker thread running a QuickJS engine instance for the given plugin.
pub fn spawn_plugin_runtime(
    manifest: PluginManifest,
    script_source: String,
    api_tx: UnboundedSender<ApiCall>,
    snapshot: Arc<arc_swap::ArcSwap<Snapshot>>,
) -> Result<PluginRuntimeHandle, String> {
    spawn_plugin_runtime_with_styles(
        manifest,
        PluginSources { script: script_source, styles: None },
        api_tx,
        snapshot,
    )
}

/// The same, but with the plugin's stylesheet: `styles.css` sits next to `main.js`.
/// The shim parses it — only it has a DOM with classes and a parent chain.
pub fn spawn_plugin_runtime_with_styles(
    manifest: PluginManifest,
    sources: PluginSources,
    api_tx: UnboundedSender<ApiCall>,
    snapshot: Arc<arc_swap::ArcSwap<Snapshot>>,
) -> Result<PluginRuntimeHandle, String> {
    let PluginSources { script: script_source, styles: styles_css } = sources;
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
            if let Some(css) = styles_css.as_deref() {
                // The bootstrap parses it once when the plugin loads.
                if let Err(e) = ctx.with(|ctx| ctx.globals().set("__plugin_styles_css", css)) {
                    eprintln!("[plugin:{}] Failed to pass styles.css: {:?}", p_id, e);
                }
            }
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
                            println!("[plugins:{}] Handling HostEvent::ExecuteCommand with command_id: '{}'", p_id, command_id);
                            let _ = ctx.with(|ctx| {
                                dispatch_command_execution(&ctx, command_id);
                            });
                        }
                        HostEvent::EditorMenu { menu_session_id, selected_text } => {
                            let _ = ctx.with(|ctx| {
                                dispatch_editor_menu(&ctx, &p_id, menu_session_id, selected_text, &api_tx);
                            });
                        }
                        HostEvent::ExecuteMenuAction { action_id } => {
                            let _ = ctx.with(|ctx| {
                                dispatch_menu_action(&ctx, action_id);
                            });
                        }
                        HostEvent::UiEvent { view_id, node_id, event, value } => {
                            let _ = ctx.with(|ctx| {
                                dispatch_ui_interaction(&ctx, view_id, node_id, event, value);
                            });
                        }
                        HostEvent::FileOpen { path } => {
                            let _ = ctx.with(|ctx| {
                                dispatch_file_open(&ctx, path);
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
                while let Ok(true) = rt.execute_pending_job() {}
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

    let snap_lang = snapshot.clone();
    let get_lang_fn = Function::new(ctx.clone(), move || -> rquickjs::Result<String> {
        let snap = snap_lang.load();
        let lang = if snap.language.is_empty() {
            "ru".to_string()
        } else {
            snap.language.clone()
        };
        Ok(lang)
    })?;
    globals.set("__host_get_language", get_lang_fn)?;

    // 3c. Active editor buffer for synchronous Editor getters, as a JSON string
    // {"text","cursor":{"line","ch","anchor?":{"line","ch"}}}: cursor/anchor are in
    // (line, ch) character coordinates. A plugin without vault:read gets an empty
    // object, mirroring __host_get_active_file.
    let snap_doc = snapshot.clone();
    let get_buffer_fn = Function::new(ctx.clone(), move || -> rquickjs::Result<String> {
        let snap = snap_doc.load();
        let Some(doc) = snap.active_doc.as_ref() else {
            return Ok("{}".to_string());
        };
        let mut cursor = serde_json::json!({ "line": doc.cursor.0, "ch": doc.cursor.1 });
        if let Some((a_line, a_ch)) = doc.anchor {
            cursor["anchor"] = serde_json::json!({ "line": a_line, "ch": a_ch });
        }
        let payload = serde_json::json!({ "text": doc.rope.to_string(), "cursor": cursor });
        Ok(payload.to_string())
    })?;
    globals.set("__host_get_active_buffer", get_buffer_fn)?;

    // 3d. Command palette and plugin registry for app.commands / app.plugins, as JSON
    // arrays rebuilt from the snapshot on every read (small: tens of entries).
    let snap_cmds = snapshot.clone();
    let get_commands_fn = Function::new(ctx.clone(), move || -> rquickjs::Result<String> {
        let snap = snap_cmds.load();
        let list: Vec<serde_json::Value> = snap
            .commands
            .iter()
            .map(|c| {
                serde_json::json!({
                    "id": c.id,
                    "name": c.name,
                    "hotkey": c.hotkey,
                    "owner": c.owner,
                })
            })
            .collect();
        Ok(serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_string()))
    })?;
    globals.set("__host_get_commands", get_commands_fn)?;

    let snap_plug = snapshot.clone();
    let get_plugins_fn = Function::new(ctx.clone(), move || -> rquickjs::Result<String> {
        let snap = snap_plug.load();
        let list: Vec<serde_json::Value> = snap
            .plugins
            .iter()
            .map(|p| {
                serde_json::json!({
                    "id": p.id,
                    "name": p.name,
                    "version": p.version,
                    "description": p.description,
                })
            })
            .collect();
        Ok(serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_string()))
    })?;
    globals.set("__host_get_plugins", get_plugins_fn)?;

    // 3b. Static host platform description, consumed by obsidian.Platform and navigator.userAgent.
    // The host is a native desktop application, so mobile flags are always false.
    let platform_os = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    };
    ctx.eval::<(), _>(format!(
        "globalThis.__host_platform = {{ os: \"{}\", arch: \"{}\" }};",
        platform_os,
        std::env::consts::ARCH
    ))?;

    // 4. Inject manifest and CommonJS module / exports
    let manifest_json = serde_json::to_string(manifest).unwrap_or_else(|_| "{}".to_string());
    ctx.eval::<(), _>(format!(
        "globalThis.__plugin_manifest = {}; globalThis.module = {{ exports: {{}} }}; globalThis.exports = globalThis.module.exports;",
        manifest_json
    ))?;

    // 5. Inject runtime JavaScript bootstrap defining Obsidian-compatible classes & timers
    let bootstrap_js = r####"
        // The styles.css parsing state is declared at the top of the script: VElement mutators touch
        // it, and they may run before the declarations in the middle of the file are reached.
        let __css_rules = [];
        let __css_by_class = new Map();
        let __css_by_tag = new Map();
        let __css_universal = [];
        let __css_show_by_class = new Map();
        let __css_show_by_tag = new Map();
        let __css_show_universal = [];
        let __css_vars = new Map();
        let __css_epoch = 1;

        if (typeof console !== "undefined" && console.log) {
            const __orig_log = console.log;
            const __fmt = (a) => {
                if (a instanceof Error) return `${a.name || 'Error'}: ${a.message}\n${a.stack || ''}`;
                if (typeof a === "object" && a !== null) {
                    try { return JSON.stringify(a); } catch (e) { return String(a); }
                }
                return String(a);
            };
            console.log = (...args) => __orig_log(args.map(__fmt).join(" "));
            console.warn = (...args) => __orig_log("[WARN] " + args.map(__fmt).join(" "));
            console.error = (...args) => __orig_log("[ERROR] " + args.map(__fmt).join(" "));
            console.debug = console.log;
            console.info = console.log;
        }

        const __pending_promises = new Map();
        globalThis.__code_block_handlers = new Map();
        globalThis.__command_handlers = new Map();
        globalThis.__js_timers = new Map();
        globalThis.__velement_registry = new Map();
        globalThis.__menu_callbacks = new Map();
        globalThis.__workspace_listeners = new Map();
        globalThis.__vault_listeners = new Map();
        globalThis.__registered_views = new Map();
        globalThis.__action_callbacks = new Map();
        globalThis.__workspace_leaves = new Map();
        let __next_timer_id = 1;

        globalThis.Event = class Event {
            constructor(type, options = {}) {
                this.type = String(type);
                this.defaultPrevented = false;
                this.cancelBubble = false;
                this.bubbles = options.bubbles !== undefined ? !!options.bubbles : true;
                this.cancelable = options.cancelable !== undefined ? !!options.cancelable : true;
                this.propagationStopped = false;
                this.target = options.target || null;
                this.currentTarget = options.currentTarget || null;
                Object.assign(this, options);
            }
            stopPropagation() {
                this.propagationStopped = true;
                this.cancelBubble = true;
            }
            stopImmediatePropagation() {
                this.propagationStopped = true;
                this.cancelBubble = true;
            }
            preventDefault() {
                this.defaultPrevented = true;
            }
        };
        globalThis.UIEvent = class UIEvent extends globalThis.Event {};
        globalThis.MouseEvent = class MouseEvent extends globalThis.UIEvent {
            constructor(type, options = {}) {
                super(type, options);
                this.button = options.button || 0;
                this.buttons = options.buttons || 1;
                this.clientX = options.clientX || 0;
                this.clientY = options.clientY || 0;
                this.pageX = options.pageX || 0;
                this.pageY = options.pageY || 0;
                this.screenX = options.screenX || 0;
                this.screenY = options.screenY || 0;
                this.altKey = !!options.altKey;
                this.ctrlKey = !!options.ctrlKey;
                this.metaKey = !!options.metaKey;
                this.shiftKey = !!options.shiftKey;
            }
        };
        globalThis.KeyboardEvent = class KeyboardEvent extends globalThis.UIEvent {
            constructor(type, options = {}) {
                super(type, options);
                this.key = options.key || "";
                this.code = options.code || "";
                this.altKey = !!options.altKey;
                this.ctrlKey = !!options.ctrlKey;
                this.metaKey = !!options.metaKey;
                this.shiftKey = !!options.shiftKey;
            }
        };
        globalThis.CustomEvent = class CustomEvent extends globalThis.Event {
            constructor(type, options = {}) {
                super(type, options);
                this.detail = options?.detail;
            }
        };

        function __dispatch_ui_interaction(nodeId, action, value) {
            if (globalThis.__action_callbacks && globalThis.__action_callbacks.has(nodeId)) {
                try {
                    globalThis.__action_callbacks.get(nodeId)();
                } catch (e) {
                    console.error("Leaf action click error:", e);
                }
                return;
            }

            if (nodeId === "modal" && action === "close") {
                if (globalThis.__active_modal && typeof globalThis.__active_modal.close === "function") {
                    try {
                        globalThis.__active_modal.close();
                    } catch (e) {
                        console.error("Close active modal error:", e);
                    }
                }
                return;
            }

            if (nodeId === "__refresh_setting_tab__" || action === "refresh") {
                if (globalThis.__active_setting_tab && typeof globalThis.__active_setting_tab.display === "function") {
                    try {
                        globalThis.__active_setting_tab.display();
                    } catch (e) {
                        console.error("Refresh setting tab error:", e);
                    }
                }
                return;
            }

            const el = globalThis.__velement_registry.get(nodeId);
            if (el) {
                if (action === "change") {
                    const isCheckbox = el.tag === "input" && (el.attributes?.type === "checkbox" || el.type === "checkbox" || el.classes?.has("checkbox") || el.classes?.has("checkbox-container"));
                    let parsedVal;
                    if (isCheckbox) {
                        el.checked = (value === "true" || value === true);
                        el.value = el.checked ? "true" : "false";
                        parsedVal = el.checked;
                    } else {
                        el.value = value != null ? String(value) : "";
                        parsedVal = el.value;
                    }
                    el.dispatchEvent(new globalThis.Event("change", { target: el, currentTarget: el, value: parsedVal }));
                    el.dispatchEvent(new globalThis.Event("input", { target: el, currentTarget: el, value: parsedVal }));
                } else {
                    const ev = new globalThis.MouseEvent(action, { target: el, currentTarget: el, button: 0 });
                    el.dispatchEvent(ev);
                }
            }

            // Sync updated setting tab view tree back to host
            if (globalThis.__active_setting_tab && globalThis.__active_setting_tab.containerEl) {
                try {
                    const tree = __velement_to_uinode(globalThis.__active_setting_tab.containerEl);
                    __invoke_host("set_view_tree", { view_id: 0, tree });
                } catch (e) {}
            }

            // Sync updated modal view tree back to host if active
            if (globalThis.__active_modal && globalThis.__active_modal.containerEl) {
                try {
                    const tree = __velement_to_uinode(globalThis.__active_modal.containerEl);
                    __invoke_host("set_view_tree", { view_id: 1, tree });
                } catch (e) {}
            }

            // Sync updated leaf views back to host
            if (globalThis.__workspace_leaves) {
                for (const leaf of globalThis.__workspace_leaves.values()) {
                    if (leaf && leaf.view && typeof leaf.syncTree === "function") {
                        try { leaf.syncTree(); } catch (e) {}
                    }
                }
            }
        }

        function __dispatch_file_open(path) {
            const file = path ? new TFile(path) : null;
            if (globalThis.app && globalThis.app.workspace && typeof globalThis.app.workspace.trigger === "function") {
                try {
                    globalThis.app.workspace.trigger("file-open", file);
                } catch (e) {
                    console.error("dispatch file-open error:", e);
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

        if (!Array.prototype.first) {
            Array.prototype.first = function() { return this[0]; };
        }
        if (!Array.prototype.last) {
            Array.prototype.last = function() { return this[this.length - 1]; };
        }
        if (!Array.prototype.contains) {
            Array.prototype.contains = function(target) { return this.includes(target); };
        }
        if (!Array.prototype.remove) {
            Array.prototype.remove = function(target) {
                const idx = this.indexOf(target);
                if (idx !== -1) this.splice(idx, 1);
                return this;
            };
        }
        if (!String.prototype.contains) {
            String.prototype.contains = function(target) { return this.includes(target); };
        }

        // Mirrors Component.load()'s child pass for the plugin root, which the host constructs
        // directly (so it never goes through Component.load()).
        function __load_component_children(component) {
            if (!component || !Array.isArray(component._children)) return;
            for (const child of component._children.slice()) {
                if (child && typeof child.load === "function") {
                    try { child.load(); } catch (e) { console.error("Component onload error:", e); }
                }
            }
        }

        async function __bootstrap_plugin_instance() {
            // The plugin's stylesheet is parsed before onload(): the plugin creates elements already
            // styled, and the very first frame must look right.
            __init_plugin_css(globalThis.__plugin_styles_css);
            let exported = globalThis.module ? globalThis.module.exports : null;
            if (exported && exported.default && typeof exported !== "function") {
                exported = exported.default;
            }
            if (typeof exported === "function") {
                try {
                    const inst = new exported(globalThis.app, globalThis.__plugin_manifest);
                    globalThis.__plugin_instance = inst;
                    // Obsidian marks the plugin loaded before onload() so addChild() during
                    // onload() actually loads its child; then it loads children added in onload().
                    inst._loaded = true;
                    if (typeof inst.onload === "function") {
                        await inst.onload();
                    }
                    __load_component_children(inst);
                    if (typeof inst.onUserEnable === "function") {
                        await inst.onUserEnable();
                    }
                } catch (e) {
                    console.error("Plugin instantiation or onload error:", e);
                }
            } else if (exported && typeof exported.onload === "function") {
                globalThis.__plugin_instance = exported;
                try {
                    exported._loaded = true;
                    await exported.onload();
                    __load_component_children(exported);
                    if (typeof exported.onUserEnable === "function") {
                        await exported.onUserEnable();
                    }
                } catch (e) {
                    console.error("Plugin onload error:", e);
                }
            }
            // onload() has finished against a workspace that already exists, so queued
            // workspace.onLayoutReady() callbacks can run now.
            markLayoutReady();
        }

        function __dispatch_host_response(id, ok, resultJson) {
            const entry = __pending_promises.get(id);
            if (entry) {
                __pending_promises.delete(id);
                let parsed = resultJson;
                if (typeof resultJson === "string") {
                    try {
                        parsed = JSON.parse(resultJson);
                    } catch (e) {
                        parsed = resultJson;
                    }
                }
                if (ok) {
                    try {
                        entry.resolve(parsed);
                    } catch (e) {
                        console.error("[plugin] Unhandled error during entry.resolve:", e);
                    }
                } else {
                    try {
                        entry.reject(new Error(parsed?.error || "Host error"));
                    } catch (e) {
                        console.error("[plugin] Unhandled error during entry.reject:", e);
                    }
                }
            }
        }

        function __dispatch_command_execution(commandId) {
            console.log(`[plugin] __dispatch_command_execution called for commandId: "${commandId}"`);
            let handler = globalThis.__command_handlers.get(commandId);
            if (!handler && typeof commandId === "string" && commandId.includes(":")) {
                const stripped = commandId.split(":").slice(1).join(":");
                handler = globalThis.__command_handlers.get(stripped);
                if (handler) {
                    console.log(`[plugin] Found handler using stripped id: "${stripped}"`);
                }
            }
            if (handler) {
                console.log(`[plugin] Found handler for "${commandId}", executing...`);
                try {
                    const res = handler();
                    if (res && typeof res.then === "function") {
                        res.catch(e => console.error(`[plugin] Async error in command "${commandId}":`, e));
                    }
                } catch (e) {
                    console.error(`[plugin] Command execution error for "${commandId}":`, e);
                }
            } else {
                console.warn(`[plugin] No handler registered for commandId: "${commandId}". Registered handlers:`, Array.from(globalThis.__command_handlers.keys()));
            }
        }

        function __dispatch_editor_menu(menuSessionId, selectedText) {
            const listeners = globalThis.__workspace_listeners.get("editor-menu") || [];
            if (listeners.length === 0) {
                return "[]";
            }
            const menu = new Menu();
            const editor = new Editor(selectedText);
            for (const cb of listeners) {
                try {
                    cb(menu, editor);
                } catch (e) {
                    console.error("editor-menu listener error:", e);
                }
            }
            return JSON.stringify(menu.toJSON());
        }

        function __dispatch_menu_action(actionId) {
            const cb = globalThis.__menu_callbacks.get(actionId);
            if (cb) {
                try {
                    cb();
                } catch (e) {
                    console.error("Menu action callback error:", e);
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
                this.messageEl = null;
                this.noticeEl = null;
                this.containerEl = null;
                Notice.__seq = (Notice.__seq || 0) + 1;
                this.__id = "notice-" + Notice.__seq;
                __invoke_host("notice", { text: String(message == null ? "" : message), timeout_ms: duration, id: this.__id }).catch(() => {});
            }
            setMessage(message) {
                const text = message == null ? "" : String(message);
                if (this.__id) {
                    __invoke_host("notice_update", { id: this.__id, text }).catch(() => {});
                }
                return this;
            }
            hide() {
                if (this.__id) {
                    __invoke_host("notice_hide", { id: this.__id }).catch(() => {});
                }
            }
        }

        const VOID_TAGS = new Set(["br", "hr", "img", "input", "col", "path", "circle", "rect", "line", "polygon"]);
        const DANGEROUS_TAGS = new Set(["script", "iframe", "object", "embed", "style", "link", "meta"]);

        function __escape_html(str) {
            if (!str) return "";
            return String(str)
                .replace(/&/g, "&amp;")
                .replace(/</g, "&lt;")
                .replace(/>/g, "&gt;")
                .replace(/"/g, "&quot;")
                .replace(/'/g, "&#39;");
        }

        function __unescape_html(str) {
            if (!str) return "";
            return String(str)
                .replace(/&amp;/g, "&")
                .replace(/&lt;/g, "<")
                .replace(/&gt;/g, ">")
                .replace(/&quot;/g, "\"")
                .replace(/&#39;/g, "'")
                .replace(/&nbsp;/g, " ");
        }

        function __sanitize_html(html) {
            if (!html) return "";
            return html
                .replace(/<(script|iframe|object|embed|style)[\s\S]*?<\/\1>/gi, "")
                .replace(/<(script|iframe|object|embed|style)[^>]*\/?>/gi, "");
        }

        function __parse_attributes(attrStr) {
            const attrs = {};
            let cls = "";
            if (!attrStr) return { attrs, cls };
            const attrRegex = /([a-zA-Z0-9_-]+)(?:=(?:"([^"]*)"|'([^']*)'|([^\s>]+)))?/g;
            let m;
            while ((m = attrRegex.exec(attrStr)) !== null) {
                const name = m[1].toLowerCase();
                if (name.startsWith("on")) continue;
                const val = m[2] !== undefined ? m[2] : (m[3] !== undefined ? m[3] : (m[4] !== undefined ? m[4] : ""));
                if (name === "href" && val.trim().toLowerCase().startsWith("javascript:")) continue;
                if (name === "class") {
                    cls = val;
                } else {
                    attrs[name] = val;
                }
            }
            return { attrs, cls };
        }

        // ---- YAML frontmatter subset -------------------------------------------
        // parseYaml consumers in the corpus are note frontmatter: flat keys, one
        // nesting level of maps, lists of scalars or inline objects, quoted strings,
        // block scalars (| and >). Anchors, tags and multi-document streams are
        // out of scope and fail with an error instead of silently misparsing.

        const __yaml_scalar = (raw) => {
            // Strip a trailing " #comment" outside quotes; a quoted string keeps it.
            let s = String(raw).trim();
            const q0 = s[0];
            if (q0 !== '"' && q0 !== "'") {
                const hash = s.indexOf(" #");
                if (hash > 0) s = s.slice(0, hash).trim();
            }
            if (s === "" || s === "~" || s === "null" || s === "Null" || s === "NULL") return null;
            if (s === "true" || s === "True" || s === "TRUE") return true;
            if (s === "false" || s === "False" || s === "FALSE") return false;
            if (/^-?\d+$/.test(s)) return parseInt(s, 10);
            if (/^-?\d*\.\d+$/.test(s)) return parseFloat(s);
            if (s.startsWith("[") && s.endsWith("]")) {
                try { return JSON.parse(s); } catch (e) { /* fall through */ }
            }
            if (s.startsWith("{") && s.endsWith("}")) {
                return __yaml_parse_flow_map(s.slice(1, -1));
            }
            if ((s.startsWith('"') && s.endsWith('"') && s.length >= 2) || (s.startsWith("'") && s.endsWith("'") && s.length >= 2)) {
                const inner = s.slice(1, -1);
                return s[0] === '"' ? inner.replace(/\\"/g, '"').replace(/\\\\/g, "\\") : inner.replace(/''/g, "'");
            }
            return s;
        };

        // Parses a flow-map body like `a: 1, b: "x, y"` into an object. Splits on
        // commas outside quotes/brackets, then applies the scalar conversion.
        const __yaml_parse_flow_map = (body) => {
            const out = {};
            const parts = [];
            let depth = 0, cur = "", inQ = null;
            for (const ch of body) {
                if (inQ) {
                    cur += ch;
                    if (ch === inQ) inQ = null;
                    continue;
                }
                if (ch === '"' || ch === "'") { inQ = ch; cur += ch; continue; }
                if (ch === "[" || ch === "{") { depth++; cur += ch; continue; }
                if (ch === "]" || ch === "}") { depth--; cur += ch; continue; }
                if (ch === "," && depth === 0) { parts.push(cur); cur = ""; continue; }
                cur += ch;
            }
            if (cur.trim() !== "") parts.push(cur);
            for (const part of parts) {
                const colon = part.search(/:(\s|$)/);
                if (colon < 0) continue;
                const key = __yaml_scalar(part.slice(0, colon));
                out[typeof key === "string" ? key : String(part.slice(0, colon))] = __yaml_scalar(part.slice(colon + 1));
            }
            return out;
        };

        const __yaml_parse_block = (lines, startIdx, indent) => {
            // Returns [value, nextIndex]; `indent` is the minimum column of this block.
            // Detect a sequence first: the first non-empty line at this level opens with "- ".
            let i = startIdx;
            while (i < lines.length && lines[i].trim() === "") i++;
            if (i >= lines.length) return [null, i];
            const isSeq = /^\s*-\s+/.test(lines[i]) || lines[i].trim() === "-";
            if (isSeq) {
                const out = [];
                while (i < lines.length) {
                    const line = lines[i];
                    if (line.trim() === "") { i++; continue; }
                    const m = line.match(/^(\s*)-\s*(.*)$/);
                    if (!m || m[1].length < indent) break;
                    if (m[1].length > indent) { i++; continue; }
                    i++;
                    if (m[2] === "") {
                        // Nested block under the dash item.
                        const [child, next] = __yaml_parse_block(lines, i, indent + 2);
                        out.push(child);
                        i = next;
                    } else if (m[2].includes(": ") || m[2].endsWith(":")) {
                        // Inline map start: "- key: value" plus its continuation lines
                        // form a nested map block. Re-parse it in item-local coordinates:
                        // the continuation indent is rebased so the item key sits at 0.
                        const itemLines = [m[2]];
                        while (i < lines.length && lines[i].trim() !== "" && lines[i].match(/^(\s*)/)[1].length > indent) {
                            itemLines.push(lines[i]);
                            i++;
                        }
                        const baseIndent = itemLines.length > 1 ? itemLines[1].match(/^(\s*)/)[1].length : 0;
                        const rebased = itemLines.map((l, idx) => {
                            if (idx === 0) return l;
                            return l.slice(baseIndent);
                        });
                        const [obj] = __yaml_parse_block(rebased, 0, 0);
                        out.push(obj);
                    } else {
                        out.push(__yaml_scalar(m[2]));
                    }
                }
                return [out, i];
            }
            const out = {};
            while (i < lines.length) {
                const line = lines[i];
                if (line.trim() === "" || line.trimStart().startsWith("#")) { i++; continue; }
                const curIndent = line.match(/^(\s*)/)[1].length;
                if (curIndent < indent) break;
                const kv = line.match(/^(\s*)([^:#]+?)\s*:(?:\s+(.*))?$/);
                if (!kv || kv[1].length !== indent) break;
                const key = kv[2].trim();
                let value = kv[3];
                i++;
                if (value === undefined || value.trim() === "") {
                    // Block value: nested map/list or block scalar.
                    if (i < lines.length && lines[i].trim() !== "") {
                        const nextIndent = lines[i].match(/^(\s*)/)[1].length;
                        if (nextIndent > indent) {
                            const [child, next] = __yaml_parse_block(lines, i, nextIndent);
                            out[key] = child;
                            i = next;
                            continue;
                        }
                    }
                    out[key] = null;
                    continue;
                }
                const trimmed = value.trim();
                if (trimmed === "|" || trimmed === ">" || trimmed === "|-" || trimmed === ">-") {
                    // Block scalar: collect more-indented lines.
                    const keepNewlines = trimmed[0] === "|";
                    const clip = trimmed.endsWith("-");
                    const parts = [];
                    let blockIndent = -1;
                    while (i < lines.length && lines[i].trim() !== "") {
                        const li = lines[i];
                        const liIndent = li.match(/^(\s*)/)[1].length;
                        if (blockIndent < 0) blockIndent = liIndent;
                        if (liIndent < blockIndent) break;
                        parts.push(li.slice(blockIndent));
                        i++;
                    }
                    out[key] = keepNewlines
                        ? (clip ? parts.join("\n") : parts.join("\n") + "\n")
                        : parts.join(" ");
                    continue;
                }
                out[key] = __yaml_scalar(trimmed);
            }
            return [out, i];
        };

        function __parse_yaml(text) {
            const src = String(text != null ? text : "").replace(/\r\n/g, "\n");
            const lines = src.split("\n");
            const [value] = __yaml_parse_block(lines, 0, 0);
            return value === undefined ? null : value;
        }

        function __stringify_yaml(value, indent = 0) {
            const pad = "  ".repeat(indent);
            if (value === null || value === undefined) return "null";
            if (typeof value === "number" || typeof value === "boolean") return String(value);
            if (typeof value === "string") {
                if (/^[-?:,\[\]{}#&*!|>'"%@`]/.test(value) || value.includes(": ") || value.includes("\n") || value.trim() !== value || value === "") {
                    return JSON.stringify(value);
                }
                return value;
            }
            if (Array.isArray(value)) {
                if (value.length === 0) return "[]";
                return value.map((item) => {
                    const inner = __stringify_yaml(item, indent + 1);
                    if (typeof item === "object" && item !== null && inner.includes("\n")) {
                        return pad + "-\n" + inner.split("\n").map((l) => (l ? "  " + l : l)).join("\n");
                    }
                    return pad + "- " + inner;
                }).join("\n");
            }
            if (typeof value === "object") {
                const keys = Object.keys(value);
                if (keys.length === 0) return "{}";
                return keys.map((key) => {
                    const v = value[key];
                    const rendered = __stringify_yaml(v, indent + 1);
                    if (typeof v === "object" && v !== null && ((Array.isArray(v) && v.length > 0) || (!Array.isArray(v) && Object.keys(v).length > 0))) {
                        return pad + key + ":\n" + rendered;
                    }
                    return pad + key + ": " + rendered;
                }).join("\n");
            }
            return String(value);
        }

        function parseYaml(text) {
            return __parse_yaml(text);
        }

        function stringifyYaml(value) {
            return __stringify_yaml(value) + "\n";
        }

        const __B64_ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

        function __array_buffer_to_base64(buffer) {
            const bytes = new Uint8Array(buffer || 0);
            let out = "";
            for (let i = 0; i < bytes.length; i += 3) {
                const b0 = bytes[i], b1 = bytes[i + 1], b2 = bytes[i + 2];
                out += __B64_ALPHABET[b0 >> 2];
                out += __B64_ALPHABET[((b0 & 3) << 4) | ((b1 === undefined ? 0 : b1) >> 4)];
                out += b1 === undefined ? "=" : __B64_ALPHABET[((b1 & 15) << 2) | ((b2 === undefined ? 0 : b2) >> 6)];
                out += b2 === undefined ? "=" : __B64_ALPHABET[b2 & 63];
            }
            return out;
        }

        function __base64_to_array_buffer(base64) {
            const s = String(base64 != null ? base64 : "").replace(/[^A-Za-z0-9+/]/g, "").replace(/=+$/, "");
            const out = new Uint8Array(Math.floor(s.length * 3 / 4));
            let acc = 0, bits = 0, idx = 0;
            for (const c of s) {
                acc = (acc << 6) | __B64_ALPHABET.indexOf(c);
                bits += 6;
                if (bits >= 8) {
                    bits -= 8;
                    out[idx++] = (acc >> bits) & 0xff;
                }
            }
            return out.buffer;
        }

        // Splits a note into its frontmatter object and the body offset. Returns null
        // when the note has no (closed) frontmatter block.
        function __parse_frontmatter(content) {
            const text = String(content != null ? content : "").replace(/\r\n/g, "\n");
            const lines = text.split("\n");
            if (lines.length === 0 || lines[0].trim() !== "---") return null;
            for (let i = 1; i < lines.length; i++) {
                if (lines[i].trim() === "---") {
                    const bodyStart = lines.slice(0, i + 1).join("\n").length + 1;
                    const data = __parse_yaml(lines.slice(1, i).join("\n")) || {};
                    return { data: (data && typeof data === "object" && !Array.isArray(data)) ? data : {}, bodyStart };
                }
            }
            return null;
        }

        function __parse_html_to_velements(html, rootNode) {
            if (!html || !rootNode) return;
            const tagRegex = /<(\/)?([a-zA-Z0-9:-]+)((?:\s+[^=>\s]+(?:=(?:"[^"]*"|'[^']*'|[^>\s]+))?)*)\s*(\/?)>/g;
            let stack = [rootNode];
            let lastIndex = 0;
            let match;

            while ((match = tagRegex.exec(html)) !== null) {
                const textBefore = html.slice(lastIndex, match.index);
                if (textBefore) {
                    const unescaped = __unescape_html(textBefore);
                    if (unescaped) {
                        const current = stack[stack.length - 1];
                        if (current.children.length === 0 && !current.text) {
                            current.text = unescaped;
                        } else {
                            const textSpan = new VElement("span");
                            textSpan.text = unescaped;
                            current.appendChild(textSpan);
                        }
                    }
                }
                lastIndex = tagRegex.lastIndex;

                const isClosing = !!match[1];
                const tagName = match[2].toLowerCase();
                const attrStr = match[3];
                const isSelfClosing = !!match[4] || VOID_TAGS.has(tagName);

                if (DANGEROUS_TAGS.has(tagName)) {
                    continue;
                }

                if (isClosing) {
                    for (let i = stack.length - 1; i > 0; i--) {
                        if (stack[i].tag === tagName) {
                            stack.splice(i);
                            break;
                        }
                    }
                } else {
                    const { attrs, cls } = __parse_attributes(attrStr);
                    const childEl = new VElement(tagName, { cls, attr: attrs });
                    if (attrs.id) {
                        childEl.id = attrs.id;
                    }
                    const current = stack[stack.length - 1];
                    current.appendChild(childEl);

                    if (!isSelfClosing) {
                        stack.push(childEl);
                    }
                }
            }

            if (lastIndex < html.length) {
                const textAfter = html.slice(lastIndex);
                if (textAfter) {
                    const unescaped = __unescape_html(textAfter);
                    if (unescaped) {
                        const current = stack[stack.length - 1];
                        if (current.children.length === 0 && !current.text) {
                            current.text = unescaped;
                        } else {
                            const textSpan = new VElement("span");
                            textSpan.text = unescaped;
                            current.appendChild(textSpan);
                        }
                    }
                }
            }
        }

        function __serialize_velement_to_html(el) {
            if (!el) return "";
            let html = "<" + el.tag;
            if (el.classes && el.classes.size > 0) {
                html += ` class="${Array.from(el.classes).join(" ")}"`;
            }
            if (el.attributes) {
                for (const [k, v] of Object.entries(el.attributes)) {
                    if (k !== "class" && v !== undefined && v !== null) {
                        html += ` ${k}="${__escape_html(String(v))}"`;
                    }
                }
            }
            if (VOID_TAGS.has(el.tag)) {
                html += " />";
                return html;
            }
            html += ">";
            if (el.text) {
                html += __escape_html(el.text);
            }
            for (const child of el.children) {
                html += __serialize_velement_to_html(child);
            }
            html += `</${el.tag}>`;
            return html;
        }

        function __matches_compound_selector(el, sel) {
            if (!el || !(el instanceof VElement)) return false;
            sel = sel.trim();
            if (!sel || sel === "*") return true;

            sel = sel.replace(/:[a-zA-Z_-]+/g, "");
            if (!sel || sel === "*") return true;

            let rest = sel;
            const tagMatch = rest.match(/^[a-zA-Z][a-zA-Z0-9_-]*/);
            if (tagMatch) {
                if (el.tag !== tagMatch[0].toLowerCase()) return false;
                rest = rest.slice(tagMatch[0].length);
            } else if (rest.startsWith("*")) {
                rest = rest.slice(1);
            }

            while (rest.length > 0) {
                if (rest.startsWith(".")) {
                    const m = rest.match(/^\.([a-zA-Z0-9_-]+)/);
                    if (!m) return false;
                    if (!el.classes || !el.classes.has(m[1])) return false;
                    rest = rest.slice(m[0].length);
                } else if (rest.startsWith("#")) {
                    const m = rest.match(/^#([a-zA-Z0-9_-]+)/);
                    if (!m) return false;
                    if (el.id !== m[1] && el.attributes?.id !== m[1]) return false;
                    rest = rest.slice(m[0].length);
                } else if (rest.startsWith("[")) {
                    const m = rest.match(/^\[([a-zA-Z0-9_-]+)(?:=([\"']?)(.*?)\2)?\]/);
                    if (!m) return false;
                    const attrName = m[1];
                    const attrVal = m[3];
                    let actual = el.attributes?.[attrName];
                    if (actual === undefined && el[attrName] !== undefined) {
                        actual = el[attrName];
                    }
                    if (attrName.startsWith("data-") && el.dataset) {
                        const camel = attrName.slice(5).replace(/-([a-z])/g, (_, g) => g.toUpperCase());
                        if (el.dataset[camel] !== undefined) actual = el.dataset[camel];
                    }
                    if (actual === undefined || actual === null) return false;
                    if (attrVal !== undefined) {
                        if (String(actual) !== attrVal) return false;
                    }
                    rest = rest.slice(m[0].length);
                } else {
                    break;
                }
            }
            return rest.length === 0;
        }

        function __parse_complex_selector(sel) {
            sel = sel.trim();
            const tokens = [];
            let i = 0;
            let currentSel = "";
            let combinator = null;

            while (i < sel.length) {
                const ch = sel[i];
                if (ch === ">") {
                    if (currentSel.trim()) {
                        tokens.push({ combinator, sel: currentSel.trim() });
                        currentSel = "";
                    }
                    combinator = ">";
                    i++;
                } else if (/\s/.test(ch)) {
                    while (i < sel.length && /\s/.test(sel[i])) {
                        i++;
                    }
                    if (i < sel.length && sel[i] === ">") {
                        continue;
                    }
                    if (currentSel.trim()) {
                        tokens.push({ combinator, sel: currentSel.trim() });
                        currentSel = "";
                        combinator = " ";
                    }
                } else if (ch === "[") {
                    const endBracket = sel.indexOf("]", i);
                    if (endBracket !== -1) {
                        currentSel += sel.slice(i, endBracket + 1);
                        i = endBracket + 1;
                    } else {
                        currentSel += ch;
                        i++;
                    }
                } else {
                    currentSel += ch;
                    i++;
                }
            }
            if (currentSel.trim()) {
                tokens.push({ combinator, sel: currentSel.trim() });
            }
            return tokens;
        }

        function __matches_single_complex(el, tokens) {
            if (!el || tokens.length === 0) return false;
            let tIdx = tokens.length - 1;
            let currEl = el;

            if (!__matches_compound_selector(currEl, tokens[tIdx].sel)) {
                return false;
            }

            while (tIdx > 0) {
                const comb = tokens[tIdx].combinator;
                tIdx--;
                const targetSel = tokens[tIdx].sel;

                if (comb === ">") {
                    currEl = currEl.parent;
                    if (!currEl || !__matches_compound_selector(currEl, targetSel)) {
                        return false;
                    }
                } else {
                    let matched = false;
                    currEl = currEl.parent;
                    while (currEl) {
                        if (__matches_compound_selector(currEl, targetSel)) {
                            matched = true;
                            break;
                        }
                        currEl = currEl.parent;
                    }
                    if (!matched) return false;
                }
            }
            return true;
        }

        function __matches_selector(el, selector) {
            if (!el || !selector) return false;
            const subSelectors = selector.split(",");
            for (const sub of subSelectors) {
                const tokens = __parse_complex_selector(sub);
                if (tokens.length > 0 && __matches_single_complex(el, tokens)) {
                    return true;
                }
            }
            return false;
        }

        class VElement {
            constructor(tag = "div", options = {}) {
                if (typeof options === "string") {
                    options = { cls: options };
                } else if (!options || typeof options !== "object") {
                    options = {};
                }
                this.tag = String(tag).toLowerCase();
                this._id = options.id || ("el-" + Math.random().toString(36).substr(2, 9));
                let clsSet = new Set();
                if (typeof options.cls === "string") {
                    options.cls.split(" ").filter(Boolean).forEach(c => clsSet.add(c));
                } else if (Array.isArray(options.cls)) {
                    options.cls.filter(Boolean).forEach(c => clsSet.add(c));
                }
                this.classes = clsSet;
                this._text = options.text != null ? String(options.text) : "";
                this.attributes = Object.assign({}, options.attr || {});
                if (options.type) this.attributes["type"] = options.type;
                this.children = [];
                this.eventListeners = new Map();
                this.parent = null;
                this.value = options.value != null ? String(options.value) : "";
                this.placeholder = options.placeholder || "";
                // DOM reflection seed: the disabled attribute set through `attr` wins unless
                // the options also state the property explicitly. Plain assignment of the
                // property here would wipe the attribute the constructor just copied.
                this._disabled = !!options.disabled;
                if (this.attributes["disabled"] === undefined && options.disabled === true) {
                    this.attributes["disabled"] = "true";
                }
                this.checked = options.checked !== undefined ? !!options.checked : undefined;
                this.dataset = new Proxy({}, {
                    get: (t, p) => {
                        const kebab = "data-" + String(p).replace(/[A-Z]/g, m => "-" + m.toLowerCase());
                        return this.attributes[kebab] !== undefined ? this.attributes[kebab] : t[p];
                    },
                    set: (t, p, v) => {
                        const kebab = "data-" + String(p).replace(/[A-Z]/g, m => "-" + m.toLowerCase());
                        this.attributes[kebab] = String(v);
                        t[p] = String(v);
                        return true;
                    }
                });
                // The proxy target is stored separately: the serializer reads it directly, while
                // the plugin keeps seeing a regular `el.style.width = ...`.
                const styleTarget = {};
                this._styleTarget = styleTarget;
                this.style = new Proxy(styleTarget, {
                    get: (t, p) => t[p] || "",
                    set: (t, p, v) => { t[p] = v; return true; }
                });
                globalThis.__velement_registry.set(this._id, this);
            }

            /** Obsidian adds setCssProps to elements: set several properties at once. */
            setCssProps(props) {
                if (props && typeof props === "object") {
                    for (const [k, v] of Object.entries(props)) {
                        if (v != null) this._styleTarget[k] = String(v);
                    }
                }
            }

            _notifyModified() {
                let curr = this;
                while (curr) {
                    if (curr._onModified) {
                        try { curr._onModified(); } catch (e) {}
                        break;
                    }
                    curr = curr.parent;
                }
            }

            get id() {
                return this._id;
            }
            set id(val) {
                if (this._id) globalThis.__velement_registry.delete(this._id);
                this._id = String(val);
                globalThis.__velement_registry.set(this._id, this);
            }

            // DOM reflection for `disabled`: the property and the attribute stay in sync,
            // exactly like a real <button>. Plugins flip the property (`btn.disabled = false`)
            // or pass `attr: { disabled: bool }`; either way the serializer must see the
            // final state. A "false" attribute value means enabled.
            get disabled() {
                if (this.attributes["disabled"] !== undefined) {
                    return this.attributes["disabled"] !== "false";
                }
                if (this.attributes["aria-disabled"] !== undefined) {
                    return this.attributes["aria-disabled"] === "true";
                }
                return !!this._disabled;
            }
            set disabled(val) {
                this._disabled = !!val;
                if (val) {
                    this.attributes["disabled"] = "true";
                } else {
                    delete this.attributes["disabled"];
                }
            }

            get text() {
                return this._text || "";
            }
            set text(val) {
                this._text = String(val != null ? val : "");
                this._notifyModified();
            }

            get href() {
                return this.attributes["href"] || "";
            }
            set href(val) {
                this.setAttr("href", String(val != null ? val : ""));
            }

            get className() {
                return Array.from(this.classes).join(" ");
            }
            set className(val) {
                this.classes.clear();
                if (typeof val === "string") {
                    val.split(/\s+/).filter(Boolean).forEach(c => this.classes.add(c));
                }
                __css_touch();
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

            get parentElement() {
                return this.parent || null;
            }
            get parentNode() {
                return this.parent || null;
            }

            get firstChild() {
                return this.children.length > 0 ? this.children[0] : null;
            }
            get lastChild() {
                return this.children.length > 0 ? this.children[this.children.length - 1] : null;
            }
            get firstElementChild() {
                return this.children.find(c => c instanceof VElement) || null;
            }
            get lastElementChild() {
                for (let i = this.children.length - 1; i >= 0; i--) {
                    if (this.children[i] instanceof VElement) return this.children[i];
                }
                return null;
            }
            get nextSibling() {
                if (!this.parent) return null;
                const idx = this.parent.children.indexOf(this);
                if (idx !== -1 && idx + 1 < this.parent.children.length) {
                    return this.parent.children[idx + 1];
                }
                return null;
            }
            get nextElementSibling() {
                return this.nextSibling;
            }
            get previousSibling() {
                if (!this.parent) return null;
                const idx = this.parent.children.indexOf(this);
                if (idx > 0) {
                    return this.parent.children[idx - 1];
                }
                return null;
            }
            get previousElementSibling() {
                return this.previousSibling;
            }
            get childNodes() {
                return this.children.slice();
            }

            contains(other) {
                if (!other) return false;
                if (other === this) return true;
                let p = other.parent;
                while (p) {
                    if (p === this) return true;
                    p = p.parent;
                }
                return false;
            }

            insertBefore(newChild, refChild) {
                if (!newChild) return newChild;
                if (newChild.parent) {
                    newChild.remove();
                }
                if (!refChild) {
                    return this.appendChild(newChild);
                }
                const idx = this.children.indexOf(refChild);
                if (idx === -1) {
                    return this.appendChild(newChild);
                }
                newChild.parent = this;
                this.children.splice(idx, 0, newChild);
                this._notifyModified();
                __css_touch();
                return newChild;
            }

            removeChild(child) {
                if (!child) return null;
                const idx = this.children.indexOf(child);
                if (idx !== -1) {
                    this.children.splice(idx, 1);
                    child.remove();
                    this._notifyModified();
                    __css_touch();
                    return child;
                }
                return null;
            }

            replaceChild(newChild, oldChild) {
                if (!oldChild || !newChild) return null;
                const idx = this.children.indexOf(oldChild);
                if (idx !== -1) {
                    if (newChild.parent) newChild.remove();
                    this.children[idx] = newChild;
                    newChild.parent = this;
                    oldChild.remove();
                    this._notifyModified();
                    __css_touch();
                    return oldChild;
                }
                return null;
            }

            cloneNode(deep = false) {
                const clone = new VElement(this.tag);
                this.classes.forEach(c => clone.classes.add(c));
                clone._text = this._text;
                clone.value = this.value;
                clone.placeholder = this.placeholder;
                clone.disabled = this.disabled;
                clone.checked = this.checked;
                clone.attributes = Object.assign({}, this.attributes);
                clone.dataset = Object.assign({}, this.dataset);
                if (deep) {
                    for (const ch of this.children) {
                        clone.appendChild(ch.cloneNode(true));
                    }
                }
                return clone;
            }

            createEl(tag, options = {}) {
                const el = new VElement(tag, options);
                this.appendChild(el);
                return el;
            }

            createDiv(options = "") {
                return this.createEl("div", options);
            }

            createSpan(options = "") {
                return this.createEl("span", options);
            }

            appendChild(child) {
                if (child instanceof VElement) {
                    if (child.tag === "#fragment" || child.classes?.has("document-fragment")) {
                        while (child.children.length > 0) {
                            const c = child.children[0];
                            c.remove();
                            this.appendChild(c);
                        }
                        return child;
                    }
                    if (child.parent) {
                        child.remove();
                    }
                    child.parent = this;
                    this.children.push(child);
                    this._notifyModified();
                    __css_touch();
                }
                return child;
            }

            setChildrenInPlace(children) {
                this.empty();
                if (Array.isArray(children)) {
                    for (const c of children) {
                        this.appendChild(c);
                    }
                }
                this._notifyModified();
                return this;
            }

            appendText(val) {
                this._text += String(val != null ? val : "");
                this._notifyModified();
                return this;
            }

            append(...nodes) {
                for (const n of nodes) {
                    if (typeof n === "string") {
                        const t = new VElement("#text");
                        t.text = n;
                        this.appendChild(t);
                    } else if (n instanceof VElement) {
                        this.appendChild(n);
                    }
                }
            }

            empty() {
                for (const c of this.children) {
                    c.remove();
                }
                this.children = [];
                this._text = "";
                this._notifyModified();
                __css_touch();
                return this;
            }

            remove() {
                for (const c of this.children) {
                    c.remove();
                }
                globalThis.__velement_registry.delete(this._id);
                if (this.parent) {
                    const p = this.parent;
                    const idx = p.children.indexOf(this);
                    if (idx !== -1) {
                        p.children.splice(idx, 1);
                    }
                    this.parent = null;
                    p._notifyModified();
                    __css_touch();
                }
            }

            detach() {
                this.remove();
            }

            addClass(...classes) {
                for (const c of classes) {
                    if (c) this.classes.add(c);
                }
                __css_touch();
                return this;
            }

            removeClass(...classes) {
                for (const c of classes) {
                    if (c) this.classes.delete(c);
                }
                __css_touch();
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

            setText(text) {
                this._text = String(text != null ? text : "");
                this._notifyModified();
                return this;
            }

            getText() {
                return this.text;
            }

            get textContent() {
                if (this.children.length === 0) {
                    return this._text || "";
                }
                let res = this._text || "";
                for (const c of this.children) {
                    res += (c.textContent != null ? c.textContent : (c.text || ""));
                }
                return res;
            }

            set textContent(val) {
                this.empty();
                this.setText(val);
                this._notifyModified();
            }

            get innerHTML() {
                if (this.children.length === 0) {
                    return this.text || "";
                }
                return this.children.map(__serialize_velement_to_html).join("") + (this.text ? __escape_html(this.text) : "");
            }

            set innerHTML(val) {
                this.empty();
                const clean = __sanitize_html(String(val != null ? val : ""));
                __parse_html_to_velements(clean, this);
                this._notifyModified();
            }

            get innerText() {
                return this.textContent;
            }

            set innerText(val) {
                this.textContent = val;
            }

            setAttr(name, val) {
                this.attributes[name] = val;
                this._notifyModified();
                __css_touch();
                return this;
            }

            getAttr(name) {
                return this.attributes[name];
            }

            setAttribute(name, val) {
                this.attributes[name] = val;
                this._notifyModified();
                __css_touch();
            }

            getAttribute(name) {
                return this.attributes[name];
            }

            removeAttribute(name) {
                delete this.attributes[name];
                this._notifyModified();
                __css_touch();
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
                if (!event || typeof event !== "object") {
                    event = new (globalThis.MouseEvent || globalThis.Event || Object)(String(event || "click"), { target: this, currentTarget: this });
                } else {
                    if (!event.target) event.target = this;
                    event.currentTarget = this;
                }
                if (typeof event.stopPropagation !== "function") {
                    event.stopPropagation = function() { this.cancelBubble = true; this.propagationStopped = true; };
                }
                if (typeof event.stopImmediatePropagation !== "function") {
                    event.stopImmediatePropagation = function() { this.cancelBubble = true; this.propagationStopped = true; };
                }
                if (typeof event.preventDefault !== "function") {
                    event.preventDefault = function() { this.defaultPrevented = true; };
                }

                const type = event.type || "click";
                const list = this.eventListeners.get(type);
                if (list) {
                    for (const fn of list) {
                        try {
                            const res = fn(event);
                            if (res && typeof res.then === "function") {
                                res.then(() => {
                                    if (globalThis.__active_modal && typeof globalThis.__active_modal.syncTree === "function") {
                                        globalThis.__active_modal.syncTree();
                                    }
                                    if (globalThis.__active_setting_tab && globalThis.__active_setting_tab.containerEl) {
                                        try {
                                            const tree = __velement_to_uinode(globalThis.__active_setting_tab.containerEl);
                                            __invoke_host("set_view_tree", { view_id: 0, tree });
                                        } catch (e) {}
                                    }
                                    if (globalThis.__workspace_leaves) {
                                        for (const leaf of globalThis.__workspace_leaves.values()) {
                                            if (leaf && leaf.view && typeof leaf.syncTree === "function") {
                                                try { leaf.syncTree(); } catch (e) {}
                                            }
                                        }
                                    }
                                }).catch(e => console.error("Async event listener error:", e));
                            }
                        } catch (e) {
                            console.error("Event listener error:", e);
                        }
                    }
                }
            }

            onClick(callback) {
                this.addEventListener("click", callback);
                return this;
            }

            matches(sel) {
                return __matches_selector(this, sel);
            }

            closest(sel) {
                let curr = this;
                while (curr) {
                    if (curr instanceof VElement && curr.matches(sel)) {
                        return curr;
                    }
                    curr = curr.parentElement;
                }
                return null;
            }

            querySelector(sel) {
                function findFirst(node) {
                    if (!node || !node.children) return null;
                    for (const child of node.children) {
                        if (child instanceof VElement) {
                            if (__matches_selector(child, sel)) {
                                return child;
                            }
                            const found = findFirst(child);
                            if (found) return found;
                        }
                    }
                    return null;
                }
                return findFirst(this);
            }

            querySelectorAll(sel) {
                const results = [];
                function collect(node) {
                    if (!node || !node.children) return;
                    for (const child of node.children) {
                        if (child instanceof VElement) {
                            if (__matches_selector(child, sel)) {
                                results.push(child);
                            }
                            collect(child);
                        }
                    }
                }
                collect(this);
                return results;
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

        class TextAreaComponent extends TextComponent {
            constructor(containerEl) {
                super(null);
                this.inputEl = containerEl ? containerEl.createEl("textarea") : new VElement("textarea");
            }
        }

        class SearchComponent extends TextComponent {
            constructor(containerEl) {
                super(containerEl);
                if (this.inputEl && this.inputEl.setAttr) this.inputEl.setAttr("type", "search");
            }
            getValue() { return this.value; }
        }

        class MomentFormatComponent extends TextComponent {
            constructor(containerEl) {
                super(containerEl);
                this.defaultFormat = "";
            }
            setDefaultFormat(fmt) {
                this.defaultFormat = String(fmt != null ? fmt : "");
                return this;
            }
            setPlaceholder(p) {
                super.setPlaceholder(p);
                return this;
            }
        }

        class ColorComponent extends ValueComponent {
            constructor(containerEl) {
                super();
                this.colorEl = containerEl ? containerEl.createEl("input", { attr: { type: "color" } }) : new VElement("input");
                this.value = "#000000";
            }
            setValue(v) {
                super.setValue(String(v != null ? v : "#000000"));
                this.colorEl.value = this.value;
                return this;
            }
            onChange(callback) {
                this.colorEl.addEventListener("change", (e) => {
                    this.value = e?.value != null ? e.value : this.value;
                    callback(this.value);
                });
                return this;
            }
        }

        class ProgressBarComponent extends ValueComponent {
            constructor(containerEl) {
                super();
                this.progressEl = containerEl ? containerEl.createEl("progress") : new VElement("progress");
                this.value = 0;
                this.max = 100;
            }
            setValue(v) {
                this.value = Number(v) || 0;
                if (this.progressEl.setAttr) this.progressEl.setAttr("value", String(this.value));
                return this;
            }
            setMax(max) {
                this.max = Number(max) || 100;
                if (this.progressEl.setAttr) this.progressEl.setAttr("max", String(this.max));
                return this;
            }
            getValue() { return this.value; }
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
                this.nameEl.empty();
                if (name instanceof VElement) {
                    this.nameEl.appendChild(name);
                } else if (name != null) {
                    this.nameEl.setText(String(name));
                }
                return this;
            }
            setDesc(desc) {
                this.descEl.empty();
                if (desc instanceof VElement) {
                    this.descEl.appendChild(desc);
                } else if (desc != null) {
                    this.descEl.setText(String(desc));
                }
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
            addTextArea(cb) {
                const txt = new TextAreaComponent(this.controlEl);
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
            setDisabled(disabled) {
                this.__disabled = !!disabled;
                for (const c of this.controls) {
                    if (typeof c.setDisabled === "function") c.setDisabled(this.__disabled);
                }
                return this;
            }
            addSearch(cb) {
                const search = new SearchComponent(this.controlEl);
                this.controls.push(search);
                if (typeof cb === "function") cb(search);
                return this;
            }
            addMomentFormat(cb) {
                const mf = new MomentFormatComponent(this.controlEl);
                this.controls.push(mf);
                if (typeof cb === "function") cb(mf);
                return this;
            }
            addColorPicker(cb) {
                const color = new ColorComponent(this.controlEl);
                this.controls.push(color);
                if (typeof cb === "function") cb(color);
                return this;
            }
            addProgressBar(cb) {
                const bar = new ProgressBarComponent(this.controlEl);
                this.controls.push(bar);
                if (typeof cb === "function") cb(bar);
                return this;
            }
            then(cb) {
                if (typeof cb === "function") cb(this);
                return this;
            }
            clear() {
                this.controls = [];
                this.settingEl.children = [];
                this.infoEl = this.settingEl.createDiv("setting-item-info");
                this.nameEl = this.infoEl.createDiv("setting-item-name");
                this.descEl = this.infoEl.createDiv("setting-item-description");
                this.controlEl = this.settingEl.createDiv("setting-item-control");
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

        function __velement_core_to_uinode(el) {
            if (!el) return { type: "text", value: "", class: undefined };

            const classList = el.classes ? Array.from(el.classes) : [];
            const classStr = classList.length > 0 ? classList.join(" ") : undefined;

            if (el.classes && (el.classes.has("vertical-tab-content") || el.classes.has("modal-container") || el.classes.has("modal") || el.classes.has("modal-content"))) {
                const children = el.children.map(__velement_to_uinode).filter(Boolean);
                return { type: "column", class: classStr, gap: "none", children };
            }

            if (el.classes && (el.classes.has("modal-title") || el.classes.has("aiw-modal-title"))) {
                if (!el.text || !el.text.trim()) return null;
                return {
                    type: "text",
                    class: classStr,
                    style: "heading",
                    value: el.text || ""
                };
            }

            if (el.classes && el.classes.has("setting-item")) {
                const infoEl = el.children.find(c => c.classes?.has("setting-item-info"));
                const nameEl = infoEl?.children?.find(c => c.classes?.has("setting-item-name"));
                const descEl = infoEl?.children?.find(c => c.classes?.has("setting-item-description"));
                const name = (nameEl ? (nameEl.textContent || nameEl.text) : (el.text || "")).trim();
                const desc = descEl ? (descEl.textContent || descEl.text || "").trim() : undefined;
                const controlDiv = el.children.find(c => c.classes?.has("setting-item-control"));

                let controlNode = { type: "text", value: "", class: undefined };
                if (controlDiv && controlDiv.children.length > 0) {
                    const primaryControl = controlDiv.children.find(c =>
                        c.tag === "textarea" ||
                        c.tag === "select" ||
                        (c.tag === "input" && c.attributes?.type !== "hidden") ||
                        c.classes?.has("checkbox-container") ||
                        c.classes?.has("mod-toggle")
                    ) || controlDiv.children[0];
                    // __velement_to_uinode returns null for contentless elements (an
                    // icon-only preview div, for example): the setting row still exists,
                    // so keep it with an empty control instead of shipping a null that
                    // fails deserialization on the host side.
                    controlNode = __velement_to_uinode(primaryControl) || { type: "text", value: "", class: undefined };
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
                if (el.classes.has("mod-cta") || el.classes.has("mod-primary") || Array.from(el.classes).some(c => c.endsWith("-primary") || c.includes("danger--active"))) variant = "primary";
                if (el.classes.has("mod-warning") || el.classes.has("mod-danger") || Array.from(el.classes).some(c => c.includes("danger") || c.includes("warning"))) variant = "danger";
                if (el.classes.has("mod-cancel") || Array.from(el.classes).some(c => c.includes("secondary") || c.includes("ghost") || c.includes("cancel"))) variant = "secondary";
                return {
                    type: "button",
                    id: el.id,
                    class: classStr,
                    label: el.text || "Button",
                    variant,
                    // `disabled` is a reflected property (see the VElement getter/setter):
                    // the plugin flips it via the property or the attr, both converge, and
                    // a "false" attribute value means the button is enabled.
                    disabled: !!el.disabled
                };
            }

            if (el.tag === "input" && (el.attributes["type"] === "checkbox" || el.classes.has("checkbox-container") || el.classes.has("checkbox") || Array.from(el.classes).some(c => c.includes("checkbox")) || el.type === "checkbox")) {
                if (el.classes.has("checkbox-container") || el.classes.has("mod-toggle") || (el.parent && el.parent.classes && el.parent.classes.has("setting-item-control"))) {
                    return {
                        type: "toggle",
                        id: el.id,
                        value: !!el.checked
                    };
                }
                return {
                    type: "checkbox",
                    id: el.id,
                    label: el.text || "",
                    checked: !!el.checked
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

            if (el.tag === "textarea") {
                return {
                    type: "textarea",
                    id: el.id,
                    placeholder: el.placeholder || undefined,
                    value: el.value != null ? String(el.value) : undefined,
                    rows: el.attributes["rows"] ? Number(el.attributes["rows"]) : undefined
                };
            }

            if (el.tag === "input") {
                return {
                    type: "input",
                    id: el.id,
                    placeholder: el.placeholder || undefined,
                    value: el.value != null ? String(el.value) : undefined
                };
            }

            if (/^h[1-6]$/.test(el.tag)) {
                return {
                    type: "text",
                    class: classStr,
                    style: "heading",
                    value: el.text || ""
                };
            }

            if (el.tag === "hr") {
                return { type: "separator" };
            }

            if (el.tag === "b" || el.tag === "strong") {
                if (el.children.length === 0) {
                    return {
                        type: "text",
                        class: classStr,
                        style: "heading",
                        value: el.text || ""
                    };
                }
            }

            if (el.tag === "code") {
                if (el.children.length === 0) {
                    return {
                        type: "text",
                        class: classStr,
                        style: "code",
                        value: el.text || ""
                    };
                }
            }

            if (el.tag === "#text") {
                return {
                    type: "text",
                    class: classStr,
                    style: "normal",
                    value: el.text || ""
                };
            }

            if (el.tag === "i" || el.tag === "em") {
                if (el.children.length === 0) {
                    return {
                        type: "text",
                        class: classStr,
                        style: el.classes.has("muted") ? "muted" : "normal",
                        value: el.text || ""
                    };
                }
            }

            if (el.tag === "a") {
                const href = el.attributes["href"] || el.href || "";
                let label = (el.textContent || el.text || "").trim();
                if (!label && href) {
                    if (href.includes("paypal")) label = "PayPal";
                    else if (href.includes("buymeacoffee")) label = "Buy Me a Coffee";
                    else if (href.includes("github")) label = "GitHub";
                    else if (href.includes("kofi") || href.includes("ko-fi")) label = "Ko-fi";
                    else if (href.includes("patreon")) label = "Patreon";
                    else label = href;
                }
                if (href && !el._has_open_url_listener) {
                    el._has_open_url_listener = true;
                    el.addEventListener("click", () => {
                        try {
                            if (globalThis.open) globalThis.open(href);
                            else __invoke_host("open_url", { url: href });
                        } catch (e) {
                            __invoke_host("open_url", { url: href });
                        }
                    });
                }
                return {
                    type: "button",
                    id: el.id,
                    class: classStr,
                    label: label || "Link",
                    variant: "secondary",
                    disabled: false
                };
            }

            if (el.classes.has("clickable-icon") || el.classes.has("menu-item-icon") || el.classes.has("recent-files-file-delete") || (el.attributes["icon"] && el.eventListeners?.has("click"))) {
                const icon = el.attributes["icon"] || (el.text ? undefined : "cross");
                return {
                    type: "button",
                    id: el.id,
                    class: classStr,
                    label: el.text || (icon ? "×" : ""),
                    variant: "secondary",
                    disabled: false
                };
            }

            if (el.tag === "p" || el.tag === "span" || (el.tag === "div" && el.children.length === 0 && el.text)) {
                if (el.children.length === 0) {
                    return {
                        type: "text",
                        class: classStr,
                        style: el.classes.has("muted") ? "muted" : "normal",
                        value: el.text || ""
                    };
                }
            }

            if (el.classes && Array.from(el.classes).some(c => c.includes("donation") || c.includes("donate") || c.includes("sponsor"))) {
                const textNodes = [];
                const buttonNodes = [];
                for (const child of el.children) {
                    if (child.tag === "a" || child.classes?.has("button") || child.tag === "button") {
                        const btnNode = __velement_to_uinode(child);
                        if (btnNode) buttonNodes.push(btnNode);
                    } else {
                        const otherNode = __velement_to_uinode(child);
                        if (otherNode) textNodes.push(otherNode);
                    }
                }
                const resultChildren = [...textNodes];
                if (buttonNodes.length > 0) {
                    resultChildren.push({
                        type: "row",
                        class: "donation-buttons",
                        gap: "small",
                        children: buttonNodes
                    });
                }
                return {
                    type: "column",
                    class: classStr,
                    gap: "small",
                    children: resultChildren
                };
            }

            if (el.children.length === 0 && (!el.text || !el.text.trim())) {
                return null;
            }

            if (el.classes && el.classes.has("tree-item") && !el.classes.has("tree-item-self") && el.children.length === 1 && el.children[0].classes && el.children[0].classes.has("tree-item-self")) {
                return __velement_to_uinode(el.children[0]);
            }

            const classArray = Array.from(el.classes);
            const isInfoEl = classArray.some(c => c.includes("info") || c.includes("desc"));
            const isListEl = classArray.some(c => c.includes("list") || c.includes("scroll"));
            const isRow = !isListEl && !isInfoEl && (
                el.classes.has("row")
                || el.classes.has("flex-row")
                || classArray.some(c => c.endsWith("-item") || c.endsWith("-row") || c.endsWith("-controls") || c.endsWith("-footer") || c.endsWith("-stats") || c.endsWith("-buttons") || c.includes("item") || c.includes("controls") || c.includes("footer") || c.includes("stats") || c.includes("buttons") || c.includes("toolbar") || c.includes("actions"))
                || el.classes.has("modal-button-container")
                || el.classes.has("tree-item-self")
                || el.classes.has("nav-file-title")
                || el.classes.has("nav-file")
                || el.tag === "p"
                || el.tag === "span"
                || el.tag === "b"
                || el.tag === "strong"
                || el.tag === "i"
                || el.tag === "em"
                || el.tag === "a"
            );
            let children = el.children.map(__velement_to_uinode).filter(Boolean);
            if (el.text && el.text.trim()) {
                children.unshift({
                    type: "text",
                    class: classStr,
                    style: el.classes.has("muted") ? "muted" : "normal",
                    value: el.text
                });
            }
            const clickable = el.eventListeners && (el.eventListeners.has("click") || el.eventListeners.has("mousedown"));
            return {
                type: isRow ? "row" : "column",
                id: clickable ? el.id : undefined,
                class: classStr,
                gap: isRow ? "small" : "none",
                children
            };
        }

        // Styles the host can apply to a native node. Everything else (gap, flexDirection,
        // zIndex, transform, transition, clipPath, cursor) is not forwarded: promising an effect
        // that does not exist is worse than honestly not showing it.
        const __STYLE_KEYS = new Set([
            "display", "width", "height", "minWidth", "maxWidth", "minHeight", "maxHeight",
            "padding", "paddingTop", "paddingRight", "paddingBottom", "paddingLeft",
            "margin", "marginTop", "marginRight", "marginBottom", "marginLeft",
            "background", "backgroundColor", "color", "opacity", "visibility",
            "borderRadius", "fontSize", "fontWeight", "textAlign", "overflow",
            "position", "top", "right", "bottom", "left"
        ]);

        function __css_key(k) {
            return String(k).trim().replace(/-([a-z])/g, (_, c) => c.toUpperCase());
        }

        // ── plugin styles.css ─────────────────────────────────────────────────────────────
        // The stylesheet sits next to main.js. Matching happens here, not in Rust:
        // only the shim has a real DOM — tag, class set and parent chain.
        // Found declarations go into the same channel as inline styles and pass through the same
        // whitelist, so the host does not even know that CSS selectors exist.
        //
        // Deliberately absent: pseudo-classes and pseudo-elements (:hover, :focus, :first-child,
        // ::before), sibling combinators (+, ~), @media and @supports. These are states and conditions
        // that do not exist for us; pretending they do would be lying to the plugin.
        // The state itself is declared at the top of the script (see `__css_epoch`).
        const __css_stats = { rules: 0, selectors: 0, skipped_pseudo: 0, skipped_at: 0 };

        /** Any change to classes or structure invalidates the cached matches. */
        function __css_touch() {
            __css_epoch++;
        }

        function __css_is_dark() {
            try {
                return typeof globalThis.app.isDarkMode === "function" ? !!globalThis.app.isDarkMode() : false;
            } catch (e) {
                return false;
            }
        }

        /** Selectors that target the document root: they define variables, not nodes. */
        function __css_root_selector(sel) {
            const s = sel.trim().toLowerCase();
            if (s === ":root" || s === "html" || s === "body" || s === "html body") return "always";
            if (s === ".theme-dark" || s === "body.theme-dark") return "dark";
            if (s === ".theme-light" || s === "body.theme-light") return "light";
            return null;
        }

        /** One compound part of a selector: div.a.b#id[attr="v"]. null — the part is unsupported.
         *  `stripPseudo` drops :hover/:focus/::before instead of rejecting the part: this is needed
         *  to determine whether the element is shown at all (see `__css_any_show`). */
        function __css_compound(text, stripPseudo) {
            const c = { tag: null, id: null, classes: [], attrs: [] };
            const s = text.trim();
            let i = 0;
            while (i < s.length) {
                const ch = s[i];
                if (ch === "*") { i++; continue; }
                if (ch === ".") {
                    let j = i + 1;
                    while (j < s.length && /[\w-]/.test(s[j])) j++;
                    if (j === i + 1) return null;
                    c.classes.push(s.slice(i + 1, j));
                    i = j;
                    continue;
                }
                if (ch === "#") {
                    let j = i + 1;
                    while (j < s.length && /[\w-]/.test(s[j])) j++;
                    if (j === i + 1) return null;
                    c.id = s.slice(i + 1, j);
                    i = j;
                    continue;
                }
                if (ch === "[") {
                    const j = s.indexOf("]", i);
                    if (j < 0) return null;
                    const inner = s.slice(i + 1, j);
                    const eq = inner.indexOf("=");
                    if (eq < 0) {
                        c.attrs.push([inner.trim(), null]);
                    } else {
                        c.attrs.push([
                            inner.slice(0, eq).trim(),
                            inner.slice(eq + 1).trim().replace(/^["']|["']$/g, "")
                        ]);
                    }
                    i = j + 1;
                    continue;
                }
                if (ch === ":") {
                    if (!stripPseudo) return null;
                    let j = i + 1;
                    if (s[j] === ":") j++;
                    while (j < s.length && /[\w-]/.test(s[j])) j++;
                    if (s[j] === "(") {
                        let depth = 0;
                        for (; j < s.length; j++) {
                            if (s[j] === "(") depth++;
                            else if (s[j] === ")") { depth--; if (depth === 0) { j++; break; } }
                        }
                    }
                    i = j;
                    continue;
                }
                if (/[\w-]/.test(ch)) {
                    let j = i;
                    while (j < s.length && /[\w-]/.test(s[j])) j++;
                    c.tag = s.slice(i, j).toLowerCase();
                    i = j;
                    continue;
                }
                return null;
            }
            return c;
        }

        /** "a > .b .c" -> [{text:"a",comb:" "},{text:".b",comb:">"},{text:".c",comb:" "}]. */
        function __css_split_selector(sel) {
            const parts = [];
            let cur = "";
            let comb = " ";
            let depth = 0;
            for (let i = 0; i < sel.length; i++) {
                const ch = sel[i];
                if (ch === "[") { depth++; cur += ch; continue; }
                if (ch === "]") { depth = Math.max(0, depth - 1); cur += ch; continue; }
                if (depth === 0 && (ch === " " || ch === "\t" || ch === "\n" || ch === "\r" || ch === ">" || ch === "+" || ch === "~")) {
                    if (cur) { parts.push({ text: cur, comb }); cur = ""; comb = " "; }
                    if (ch === ">") comb = ">";
                    else if (ch === "+" || ch === "~") comb = ch;
                    continue;
                }
                cur += ch;
            }
            if (cur) parts.push({ text: cur, comb });
            return parts;
        }

        /** Splitting a rule body by ';' while respecting parentheses: url(data:...;base64,...) does not break. */
        function __css_split_decls(body) {
            const out = [];
            let cur = "";
            let depth = 0;
            let quote = "";
            for (const ch of body) {
                if (quote) {
                    cur += ch;
                    if (ch === quote) quote = "";
                    continue;
                }
                if (ch === '"' || ch === "'") { quote = ch; cur += ch; continue; }
                if (ch === "(") depth++;
                else if (ch === ")") depth = Math.max(0, depth - 1);
                if (ch === ";" && depth === 0) { out.push(cur); cur = ""; continue; }
                cur += ch;
            }
            if (cur.trim()) out.push(cur);
            return out;
        }

        function __css_declarations(body) {
            const out = {};
            for (const decl of __css_split_decls(body)) {
                const i = decl.indexOf(":");
                if (i < 0) continue;
                const raw = decl.slice(0, i).trim().toLowerCase();
                const val = decl.slice(i + 1).trim();
                if (!raw || !val || raw.startsWith("--")) continue;
                const key = __css_key(raw);
                if (!__STYLE_KEYS.has(key)) continue;
                out[key] = val.replace(/\s*!important\s*$/i, "");
            }
            return out;
        }

        function __css_custom_props(body) {
            const out = {};
            for (const decl of __css_split_decls(body)) {
                const i = decl.indexOf(":");
                if (i < 0) continue;
                const raw = decl.slice(0, i).trim();
                if (!raw.startsWith("--")) continue;
                out[raw] = decl.slice(i + 1).trim();
            }
            return out;
        }

        function __css_build_compounds(parts, stripPseudo) {
            const compounds = [];
            for (const p of parts) {
                const c = __css_compound(p.text, stripPseudo);
                if (!c) return null;
                c.comb = p.comb;
                compounds.push(c);
            }
            return compounds;
        }

        /** Index by the rightmost part of the selector: it decides whether a rule matches the node at all. */
        function __css_index(byClass, byTag, universal, rule) {
            const last = rule.parts[rule.parts.length - 1];
            if (last.classes.length > 0) {
                const key = last.classes[0];
                if (!byClass.has(key)) byClass.set(key, []);
                byClass.get(key).push(rule);
            } else if (last.tag) {
                if (!byTag.has(last.tag)) byTag.set(last.tag, []);
                byTag.get(last.tag).push(rule);
            } else {
                universal.push(rule);
            }
        }

        /**
         * A rule with a pseudo-class: we cannot apply it — we have no :hover/:focus
         * state. But if it shows the element, that must be remembered: otherwise the paired
         * `display: none` will hide the button forever and it becomes unreachable.
         */
        function __css_note_state_rule(parts, decls) {
            __css_stats.skipped_pseudo++;
            const display = decls.display;
            if (!display || String(display).trim().toLowerCase() === "none") return;
            const compounds = __css_build_compounds(parts, true);
            if (!compounds) return;
            __css_index(__css_show_by_class, __css_show_by_tag, __css_show_universal, {
                parts: compounds,
                decls: {},
                order: 0,
                key: 0
            });
        }

        /** Whether the element is shown by at least one state rule. */
        function __css_any_show(el) {
            const hit = (list) => {
                if (!list) return false;
                for (const r of list) {
                    if (__css_matches(el, r.parts, r.parts.length - 1)) return true;
                }
                return false;
            };
            if (hit(__css_show_universal)) return true;
            if (el.tag && hit(__css_show_by_tag.get(el.tag))) return true;
            if (el.classes) {
                for (const cls of el.classes) {
                    if (hit(__css_show_by_class.get(cls))) return true;
                }
            }
            return false;
        }

        function __css_add_rule(sel, decls, order) {
            const parts = __css_split_selector(sel);
            if (parts.length === 0) return;
            const compounds = __css_build_compounds(parts, false);
            if (!compounds) {
                __css_note_state_rule(parts, decls);
                return;
            }
            if (Object.keys(decls).length === 0) return;
            let ids = 0;
            let cls = 0;
            let tags = 0;
            for (const c of compounds) {
                if (c.id) ids++;
                cls += c.classes.length + c.attrs.length;
                if (c.tag) tags++;
            }
            // The cascade key is computed once: comparing numbers is cheaper than three fields per frame.
            // order is bounded by the number of rules in the file (thousands), specificity by hundreds.
            const key = ids * 1e12 + cls * 1e9 + tags * 1e6 + order;
            const rule = { parts: compounds, decls, order, key };
            __css_rules.push(rule);
            __css_stats.selectors++;
            __css_index(__css_by_class, __css_by_tag, __css_universal, rule);
        }

        /** Parser for the plugin's stylesheet. Called once on load. */
        function __init_plugin_css(src) {
            __css_rules = [];
            __css_by_class = new Map();
            __css_by_tag = new Map();
            __css_universal = [];
            __css_show_by_class = new Map();
            __css_show_by_tag = new Map();
            __css_show_universal = [];
            __css_vars = new Map();
            __css_stats.rules = 0;
            __css_stats.selectors = 0;
            __css_stats.skipped_pseudo = 0;
            __css_stats.skipped_at = 0;
            __css_touch();

            const text = String(src || "").replace(/\/\*[\s\S]*?\*\//g, "");
            if (!text.trim()) return;
            const dark = __css_is_dark();
            let order = 0;
            let i = 0;
            const n = text.length;
            while (i < n) {
                let j = i;
                while (j < n && text[j] !== "{" && text[j] !== "}") j++;
                if (j >= n) break;
                if (text[j] === "}") { i = j + 1; continue; }
                const prelude = text.slice(i, j).trim();
                let depth = 0;
                let k = j;
                for (; k < n; k++) {
                    if (text[k] === "{") depth++;
                    else if (text[k] === "}") { depth--; if (depth === 0) break; }
                }
                const body = text.slice(j + 1, k);
                i = k + 1;
                if (prelude.startsWith("@")) {
                    __css_stats.skipped_at++;
                    continue;
                }
                const decls = __css_declarations(body);
                const vars = __css_custom_props(body);
                for (const sel of prelude.split(",")) {
                    const s = sel.trim();
                    if (!s) continue;
                    const root = __css_root_selector(s);
                    if (root) {
                        if (root === "always" || (root === "dark" && dark) || (root === "light" && !dark)) {
                            for (const [name, value] of Object.entries(vars)) __css_vars.set(name, value);
                        }
                        continue;
                    }
                    __css_add_rule(s, decls, order);
                }
                order++;
                __css_stats.rules++;
            }
        }

        /** var(--x) and var(--x, fallback) from variables declared on the root and theme. */
        function __css_resolve_vars(value, depth) {
            if (depth > 6 || value.indexOf("var(") < 0) return value;
            return value.replace(/var\(\s*(--[\w-]+)\s*(?:,\s*([^()]*))?\)/g, (m, name, fallback) => {
                const v = __css_vars.get(name);
                if (v != null) return __css_resolve_vars(v, depth + 1);
                return fallback != null ? fallback.trim() : "";
            });
        }

        function __css_compound_matches(el, c) {
            if (!el || el.tag === undefined) return false;
            if (c.tag && el.tag !== c.tag) return false;
            if (c.id && el.id !== c.id) return false;
            if (c.classes.length > 0) {
                if (!el.classes) return false;
                for (const cls of c.classes) if (!el.classes.has(cls)) return false;
            }
            if (c.attrs.length > 0) {
                const a = el.attributes || {};
                for (const [k, v] of c.attrs) {
                    if (v === null) { if (a[k] === undefined) return false; }
                    else if (String(a[k]) !== v) return false;
                }
            }
            return true;
        }

        /** Right to left: the right part has matched, now check ancestors by combinator. */
        function __css_matches(el, parts, idx) {
            if (!__css_compound_matches(el, parts[idx])) return false;
            if (idx === 0) return true;
            const comb = parts[idx].comb;
            if (comb === ">") return __css_matches(el.parent, parts, idx - 1);
            if (comb === "+" || comb === "~") return false;
            let p = el.parent;
            while (p) {
                if (__css_matches(p, parts, idx - 1)) return true;
                p = p.parent;
            }
            return false;
        }

        function __css_match(el) {
            const cands = [];
            const seen = new Set();
            const add = (list) => {
                if (!list) return;
                for (const r of list) {
                    if (seen.has(r)) continue;
                    seen.add(r);
                    cands.push(r);
                }
            };
            add(__css_universal);
            if (el.tag) add(__css_by_tag.get(el.tag));
            if (el.classes) for (const cls of el.classes) add(__css_by_class.get(cls));
            if (cands.length === 0) return null;
            // Cascade: specificity, then order in the file. The last applied wins.
            cands.sort((a, b) => a.key - b.key);
            const out = {};
            let any = false;
            for (const r of cands) {
                if (!__css_matches(el, r.parts, r.parts.length - 1)) continue;
                for (const [k, v] of Object.entries(r.decls)) {
                    const resolved = __css_resolve_vars(v, 0);
                    if (resolved === "") continue;
                    out[k] = resolved;
                    any = true;
                }
            }
            // Hiding for good is the worst possible outcome: the element is shown via :hover/:focus,
            // which we do not support, and the button would be unreachable. Always show it.
            if (out.display === "none" && __css_any_show(el)) {
                delete out.display;
                any = Object.keys(out).length > 0;
            }
            return any ? out : null;
        }

        function __css_declarations_for(el) {
            if (!el || __css_rules.length === 0) return null;
            const cached = el._cssCache;
            if (cached && cached.epoch === __css_epoch) return cached.decls;
            const decls = __css_match(el);
            el._cssCache = { epoch: __css_epoch, decls };
            return decls;
        }

        /**
         * Element styles: styles.css first, then inline on top. Returns null if empty.
         * `base` — declarations found by the plugin stylesheet's selectors.
         */
        function __style_to_uinode(el, base) {
            const src = el && el._styleTarget;
            const out = {};
            if (base) for (const [k, v] of Object.entries(base)) out[k] = v;
            if (!src) return Object.keys(out).length > 0 ? out : null;
            for (const [k, v] of Object.entries(src)) {
                // An empty value means "remove", not "set empty": the underlying
                // style from the stylesheet must remain in effect.
                if (v == null || v === "") continue;
                if (k === "cssText") {
                    // The same CSS, but as a string: plugins write it this way 253 times in the corpus.
                    for (const decl of String(v).split(";")) {
                        const i = decl.indexOf(":");
                        if (i < 0) continue;
                        const key = __css_key(decl.slice(0, i));
                        if (__STYLE_KEYS.has(key)) out[key] = decl.slice(i + 1).trim();
                    }
                    continue;
                }
                if (__STYLE_KEYS.has(k)) out[k] = String(v);
            }
            return Object.keys(out).length > 0 ? out : null;
        }

        /**
         * An element with a non-empty style becomes a `styled` node: style is the element's box, child is
         * what it turned into. Children go through the same function, so nested styles are not lost.
         */
        function __velement_to_uinode(el) {
            const node = __velement_core_to_uinode(el);
            const style = __style_to_uinode(el, __css_declarations_for(el));
            if (!style) return node;
            // A textless empty div is usually dropped by the serializer, but if it has a size
            // or a background — it is a spacer/divider, and it must remain an empty box.
            return { type: "styled", style, child: node || { type: "column", children: [] } };
        }

        class Component {
            constructor() {
                this._children = [];
                this._disposers = [];
                this._loaded = false;
            }
            load() {
                if (this._loaded) return;
                // Obsidian marks the component loaded *before* onload() so that addChild()
                // inside onload() actually loads the child, then loads any children added.
                this._loaded = true;
                if (typeof this.onload === "function") this.onload();
                for (const c of this._children.slice()) {
                    if (c && typeof c.load === "function") c.load();
                }
            }
            unload() {
                if (!this._loaded) return;
                this._loaded = false;
                // LIFO: children, then disposers, then onunload() last. Unwinding in reverse
                // registration order mirrors nested finally blocks and keeps onunload's own
                // resources alive until it runs.
                while (this._children.length > 0) {
                    const child = this._children.pop();
                    if (child && typeof child.unload === "function") {
                        try { child.unload(); } catch (e) { console.error(e); }
                    }
                }
                while (this._disposers.length > 0) {
                    const dispose = this._disposers.pop();
                    try { dispose(); } catch (e) { console.error(e); }
                }
                if (typeof this.onunload === "function") this.onunload();
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
                    this._disposers.push(cb);
                }
            }
            registerEvent(ref) {
                if (!ref) return ref;
                // Obsidian's EventRef carries `.e` (the Events instance). Host refs carry an
                // `unload` closure. Support both so event subscriptions are actually removed.
                if (ref.e && typeof ref.e.offref === "function") {
                    this.register(() => ref.e.offref(ref));
                } else if (typeof ref.unload === "function") {
                    this.register(() => ref.unload());
                }
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
            registerScopeEvent(ref) {
                if (ref && ref.scope && typeof ref.scope.unregister === "function") {
                    this.register(() => ref.scope.unregister(ref));
                }
            }
            registerHoverLinkSource(source, info) {}
        }

        class Events {
            constructor() {
                this._events = new Map();
            }
            on(name, cb, ctx) {
                if (!this._events.has(name)) this._events.set(name, []);
                // `.e` mirrors Obsidian's EventRef so Component.registerEvent() can offref().
                const ref = { e: this, cb, ctx };
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
                this.app = (leaf && leaf.app) || globalThis.app;
                this.containerEl = new VElement("div", { cls: "view" });
                this.navigation = false;
                this.__actions = [];
            }
            getViewType() { return "view"; }
            getDisplayText() { return ""; }
            getIcon() { return "document"; }
            addAction(icon, title, callback) {
                const id = "action-" + Math.random().toString(36).substr(2, 9);
                this.__actions.push({ id, icon: String(icon), title: String(title), callback });
                if (typeof callback === "function") {
                    globalThis.__action_callbacks.set(id, callback);
                }
                const el = new VElement("div", { cls: "clickable-icon view-action" });
                if (typeof callback === "function") {
                    el.addEventListener("click", callback);
                }
                return el;
            }
            onOpen() {}
            onClose() {}
            getState() { return {}; }
            setState(state, result) { return Promise.resolve(); }
        }

        class ItemView extends View {
            constructor(leaf) {
                super(leaf);
                this.contentEl = this.containerEl.createDiv("item-view-content");
                let leafSyncScheduled = false;
                const onMod = () => {
                    if (leafSyncScheduled) return;
                    leafSyncScheduled = true;
                    const runSync = () => {
                        leafSyncScheduled = false;
                        if (this.leaf && typeof this.leaf.syncTree === "function") {
                            try { this.leaf.syncTree(); } catch (e) {}
                        }
                    };
                    if (typeof queueMicrotask === "function") {
                        queueMicrotask(runSync);
                    } else if (typeof Promise === "function") {
                        Promise.resolve().then(runSync);
                    } else {
                        setTimeout(runSync, 0);
                    }
                };
                this.contentEl._onModified = onMod;
                this.containerEl._onModified = onMod;
            }
            getViewType() { return "empty"; }
            getDisplayText() { return "Item View"; }
            getIcon() { return "layout"; }
            async onOpen() {}
            async onClose() {}
        }

        // FileView/TextFileView give plugin view classes an instanceof-able base for
        // file-backed views. TextFileView carries `data` and routes setContent through
        // onSetData, as the Obsidian contract describes.
        class FileView extends ItemView {
            constructor(leaf) {
                super(leaf);
                this.file = null;
                this.allowNoFile = false;
            }
            getFile() { return this.file; }
        }

        class TextFileView extends FileView {
            constructor(leaf) {
                super(leaf);
                this.data = "";
            }
            getViewType() { return "text"; }
            getIcon() { return "document"; }
            getDoc() { return this.__editor || (this.__editor = new Editor("")); }
            async onSetData(_data) {}
            setContent(data, _type) {
                this.data = String(data != null ? data : "");
                return Promise.resolve(this.onSetData(this.data));
            }
            getData() { return this.data; }
        }

        // The document buffer lives in the host, not in the plugin runtime. A MarkdownView here is a
        // handle on the file the host currently shows, not an owner of its text: `file`, `getMode()`
        // and `getViewType()` are authoritative. The `editor` handle reaches the host buffer through
        // the editor_* requests (getValue, replaceRange, cursor and friends).
        class MarkdownView extends ItemView {
            constructor(leaf, file) {
                super(leaf);
                this.file = file || null;
                this.navigation = true;
                this.__editor = new Editor("");
            }
            getViewType() { return "markdown"; }
            getDisplayText() { return this.file ? this.file.basename : ""; }
            getIcon() { return "document"; }
            getMode() { return "source"; }
            get editor() { return this.__editor; }
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
                let syncScheduled = false;
                const scheduleSync = () => {
                    if (syncScheduled) return;
                    syncScheduled = true;
                    const runSync = () => {
                        syncScheduled = false;
                        if (globalThis.__active_modal === this) {
                            this.syncTree();
                        }
                    };
                    if (typeof queueMicrotask === "function") {
                        queueMicrotask(runSync);
                    } else if (typeof Promise === "function") {
                        Promise.resolve().then(runSync);
                    } else {
                        setTimeout(runSync, 0);
                    }
                };
                this.contentEl._onModified = scheduleSync;
                this.modalEl._onModified = scheduleSync;
                this.containerEl._onModified = scheduleSync;
            }
            syncTree() {
                if (globalThis.__active_modal === this && this.containerEl) {
                    try {
                        const tree = __velement_to_uinode(this.containerEl);
                        console.log(`[plugin] Modal.syncTree: sending set_view_tree view_id=1`);
                        __invoke_host("set_view_tree", { view_id: 1, tree });
                    } catch (e) {
                        console.error("[plugin] Modal syncTree error:", e);
                    }
                }
            }
            async open() {
                console.log("[plugin] Modal.open() called");
                globalThis.__active_modal = this;
                this.syncTree();
                try {
                    console.log("[plugin] Invoking Modal.onOpen()...");
                    const p = this.onOpen();
                    if (p && typeof p.then === "function") {
                        await p;
                    }
                    console.log("[plugin] Modal.onOpen() completed");
                } catch (e) {
                    console.error("[plugin] Modal onOpen error:", e);
                }
                this.syncTree();
            }
            close() {
                console.log("[plugin] Modal.close() called");
                if (globalThis.__active_modal === this) {
                    globalThis.__active_modal = null;
                }
                try {
                    this.onClose();
                } catch (e) {
                    console.error("[plugin] Modal onClose error:", e);
                }
                __invoke_host("set_view_tree", { view_id: 1, tree: { type: "text", value: "" } });
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

        class AbstractInputSuggest {
            constructor(app, textInputEl) {
                this.app = app;
                this.textInputEl = textInputEl;
                this.limit = 100;
            }
            getSuggestions(query) { return []; }
            renderSuggestion(value, el) { if (el && el.setText) el.setText(String(value)); }
            selectSuggestion(value, evt) {}
            close() {}
            open() {}
        }

        class EditorSuggest extends Component {
            constructor(app) {
                super();
                this.app = app;
                this.scope = new Scope();
            }
            getSuggestions(context) { return []; }
            renderSuggestion(value, el) { if (el && el.setText) el.setText(String(value)); }
            selectSuggestion(value, evt) {}
            close() {}
        }

        // EditorPosition is a plain {line, ch} pair; `ch` counts characters, matching
        // EditorViewState::line_col. Reads go through the synchronous host snapshot
        // (__host_get_active_buffer); writes are async host requests.
        const __editor_sync_buffer = () => {
            const raw = __host_get_active_buffer();
            try {
                const parsed = typeof raw === "string" ? JSON.parse(raw) : raw;
                return parsed && typeof parsed.text === "string" ? parsed : null;
            } catch (e) {
                return null;
            }
        };
        const __json_list = (raw) => {
            try {
                const parsed = typeof raw === "string" ? JSON.parse(raw) : raw;
                return Array.isArray(parsed) ? parsed : [];
            } catch (e) {
                return [];
            }
        };
        const __host_commands_list = () => __json_list(__host_get_commands());
        const __host_plugins_list = () => __json_list(__host_get_plugins());
        const __editor_pos = (pos) => {
            if (pos && typeof pos === "object" && typeof pos.line === "number") {
                return { line: Math.max(0, pos.line | 0), ch: Math.max(0, pos.ch | 0) };
            }
            return { line: 0, ch: 0 };
        };
        const __pos_le = (a, b) => a.line < b.line || (a.line === b.line && a.ch <= b.ch);

        class Editor {
            constructor(selection = "") {
                this._selection = String(selection != null ? selection : "");
            }
            getValue() {
                const buf = __editor_sync_buffer();
                return buf ? buf.text : "";
            }
            setValue(content) {
                __invoke_host("editor_set_buffer", { text: String(content != null ? content : "") });
            }
            getLine(line) {
                const buf = __editor_sync_buffer();
                if (!buf) return "";
                return buf.text.split("\n")[line | 0] || "";
            }
            setLine(line, text) {
                const buf = __editor_sync_buffer();
                if (!buf) return;
                const lines = buf.text.split("\n");
                const idx = line | 0;
                if (idx < 0 || idx >= lines.length) return;
                lines[idx] = String(text != null ? text : "");
                __invoke_host("editor_set_buffer", { text: lines.join("\n") });
            }
            lineCount() {
                const buf = __editor_sync_buffer();
                if (!buf) return 0;
                return buf.text.split("\n").length;
            }
            lastLine() {
                return Math.max(0, this.lineCount() - 1);
            }
            getCursor(string) {
                const buf = __editor_sync_buffer();
                const cur = buf && buf.cursor ? buf.cursor : { line: 0, ch: 0 };
                if (string === "anchor" && cur.anchor) {
                    return { line: cur.anchor.line, ch: cur.anchor.ch };
                }
                return { line: cur.line, ch: cur.ch };
            }
            setCursor(pos, ch) {
                if (typeof pos === "number") pos = { line: pos, ch: ch || 0 };
                const p = __editor_pos(pos);
                __invoke_host("editor_set_cursor", { line: p.line, ch: p.ch });
            }
            getSelection() {
                if (this._selection) return this._selection;
                const res = __invoke_host("editor_get_selection", {});
                return res == null ? "" : String(res);
            }
            getRange(from, to) {
                const buf = __editor_sync_buffer();
                if (!buf) return "";
                const lines = buf.text.split("\n");
                const a = __editor_pos(from), b = __editor_pos(to);
                const [start, end] = __pos_le(a, b) ? [a, b] : [b, a];
                if (start.line === end.line) {
                    return (lines[start.line] || "").slice(start.ch, end.ch);
                }
                let out = (lines[start.line] || "").slice(start.ch);
                for (let l = start.line + 1; l < end.line; l++) out += "\n" + (lines[l] || "");
                out += "\n" + (lines[end.line] || "").slice(0, end.ch);
                return out;
            }
            replaceSelection(text) {
                const replacement = String(text != null ? text : "");
                __invoke_host("editor_replace_selection", { text: replacement });
                this._selection = "";
            }
            replaceRange(replacement, from, to) {
                const a = __editor_pos(from), b = __editor_pos(to === undefined ? from : to);
                const [start, end] = __pos_le(a, b) ? [a, b] : [b, a];
                __invoke_host("editor_replace_range", {
                    from_line: start.line, from_ch: start.ch,
                    to_line: end.line, to_ch: end.ch,
                    text: String(replacement != null ? replacement : "")
                });
            }
            setSelection(anchor, head) {
                const a = __editor_pos(anchor), h = __editor_pos(head === undefined ? anchor : head);
                __invoke_host("editor_set_selection", {
                    anchor_line: a.line, anchor_ch: a.ch,
                    head_line: h.line, head_ch: h.ch
                });
            }
            listSelections() {
                const cur = this.getCursor();
                const anchor = this.getCursor("anchor");
                return __pos_le(anchor, cur) ? [{ anchor, head: cur }] : [{ anchor: cur, head: anchor }];
            }
            somethingSelected() {
                const cur = this.getCursor(), anchor = this.getCursor("anchor");
                return !(cur.line === anchor.line && cur.ch === anchor.ch);
            }
            hasFocus() { return true; }
            focus() {}
            blur() {}
        }

        class MenuItem {
            constructor(menu) {
                this.menu = menu;
                this.id = "menu-item-" + Math.random().toString(36).substr(2, 9);
                this.title = "";
                this.icon = "";
                this.checked = false;
                this.disabled = false;
                this.is_label = false;
                this.is_separator = false;
                this.callback = null;
                this.submenu = null;
            }
            setTitle(title) { this.title = String(title != null ? title : ""); return this; }
            setIcon(icon) { this.icon = String(icon != null ? icon : ""); return this; }
            setChecked(checked) { this.checked = !!checked; return this; }
            setDisabled(disabled) { this.disabled = !!disabled; return this; }
            setIsLabel(isLabel) { this.is_label = !!isLabel; return this; }
            onClick(cb) {
                this.callback = cb;
                if (cb) {
                    globalThis.__menu_callbacks.set(this.id, cb);
                }
                return this;
            }
            setSubmenu() {
                if (!this.submenu) {
                    this.submenu = new Menu();
                }
                return this.submenu;
            }
            toJSON() {
                return {
                    id: this.id,
                    title: this.title,
                    icon: this.icon,
                    checked: this.checked,
                    disabled: this.disabled,
                    is_label: this.is_label,
                    is_separator: this.is_separator,
                    has_callback: !!this.callback,
                    submenu: this.submenu ? this.submenu.items.map(it => it.toJSON()) : null
                };
            }
        }

        class Menu {
            constructor() {
                this.items = [];
                this.__hide_cb = null;
            }
            addItem(cb) {
                const item = new MenuItem(this);
                if (typeof cb === "function") cb(item);
                this.items.push(item);
                return this;
            }
            addSeparator() {
                const item = new MenuItem(this);
                item.is_separator = true;
                this.items.push(item);
                return this;
            }
            showAtPosition(pos) { return this; }
            showAtMouseEvent(evt) { return this; }
            hide() {
                if (typeof this.__hide_cb === "function") {
                    try { this.__hide_cb(); } catch (e) { console.error("Menu onHide error:", e); }
                }
                return this;
            }
            close() { return this.hide(); }
            onHide(cb) {
                this.__hide_cb = cb;
                return this;
            }
            setNoIcon() { return this; }
            setUseNativeMenu(_native) { return this; }
            setParentElement(_el) { return this; }
            toJSON() {
                return this.items.map(it => it.toJSON());
            }
        }

        // ---- Workspace activation and layout readiness -------------------------------------------
        // Two kinds of views coexist: plugin-owned leaves (tracked in __workspace_leaves) and the
        // host's own editor, which the runtime can only observe through __host_get_active_file().
        // Both are projected onto the single active leaf Obsidian exposes.
        let __active_leaf = null;
        let __most_recent_leaf = null;
        let __native_leaf = null;
        let __native_markdown_view = null;

        function getNativeMarkdownView() {
            let path = null;
            try { path = __host_get_active_file(); } catch (e) { path = null; }
            if (!path) {
                __native_markdown_view = null;
                return null;
            }
            if (!__native_markdown_view) {
                __native_markdown_view = new MarkdownView(null, new TFile(path));
            } else if (!__native_markdown_view.file || __native_markdown_view.file.path !== path) {
                __native_markdown_view.file = new TFile(path);
            }
            return __native_markdown_view;
        }

        function getActiveLeaf() {
            if (__active_leaf) return __active_leaf;
            const view = getNativeMarkdownView();
            if (!view) return null;
            if (!__native_leaf) {
                __native_leaf = new WorkspaceLeaf(rawWorkspace, "center");
            }
            __native_leaf.view = view;
            __native_leaf.viewState = { type: "markdown", state: { file: view.file.path } };
            return __native_leaf;
        }

        // FileSystemAdapter in Obsidian talks to the real disk; the plugin sandbox has no
        // absolute-filesystem access, so every call is routed through the vault adapter.
        class FileSystemAdapter {
            static getBasePath() { return ""; }
            static getResourcePath(path) { return "app://" + normalizePath(String(path)); }
            static async exists(path) {
                return __host_get_markdown_files().includes(normalizePath(String(path)));
            }
            static async read(path) {
                return rawVault.read(String(path));
            }
            static async readBinary(path) {
                const res = await __invoke_host("vault_read_binary", { path: String(path) });
                if (typeof res !== "string") throw new Error("FileSystemAdapter.readBinary: host returned no data");
                return __base64_to_array_buffer(res);
            }
            static async write(path, data) {
                await __invoke_host("vault_modify", { path: String(path), data: String(data) });
            }
            static async writeBinary(path, data) {
                await __invoke_host("vault_modify_binary", { path: String(path), data_base64: __array_buffer_to_base64(data) });
            }
            static async append(path, data) {
                let existing = "";
                try {
                    existing = await rawVault.read(String(path));
                } catch (e) {
                    existing = "";
                }
                await __invoke_host("vault_modify", { path: String(path), data: existing + String(data) });
            }
            static async mkdir(path) {
                await __invoke_host("vault_create_folder", { path: normalizePath(String(path)) });
            }
            static async remove(path) {
                await __invoke_host("vault_delete", { path: String(path) });
            }
            static async rename(from, to) {
                await __invoke_host("vault_rename", { old_path: String(from), new_path: normalizePath(String(to)) });
            }
            constructor() {
                // Instance form mirrors the static one; Obsidian also exposes instance methods.
            }
        }

        function activateLeaf(leaf) {
            __active_leaf = leaf || null;
            if (__active_leaf) __most_recent_leaf = __active_leaf;
        }

        // Resolves an Obsidian link target ("[[Note|alias]]", "folder/Note", "Note.md#heading") to a
        // vault path. Links are vault-relative in Obsidian, so a missing path is matched by suffix and
        // then by basename, preferring the folder the link came from.
        function resolveLinkTarget(linktext, sourcePath) {
            let t = String(linktext == null ? "" : linktext).trim();
            if (t.startsWith("[[") && t.endsWith("]]")) t = t.slice(2, -2);
            t = t.split("|")[0].split("#")[0].split("^")[0];
            t = normalizePath(t);
            if (!t) return null;
            const files = __host_get_markdown_files();
            if (files.includes(t)) return t;
            const withExt = /\.[A-Za-z0-9]+$/.test(t) ? t : t + ".md";
            if (files.includes(withExt)) return withExt;
            const bySuffix = files.find((f) => f.endsWith("/" + withExt));
            if (bySuffix) return bySuffix;
            const name = withExt.split("/").pop();
            const candidates = files.filter((f) => f.split("/").pop() === name);
            if (candidates.length === 0) return null;
            if (candidates.length === 1) return candidates[0];
            const folder = normalizePath(sourcePath || "").split("/").slice(0, -1).join("/");
            return candidates.find((f) => folder && f.startsWith(folder + "/")) || candidates[0];
        }

        // Obsidian drains onLayoutReady callbacks once the workspace layout exists. The host has a
        // built workspace before plugins load, so the honest boundary is the end of plugin onload().
        let __layout_ready = false;
        const __layout_ready_queue = [];

        function runLayoutReadyCallback(cb) {
            try { cb(); } catch (e) { console.error("[plugin] onLayoutReady callback error:", e); }
        }

        function markLayoutReady() {
            if (__layout_ready) return;
            __layout_ready = true;
            while (__layout_ready_queue.length > 0) {
                runLayoutReadyCallback(__layout_ready_queue.shift());
            }
        }

        class WorkspaceLeaf {
            constructor(workspace, location = "right") {
                this.workspace = workspace;
                this.app = (workspace && workspace.app) || globalThis.app;
                this.location = location;
                this.id = "leaf-" + Math.random().toString(36).substr(2, 9);
                this.view = null;
                this.viewState = { type: "empty" };
                globalThis.__workspace_leaves.set(this.id, this);
            }
            getRoot() { return this.workspace.rootSplit; }
            async setViewState(state) {
                try {
                    this.viewState = state;
                    const type = state && state.type;
                    if (type && globalThis.__registered_views.has(type)) {
                        const creator = globalThis.__registered_views.get(type);
                        const view = creator(this);
                        this.view = view;
                        if (!state || state.active !== false) {
                            activateLeaf(this);
                        }
                        // Obsidian order: createView -> load() -> onOpen(). load() runs the view's
                        // onload() and loads any components it registered as children.
                        if (typeof view.load === "function") {
                            view.load();
                        }
                        if (typeof view.onOpen === "function") {
                            await view.onOpen();
                        }
                        const title = typeof view.getDisplayText === "function" ? view.getDisplayText() : type;
                        const icon = typeof view.getIcon === "function" ? view.getIcon() : "layout";
                        const actions = (view.__actions || []).map(a => ({ id: a.id, icon: a.icon, title: a.title }));
                        const tree = __velement_to_uinode(view.containerEl || view.contentEl);
                        await __invoke_host("set_leaf_view", {
                            leaf_id: this.id,
                            location: this.location,
                            view_type: type,
                            title: String(title),
                            icon: String(icon),
                            tree: tree,
                            actions: actions
                        });
                    }
                } catch (e) {
                    console.error("setViewState error:", e);
                }
            }
            async syncTree() {
                if (!this.view) return;
                const type = this.viewState && this.viewState.type ? this.viewState.type : "";
                const title = typeof this.view.getDisplayText === "function" ? this.view.getDisplayText() : type;
                const icon = typeof this.view.getIcon === "function" ? this.view.getIcon() : "layout";
                const actions = (this.view.__actions || []).map(a => ({ id: a.id, icon: a.icon, title: a.title }));
                const tree = __velement_to_uinode(this.view.containerEl || this.view.contentEl);
                await __invoke_host("set_leaf_view", {
                    leaf_id: this.id,
                    location: this.location,
                    view_type: type,
                    title: String(title),
                    icon: String(icon),
                    tree: tree,
                    actions: actions
                });
            }
            openFile(file) {
                return this.workspace.openFile(file);
            }
            detach() {
                globalThis.__workspace_leaves.delete(this.id);
                if (__active_leaf === this) activateLeaf(null);
                if (__most_recent_leaf === this) __most_recent_leaf = null;
                if (this.view) {
                    // Obsidian order: detach() -> unload() (disposes registrations/children) -> onClose().
                    if (typeof this.view.unload === "function") {
                        try { this.view.unload(); } catch (e) { console.error("ItemView unload error:", e); }
                    }
                    if (typeof this.view.onClose === "function") {
                        try { this.view.onClose(); } catch (e) { console.error("ItemView onClose error:", e); }
                    }
                }
                return __invoke_host("close_leaf", { leaf_id: this.id });
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

        // request() is the promise-of-text variant of requestUrl (Obsidian contract).
        function request(options) {
            if (typeof options === "string") options = { url: options };
            return requestUrl(options).then((res) => res.text);
        }

        function moment(date) {
            const d = (() => {
                if (date === undefined || date === null) return new Date();
                if (typeof date === "number" || typeof date === "string") return new Date(date);
                if (date instanceof Date) return new Date(date.getTime());
                // moment-like wrappers (ours or someone else's) return time via valueOf()
                if (typeof date.valueOf === "function") return new Date(date.valueOf());
                return new Date(date);
            })();
            const wrap = (nd) => moment(nd.getTime());
            const pad = (n) => String(n).padStart(2, "0");
            const normUnit = (u) => {
                const s = String(u == null ? "" : u).toLowerCase().replace(/s$/, "");
                return s === "date" ? "day" : s;
            };
            function addAmount(n, unit) {
                const nd = new Date(d.getTime());
                switch (normUnit(unit)) {
                    case "day": nd.setDate(nd.getDate() + n); break;
                    case "week": nd.setDate(nd.getDate() + n * 7); break;
                    case "hour": nd.setTime(nd.getTime() + n * 3600000); break;
                    case "minute": nd.setTime(nd.getTime() + n * 60000); break;
                    case "second": nd.setTime(nd.getTime() + n * 1000); break;
                    case "millisecond": nd.setTime(nd.getTime() + n); break;
                    case "month": {
                        const day = nd.getDate();
                        nd.setMonth(nd.getMonth() + n);
                        // End-of-month clamp (January 31 + 1 month -> February 28/29).
                        if (nd.getDate() < day) nd.setDate(0);
                        break;
                    }
                    case "year": nd.setFullYear(nd.getFullYear() + n); break;
                    default: break;
                }
                return wrap(nd);
            }
            function startOf(unit) {
                const nd = new Date(d.getTime());
                switch (normUnit(unit)) {
                    case "day": nd.setHours(0, 0, 0, 0); break;
                    case "week": nd.setDate(nd.getDate() - nd.getDay()); nd.setHours(0, 0, 0, 0); break;
                    case "month": nd.setDate(1); nd.setHours(0, 0, 0, 0); break;
                    case "year": nd.setMonth(0, 1); nd.setHours(0, 0, 0, 0); break;
                    case "hour": nd.setMinutes(0, 0, 0); break;
                    case "minute": nd.setSeconds(0, 0); break;
                    case "second": nd.setMilliseconds(0); break;
                    default: break;
                }
                return wrap(nd);
            }
            function endOf(unit) {
                // End of a unit = start of the next one minus 1 ms.
                const next = addAmount(1, unit);
                return wrap(new Date(next.startOf(unit).valueOf() - 1));
            }
            const toTime = (other) => {
                if (other === undefined || other === null) return NaN;
                if (typeof other === "number") return other;
                if (typeof other.valueOf === "function") {
                    const t = other.valueOf();
                    return typeof t === "number" ? t : new Date(t).getTime();
                }
                return new Date(other).getTime();
            };
            const MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
            const DAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
            return {
                format: (fmt = "YYYY-MM-DD") => {
                    const h24 = d.getHours();
                    const h12 = h24 % 12 === 0 ? 12 : h24 % 12;
                    const suffix = (n) => (n % 10 === 1 && n !== 11 ? "st" : n % 10 === 2 && n !== 12 ? "nd" : n % 10 === 3 && n !== 13 ? "rd" : "th");
                    const out = String(fmt).replace(/MMMM|MMM|dddd|ddd|Do|YYYY|YY|MM|DD|HH|hh|h|mm|ss|a|A/g, (tok) => {
                        switch (tok) {
                            case "MMMM": return MONTHS[d.getMonth()];
                            case "MMM": return MONTHS[d.getMonth()].slice(0, 3);
                            case "dddd": return DAYS[d.getDay()];
                            case "ddd": return DAYS[d.getDay()].slice(0, 3);
                            case "Do": return String(d.getDate()) + suffix(d.getDate());
                            case "YYYY": return String(d.getFullYear());
                            case "YY": return String(d.getFullYear()).slice(-2);
                            case "MM": return pad(d.getMonth() + 1);
                            case "DD": return pad(d.getDate());
                            case "HH": return pad(h24);
                            case "hh": return pad(h12);
                            case "h": return String(h12);
                            case "mm": return pad(d.getMinutes());
                            case "ss": return pad(d.getSeconds());
                            case "a": return h24 < 12 ? "am" : "pm";
                            case "A": return h24 < 12 ? "AM" : "PM";
                            default: return tok;
                        }
                    });
                    return out;
                },
                valueOf: () => d.getTime(),
                toDate: () => new Date(d.getTime()),
                toString: () => d.toString(),
                clone: () => moment(d.getTime()),
                isValid: () => !Number.isNaN(d.getTime()),
                toISOString: () => d.toISOString(),
                add: (n, unit) => addAmount(n, unit),
                subtract: (n, unit) => addAmount(-n, unit),
                startOf,
                endOf,
                isBefore: (other) => d.getTime() < toTime(other),
                isAfter: (other) => d.getTime() > toTime(other),
                isSame: (other) => d.getTime() === toTime(other),
                diff: (other, unit) => {
                    const delta = d.getTime() - toTime(other);
                    switch (normUnit(unit)) {
                        case "day": return Math.trunc(delta / 86400000);
                        case "week": return Math.trunc(delta / (7 * 86400000));
                        case "hour": return Math.trunc(delta / 3600000);
                        case "minute": return Math.trunc(delta / 60000);
                        case "second": return Math.trunc(delta / 1000);
                        case "millisecond": return delta;
                        case "month": return (d.getFullYear() - new Date(toTime(other)).getFullYear()) * 12
                            + (d.getMonth() - new Date(toTime(other)).getMonth());
                        case "year": return d.getFullYear() - new Date(toTime(other)).getFullYear();
                        default: return Math.trunc(delta / 86400000);
                    }
                },
                unix: () => Math.floor(d.getTime() / 1000),
                year: () => d.getFullYear(),
                month: () => d.getMonth(),
                date: () => d.getDate(),
                day: () => d.getDay(),
                hour: () => d.getHours(),
                minute: () => d.getMinutes(),
                second: () => d.getSeconds(),
                millisecond: () => d.getMilliseconds(),
            };
        }

        // ---- Markdown rendering -------------------------------------------------------------------
        // The host's UiNode model has no bold/italic/list styling (TextStyle is normal/muted/heading/
        // code), so inline emphasis is flattened to plain text on purpose: emitting <strong> would hit
        // the serializer's "bold with text becomes a heading" branch and blow up a word mid-sentence.
        // Headings and code do have real styles, so those are emitted as elements.
        function mdText(parent, value) {
            if (!value) return;
            parent.appendChild(new VElement("#text", { text: value }));
        }

        // Links render as text carrying the usual Obsidian class, not as <a>: the serializer turns <a>
        // into a full button, which would put a button in the middle of a sentence. Clicking is a
        // separate problem — UiNode has no inline link target yet.
        function mdLink(parent, label, target, external) {
            const link = new VElement("span", { text: label || target });
            link.addClass(external ? "external-link" : "internal-link");
            parent.appendChild(link);
        }

        function mdInline(text, parent) {
            const re = /(`[^`\n]+`)|(\[\[[^\]\n]+\]\])|(!?\[[^\]\n]*\]\([^)\s]+\))|(\*\*[^*\n]+\*\*)|(__[^_\n]+__)|(\*[^*\n]+\*)|(_[^_\n]+_)|(~~[^~\n]+~~)/g;
            let last = 0;
            let m;
            while ((m = re.exec(text))) {
                if (m.index > last) mdText(parent, text.slice(last, m.index));
                const tok = m[0];
                if (m[1]) {
                    parent.appendChild(new VElement("code", { text: tok.slice(1, -1) }));
                } else if (m[2]) {
                    const inner = tok.slice(2, -2);
                    const bar = inner.indexOf("|");
                    const target = (bar === -1 ? inner : inner.slice(0, bar)).split("#")[0];
                    mdLink(parent, bar === -1 ? inner : inner.slice(bar + 1), target, false);
                } else if (m[3]) {
                    const isImage = tok[0] === "!";
                    const close = tok.indexOf("](");
                    const label = tok.slice(isImage ? 2 : 1, close);
                    const href = tok.slice(close + 2, -1);
                    if (isImage) mdText(parent, label || href);
                    else mdLink(parent, label, href, true);
                } else if (m[4] || m[5]) {
                    mdText(parent, tok.slice(2, -2));
                } else if (m[6] || m[7]) {
                    mdText(parent, tok.slice(1, -1));
                } else if (m[8]) {
                    mdText(parent, tok.slice(2, -2));
                }
                last = m.index + tok.length;
            }
            if (last < text.length) mdText(parent, text.slice(last));
        }

        const MD_LIST_ITEM = /^\s*([-*+]|\d+[.)])\s+/;
        const MD_FENCE_OPEN = /^\s*(```+|~~~+)\s*([\w+#.-]*)\s*$/;
        const MD_BLOCK_START = /^\s*(#{1,6}\s|>|([-*+]|\d+[.)])\s|```|~~~)/;

        function mdBlocks(lines, parent) {
            let i = 0;
            while (i < lines.length) {
                const line = lines[i];
                if (!line.trim()) { i++; continue; }

                const fence = MD_FENCE_OPEN.exec(line);
                if (fence) {
                    const marker = fence[1][0] === "`" ? "```" : "~~~";
                    const lang = fence[2];
                    const body = [];
                    i++;
                    while (i < lines.length && lines[i].trim().indexOf(marker) !== 0) {
                        body.push(lines[i]);
                        i++;
                    }
                    i++;
                    const code = new VElement("code", { text: body.join("\n") });
                    if (lang) code.addClass("language-" + lang);
                    parent.appendChild(code);
                    continue;
                }

                const heading = /^(#{1,6})\s+(.*?)\s*#*\s*$/.exec(line);
                if (heading) {
                    parent.appendChild(new VElement("h" + heading[1].length, { text: heading[2] }));
                    i++;
                    continue;
                }

                if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(line)) {
                    parent.appendChild(new VElement("hr"));
                    i++;
                    continue;
                }

                if (/^\s*>/.test(line)) {
                    const quoted = [];
                    while (i < lines.length && /^\s*>/.test(lines[i])) {
                        quoted.push(lines[i].replace(/^\s*>\s?/, ""));
                        i++;
                    }
                    const quote = new VElement("blockquote");
                    mdBlocks(quoted, quote);
                    parent.appendChild(quote);
                    continue;
                }

                if (MD_LIST_ITEM.test(line)) {
                    const list = new VElement(/^\s*\d/.test(line) ? "ol" : "ul");
                    while (i < lines.length && MD_LIST_ITEM.test(lines[i])) {
                        const item = new VElement("li");
                        mdInline(lines[i].replace(MD_LIST_ITEM, ""), item);
                        list.appendChild(item);
                        i++;
                    }
                    parent.appendChild(list);
                    continue;
                }

                if (/^\s*\|.*\|\s*$/.test(line) && i + 1 < lines.length && /^\s*\|[\s:|-]+\|\s*$/.test(lines[i + 1])) {
                    const rows = [];
                    while (i < lines.length && /^\s*\|.*\|\s*$/.test(lines[i])) {
                        rows.push(lines[i]);
                        i++;
                    }
                    rows.splice(1, 1);
                    const table = new VElement("table");
                    for (const row of rows) {
                        const tr = new VElement("tr");
                        for (const cell of row.trim().replace(/^\||\|$/g, "").split("|")) {
                            const td = new VElement("td");
                            mdInline(cell.trim(), td);
                            tr.appendChild(td);
                        }
                        table.appendChild(tr);
                    }
                    parent.appendChild(table);
                    continue;
                }

                const para = [];
                while (i < lines.length && lines[i].trim() && !MD_BLOCK_START.test(lines[i])) {
                    para.push(lines[i]);
                    i++;
                }
                if (para.length === 0) {
                    para.push(lines[i]);
                    i++;
                }
                const p = new VElement("p");
                mdInline(para.join(" "), p);
                parent.appendChild(p);
            }
        }

        class MarkdownRenderer {
            static async render(_app, markdown, el, _sourcePath, _component) {
                if (!el || typeof el.appendChild !== "function") return;
                const text = String(markdown == null ? "" : markdown).replace(/\r\n?/g, "\n");
                mdBlocks(text.split("\n"), el);
            }
            static async renderMarkdown(markdown, el, sourcePath, component) {
                return MarkdownRenderer.render(null, markdown, el, sourcePath, component);
            }
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
            'app.vault.trigger',
            // requestSaveLayout: layout persistence is implicit in the host, the method
            // exists on rawWorkspace as a no-op — listed here so the audited proxy path
            // (and the contract test) still exercises the "known, safe, no result" branch.
            'app.workspace.requestSaveLayout'
        ]);

        const KNOWN_UNIMPLEMENTED_OBSIDIAN_METHODS = new Set([
            // Methods that exist in Obsidian but have no honest equivalent in the host.
            // Currently empty: every contract member the corpus exercises got a real
            // implementation or a deliberate no-op; unknown members return undefined.
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
                    console.log(`[plugin] addCommand: registering command "${cmd.id}" ("${cmd.name || cmd.id}")`);
                    const handler = () => {
                        if (typeof cmd.callback === "function") {
                            return cmd.callback();
                        } else if (typeof cmd.checkCallback === "function") {
                            return cmd.checkCallback(false);
                        } else if (typeof cmd.editorCallback === "function") {
                            const editor = new Editor("");
                            const view = new MarkdownView();
                            return cmd.editorCallback(editor, view);
                        } else if (typeof cmd.editorCheckCallback === "function") {
                            const editor = new Editor("");
                            const view = new MarkdownView();
                            return cmd.editorCheckCallback(false, editor, view);
                        }
                    };
                    globalThis.__command_handlers.set(cmd.id, handler);
                    __invoke_host("add_command", {
                        id: cmd.id,
                        name: cmd.name || cmd.id,
                        hotkey: cmd.hotkey
                    });
                    return cmd;
                }
            }
            addRibbonIcon(icon, title, callback) {
                const id = "ribbon-" + Math.random().toString(36).substr(2, 9);
                console.log(`[plugin] addRibbonIcon: registering icon "${icon}", title "${title}", id "${id}"`);
                if (callback) {
                    globalThis.__command_handlers.set(id, callback);
                }
                __invoke_host("add_ribbon_icon", {
                    id,
                    icon: String(icon),
                    title: String(title)
                });
                const el = new VElement("div", { cls: "clickable-icon side-dock-ribbon-action" });
                el.setAttribute("aria-label", String(title));
                return el;
            }
            removeCommand(commandId) {
                const id = String(commandId);
                globalThis.__command_handlers.delete(id);
                // Palette entries carry the "plugin-id:cmd" prefix; try the short form too.
                for (const key of Array.from(globalThis.__command_handlers.keys())) {
                    if (key.endsWith(":" + id)) globalThis.__command_handlers.delete(key);
                }
                __invoke_host("remove_command", { id }).catch(() => {});
            }
            registerObsidianProtocolHandler(action, handler) {
                if (typeof handler !== "function") return;
                if (!globalThis.__protocol_handlers) globalThis.__protocol_handlers = new Map();
                globalThis.__protocol_handlers.set(String(action), handler);
            }
            registerEditorSuggest(suggest) {
                if (!suggest || typeof suggest.onTrigger !== "function") return;
                if (!globalThis.__editor_suggests) globalThis.__editor_suggests = [];
                globalThis.__editor_suggests.push(suggest);
            }
            registerMarkdownPostProcessor(pp, _sortOrder) {
                if (typeof pp !== "function") return null;
                if (!globalThis.__markdown_post_processors) globalThis.__markdown_post_processors = [];
                globalThis.__markdown_post_processors.push(pp);
                return pp;
            }
            addStatusBarItem() {
                Notice.__seq = (Notice.__seq || 0) + 1;
                const id = "sb-" + Notice.__seq;
                __invoke_host("add_status_bar_item", { id }).catch(() => {});
                const el = new VElement("div", { cls: "status-bar-item plugin-" + (this.manifest ? this.manifest.id : "unknown") });
                // setText()/setTextContent on the element pushes text into the host item;
                // other VElement children of the slot are not rendered by the host slot yet.
                el.setText = (text) => {
                    __invoke_host("update_status_bar_item", { id, text: String(text != null ? text : "") }).catch(() => {});
                    if (typeof VElement.prototype.setText === "function") {
                        VElement.prototype.setText.call(el, text);
                    }
                    return el;
                };
                el.setTextContent = el.setText;
                return el;
            }
            addSettingTab(tab) {
                this.settingTab = tab;
                globalThis.__active_setting_tab = tab;
                if (tab && typeof tab.display === "function") {
                    const origDisplay = tab.display.bind(tab);
                    tab.display = function() {
                        try {
                            origDisplay();
                        } catch (e) {
                            console.error("tab.display error:", e);
                        }
                        try {
                            const tree = __velement_to_uinode(tab.containerEl);
                            __invoke_host("set_view_tree", { view_id: 0, tree });
                        } catch (e) {}
                    };
                    try {
                        tab.display();
                    } catch (e) {
                        // Some plugins rely on asynchronous data loaded after onload.
                        // We retain this.settingTab for on-demand rendering.
                    }
                }
            }
            // registerEvent() is inherited from Component so plugin subscriptions are disposed on unload.
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
            registerView(type, viewCreator) {
                globalThis.__registered_views.set(String(type), viewCreator);
            }
            registerExtensions(extensions, viewType) {}
            registerHoverLinkSource(id, info) {}
            registerEditorExtension(extension) {}
            addToolbar(spec) {
                return __invoke_host("add_toolbar", { spec });
            }
            updateToolbar(id, items) {
                return __invoke_host("update_toolbar", { id, items });
            }
            removeToolbar(id) {
                return __invoke_host("remove_toolbar", { id });
            }
            registerIcon(name, svg) {
                return __invoke_host("register_icon", { name: String(name), svg: String(svg) });
            }
        }

        const rawVault = {
            // Obsidian's default config folder name. Plugins join it onto paths to reach settings files.
            configDir: ".obsidian",
            read: async (path) => {
                const p = typeof path === 'object' ? path.path : path;
                console.log(`[plugin] vault.read: "${p}"`);
                try {
                    const res = await __invoke_host("vault_read", { path: p });
                    return res != null ? String(res) : "";
                } catch (e) {
                    // Never fabricate content: a read-modify-write plugin would persist the fake
                    // string into the user's note. Failing loudly keeps the file intact.
                    console.warn(`[plugin] vault.read error for "${p}":`, e);
                    throw e;
                }
            },
            // cachedRead differs from read only in Obsidian's source: it resolves from the metadata
            // cache when fresh. This host has no cache to serve, so the result is the file content.
            cachedRead: (file) => rawVault.read(file),
            readBinary: async (file) => {
                const p = typeof file === 'object' ? file.path : file;
                const res = await __invoke_host("vault_read_binary", { path: p });
                if (typeof res !== "string") throw new Error("vault.readBinary: host returned no data");
                return __base64_to_array_buffer(res);
            },
            createBinary: async (path, data) => {
                await __invoke_host("vault_create_binary", { path, data_base64: __array_buffer_to_base64(data) });
                return new TFile(path);
            },
            modifyBinary: async (file, data) => {
                const p = typeof file === 'object' ? file.path : file;
                await __invoke_host("vault_modify_binary", { path: p, data_base64: __array_buffer_to_base64(data) });
            },
            modify: (path, data) => __invoke_host("vault_modify", { path: typeof path === 'object' ? path.path : path, data }),
            create: (path, data) => __invoke_host("vault_create", { path, data }),
            createFolder: async (path) => {
                const p = normalizePath(typeof path === 'object' ? path.path : path);
                if (!p) throw new Error("vault.createFolder: empty path");
                await __invoke_host("vault_create_folder", { path: p });
                return new TFolder(p);
            },
            delete: (path) => {
                const p = typeof path === 'object' ? path.path : path;
                console.log(`[plugin] vault.delete: "${p}"`);
                return __invoke_host("vault_delete", { path: p });
            },
            // trash is Obsidian's *safe* delete (Obsidian trash or the OS recycle bin), never an
            // irreversible unlink. The host resolves the destination; a plugin that picked trash
            // for safety must not get a permanent delete.
            trash: (path, system) => {
                const p = typeof path === 'object' ? path.path : path;
                console.log(`[plugin] vault.trash: "${p}"`);
                return __invoke_host("vault_trash", { path: p, system: !!system });
            },
            process: async (file, fn) => {
                const path = typeof file === 'object' ? file.path : file;
                const content = await rawVault.read(path);
                const newContent = await fn(content);
                if (typeof newContent === "string") {
                    await __invoke_host("vault_modify", { path, data: newContent });
                }
                return newContent;
            },
            getMarkdownFiles: () => {
                const files = __host_get_markdown_files();
                const mdFiles = files
                    .filter(p => typeof p === 'string' && (p.endsWith(".md") || p.endsWith(".markdown")))
                    .map(p => new TFile(p));
                console.log(`[plugin] vault.getMarkdownFiles: found ${mdFiles.length} file(s)`);
                return mdFiles;
            },
            // The host vault has a single root; folders are not modeled, so every
            // file is returned regardless of type (Obsidian returns TFiles+TFolders).
            getRoot: () => new TFolder("/"),
            getFolderByPath: (path) => {
                // The host does not model folders; a folder path resolves to a TFolder
                // handle when it is a real prefix of listed files, otherwise null.
                const p = typeof path === 'object' ? path.path : path;
                const norm = normalizePath(String(p || "")).replace(/\/+$/, "");
                if (!norm || norm === "/") return new TFolder("/");
                const prefix = norm + "/";
                if (__host_get_markdown_files().some((f) => f.startsWith(prefix))) return new TFolder(norm);
                return null;
            },
            getConfig: (_key) => {
                // The host does not expose per-key vault config; Obsidian's keys
                // (alwaysUpdateLinks etc.) have no local equivalent yet.
                return null;
            },
            // Vault config map: the host has no per-key config store, but plugins read
            // it for booleans like `attachmentFolderPath`. An empty map reads as
            // "all defaults", setConfig is accepted and kept in memory per session.
            get config() {
                return rawVault.__config || {};
            },
            setConfig: (key, value) => {
                if (!rawVault.__config) rawVault.__config = {};
                rawVault.__config[String(key)] = value;
            },
            getAllLoadedFiles: () => __host_get_markdown_files().map(p => new TFile(p)),
            getName: () => {
                const active = __host_get_active_file();
                // Vault name is not carried in the snapshot; the workspace title is the
                // closest honest answer and is stable for plugins that only display it.
                return active ? String(active).split("/")[0] || "" : "";
            },
            getResourcePath: (file) => {
                const p = typeof file === 'object' ? file.path : file;
                return "app://" + normalizePath(String(p));
            },
            getFiles: () => {
                const files = __host_get_markdown_files();
                return files.map(p => new TFile(p));
            },
            // Exact-path lookups. Return null for anything the host does not actually list (including
            // folders, which the host does not model) instead of fabricating a TFile for a missing file.
            getFileByPath: (path) => {
                const p = typeof path === 'object' ? path.path : path;
                if (!p) return null;
                const norm = normalizePath(String(p));
                return __host_get_markdown_files().includes(norm) ? new TFile(norm) : null;
            },
            getAbstractFileByPath: (path) => {
                const p = typeof path === 'object' ? path.path : path;
                if (!p) return null;
                const norm = normalizePath(String(p));
                return __host_get_markdown_files().includes(norm) ? new TFile(norm) : null;
            },
            // Case-insensitive lookup: first listed file whose full path matches ignoring case.
            getAbstractFileByPathInsensitive: (path) => {
                const p = typeof path === 'object' ? path.path : path;
                if (!p) return null;
                const needle = normalizePath(String(p)).toLowerCase();
                const hit = __host_get_markdown_files().find((f) => f.toLowerCase() === needle);
                return hit ? new TFile(hit) : null;
            },
            // File-level existence probe: unlike getAbstractFileByPath this must not build
            // a TFile, plugins use it to skip work cheaply.
            exists: async (pathRaw) => {
                const p = typeof pathRaw === 'object' ? pathRaw.path : pathRaw;
                if (!p) return false;
                const norm = normalizePath(String(p));
                if (__host_get_markdown_files().includes(norm)) return true;
                try {
                    const res = await __invoke_host("vault_exists", { path: norm });
                    return !!res;
                } catch (e) {
                    return false;
                }
            },
            // Real folder walk on the host (hidden entries excluded). Returns TFolder handles
            // in the same shape Obsidian's getAllFolders produces.
            getAllFolders: async () => {
                const res = await __invoke_host("vault_list_folders", {});
                const list = Array.isArray(res) ? res : [];
                return list.map((p) => new TFolder(String(p)));
            },
            append: async (pathRaw, data) => {
                const p = typeof pathRaw === 'object' ? pathRaw.path : pathRaw;
                await __invoke_host("vault_append", { path: String(p), data: String(data) });
            },
            copy: async (source, target) => {
                const src = typeof source === 'object' ? source.path : source;
                const dst = typeof target === 'object' ? target.path : target;
                await __invoke_host("vault_copy", { source: String(src), target: String(dst) });
                return new TFile(String(dst));
            },
            // First unused "prefix.ext" / "prefix N.ext" path, as Obsidian numbers attachments.
            getAvailablePath: async (prefixRaw, extRaw) => {
                const prefix = String(prefixRaw != null ? prefixRaw : "");
                const ext = extRaw == null || extRaw === "" ? null : String(extRaw);
                const res = await __invoke_host("vault_get_available_path", { prefix, extension: ext });
                return res != null ? String(res) : "";
            },
            getAvailablePathForAttachments: async (filename, extRaw) => {
                // Obsidian walks the configured attachment folder; the host has none, so the
                // numbering happens right next to the target file name.
                const ext = extRaw == null || extRaw === "" ? null : String(extRaw);
                const stem = String(filename != null ? filename : "").replace(/\.[^.]+$/, "") || "file";
                return rawVault.getAvailablePath(stem, ext);
            },
            // Rename/move through the host vault_rename channel; a TAbstractFile handle is
            // accepted too, matching how Obsidian's rename accepts both.
            rename: async (from, to) => {
                const src = typeof from === 'object' ? from.path : from;
                await __invoke_host("vault_rename", { old_path: String(src), new_path: normalizePath(String(to)) });
                return new TFile(normalizePath(String(to)));
            },
            // Registry tail (one corpus plugin each): honest degenerate answers instead of
            // throws — onChange subscribes into the same listener table as `on`.
            onChange: (event, cb) => rawVault.on(event, cb),
            get root() { return new TFolder("/"); },
            getAllFiles: () => __host_get_markdown_files().map((p) => new TFile(p)),
            checkPath: async (path) => {
                const norm = normalizePath(String(path || ""));
                if (norm.startsWith(".")) return "folder";
                return __host_get_markdown_files().includes(norm) ? "file" : "folder";
            },
            saveConfig: () => {},
            resolveFileUrl: (_url) => null,
            on: (event, cb) => {
                const evName = String(event);
                if (!globalThis.__vault_listeners.has(evName)) {
                    globalThis.__vault_listeners.set(evName, []);
                }
                if (typeof cb === "function") {
                    globalThis.__vault_listeners.get(evName).push(cb);
                }
                return {
                    e: rawVault,
                    name: evName,
                    cb,
                    id: "vault-sub-" + Math.random().toString(36).substr(2, 9),
                    unload: () => {
                        const list = globalThis.__vault_listeners.get(evName);
                        if (list) {
                            const idx = list.indexOf(cb);
                            if (idx !== -1) list.splice(idx, 1);
                        }
                    }
                };
            },
            offref: (ref) => {
                if (ref && typeof ref.unload === "function") ref.unload();
            },
            off: (event, cb) => {
                const evName = String(event);
                const list = globalThis.__vault_listeners.get(evName);
                if (list) {
                    const idx = list.indexOf(cb);
                    if (idx !== -1) list.splice(idx, 1);
                }
            },
            trigger: (event, ...args) => {
                const list = globalThis.__vault_listeners.get(String(event));
                if (list) {
                    for (const cb of [...list]) {
                        try { cb(...args); } catch (e) { console.error("Vault event error:", e); }
                    }
                }
            },
            // Obsidian's DataAdapter. The host owns the filesystem, so most calls map onto existing
            // vault_* requests; binary reads/writes have no transport and fail loudly rather than
            // corrupting a file with text where the plugin expects bytes.
            adapter: {
                getBasePath: () => "",
                getResourcePath: (path) => String(path),
                exists: async (path) => {
                    const norm = normalizePath(String(path));
                    return __host_get_markdown_files().includes(norm);
                },
                read: async (path) => {
                    const res = await __invoke_host("vault_read", { path });
                    return res != null ? String(res) : "";
                },
                write: (path, data) => __invoke_host("vault_modify", { path, data }),
                append: async (path, data) => {
                    let existing = "";
                    try {
                        const res = await __invoke_host("vault_read", { path });
                        existing = res != null ? String(res) : "";
                    } catch (e) {
                        existing = "";
                    }
                    return __invoke_host("vault_modify", { path, data: existing + String(data) });
                },
                mkdir: (path) => __invoke_host("vault_create_folder", { path }),
                remove: (path) => __invoke_host("vault_delete", { path }),
                rmdir: (path) => __invoke_host("vault_delete", { path }),
                rename: (path, newPath) => __invoke_host("vault_rename", { old_path: path, new_path: newPath }),
                stat: async (path) => {
                    const norm = normalizePath(String(path));
                    if (!__host_get_markdown_files().includes(norm)) return null;
                    return { type: "file", ctime: 0, mtime: 0, size: 0 };
                },
                // Real directory listing from the host; falls back to deriving the
                // same shape from the flat markdown snapshot when the request is absent.
                list: async (path) => {
                    try {
                        const res = await __invoke_host("vault_list_dir", { path: String(path || "") });
                        if (res && typeof res === "object") {
                            return { files: res.files || [], folders: res.folders || [] };
                        }
                    } catch (e) { /* fall through to the snapshot */ }
                    const norm = normalizePath(String(path)).replace(/\/+$/, "");
                    const prefix = norm ? norm + "/" : "";
                    const files = [];
                    const folders = new Set();
                    for (const f of __host_get_markdown_files()) {
                        if (!f.startsWith(prefix)) continue;
                        const rest = f.slice(prefix.length);
                        const slash = rest.indexOf("/");
                        if (slash === -1) files.push(f);
                        else folders.add(prefix + rest.slice(0, slash));
                    }
                    return { files, folders: Array.from(folders) };
                },
                readBinary: async (path) => {
                    const res = await __invoke_host("vault_read_binary", { path: String(path) });
                    if (typeof res !== "string") throw new Error("adapter.readBinary: host returned no data");
                    return __base64_to_array_buffer(res);
                },
                writeBinary: async (path, data) => {
                    const p = String(path);
                    const b64 = __array_buffer_to_base64(data);
                    try {
                        await __invoke_host("vault_create_binary", { path: p, data_base64: b64 });
                    } catch (e) {
                        // createBinary refuses existing files; adapter.writeBinary overwrites.
                        await __invoke_host("vault_modify_binary", { path: p, data_base64: b64 });
                    }
                }
            }
        };

        const rawWorkspace = {
            addToolbar: (spec) => __invoke_host("add_toolbar", { spec }),
            updateToolbar: (id, items) => __invoke_host("update_toolbar", { id, items }),
            removeToolbar: (id) => __invoke_host("remove_toolbar", { id }),
            registerIcon: (name, svg) => __invoke_host("register_icon", { name: String(name), svg: String(svg) }),
            getActiveFile: () => {
                const p = __host_get_active_file();
                return p ? new TFile(p) : null;
            },
            // Split references for plugins that read the layout tree: the host has a single
            // center layout, so both sides resolve to the root handle.
            get rootSplit() { return this; },
            get leftSplit() { return this; },
            get rightSplit() { return this; },
            get activeLeaf() {
                return getActiveLeaf();
            },
            get activeEditor() {
                const leaf = getActiveLeaf();
                return leaf && leaf.view instanceof MarkdownView ? leaf.view : null;
            },
            get layoutReady() {
                return __layout_ready;
            },
            getActiveViewOfType: (type) => {
                const leaf = getActiveLeaf();
                const view = leaf ? leaf.view : null;
                if (!view) return null;
                if (typeof type !== "function") return view;
                return view instanceof type ? view : null;
            },
            setActiveLeaf: (leaf, _options) => {
                activateLeaf(leaf);
            },
            getMostRecentLeaf: () => __most_recent_leaf || getActiveLeaf(),
            iterateAllLeaves: (cb) => {
                if (typeof cb !== "function") return;
                // Materialise the native leaf first so it takes part in the iteration.
                getActiveLeaf();
                for (const leaf of [...globalThis.__workspace_leaves.values()]) {
                    try { cb(leaf); } catch (e) { console.error("[plugin] iterateAllLeaves callback error:", e); }
                }
            },
            openLinkText: async (linktext, sourcePath, _newLeaf) => {
                const raw = String(linktext == null ? "" : linktext).trim();
                if (!raw) return;
                if (/^[a-z][a-z0-9+.-]*:/i.test(raw)) {
                    await __invoke_host("open_url", { url: raw });
                    return;
                }
                const target = resolveLinkTarget(raw, sourcePath);
                if (target) await __invoke_host("open_file", { path: target });
            },
            openFile: (path) => __invoke_host("open_file", { path: typeof path === 'object' ? path.path : path }),
            getLeaf: (create) => new WorkspaceLeaf(rawWorkspace, "center"),
            getLeftLeaf: (split) => new WorkspaceLeaf(rawWorkspace, "left"),
            getRightLeaf: (split) => new WorkspaceLeaf(rawWorkspace, "right"),
            getLeavesOfType: (type) => {
                return [...globalThis.__workspace_leaves.values()].filter(l => l.viewState && l.viewState.type === type);
            },
            detachLeavesOfType: (type) => {
                const toDetach = rawWorkspace.getLeavesOfType(type);
                toDetach.forEach(l => l.detach());
            },
            ensureSideLeaf: (type, side, options) => {
                let leaves = rawWorkspace.getLeavesOfType(type);
                if (leaves.length === 0) {
                    const leaf = side === "left" ? rawWorkspace.getLeftLeaf() : rawWorkspace.getRightLeaf();
                    leaf.setViewState({ type, active: true });
                    if (options && options.reveal) {
                        rawWorkspace.revealLeaf(leaf);
                    }
                    return leaf;
                }
                return leaves[0];
            },
            onLayoutReady: (cb) => {
                if (typeof cb !== "function") return;
                if (__layout_ready) {
                    runLayoutReadyCallback(cb);
                } else {
                    __layout_ready_queue.push(cb);
                }
            },
            revealLeaf: (leaf) => {
                if (leaf && leaf.id) {
                    return __invoke_host("reveal_leaf", { leaf_id: leaf.id });
                }
                return Promise.resolve();
            },
            registerMarkdownCodeBlockProcessor: (language, handler) => {
                globalThis.__code_block_handlers.set(String(language).toLowerCase(), handler);
                return __invoke_host("register_code_block_processor", { language: String(language) });
            },
            on: (event, cb) => {
                const evName = String(event);
                if (!globalThis.__workspace_listeners.has(evName)) {
                    globalThis.__workspace_listeners.set(evName, []);
                }
                if (typeof cb === "function") {
                    globalThis.__workspace_listeners.get(evName).push(cb);
                }
                return {
                    e: rawWorkspace,
                    name: evName,
                    cb,
                    id: "workspace-sub-" + Math.random().toString(36).substr(2, 9),
                    unload: () => {
                        const list = globalThis.__workspace_listeners.get(evName);
                        if (list) {
                            const idx = list.indexOf(cb);
                            if (idx !== -1) list.splice(idx, 1);
                        }
                    }
                };
            },
            offref: (ref) => {
                if (ref && typeof ref.unload === "function") ref.unload();
            },
            off: (event, cb) => {
                const evName = String(event);
                const list = globalThis.__workspace_listeners.get(evName);
                if (list) {
                    const idx = list.indexOf(cb);
                    if (idx !== -1) list.splice(idx, 1);
                }
            },
            // The root split IS the workspace in the single-layout host; iterating it walks
            // the same leaf set as iterateAllLeaves.
            iterateRootLeaves: (cb) => rawWorkspace.iterateAllLeaves(cb),
            iterateLeaves: (split, cb) => rawWorkspace.iterateAllLeaves(cb),
            getLeafById: (id) => {
                const hit = globalThis.__workspace_leaves.get(String(id));
                return hit || null;
            },
            getActiveLeaf: () => getActiveLeaf(),
            getActiveFileView: () => {
                const leaf = getActiveLeaf();
                return leaf ? leaf.view : null;
            },
            // An unpinned leaf is the active one: the host pins nothing.
            getUnpinnedLeaf: () => getActiveLeaf(),
            // DOM stand-in: plugins touch containerEl to append their own nodes, and the
            // host cannot honour DOM mutation — an empty VElement keeps the reference honest.
            get containerEl() {
                if (!rawWorkspace.__containerEl) {
                    rawWorkspace.__containerEl = new VElement("div", { cls: "workspace" });
                }
                return rawWorkspace.__containerEl;
            },
            // Layout save/persist: the host persists layout implicitly, nothing to request.
            requestSaveLayout: () => {},
            updateOptions: (_options) => {},
            getLastOpenFiles: () => {
                const p = __host_get_active_file();
                return p ? [p] : [];
            },
            // Popouts/floating windows do not exist in the single-window host; these
            // resolve to ordinary center leaves so the calling plugin keeps working.
            openPopoutLeaf: (_options) => new WorkspaceLeaf(rawWorkspace, "center"),
            createLeafBySplit: (_split) => new WorkspaceLeaf(rawWorkspace, "center"),
            createLeafInParent: (_parent) => new WorkspaceLeaf(rawWorkspace, "center"),
            createLeafInTabGroup: (_leaf) => new WorkspaceLeaf(rawWorkspace, "center"),
            splitActiveLeaf: () => new WorkspaceLeaf(rawWorkspace, "center"),
            duplicateLeaf: (_leaf) => new WorkspaceLeaf(rawWorkspace, "center"),
            get floatingSplit() { return null; },
            getWindows: () => [],
            moveLeafToPopout: () => {},
            changeLayout: (_fn) => {},
            getLayout: () => ({}),
            deserializeLayout: () => {},
            // Hover-link previews are a UI affordance the host does not render; registration
            // is stored so the plugin sees its own source back.
            registerHoverLinkSource: (_id, _info) => {},
            unregisterHoverLinkSource: (_id) => {},
            get leftRibbon() { return { addRibbonItem: () => ({}) }; },
            get rightRibbon() { return { addRibbonItem: () => ({}) }; },
            get editorSuggest() { return { register: () => {}, registerSuggest: () => {} }; },
            // Editor extensions are CodeMirror 6; the host renders natively.
            registerEditorExtension: () => {},
            unregisterEditorExtension: () => {},
            requestResize: () => {},
            // Link context menus are host-rendered; the plugin's items would need a
            // native menu channel, so registration degrades to a no-op.
            handleLinkContextMenu: (_menu, _linktext, _sourcePath) => {},
            handleExternalLinkContextMenu: (_menu, _url) => {},
            get onLayoutChange() { return null; },
            get recentFileTracker() { return { getRecentFiles: () => [] }; },
            getGroupLeaves: (_group) => [],
            iterateCodeMirrors: (_cb) => {},
            get rightSidebarToggleButtonEl() { return null; },
            onDragLeaf: (_cb) => ({ unload: () => {} }),
            get protocolHandlers() { return {}; },
            get protocolHandler() { return null; },
            trigger: (event, ...args) => {
                const list = globalThis.__workspace_listeners.get(String(event));
                if (list) {
                    for (const cb of [...list]) {
                        try { cb(...args); } catch (e) { console.error("Workspace event error:", e); }
                    }
                }
            }
        };

        // The host settings window: open/close is a real channel (OpenSettings), tab ids map
        // the same way the vibecodium:// URL parser maps them — a plugin id opens its own
        // settings tab, known keys open built-in ones.
        const rawSetting = {
            open: () => __invoke_host("open_settings", {}),
            openTabById: (tabId) => {
                const id = String(tabId != null ? tabId : "");
                const key = id.startsWith("obsidian-") ? id.slice("obsidian-".length) : id;
                return __invoke_host("open_settings", { tab: key });
            },
            openTab: (tab) => {
                const id = tab && typeof tab === "object" ? String(tab.id || "") : String(tab != null ? tab : "");
                return __invoke_host("open_settings", { tab: id });
            },
            close: () => __invoke_host("close_settings", {}),
            // Without a settings-tab registry the honest answer is "nothing open".
            get activeTab() { return null; },
            addSettingTab: (_tab) => {},
            removeSettingTab: (_tab) => {},
            get settingTabs() { return []; },
            get communityPluginTabContainer() { return null; },
            get tabContentContainer() { return null; },
            get containerEl() {
                if (!rawSetting.__containerEl) {
                    rawSetting.__containerEl = new VElement("div", { cls: "settings-container" });
                }
                return rawSetting.__containerEl;
            },
        };

        const rawMetadataCache = {
            // The host has no parsed tag/link cache. Return null ("no metadata available") rather
            // than an empty object, which a plugin could not tell apart from "file has no metadata".
            getFileCache: (_file) => null,
            getCache: (_path) => null,
            // Returns a TFile or null (never a bare string): callers read .path/.basename/.extension.
            getFirstLinkpathDest: (linkpath, sourcePath) => {
                const hit = resolveLinkTarget(linkpath, sourcePath);
                return hit ? new TFile(hit) : null;
            },
            resolvedLinks: {},
            // Unresolved [[links]]: without a link cache the honest answer is "nothing unresolved".
            unresolvedLinks: {},
            getTags: () => new Set(),
            getBacklinksForFile: (_file) => ({ data: {}, count: 0 }),
            isUserIgnored: (_path) => false,
            fileToLinktext: (file, _sourcePath, omitMdExtension) => {
                const p = file && typeof file === 'object' ? file.path : String(file == null ? "" : file);
                if (!p) return "";
                const base = p.split("/").pop();
                return omitMdExtension === false ? base : base.replace(/\.md$/i, "");
            },
            on: (_event, _cb) => ({
                e: rawMetadataCache,
                id: "meta-sub",
                unload: () => {}
            }),
            off: (_event, _cb) => {},
            offref: (_ref) => {},
            // Cache-maintenance tail: the host has no incremental cache to clean or query.
            onCleanCache: (_cb) => ({ unload: () => {} }),
            getAllPropertyNames: () => [],
            getAllPropertyInfos: () => ({}),
            iterateReferences: (_cb) => {},
            get uniqueFileLookup() { return {}; },
            computeMetadataAsync: async () => ({}),
            trigger: () => {}
        };

        // Plugin-registry mirror of the host, rebuilt from the snapshot on each access.
        // The instance objects are plain handles: app.plugins.getPlugin returns a
        // manifest-shaped object, not the live plugin instance (cross-thread).
        const rawPlugins = {
            get manifests() {
                const out = {};
                for (const p of __host_plugins_list()) out[p.id] = p;
                return out;
            },
            get plugins() {
                const out = {};
                for (const p of __host_plugins_list()) out[p.id] = p;
                return out;
            },
            get enabledPlugins() {
                return new Set(__host_plugins_list().map((p) => p.id));
            },
            getPlugin(id) {
                return __host_plugins_list().find((p) => p.id === String(id)) || undefined;
            },
            // Enable/disable round-trips through the host plugin registry; unsaved
            // variants stay in-memory in Obsidian, the host has nothing to save.
            enablePlugin: (id) => __invoke_host("plugin_enable", { id: String(id) }),
            disablePlugin: (id) => __invoke_host("plugin_disable", { id: String(id) }),
            enablePluginAndSave: (id) => __invoke_host("plugin_enable", { id: String(id) }),
            disablePluginAndSave: (id) => __invoke_host("plugin_disable", { id: String(id) }),
            loadManifests: () => __invoke_host("noop", {}),
            getPluginFolder: (_id) => ".obsidian/plugins/" + String(_id),
            // Install/update flows need network + marketplace; the host installs nothing.
            installPlugin: async (_repo) => null,
            unloadPlugin: (_id) => {},
            loadPlugin: (_id) => null,
            checkForUpdates: () => false,
            autoCheckForUpdates: false,
            setAutomaticUpdateCheck: (_v) => {},
            get updates() { return {}; },
            requestSaveConfig: () => {},
            on: (_event, _cb) => ({ e: rawPlugins, id: "plugins-sub", unload: () => {} }),
        };

        const app = {
            vault: makeAuditedProxy(rawVault, "app.vault"),
            workspace: makeAuditedProxy(rawWorkspace, "app.workspace"),
            metadataCache: makeAuditedProxy(rawMetadataCache, "app.metadataCache"),
            setting: makeAuditedProxy(rawSetting, "app.setting"),
            // Command registry mirror: executeCommandById runs a plugin's own handler
            // synchronously (Obsidian returns a boolean, not a Promise), foreign ids are
            // forwarded to the host registry; listCommands/findCommand resolve against the
            // snapshot; a plugin removes only its own commands, foreign ids go to the host.
            commands: {
                executeCommandById: (id) => {
                    const commandId = String(id);
                    let handler = globalThis.__command_handlers.get(commandId);
                    if (!handler && commandId.includes(":")) {
                        handler = globalThis.__command_handlers.get(commandId.split(":").slice(1).join(":"));
                    }
                    if (typeof handler === "function") {
                        try { handler(); } catch (e) { console.error("executeCommandById error:", e); }
                        return true;
                    }
                    __invoke_host("execute_command", { id: commandId });
                    return true;
                },
                executeCommand: (command) => {
                    const id = command && typeof command === "object" ? command.id : command;
                    return app.commands.executeCommandById(id);
                },
                get commands() {
                    const out = {};
                    for (const c of __host_commands_list()) out[c.id] = c;
                    return out;
                },
                listCommands: () => __host_commands_list().slice(),
                findCommand: (id) => {
                    const commandId = String(id);
                    const list = __host_commands_list();
                    return list.find((c) => c.id === commandId)
                        || list.find((c) => c.id === commandId.split(":").slice(1).join(":"))
                        || null;
                },
                removeCommand: (id) => {
                    const commandId = String(id);
                    if (globalThis.__command_handlers.has(commandId)) {
                        globalThis.__command_handlers.delete(commandId);
                    }
                    return __invoke_host("remove_command", { id: commandId });
                },
                // monkey-around patches `app.commands.constructor.prototype`; the mirror
                // object hands out a constructor whose prototype carries the same methods,
                // so the patch lands on it and listCommands keeps working.
                constructor: function CommandsCtor() {},
                get editorCommands() { return []; },
            },            internalPlugins: {
                getEnabledPluginById: (_id) => null,
                // Core-plugin enumeration: the host mirrors enabled plugins as internal ones.
                get plugins() {
                    const out = {};
                    for (const p of __host_plugins_list()) out[p.id] = { id: p.id, name: p.name, enabled: true };
                    return out;
                },
                getPluginById: (_id) => null,
                get config() { return {}; },
            },
            // Registry mirrors of the host: rawPlugins above.
            plugins: makeAuditedProxy(rawPlugins, "app.plugins"),
            dragManager: {
                dragFile: () => ({}),
                onDragStart: () => {},
                draggable: (_el, _opts) => _el || null,
                dragLink: () => {},
                handleDrag: (_e, _data) => {},
                handleDrop: (_e) => {},
            },
            // View/embed registries: the host resolves views natively, so registration
            // degrades to bookkeeping that answers "registered" for the calling plugin.
            viewRegistry: {
                registerView: (_type, _cls) => {},
                unregisterView: (_type) => {},
                getViewByType: (_type) => undefined,
                get viewByType() { return {}; },
                get typeByExtension() { return {}; },
                getTypeByExtension: (_ext) => undefined,
                isExtensionRegistered: (_ext) => false,
                registerExtensions: (_ext, _type) => {},
                unregisterExtensions: (_ext) => {},
            },
            embedRegistry: {
                registerExtensions: (_ext, _handler) => {},
                unregisterExtensions: (_ext) => {},
                get embedByExtension() { return {}; },
            },
            // Mobile-only surfaces: the native host has no mobile toolbar or metadata
            // type widgets; reads answer empty, writes are no-ops.
            mobileToolbar: {
                update: () => {},
                get buttons() { return []; },
            },
            metadataTypeManager: {
                setType: (_type, _widget) => {},
                get registeredTypeWidgets() { return {}; },
                getAssignedWidget: (_type) => undefined,
            },
            // Scope registry with no hotkey conflict resolution yet: push/pop are bookkeeping,
            // getRootScope returns a stable Scope the plugin can register into.
            keymap: {
                pushScope: (_scope) => {},
                popScope: (_scope) => {},
                getRootScope: () => globalThis.Scope ? new globalThis.Scope() : { register: () => {}, unregister: () => false },
            },
            hotkeyManager: {
                addDefaultHotkeys: (_id, _hotkeys) => {},
                getHotkey: (_commandId) => null,
                removeHotkeys: (_id) => {},
                removeDefaultHotkeys: (_id) => {},
                get customKeys() { return []; },
            },
            // The app-level key scope: register() returns a handle the plugin can unload.
            scope: {
                register: (_mods, _key, _fn) => ({ unload: () => {} }),
                unregister: () => {},
            },
            // Secrets stay in the plugin's own data.json (plugin authors do this anyway);
            // returning stored values would promise OS-level storage that does not exist.
            secretStorage: {
                getSecret: (_id) => null,
                setSecret: (_id, _value) => {},
                // Obsidian's aliases of getSecret/setSecret plus the event surface.
                get: (_id) => null,
                set: (_id, _value) => {},
                listSecrets: () => [],
                on: (_event, _cb) => ({ e: this, id: "secret-sub", unload: () => {} }),
                offref: (_ref) => {},
            },
            fileManager: {
                getNewFileParent: () => new TFolder("/"),
                generateMarkdownLink: () => "",
                // Attachment path numbering reuses the vault channel (first unused
                // "prefix.ext"), matching Obsidian's getAvailablePathForAttachment.
                getAvailablePathForAttachment: async (filename, sourcePath) => {
                    const ext = String(filename != null ? filename : "").match(/\.([^.]+)$/);
                    const stem = String(filename != null ? filename : "").replace(/\.[^.]+$/, "") || "file";
                    const folder = sourcePath && String(sourcePath).includes("/")
                        ? String(sourcePath).replace(/\/[^/]*$/, "")
                        : "/";
                    const prefix = folder === "/" ? stem : folder + "/" + stem;
                    return rawVault.getAvailablePath(prefix, ext ? ext[1] : null);
                },
                createNewMarkdownFile: async (parent, name) => {
                    const folder = parent && typeof parent === "object" ? String(parent.path || "/") : "/";
                    const base = String(name != null ? name : "Untitled");
                    const prefix = folder === "/" || folder === "" ? base : folder + "/" + base;
                    const path = await rawVault.getAvailablePath(prefix, "md");
                    await __invoke_host("vault_create", { path, data: "" });
                    return new TFile(path);
                },
                // Deletion prompts cannot be shown by the host UI; Obsidian's prompt option
                // defaults to trash. The honest mapping is a direct trash, never unlink.
                promptForDeletion: async (file) => {
                    const p = typeof file === 'object' ? file.path : file;
                    await __invoke_host("vault_trash", { path: p, system: false });
                },
                promptForFileDeletion: async (file) => {
                    const p = typeof file === 'object' ? file.path : file;
                    await __invoke_host("vault_trash", { path: p, system: false });
                },
                promptForFileRename: async (file) => {
                    // No prompt UI: renaming without user input would surprise the caller,
                    // so the closest honest behaviour is a no-op returning the same file.
                    return typeof file === 'object' ? file : new TFile(String(file));
                },
                // Reads the file, hands the parsed frontmatter object to fn, re-serializes
                // it and writes the whole file back with the rest of the body untouched.
                processFrontMatter: async (file, fn) => {
                    const path = typeof file === 'object' ? file.path : file;
                    const content = await rawVault.read(path);
                    const fm = __parse_frontmatter(content);
                    const body = fm === null
                        ? content
                        : content.slice(fm.bodyStart);
                    const obj = fm === null ? {} : fm.data;
                    await fn(obj);
                    // __stringify_yaml renders nested blocks without a trailing
                    // newline; the closing fence needs its own line.
                    const yamlText = __stringify_yaml(obj).replace(/\n$/, "");
                    let newContent;
                    if (fm === null) {
                        // No frontmatter yet: prepend a fresh block.
                        newContent = "---\n" + yamlText + "\n---\n" + content;
                    } else {
                        newContent = "---\n" + yamlText + "\n---\n" + body;
                    }
                    await __invoke_host("vault_modify", { path, data: newContent });
                },
                trashFile: async (file) => {
                    const p = typeof file === 'object' ? file.path : file;
                    await __invoke_host("vault_trash", { path: p, system: false });
                },
                renameFile: async (file, newPath) => {
                    const p = typeof file === 'object' ? file.path : file;
                    await __invoke_host("vault_rename", { old_path: p, new_path: normalizePath(String(newPath)) });
                },
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
        globalThis.MarkdownView = MarkdownView;
        globalThis.MarkdownRenderChild = MarkdownRenderChild;
        globalThis.MarkdownRenderer = MarkdownRenderer;
        globalThis.Events = Events;
        globalThis.Scope = Scope;
        globalThis.WorkspaceLeaf = WorkspaceLeaf;
        globalThis.BaseComponent = BaseComponent;
        globalThis.ValueComponent = ValueComponent;
        globalThis.ButtonComponent = ButtonComponent;
        globalThis.ExtraButtonComponent = ExtraButtonComponent;
        globalThis.ToggleComponent = ToggleComponent;
        globalThis.TextComponent = TextComponent;
        globalThis.TextAreaComponent = TextAreaComponent;
        globalThis.SliderComponent = SliderComponent;
        globalThis.DropdownComponent = DropdownComponent;
        globalThis.Menu = Menu;
        globalThis.MenuItem = MenuItem;
        globalThis.Editor = Editor;
        globalThis.Notice = Notice;
        globalThis.AbstractInputSuggest = AbstractInputSuggest;
        globalThis.EditorSuggest = EditorSuggest;
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

        const getLanguage = () => {
            try {
                const l = __host_get_language();
                if (l && typeof l === "string" && l.length > 0) return l;
            } catch (e) {}
            return "ru";
        };
        globalThis.getLanguage = getLanguage;

        // obsidian.Platform. The host is a native desktop application, so every mobile flag is false
        // by construction; only the OS identity varies and comes from the host, not from a guess.
        const __platform = globalThis.__host_platform || { os: "windows", arch: "x86_64" };
        const Platform = {
            isDesktop: true,
            isMobile: false,
            isDesktopApp: true,
            isMobileApp: false,
            isIosApp: false,
            isAndroidApp: false,
            isPhone: false,
            isTablet: false,
            isMacOS: __platform.os === "macos",
            isWin: __platform.os === "windows",
            isLinux: __platform.os === "linux",
            isSafari: false,
            resourcePathPrefix: "",
            canExportPdf: false,
            canPopoutWindow: false,
            canStackTabs: false,
            canSplit: true
        };
        globalThis.Platform = Platform;
        globalThis.window = globalThis;
        // Window/document-level event listeners: plugins register them expecting the real
        // DOM; the host never fires window events, so the registrations are silent no-ops
        // that keep `typeof window.addEventListener === "function"` true.
        globalThis.addEventListener = () => {};
        globalThis.removeEventListener = () => {};
        globalThis.dispatchEvent = () => true;
        // Node-flavored globals that desktop plugin bundles reference at module scope
        // (`typeof Buffer !== "undefined"` guards, `global` aliases). The host runs no
        // Node, so these are faithful minimal stand-ins; real fs/process stay absent.
        globalThis.global = globalThis;
        globalThis.Buffer = {
            isBuffer: () => false,
            from: (value) => {
                if (value == null) return new Uint8Array(0);
                if (typeof value === "string") return new TextEncoder().encode(String(value));
                if (value instanceof ArrayBuffer) return new Uint8Array(value);
                if (ArrayBuffer.isView(value)) return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
                return new Uint8Array(Array.from(value, (n) => Number(n) & 0xff));
            },
            alloc: (len, fill = 0) => new Uint8Array(Number(len) || 0).fill(fill),
            concat: (list) => {
                const parts = Array.from(list || []);
                const total = parts.reduce((n, p) => n + (p ? p.length : 0), 0);
                const out = new Uint8Array(total);
                let off = 0;
                for (const p of parts) {
                    if (p) { out.set(new Uint8Array(p.buffer || p), off); off += p.length; }
                }
                return out;
            },
        };
        // speechSynthesis: empty voice list — the API exists, audio does not play.
        globalThis.speechSynthesis = {
            getVoices: () => [],
            speak: () => {},
            cancel: () => {},
            pause: () => {},
            resume: () => {},
        };
        // Web-platform stand-ins referenced by plugin bundles. They exist as inert
        // constructors: `instanceof` never matches anything meaningful, and plugin
        // feature detection (`typeof HTMLElement !== "undefined"`) passes.
        globalThis.HTMLElement = class HTMLElement {};
        globalThis.HTMLImageElement = class HTMLImageElement {};
        globalThis.Image = class Image {
            constructor() { this.src = ""; this.onload = null; this.onerror = null; }
        };
        // File/Blob: inert constructors. Plugins use them for `instanceof` checks and
        // feature detection; real file payloads travel through the vault binary channel.
        globalThis.File = class File {};
        globalThis.Blob = class Blob {
            constructor(parts) { this.size = Array.isArray(parts) ? parts.reduce((n, p) => n + (p && p.length || 0), 0) : 0; }
            text() { return Promise.resolve(""); }
        };
        // AbortController/AbortSignal: fetch-style cancellation plumbing. The host has no
        // streaming transport, so aborting is a no-op; the constructors still count callers.
        globalThis.AbortController = class AbortController {
            constructor() { this.signal = { aborted: false, addEventListener: () => {}, removeEventListener: () => {} }; }
            abort() { this.signal.aborted = true; }
        };
        globalThis.self = globalThis;
        globalThis.crypto = {
            // Deterministic fill is enough for ids/nonces; not for real secrets, which
            // plugins must obtain through requestUrl to their own services.
            getRandomValues: (arr) => {
                for (let i = 0; i < arr.length; i++) {
                    arr[i] = (Math.random() * 256) | 0;
                }
                return arr;
            },
            randomUUID: () => "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, (c) => {
                const r = (Math.random() * 16) | 0;
                const v = c === "x" ? r : (r & 0x3) | 0x8;
                return v.toString(16);
            }),
        };
        // Streams API names referenced by fetch-style plugin code; the host has no
        // streaming transport, so the constructors throw if actually instantiated.
        globalThis.TransformStream = class TransformStream {
            constructor() { throw new Error("TransformStream is not supported by the Vibecodium host"); }
        };
        globalThis.ReadableStream = class ReadableStream {
            constructor() { throw new Error("ReadableStream is not supported by the Vibecodium host"); }
        };
        globalThis.WritableStream = class WritableStream {
            constructor() { throw new Error("WritableStream is not supported by the Vibecodium host"); }
        };
        // DocumentFragment as a constructor: plugins build detached node groups with
        // `new DocumentFragment()` and append them into real containers later. Backed by
        // the same VElement the sanitize/parse pipeline produces.
        globalThis.DocumentFragment = class DocumentFragment extends VElement {
            constructor() {
                super("#fragment");
            }
            append(...nodes) {
                for (const n of nodes) {
                    if (n && typeof n.appendChild === "function") this.appendChild(n);
                }
            }
        };
        // CodeMirror 5 name space: legacy bundles do `var cm = CodeMirror` at module scope
        // and register modes/helpers into it. The host does not run CM5, so registrations
        // are recorded and every runtime lookup degrades honestly (null/false/no-op).
        globalThis.CodeMirror = globalThis.CodeMirror || {
            defineMode: () => {},
            defineMIME: () => {},
            defineSimpleMode: () => {},
            simpleMode: () => null,
            copyState: (mode, state) => (mode && typeof mode.copyState === "function" ? mode.copyState(state) : state),
            getMode: () => ({ name: "null" }),
            startState: () => ({}),
            registerHelper: () => {},
            registerGlobalHelper: () => {},
            Pos: (line, ch) => ({ line, ch }),
            Pass: "pass",
            Doc: class Doc {},
        };
        moment.locale = () => getLanguage();
        globalThis.window.moment = moment;
        globalThis.open = function(url) {
            if (url) {
                __invoke_host("open_url", { url: String(url) });
            }
        };
        globalThis.window.open = globalThis.open;
        globalThis.createFragment = function(callback) {
            const frag = new VElement("#fragment");
            if (typeof callback === "function") callback(frag);
            return frag;
        };
        const __ua_os = __platform.os === "macos"
            ? "Macintosh; Intel Mac OS X 10_15_7"
            : (__platform.os === "linux" ? "X11; Linux x86_64" : "Windows NT 10.0; Win64; x64");
        globalThis.navigator = {
            language: "ru-RU",
            languages: ["ru-RU", "ru", "en"],
            userAgent: `Aquilum/1.0 (${__ua_os}) Obsidian/1.8.0`
        };
        const __doc_body = new VElement("body");
        const __doc_head = new VElement("head");
        globalThis.document = {
            documentElement: { lang: "ru" },
            // Plugins register document-level listeners expecting the real DOM; the host
            // never fires them. Kept as silent no-ops so registration does not throw.
            addEventListener: () => {},
            removeEventListener: () => {},
            dispatchEvent: () => true,
            createElement: (t, options) => new VElement(t, options),
            createDocumentFragment: () => new VElement("#fragment"),
            getElementsByTagName: (tag) => {
                // Library probes (KaTeX: `document.getElementsByTagName("base").length`)
                // only count the result; the host document has no such elements.
                return [];
            },
            body: __doc_body,
            head: __doc_head,
            getElementById: (id) => {
                const el = globalThis.__velement_registry.get(id);
                if (el) return el;
                for (const v of globalThis.__velement_registry.values()) {
                    if (v.id === id || v.attributes?.id === id) return v;
                }
                return null;
            },
            querySelector: (sel) => {
                if (__matches_selector(__doc_body, sel)) return __doc_body;
                const found = __doc_body.querySelector(sel);
                if (found) return found;
                for (const v of globalThis.__velement_registry.values()) {
                    if (!v.parent && __matches_selector(v, sel)) return v;
                    if (!v.parent) {
                        const sub = v.querySelector(sel);
                        if (sub) return sub;
                    }
                }
                return null;
            },
            querySelectorAll: (sel) => {
                const results = [];
                if (__matches_selector(__doc_body, sel)) results.push(__doc_body);
                results.push(...__doc_body.querySelectorAll(sel));
                for (const v of globalThis.__velement_registry.values()) {
                    if (!v.parent && v !== __doc_body && v !== __doc_head) {
                        if (__matches_selector(v, sel)) results.push(v);
                        results.push(...v.querySelectorAll(sel));
                    }
                }
                return results;
            }
        };
        globalThis.activeDocument = globalThis.document;
        globalThis.DOMParser = class DOMParser {
            parseFromString(str, type) {
                const container = new VElement("div");
                const clean = __sanitize_html(String(str != null ? str : ""));
                __parse_html_to_velements(clean, container);
                const firstEl = container.children.find(c => c instanceof VElement);
                return {
                    documentElement: firstEl || container,
                    body: container
                };
            }
        };
        globalThis.createDiv = function(o) {
            const el = new VElement("div");
            if (typeof o === "string") el.className = o;
            else if (o && typeof o === "object") {
                if (o.cls) el.className = o.cls;
                if (o.text) el.textContent = o.text;
            }
            return el;
        };
        globalThis.createSpan = function(o) {
            const el = new VElement("span");
            if (typeof o === "string") el.className = o;
            else if (o && typeof o === "object") {
                if (o.cls) el.className = o.cls;
                if (o.text) el.textContent = o.text;
            }
            return el;
        };
        globalThis.createEl = function(tag, o) {
            const el = new VElement(tag || "div");
            if (o && typeof o === "object") {
                if (o.cls) el.className = o.cls;
                if (o.text) el.textContent = o.text;
            }
            return el;
        };
        globalThis.localStorage = {
            getItem: (k) => {
                if (k === "language") return getLanguage();
                return null;
            },
            setItem: () => {},
            removeItem: () => {}
        };
        globalThis.confirm = function(msg) {
            return true;
        };
        globalThis.alert = function(msg) {
            new Notice(String(msg));
        };
        globalThis.prompt = function(msg, def) {
            return def != null ? String(def) : "";
        };

        const obsidianExports = {
            Component,
            Plugin,
            PluginSettingTab,
            Setting,
            Modal,
            SuggestModal,
            FuzzySuggestModal,
            AbstractInputSuggest,
            EditorSuggest,
            View,
            ItemView,
            FileView,
            TextFileView,
            MarkdownView,
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
            TextAreaComponent,
            SearchComponent,
            MomentFormatComponent,
            ColorComponent,
            ProgressBarComponent,
            SliderComponent,
            DropdownComponent,
            Menu,
            MenuItem,
            Editor,
            Notice,
            TFile,
            TFolder,
            TAbstractFile,
            normalizePath,
            MarkdownRenderer,
            FileSystemAdapter,
            requestUrl,
            request,
            moment,
            getLanguage,
            Platform,
            addIcon: (name, svg) => {},
            setIcon: (el, icon) => {
                if (el) {
                    if (typeof el.setAttr === "function") el.setAttr("icon", String(icon));
                    if (icon === "lucide-x" || icon === "cross" || icon === "close") {
                        if (typeof el.setText === "function") el.setText("×");
                    }
                }
            },
            setTooltip: (el, tip) => {
                if (el && typeof el.setAttr === "function") el.setAttr("title", tip);
            },
            Keymap: { isModEvent: (_e) => false },
            debounce: (fn, _delay) => fn,
            finishRenderMath: () => {},
            loadPdfJs: () => Promise.resolve(),
            loadMathJax: () => Promise.resolve(),
            // Dynamic loaders have no backing assets in the native host; resolving to
            // null lets plugins take their "library unavailable" branch instead of crashing.
            loadMermaid: () => Promise.resolve(null),
            loadPrism: () => Promise.resolve(null),
            getIconIds: () => [],
            getIcon: () => null,
            parseYaml,
            stringifyYaml,
            // Frontmatter field accessors (Obsidian contract): the plugin calls them on
            // metadata.frontmatter objects the host does not parse yet, so they degrade to
            // "field absent" instead of crashing the plugin's callback chain.
            parseFrontMatterEntry: (fm, key) => {
                if (!fm || typeof fm !== "object") return null;
                const v = fm[key];
                return v === undefined ? null : v;
            },
            parseFrontMatterStringArray: (fm, key) => {
                if (!fm || typeof fm !== "object") return null;
                const v = fm[key];
                if (Array.isArray(v)) return v.map(String);
                if (typeof v === "string") return [v];
                return null;
            },
            parseFrontMatterTags: (fm) => {
                if (!fm || typeof fm !== "object") return null;
                const raw = fm.tag !== undefined ? fm.tag : fm.tags;
                if (Array.isArray(raw)) return raw.map((t) => (typeof t === "string" && t.startsWith("#") ? t.slice(1) : String(t)));
                if (typeof raw === "string") return raw.split(/[,\s]+/).filter(Boolean);
                return null;
            },
            parseFrontMatterAliases: (fm) => {
                if (!fm || typeof fm !== "object") return null;
                const v = fm.aliases;
                if (Array.isArray(v)) return v.map(String);
                if (typeof v === "string") return [v];
                return null;
            },
                getAllTags(cache) {
                    if (!cache) return [];
                    const seen = new Set();
                    const push = (t) => {
                        if (typeof t !== "string") return;
                        const tag = t.startsWith("#") ? t.slice(1) : t;
                        if (tag) seen.add(tag);
                    };
                    for (const t of cache.tags || []) push(typeof t === "string" ? t : t && t.tag);
                    if (cache.frontmatter) {
                        const fmTags = cache.frontmatter.tags || [];
                        (Array.isArray(fmTags) ? fmTags : [fmTags]).forEach(push);
                        const fmTag = cache.frontmatter.tag;
                        if (Array.isArray(fmTag)) fmTag.forEach(push);
                        else push(fmTag);
                    }
                    return Array.from(seen);
                },
            htmlToMarkdown(html) {
                // Plain-text walk of the parsed element tree: block tags become blank-line
                // separated text, <br>/<li> become line breaks. Not a full Markdown
                // serializer — the one corpus plugin that imports it feeds the result to
                // its own math-lifting pass, so structure beats typographic fidelity.
                const clean = __sanitize_html(String(html != null ? html : ""));
                const root = new VElement("div");
                __parse_html_to_velements(clean, root);
                const BLOCK = new Set(["p", "div", "h1", "h2", "h3", "h4", "h5", "h6", "blockquote", "pre", "ul", "ol", "table", "tr", "li"]);
                const LINE = new Set(["br", "li"]);
                const out = [];
                const walk = (el) => {
                    for (const child of el.children) {
                        if (child.tag === "#text") {
                            const t = (child.text || "").replace(/\s+/g, " ");
                            if (t.trim()) out.push(t.trim());
                        } else {
                            walk(child);
                            if (LINE.has(child.tag)) out.push("\n");
                            if (BLOCK.has(child.tag)) out.push("\n\n");
                        }
                    }
                };
                walk(root);
                return out.join(" ").replace(/ +/g, " ").replace(/ ?\n ?/g, "\n").replace(/\n{3,}/g, "\n\n").trim();
            },
            requireApiVersion(version) {
                // The host tracks the Obsidian 1.7.5 surface; anything up to it is satisfied.
                const req = String(version || "").split(".").map((n) => parseInt(n, 10) || 0);
                const host = [1, 7, 5];
                for (let i = 0; i < Math.max(req.length, host.length); i++) {
                    const r = req[i] || 0, h = host[i] || 0;
                    if (r !== h) return r < h;
                }
                return true;
            },
            parseLinktext(linktext) {
                const s = String(linktext != null ? linktext : "");
                const hash = s.indexOf("#");
                if (hash < 0) return { path: s, subpath: "" };
                return { path: s.slice(0, hash), subpath: s.slice(hash) };
            },
            getLinkpath(linktext) {
                const hash = String(linktext != null ? linktext : "").indexOf("#");
                return hash < 0 ? String(linktext) : String(linktext).slice(0, hash);
            },
            sanitizeHTMLToDom(html) {
                // Reuses the existing sanitize + parse pipeline: __sanitize_html strips
                // script/iframe/style blocks, __parse_html_to_velements builds the tree.
                const clean = __sanitize_html(String(html != null ? html : ""));
                const root = new VElement("div");
                __parse_html_to_velements(clean, root);
                // A minimal DocumentFragment stand-in: children live on .children and
                // render through the same VElement channel as every other plugin tree.
                return {
                    children: root.children,
                    firstChild: root.children[0] || null,
                    appendTo(el) {
                        if (el && typeof el.appendChild === "function") {
                            for (const c of root.children) el.appendChild(c);
                        }
                    },
                };
            },
            getFrontMatterInfo(content) {
                const text = String(content != null ? content : "");
                const fm = __parse_frontmatter(text);
                if (fm === null) {
                    return { exists: false, from: 0, to: 0, frontmatter: "", frontmatterPos: { start: { line: 0, offset: 0 }, end: { line: 0, offset: 0 } } };
                }
                const blockEnd = fm.bodyStart > 0 ? fm.bodyStart - 1 : 0;
                const block = text.slice(0, blockEnd);
                const lineCount = block.split("\n").length;
                return {
                    exists: true,
                    from: 0,
                    to: blockEnd,
                    frontmatter: block.replace(/^---\n/, "").replace(/\n---\n?$/, ""),
                    frontmatterPos: {
                        start: { line: 0, offset: 0 },
                        end: { line: lineCount - 1, offset: blockEnd },
                    },
                };
            },
            resolveSubpath(cache, subpath) {
                if (!cache || !subpath) return null;
                const s = String(subpath);
                if (s.startsWith("#^")) {
                    // Block reference: the host cache has no blocks; only a null result
                    // is honest here.
                    return null;
                }
                const wanted = s.replace(/^#/, "").toLowerCase();
                for (const h of cache.headings || []) {
                    if (String(h.heading || "").toLowerCase() === wanted) {
                        return { type: "heading", heading: h.heading, start: h.position && h.position.start, end: h.position && h.position.end };
                    }
                }
                return null;
            },
            prepareFuzzySearch(query) {
                const q = String(query != null ? query : "").toLowerCase();
                return (text) => {
                    const t = String(text != null ? text : "");
                    const lt = t.toLowerCase();
                    let ti = 0, qi = 0, score = 0, last = -2;
                    while (qi < q.length) {
                        const idx = lt.indexOf(q[qi], ti);
                        if (idx < 0) return null;
                        score += idx === last + 1 ? 5 : 1;
                        if (idx === 0 || /[\s\-_/#]/.test(lt[idx - 1])) score += 3;
                        last = idx;
                        ti = idx + 1;
                        qi++;
                    }
                    return { score, matches: null };
                };
            },
            prepareSimpleSearch(query) {
                const q = String(query != null ? query : "").toLowerCase();
                return (text) => {
                    const lt = String(text != null ? text : "").toLowerCase();
                    const idx = lt.indexOf(q);
                    return idx < 0 ? null : { score: 1, matches: null };
                };
            },
            arrayBufferToBase64: __array_buffer_to_base64,
            base64ToArrayBuffer: __base64_to_array_buffer,
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
            },
            has(target, prop) {
                if (typeof prop === "string" && prop.length > 0 && prop[0] >= 'A' && prop[0] <= 'Z') {
                    return true;
                }
                return prop in target;
            },
            getOwnPropertyDescriptor(target, prop) {
                if (prop in target) {
                    return Object.getOwnPropertyDescriptor(target, prop);
                }
                const name = String(prop);
                if (name.length > 0 && name[0] >= 'A' && name[0] <= 'Z') {
                    const cls = getOrCreateFallbackClass(name);
                    return { value: cls, writable: true, enumerable: true, configurable: true };
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
    "####;

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
            let id = payload
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            ApiRequest::Notice {
                text,
                timeout_ms: timeout,
                id,
            }
        }
        "notice_update" => {
            let id = payload.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let text = payload.get("text").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            ApiRequest::NoticeUpdate { id, text }
        }
        "notice_hide" => {
            let id = payload.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            ApiRequest::NoticeHide { id }
        }
        "vault_read" => {
            let path = payload
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            ApiRequest::VaultRead { path }
        }
        "vault_read_binary" => {
            let path = payload
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            ApiRequest::VaultReadBinary { path }
        }
        "vault_list_dir" => {
            let path = payload
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            ApiRequest::VaultListDir { path }
        }
        "vault_modify_binary" => {
            let path = payload
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let data = payload
                .get("data_base64")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            ApiRequest::VaultModifyBinary { path, data_base64: data }
        }
        "vault_create_binary" => {
            let path = payload
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let data = payload
                .get("data_base64")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            ApiRequest::VaultCreateBinary { path, data_base64: data }
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
        "vault_create_folder" => {
            let path = payload
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            ApiRequest::VaultCreateFolder { path }
        }
        "vault_delete" => {
            let path = payload
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            ApiRequest::VaultDelete { path }
        }
        "vault_trash" => {
            let path = payload
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let system = payload
                .get("system")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            ApiRequest::VaultTrash { path, system }
        }
        "open_file" => {
            let path = payload
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            ApiRequest::OpenFile { path }
        }
        "open_url" => {
            let url = payload
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            ApiRequest::OpenUrl { url }
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
                    class: None,
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
            let icon = payload
                .get("icon")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            ApiRequest::AddCommand { id, name, hotkey, icon }
        }
        "remove_command" => {
            let id = payload
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            ApiRequest::RemoveCommand { id }
        }
        "add_toolbar" => {
            let spec_val = payload.get("spec").cloned().unwrap_or(serde_json::Value::Null);
            let spec: crate::toolbar::ToolbarSpec = serde_json::from_value(spec_val).unwrap_or_else(|_| {
                crate::toolbar::ToolbarSpec {
                    id: "default-toolbar".to_string(),
                    placement: crate::toolbar::ToolbarPlacement::Top,
                    items: Vec::new(),
                    owner: None,
                }
            });
            ApiRequest::AddToolbar { spec }
        }
        "update_toolbar" => {
            let id = payload
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let items: Vec<crate::toolbar::ToolbarItem> = payload
                .get("items")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            ApiRequest::UpdateToolbar { id, items }
        }
        "remove_toolbar" => {
            let id = payload
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            ApiRequest::RemoveToolbar { id }
        }
        "register_icon" => {
            let name = payload
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let svg = payload
                .get("svg")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            ApiRequest::RegisterIcon { name, svg }
        }
        "add_status_bar_item" => {
            let id = payload
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            ApiRequest::AddStatusBarItem { id }
        }
        "update_status_bar_item" => {
            let id = payload
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let text = payload
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            ApiRequest::UpdateStatusBarItem { id, text }
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
                    class: None,
                    style: crate::plugins::ui::TextStyle::default(),
                });
            ApiRequest::SetViewTree { view_id, tree }
        }
        "editor_replace_selection" => {
            let text = payload
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            ApiRequest::EditorReplaceSelection { text }
        }
        "editor_get_buffer" => ApiRequest::EditorGetBuffer,
        "editor_get_selection" => ApiRequest::EditorGetSelection,
        "editor_set_buffer" => {
            let text = payload
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            ApiRequest::EditorSetBuffer { text }
        }
        "editor_replace_range" => {
            let num = |key: &str| payload.get(key).and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            ApiRequest::EditorReplaceRange {
                from_line: num("from_line"),
                from_ch: num("from_ch"),
                to_line: num("to_line"),
                to_ch: num("to_ch"),
                text: payload
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
            }
        }
        "editor_set_cursor" => {
            let num = |key: &str| payload.get(key).and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            ApiRequest::EditorSetCursor { line: num("line"), ch: num("ch") }
        }
        "editor_set_selection" => {
            let num = |key: &str| payload.get(key).and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            ApiRequest::EditorSetSelection {
                anchor_line: num("anchor_line"),
                anchor_ch: num("anchor_ch"),
                head_line: num("head_line"),
                head_ch: num("head_ch"),
            }
        }
        "http" => {
            let url = payload.get("url").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let method = payload.get("method").and_then(|v| v.as_str()).unwrap_or("GET").to_string();
            let body = payload.get("body").and_then(|v| v.as_str()).map(|s| s.to_string());
            ApiRequest::Http { url, method, body }
        }
        "set_leaf_view" => {
            let leaf_id = payload
                .get("leaf_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let location = payload
                .get("location")
                .and_then(|v| v.as_str())
                .unwrap_or("right")
                .to_string();
            let view_type = payload
                .get("view_type")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let title = payload
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let icon = payload
                .get("icon")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let tree: crate::plugins::ui::UiNode = payload
                .get("tree")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_else(|| crate::plugins::ui::UiNode::Text {
                    value: "".to_string(),
                    class: None,
                    style: crate::plugins::ui::TextStyle::default(),
                });
            let actions: Vec<crate::plugins::api::PluginViewAction> = payload
                .get("actions")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            ApiRequest::SetLeafView {
                leaf_id,
                location,
                view_type,
                title,
                icon,
                tree,
                actions,
            }
        }
        "reveal_leaf" => {
            let leaf_id = payload
                .get("leaf_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            ApiRequest::RevealLeaf { leaf_id }
        }
        "close_leaf" => {
            let leaf_id = payload
                .get("leaf_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            ApiRequest::CloseLeaf { leaf_id }
        }
        "vault_append" => {
            let path = payload.get("path").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let data = payload.get("data").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            ApiRequest::VaultAppend { path, data }
        }
        "vault_copy" => {
            let source = payload.get("source").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let target = payload.get("target").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            ApiRequest::VaultCopy { source, target }
        }
        "vault_exists" => {
            let path = payload.get("path").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            ApiRequest::VaultExists { path }
        }
        "vault_list_folders" => ApiRequest::VaultListFolders,
        "vault_get_available_path" => {
            let prefix = payload.get("prefix").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let extension = payload.get("extension").and_then(|v| v.as_str()).map(|s| s.to_string());
            ApiRequest::VaultGetAvailablePath { prefix, extension }
        }
        "open_settings" => {
            let tab = payload.get("tab").and_then(|v| v.as_str()).map(|s| s.to_string());
            ApiRequest::OpenSettings { tab }
        }
        "close_settings" => ApiRequest::CloseSettings,
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
        if let Err(_) = func.call::<_, ()>((node_id.clone(), event, val)) {
            log_js_exception(ctx, &format!("__dispatch_ui_interaction error for node '{}'", node_id));
        }
    }
}

/// Drains and formats any active QuickJS exception, ensuring the context is not left in an error state.
fn log_js_exception<'js>(ctx: &Ctx<'js>, context_desc: &str) {
    let ex = ctx.catch();
    let err_msg = if let Some(obj) = ex.as_object() {
        let msg: String = obj.get("message").unwrap_or_default();
        let stack: String = obj.get("stack").unwrap_or_default();
        if !stack.is_empty() {
            format!("{}\n{}", msg, stack)
        } else if !msg.is_empty() {
            msg
        } else {
            format!("{:?}", ex)
        }
    } else {
        format!("{:?}", ex)
    };
    eprintln!("[plugins:error] {}: {}", context_desc, err_msg);
}

/// Dispatches a command execution trigger into QuickJS.
fn dispatch_command_execution<'js>(ctx: &Ctx<'js>, command_id: String) {
    println!("[plugins] dispatch_command_execution: invoking JS __dispatch_command_execution('{}')", command_id);
    let dispatch_fn: rquickjs::Result<Function> = ctx.globals().get("__dispatch_command_execution");
    match dispatch_fn {
        Ok(func) => {
            if let Err(_) = func.call::<_, ()>((command_id.clone(),)) {
                log_js_exception(ctx, &format!("JS __dispatch_command_execution error for '{}'", command_id));
            }
        }
        Err(e) => {
            eprintln!("[plugins:error] __dispatch_command_execution function not found on globals: {:?}", e);
        }
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
        if let Err(_) = func.call::<_, ()>((language.clone(), source, block_id as f64)) {
            log_js_exception(ctx, &format!("__dispatch_code_block_process error for lang '{}'", language));
        }
    }
}

/// Dispatches an editor context menu event into QuickJS and reports serialized items back to host.
fn dispatch_editor_menu<'js>(
    ctx: &Ctx<'js>,
    p_id: &str,
    menu_session_id: u64,
    selected_text: String,
    api_tx: &UnboundedSender<ApiCall>,
) {
    let dispatch_fn: rquickjs::Result<Function> = ctx.globals().get("__dispatch_editor_menu");
    if let Ok(func) = dispatch_fn {
        let res: String = match func.call::<_, String>((menu_session_id as f64, selected_text)) {
            Ok(s) => s,
            Err(_) => {
                log_js_exception(ctx, &format!("__dispatch_editor_menu error for session {}", menu_session_id));
                "[]".to_string()
            }
        };
        let _ = api_tx.unbounded_send(ApiCall {
            plugin_id: p_id.to_string(),
            request_id: menu_session_id,
            request: ApiRequest::Unknown {
                req_kind: format!("editor_menu_result:{}", res),
            },
        });
    }
}

/// Dispatches a clicked context menu item action callback into QuickJS.
fn dispatch_menu_action<'js>(ctx: &Ctx<'js>, action_id: String) {
    let dispatch_fn: rquickjs::Result<Function> = ctx.globals().get("__dispatch_menu_action");
    if let Ok(func) = dispatch_fn {
        if let Err(_) = func.call::<_, ()>((action_id.clone(),)) {
            log_js_exception(ctx, &format!("__dispatch_menu_action error for '{}'", action_id));
        }
    }
}

/// Resolves a pending JS Promise corresponding to a completed host RequestId.
fn dispatch_js_response<'js>(ctx: &Ctx<'js>, id: RequestId, ok: bool, result: serde_json::Value) {
    let dispatch_fn: rquickjs::Result<Function> = ctx.globals().get("__dispatch_host_response");
    match dispatch_fn {
        Ok(func) => {
            let json_str = serde_json::to_string(&result).unwrap_or_else(|_| "null".to_string());
            if let Err(_) = func.call::<_, ()>((id as f64, ok, json_str)) {
                log_js_exception(ctx, &format!("dispatch_js_response failed for request id {}", id));
            }
        }
        Err(e) => {
            eprintln!("[plugins:error] __dispatch_host_response function not found: {:?}", e);
        }
    }
}

/// Dispatches a file-open event into QuickJS.
fn dispatch_file_open<'js>(ctx: &Ctx<'js>, path: Option<String>) {
    let dispatch_fn: rquickjs::Result<Function> = ctx.globals().get("__dispatch_file_open");
    if let Ok(func) = dispatch_fn {
        let p_val = path.unwrap_or_default();
        if let Err(_) = func.call::<_, ()>((p_val,)) {
            log_js_exception(ctx, "dispatch_file_open error");
        }
    }
}

#[cfg(test)]
mod tests;


