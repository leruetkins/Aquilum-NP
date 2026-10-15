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

    // 4. Inject manifest and CommonJS module / exports
    let manifest_json = serde_json::to_string(manifest).unwrap_or_else(|_| "{}".to_string());
    ctx.eval::<(), _>(format!(
        "globalThis.__plugin_manifest = {}; globalThis.module = {{ exports: {{}} }}; globalThis.exports = globalThis.module.exports;",
        manifest_json
    ))?;

    // 5. Inject runtime JavaScript bootstrap defining Obsidian-compatible classes & timers
    let bootstrap_js = r####"
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
                    el.dispatchEvent({ type: "change", target: el, value: parsedVal });
                    el.dispatchEvent({ type: "input", target: el, value: parsedVal });
                } else {
                    el.dispatchEvent({ type: action, target: el });
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
                    if (typeof inst.onUserEnable === "function") {
                        await inst.onUserEnable();
                    }
                } catch (e) {
                    console.error("Plugin instantiation or onload error:", e);
                }
            } else if (exported && typeof exported.onload === "function") {
                globalThis.__plugin_instance = exported;
                try {
                    await exported.onload();
                    if (typeof exported.onUserEnable === "function") {
                        await exported.onUserEnable();
                    }
                } catch (e) {
                    console.error("Plugin onload error:", e);
                }
            }
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
                __invoke_host("notice", { text: String(message), timeout_ms: duration });
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
                this.text = options.text != null ? String(options.text) : "";
                this.attributes = Object.assign({}, options.attr || {});
                if (options.type) this.attributes["type"] = options.type;
                this.children = [];
                this.eventListeners = new Map();
                this.parent = null;
                this.value = options.value != null ? String(options.value) : "";
                this.placeholder = options.placeholder || "";
                this.disabled = !!options.disabled;
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
                this.style = new Proxy({}, {
                    get: (t, p) => t[p] || "",
                    set: (t, p, v) => { t[p] = v; return true; }
                });
                globalThis.__velement_registry.set(this._id, this);
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

            get className() {
                return Array.from(this.classes).join(" ");
            }
            set className(val) {
                this.classes.clear();
                if (typeof val === "string") {
                    val.split(/\s+/).filter(Boolean).forEach(c => this.classes.add(c));
                }
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
                return newChild;
            }

            removeChild(child) {
                if (!child) return null;
                const idx = this.children.indexOf(child);
                if (idx !== -1) {
                    this.children.splice(idx, 1);
                    child.remove();
                    this._notifyModified();
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
                    return oldChild;
                }
                return null;
            }

            cloneNode(deep = false) {
                const clone = new VElement(this.tag);
                this.classes.forEach(c => clone.classes.add(c));
                clone.text = this.text;
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
                    if (child.parent) {
                        child.remove();
                    }
                    child.parent = this;
                    this.children.push(child);
                    this._notifyModified();
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
                this.text += String(val != null ? val : "");
                this._notifyModified();
                return this;
            }

            append(...nodes) {
                for (const n of nodes) {
                    if (typeof n === "string") {
                        this.appendText(n);
                    } else if (n) {
                        this.appendChild(n);
                    }
                }
            }

            empty() {
                for (const c of this.children) {
                    c.remove();
                }
                this.children = [];
                this.text = "";
                this._notifyModified();
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

            setText(text) {
                this.text = String(text != null ? text : "");
                this._notifyModified();
                return this;
            }

            getText() {
                return this.text;
            }

            get textContent() {
                if (this.children.length === 0) {
                    return this.text || "";
                }
                let res = this.text || "";
                for (const c of this.children) {
                    res += c.textContent || "";
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
                return this;
            }

            getAttr(name) {
                return this.attributes[name];
            }

            setAttribute(name, val) {
                this.attributes[name] = val;
                this._notifyModified();
            }

            getAttribute(name) {
                return this.attributes[name];
            }

            removeAttribute(name) {
                delete this.attributes[name];
                this._notifyModified();
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
                super(containerEl);
                this.inputEl = containerEl ? containerEl.createEl("textarea") : new VElement("textarea");
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
                const name = el.children.find(c => c.classes?.has("setting-item-info"))?.children?.find(c => c.classes?.has("setting-item-name"))?.text || el.text || "";
                const desc = el.children.find(c => c.classes?.has("setting-item-info"))?.children?.find(c => c.classes?.has("setting-item-description"))?.text || undefined;
                const controlDiv = el.children.find(c => c.classes?.has("setting-item-control"));

                let controlNode = { type: "text", value: "", class: undefined };
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
                if (el.classes.has("mod-cta") || el.classes.has("mod-primary") || Array.from(el.classes).some(c => c.endsWith("-primary") || c.includes("danger--active"))) variant = "primary";
                if (el.classes.has("mod-warning") || el.classes.has("mod-danger") || Array.from(el.classes).some(c => c.includes("danger") || c.includes("warning"))) variant = "danger";
                if (el.classes.has("mod-cancel") || Array.from(el.classes).some(c => c.includes("secondary") || c.includes("ghost") || c.includes("cancel"))) variant = "secondary";
                return {
                    type: "button",
                    id: el.id,
                    class: classStr,
                    label: el.text || "Button",
                    variant,
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

            if (el.tag === "i" || el.tag === "em" || el.tag === "a") {
                if (el.children.length === 0) {
                    return {
                        type: "text",
                        class: classStr,
                        style: el.classes.has("muted") ? "muted" : "normal",
                        value: el.text || ""
                    };
                }
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

            if (el.classes.has("clickable-icon") || el.classes.has("recent-files-file-delete")) {
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

            if (el.children.length === 0 && (!el.text || !el.text.trim())) {
                return null;
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

        class Editor {
            constructor(selection = "") {
                this._selection = String(selection != null ? selection : "");
            }
            getSelection() {
                return this._selection;
            }
            replaceSelection(text) {
                this._selection = String(text != null ? text : "");
                __invoke_host("editor_replace_selection", { text: this._selection });
            }
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
            toJSON() {
                return this.items.map(it => it.toJSON());
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
                if (this.view && typeof this.view.onClose === "function") {
                    try { this.view.onClose(); } catch (e) { console.error("ItemView onClose error:", e); }
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
            registerView(type, viewCreator) {
                globalThis.__registered_views.set(String(type), viewCreator);
            }
            registerExtensions(extensions, viewType) {}
            registerHoverLinkSource(id, info) {}
            registerEditorExtension(extension) {}
        }

        const rawVault = {
            read: async (path) => {
                const p = typeof path === 'object' ? path.path : path;
                console.log(`[plugin] vault.read: "${p}"`);
                try {
                    const res = await __invoke_host("vault_read", { path: p });
                    return res != null ? String(res) : "";
                } catch (e) {
                    console.warn(`[plugin] vault.read error for "${p}":`, e);
                    return "non-empty-fallback-on-error";
                }
            },
            modify: (path, data) => __invoke_host("vault_modify", { path: typeof path === 'object' ? path.path : path, data }),
            create: (path, data) => __invoke_host("vault_create", { path, data }),
            delete: (path) => {
                const p = typeof path === 'object' ? path.path : path;
                console.log(`[plugin] vault.delete: "${p}"`);
                return __invoke_host("vault_delete", { path: p });
            },
            trash: (path, system) => {
                const p = typeof path === 'object' ? path.path : path;
                console.log(`[plugin] vault.trash: "${p}"`);
                return __invoke_host("vault_delete", { path: p });
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
            getFiles: () => {
                const files = __host_get_markdown_files();
                return files.map(p => new TFile(p));
            },
            getFileByPath: (path) => {
                const p = typeof path === 'object' ? path.path : path;
                return p ? new TFile(p) : null;
            },
            getAbstractFileByPath: (path) => {
                const p = typeof path === 'object' ? path.path : path;
                return p ? new TFile(p) : null;
            },
            on: (event, cb) => {
                const evName = String(event);
                if (!globalThis.__vault_listeners.has(evName)) {
                    globalThis.__vault_listeners.set(evName, []);
                }
                if (typeof cb === "function") {
                    globalThis.__vault_listeners.get(evName).push(cb);
                }
                return {
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
            adapter: {
                read: async (path) => {
                    const res = await __invoke_host("vault_read", { path });
                    return res != null ? String(res) : "";
                },
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
                if (typeof cb === "function") setTimeout(cb, 10);
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
            off: (event, cb) => {
                const evName = String(event);
                const list = globalThis.__workspace_listeners.get(evName);
                if (list) {
                    const idx = list.indexOf(cb);
                    if (idx !== -1) list.splice(idx, 1);
                }
            },
            trigger: (event, ...args) => {
                const list = globalThis.__workspace_listeners.get(String(event));
                if (list) {
                    for (const cb of [...list]) {
                        try { cb(...args); } catch (e) { console.error("Workspace event error:", e); }
                    }
                }
            }
        };

        const rawMetadataCache = {
            getFileCache: (_file) => ({
                tags: [],
                links: [],
                headings: [],
                frontmatter: {}
            }),
            getFirstLinkpathDest: (path, _sourcePath) => path,
            on: (_event, _cb) => ({ id: "meta-sub" }),
            trigger: () => {}
        };

        const app = {
            vault: makeAuditedProxy(rawVault, "app.vault"),
            workspace: makeAuditedProxy(rawWorkspace, "app.workspace"),
            metadataCache: makeAuditedProxy(rawMetadataCache, "app.metadataCache"),
            commands: {
                executeCommandById: (id) => __invoke_host("execute_command", { id: String(id) }),
            },
            internalPlugins: {
                getEnabledPluginById: (_id) => null
            },
            dragManager: {
                dragFile: () => ({}),
                onDragStart: () => {}
            },
            fileManager: {
                getNewFileParent: () => new TFolder("/"),
                generateMarkdownLink: () => "",
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
        globalThis.window = globalThis;
        moment.locale = () => getLanguage();
        globalThis.window.moment = moment;
        globalThis.navigator = {
            language: "ru-RU",
            languages: ["ru-RU", "ru", "en"],
            userAgent: "Aquilum/1.0 (Windows NT 10.0; Win64; x64) Obsidian/1.8.0"
        };
        const __doc_body = new VElement("body");
        const __doc_head = new VElement("head");
        globalThis.document = {
            documentElement: { lang: "ru" },
            createElement: (t, options) => new VElement(t, options),
            createDocumentFragment: () => new VElement("div"),
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
        globalThis.Event = class Event {
            constructor(type, options = {}) {
                this.type = String(type);
                Object.assign(this, options);
            }
        };
        globalThis.CustomEvent = class CustomEvent extends globalThis.Event {
            constructor(type, options = {}) {
                super(type, options);
                this.detail = options?.detail;
            }
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
            requestUrl,
            moment,
            getLanguage,
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

    #[test]
    fn test_menu_and_editor_context_menu_dispatch() {
        let manifest = PluginManifest {
            id: "menu-test-plugin".into(),
            name: "Menu Test Plugin".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        let script = r#"
            const { Plugin } = require("obsidian");

            class MyPlugin extends Plugin {
                async onload() {
                    this.registerEvent(
                        this.app.workspace.on("editor-menu", (menu, editor) => {
                            const sel = editor.getSelection();
                            menu.addItem((item) => {
                                item.setTitle("Convert " + sel).setIcon("text");
                                const sub = item.setSubmenu();
                                sub.addItem((subItem) => {
                                    subItem.setTitle("Sub Action").onClick(() => {
                                        editor.replaceSelection("REPLACED:" + sel);
                                    });
                                });
                            });
                        })
                    );
                }
            }
            module.exports = MyPlugin;
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        )
        .unwrap();

        std::thread::sleep(Duration::from_millis(50));

        // Dispatch EditorMenu event
        let session_id = 999;
        let _ = handle.host_events_tx.unbounded_send(HostEvent::EditorMenu {
            menu_session_id: session_id,
            selected_text: "123.45".to_string(),
        });

        std::thread::sleep(Duration::from_millis(50));

        let mut menu_items_res = None;
        while let Ok(call) = api_rx.try_recv() {
            if let ApiRequest::Unknown { req_kind } = call.request {
                if let Some(json_str) = req_kind.strip_prefix("editor_menu_result:") {
                    menu_items_res = Some(json_str.to_string());
                    break;
                }
            }
        }

        let json = menu_items_res.expect("Expected editor_menu_result from plugin");
        assert!(json.contains("Convert 123.45"));
        assert!(json.contains("Sub Action"));

        // Extract sub action item id and click it
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let sub_id = parsed[0]["submenu"][0]["id"].as_str().unwrap().to_string();

        let _ = handle.host_events_tx.unbounded_send(HostEvent::ExecuteMenuAction {
            action_id: sub_id,
        });

        std::thread::sleep(Duration::from_millis(50));

        let mut replaced_text = None;
        while let Ok(call) = api_rx.try_recv() {
            if let ApiRequest::EditorReplaceSelection { text } = call.request {
                replaced_text = Some(text);
                break;
            }
        }

        assert_eq!(replaced_text, Some("REPLACED:123.45".to_string()));

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_obsidian_get_language() {
        let manifest = PluginManifest {
            id: "lang-test-plugin".into(),
            name: "Lang Test Plugin".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let mut snap = Snapshot::default();
        snap.language = "ru".to_string();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(snap));

        let script = r#"
            const { Plugin, Notice, getLanguage, moment } = require("obsidian");

            class LangPlugin extends Plugin {
                async onload() {
                    const langFromObsidian = getLanguage();
                    const langFromGlobal = globalThis.getLanguage ? globalThis.getLanguage() : "none";
                    const momentLocale = moment.locale ? moment.locale() : "none";
                    const winMomentLocale = window.moment ? window.moment.locale() : "none";
                    new Notice(`LANG:${langFromObsidian}|${langFromGlobal}|${momentLocale}|${winMomentLocale}`);
                }
            }
            module.exports = LangPlugin;
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        )
        .unwrap();

        std::thread::sleep(Duration::from_millis(50));

        let mut notice_text = None;
        while let Ok(call) = api_rx.try_recv() {
            if let ApiRequest::Notice { text, .. } = call.request {
                notice_text = Some(text);
                break;
            }
        }

        assert_eq!(
            notice_text,
            Some("LANG:ru|ru|ru|ru".to_string())
        );

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_interactive_settings_tab_toggle_and_modal_lifecycle() {
        let manifest = PluginManifest {
            id: "interactive-settings-test".into(),
            name: "Interactive Settings".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        let script = r#"
            const { Plugin, PluginSettingTab, Setting, Modal } = require("obsidian");

            class EditModal extends Modal {
                constructor(app) {
                    super(app);
                }
                onOpen() {
                    this.titleEl.setText("Currency Editor");
                }
            }

            class MySettingTab extends PluginSettingTab {
                constructor(app, plugin) {
                    super(app, plugin);
                }
                display() {
                    const { containerEl } = this;
                    containerEl.empty();
                    new Setting(containerEl)
                        .setName("Auto Detect")
                        .addToggle(t => {
                            t.setValue(this.plugin.settings.autoDetect);
                            t.onChange(val => {
                                this.plugin.settings.autoDetect = val;
                            });
                        });
                    new Setting(containerEl)
                        .setName("Edit Currency")
                        .addButton(b => {
                            b.setButtonText("Edit");
                            b.onClick(() => {
                                new EditModal(this.app).open();
                            });
                        });
                }
            }

            class InteractivePlugin extends Plugin {
                async onload() {
                    this.settings = { autoDetect: false };
                    this.addSettingTab(new MySettingTab(this.app, this));
                }
            }
            module.exports = InteractivePlugin;
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        )
        .unwrap();

        std::thread::sleep(Duration::from_millis(50));

        // 1. Initial SetViewTree for view_id: 0
        let mut toggle_id = None;
        let mut button_id = None;
        while let Ok(call) = api_rx.try_recv() {
            if let ApiRequest::SetViewTree { view_id, tree } = call.request {
                if view_id == 0 {
                    let json = serde_json::to_string(&tree).unwrap();
                    assert!(json.contains("Auto Detect"));
                    assert!(json.contains("\"value\":false"));
                    // Find toggle node id
                    if let crate::plugins::ui::UiNode::Column { children, .. } = tree {
                        for c in &children {
                            if let crate::plugins::ui::UiNode::SettingItem { name, control, .. } = c {
                                if name == "Auto Detect" {
                                    if let crate::plugins::ui::UiNode::Toggle { id, value } = &**control {
                                        assert_eq!(*value, false);
                                        toggle_id = Some(id.clone());
                                    }
                                } else if name == "Edit Currency" {
                                    if let crate::plugins::ui::UiNode::Button { id, .. } = &**control {
                                        button_id = Some(id.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let tog_id = toggle_id.expect("Toggle ID should be present");
        let btn_id = button_id.expect("Button ID should be present");

        // 2. Dispatch toggle change event from host
        let _ = handle.host_events_tx.unbounded_send(HostEvent::UiEvent {
            view_id: 0,
            node_id: tog_id,
            event: "change".to_string(),
            value: Some("true".to_string()),
        });

        std::thread::sleep(Duration::from_millis(50));

        // 3. Plugin should have re-synced SetViewTree with value: true
        let mut toggle_updated = false;
        while let Ok(call) = api_rx.try_recv() {
            if let ApiRequest::SetViewTree { view_id, tree } = call.request {
                if view_id == 0 {
                    let json = serde_json::to_string(&tree).unwrap();
                    if json.contains("\"value\":true") {
                        toggle_updated = true;
                    }
                }
            }
        }
        assert!(toggle_updated, "Expected SetViewTree to be re-synced with toggle value: true");

        // 4. Dispatch button click event from host to open modal
        let _ = handle.host_events_tx.unbounded_send(HostEvent::UiEvent {
            view_id: 0,
            node_id: btn_id,
            event: "click".to_string(),
            value: None,
        });

        std::thread::sleep(Duration::from_millis(50));

        // 5. Plugin should have emitted SetViewTree for view_id: 1 with modal content
        let mut modal_opened = false;
        while let Ok(call) = api_rx.try_recv() {
            if let ApiRequest::SetViewTree { view_id, tree } = call.request {
                if view_id == 1 {
                    let json = serde_json::to_string(&tree).unwrap();
                    if json.contains("Currency Editor") {
                        modal_opened = true;
                    }
                }
            }
        }
        assert!(modal_opened, "Expected SetViewTree for view_id: 1 with Currency Editor modal");

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_obsidian_setting_add_text_area() {
        let manifest = PluginManifest {
            id: "test-area-plugin".into(),
            name: "Test Area Plugin".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        let script = r#"
            const { Plugin, PluginSettingTab, Setting, TextAreaComponent } = require('obsidian');

            class TestAreaPlugin extends Plugin {
                onload() {
                    this.addSettingTab(new AreaTab(this.app, this));
                }
            }

            class AreaTab extends PluginSettingTab {
                display() {
                    const { containerEl } = this;
                    containerEl.empty();
                    new Setting(containerEl)
                        .setName("Multiline Notes")
                        .setDesc("Enter long notes here")
                        .addTextArea(text => {
                            text.setPlaceholder("Notes...")
                                .setValue("Line 1\nLine 2")
                                .onChange(val => {});
                        });
                }
            }

            module.exports = TestAreaPlugin;
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        ).expect("failed to spawn runtime");

        std::thread::sleep(Duration::from_millis(50));

        let mut found_textarea = false;
        while let Ok(call) = api_rx.try_recv() {
            if let ApiRequest::SetViewTree { view_id, tree } = call.request {
                if view_id == 0 {
                    let json = serde_json::to_string(&tree).unwrap();
                    if json.contains("\"type\":\"input\"") && json.contains("Multiline Notes") {
                        found_textarea = true;
                    }
                }
            }
        }
        assert!(found_textarea, "Expected SetViewTree containing setting with textarea input");

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_obsidian_item_view_and_workspace_leaf_lifecycle() {
        let manifest = PluginManifest {
            id: "test-itemview-plugin".into(),
            name: "Test ItemView Plugin".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        let script = r#"
            const { Plugin, ItemView } = require('obsidian');

            class CustomView extends ItemView {
                constructor(leaf) {
                    super(leaf);
                }
                getViewType() {
                    return "custom-tool";
                }
                getDisplayText() {
                    return "Tool Panel";
                }
                getIcon() {
                    return "hammer";
                }
                async onOpen() {
                    this.addAction("refresh", "Refresh Tool", () => {
                        this.containerEl.createDiv({ text: "Action Triggered" });
                    });
                    const container = this.contentEl;
                    container.createDiv({ text: "ItemView Body Text" });
                }
            }

            class TestItemViewPlugin extends Plugin {
                async onload() {
                    this.registerView("custom-tool", (leaf) => new CustomView(leaf));
                    const leaf = this.app.workspace.getRightLeaf(false);
                    await leaf.setViewState({ type: "custom-tool" });
                }
            }

            module.exports = TestItemViewPlugin;
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        ).expect("failed to spawn runtime");

        std::thread::sleep(Duration::from_millis(60));

        let mut found_leaf = false;
        let mut leaf_action_id = String::new();
        while let Ok(call) = api_rx.try_recv() {
            if let ApiRequest::SetLeafView { leaf_id: _, location, view_type, title, icon, tree, actions } = call.request {
                assert_eq!(location, "right");
                assert_eq!(view_type, "custom-tool");
                assert_eq!(title, "Tool Panel");
                assert_eq!(icon.as_deref(), Some("hammer"));
                assert_eq!(actions.len(), 1);
                assert_eq!(actions[0].icon, "refresh");
                assert_eq!(actions[0].title, "Refresh Tool");
                leaf_action_id = actions[0].id.clone();

                let json = serde_json::to_string(&tree).unwrap();
                assert!(json.contains("ItemView Body Text"), "tree should contain ItemView body text");
                found_leaf = true;
            }
        }
        assert!(found_leaf, "Expected SetLeafView from WorkspaceLeaf.setViewState");

        // Test triggering action
        let _ = handle.host_events_tx.unbounded_send(HostEvent::UiEvent {
            view_id: 0,
            node_id: leaf_action_id,
            event: "action_click".to_string(),
            value: None,
        });

        std::thread::sleep(Duration::from_millis(30));

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_load_actual_recent_files_script() {
        let path = std::path::Path::new(r"D:\Naben\Vibecodium\.vault\plugins\recent-files-obsidian\main.js");
        if !path.exists() {
            return;
        }
        let script = std::fs::read_to_string(path).unwrap();
        let manifest = PluginManifest {
            id: "recent-files-obsidian".into(),
            name: "Recent Files".into(),
            version: "1.3.11".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        let handle = spawn_plugin_runtime(
            manifest,
            script,
            api_tx,
            snapshot,
        ).expect("failed to spawn runtime");

        let mut got_left_leaf = false;
        let mut got_reveal = false;

        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_millis(600) {
            while let Ok(call) = api_rx.try_recv() {
                match call.request {
                    ApiRequest::LoadData => {
                        let _ = handle.host_events_tx.unbounded_send(HostEvent::response_ok(call.request_id, serde_json::json!({ "recentFiles": [] })));
                    }
                    ApiRequest::SetLeafView { ref location, ref view_type, ref icon, .. } => {
                        if location == "left" && view_type == "recent-files" {
                            got_left_leaf = true;
                            assert_eq!(icon.as_deref(), Some("clock"));
                        }
                    }
                    ApiRequest::RevealLeaf { .. } => {
                        got_reveal = true;
                    }
                    _ => {}
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        assert!(got_left_leaf, "Should have received SetLeafView for left leaf recent-files");
        assert!(got_reveal, "Should have received RevealLeaf for recent-files leaf");

        // Simulate opening a file in the app:
        let _ = handle.host_events_tx.unbounded_send(HostEvent::FileOpen {
            path: Some("notes/daily.md".to_string()),
        });
        std::thread::sleep(Duration::from_millis(100));

        let mut got_saved_data = false;
        while let Ok(call) = api_rx.try_recv() {
            match call.request {
                ApiRequest::SaveData { ref data } => {
                    let s = data.to_string();
                    if s.contains("daily.md") {
                        got_saved_data = true;
                    }
                }
                _ => {}
            }
        }
        assert!(got_saved_data, "recent-files should capture opened note and save updated data");

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_velement_micro_dom_and_selectors() {
        let manifest = PluginManifest {
            id: "test-micro-dom".into(),
            name: "Test Micro DOM".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        let script = r#"
            const { Plugin, Notice } = require('obsidian');

            class MicroDomTestPlugin extends Plugin {
                async onload() {
                    const root = new VElement("div", { cls: "container", id: "root-el" });
                    
                    // 1. innerHTML parsing and sanitization
                    root.innerHTML = '<div class="card" id="card-1"><h3>Title</h3><p>Hello <b>bold</b> and <i>italic</i></p><hr><span class="badge" data-role="status">Active</span><script>evil()</script><iframe src="bad.com"></iframe><button onclick="evil()">Click</button></div>';

                    const card = root.querySelector(".card");
                    if (!card) throw new Error("card not found");
                    if (card.id !== "card-1") throw new Error("card id mismatch");

                    // Verify dangerous tags stripped
                    if (root.querySelector("script")) throw new Error("script should be sanitized");
                    if (root.querySelector("iframe")) throw new Error("iframe should be sanitized");
                    const btn = root.querySelector("button");
                    if (btn && btn.attributes["onclick"]) throw new Error("onclick should be stripped");

                    // 2. Traversal and selectors
                    const bold = root.querySelector("p > b");
                    if (!bold || bold.textContent !== "bold") throw new Error("p > b selector failed");
                    if (!bold.matches("b")) throw new Error("matches tag failed");
                    if (!bold.matches("p b")) throw new Error("matches descendant failed");
                    if (!bold.matches("p > b")) throw new Error("matches child failed");

                    // closest
                    const parentP = bold.closest("p");
                    if (!parentP || parentP.tag !== "p") throw new Error("closest('p') failed");
                    const closestCard = bold.closest(".card");
                    if (!closestCard || closestCard.id !== "card-1") throw new Error("closest('.card') failed");

                    // querySelectorAll
                    const allSpans = root.querySelectorAll("span, b, i");
                    if (allSpans.length < 3) throw new Error("querySelectorAll multi failed: " + allSpans.length);

                    // Attribute matching
                    const badge = root.querySelector('[data-role="status"]');
                    if (!badge || !badge.classes.has("badge")) throw new Error("attribute selector failed");

                    // 3. Tree manipulation and navigation
                    const p = root.querySelector("p");
                    const newCode = new VElement("code");
                    newCode.text = "let x = 1;";
                    p.insertBefore(newCode, bold);
                    if (newCode.nextSibling !== bold) throw new Error("insertBefore nextSibling failed");
                    if (bold.previousSibling !== newCode) throw new Error("previousSibling failed");
                    if (!p.contains(newCode)) throw new Error("contains failed");

                    // className getter/setter
                    newCode.className = "inline-code hljs";
                    if (!newCode.classes.has("inline-code") || !newCode.classes.has("hljs")) throw new Error("className setter failed");
                    if (newCode.className !== "inline-code hljs" && newCode.className !== "hljs inline-code") throw new Error("className getter failed");

                    // cloneNode
                    const clone = newCode.cloneNode(true);
                    if (clone.text !== "let x = 1;" || !clone.classes.has("inline-code")) throw new Error("cloneNode failed");

                    // replaceChild
                    const replaced = p.replaceChild(clone, newCode);
                    if (replaced !== newCode) throw new Error("replaceChild return mismatch");

                    // removeChild
                    p.removeChild(clone);
                    if (p.contains(clone)) throw new Error("removeChild failed to detach");

                    // 4. document query and getElementById
                    const byId = globalThis.document.getElementById("card-1");
                    if (!byId) throw new Error("document.getElementById failed");

                    // 5. DOMParser
                    const parser = new DOMParser();
                    const xmlDoc = parser.parseFromString('<svg class="icon"><path d="M0,0 L10,10" /></svg>', "text/xml");
                    const svg = xmlDoc.documentElement;
                    if (!svg || svg.tag !== "svg" || !svg.classes.has("icon")) throw new Error("DOMParser svg failed");
                    const path = svg.querySelector("path");
                    if (!path || path.attributes["d"] !== "M0,0 L10,10") throw new Error("DOMParser path failed");

                    // 6. __velement_to_uinode check
                    const hr = root.querySelector("hr");
                    const hrNode = __velement_to_uinode(hr);
                    if (!hrNode || hrNode.type !== "separator") throw new Error("hr to uinode failed: " + JSON.stringify(hrNode));

                    const boldNode = __velement_to_uinode(bold);
                    if (!boldNode || boldNode.type !== "text" || boldNode.style !== "heading") throw new Error("bold to uinode failed: " + JSON.stringify(boldNode));

                    const codeEl = new VElement("code");
                    codeEl.text = "fn main()";
                    const codeNode = __velement_to_uinode(codeEl);
                    if (!codeNode || codeNode.type !== "text" || codeNode.style !== "code") throw new Error("code to uinode failed: " + JSON.stringify(codeNode));

                    new Notice("MICRO_DOM_ALL_PASSED");
                }
            }

            module.exports = MicroDomTestPlugin;
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        ).expect("failed to spawn runtime");

        std::thread::sleep(Duration::from_millis(100));

        let mut passed = false;
        while let Ok(call) = api_rx.try_recv() {
            if let ApiRequest::Notice { ref text, .. } = call.request {
                if text == "MICRO_DOM_ALL_PASSED" {
                    passed = true;
                }
            }
        }
        assert!(passed, "Micro-DOM test assertions should all pass");

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_empty_files_cleaner_plugin_actual_script() {
        let manifest = PluginManifest {
            id: "empty-files-cleaner-np".into(),
            name: "Empty Files Cleaner NP".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let mut snapshot_data = Snapshot::default();
        snapshot_data.files = Arc::new([
            crate::vault::FileInfo {
                path: "notes/empty.md".to_string(),
                name: "empty.md".to_string(),
                basename: "empty".to_string(),
                extension: "md".to_string(),
            },
            crate::vault::FileInfo {
                path: "notes/non_empty.md".to_string(),
                name: "non_empty.md".to_string(),
                basename: "non_empty".to_string(),
                extension: "md".to_string(),
            },
        ]);
        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(snapshot_data));

        let script = match std::fs::read_to_string("D:\\Naben\\Vibecodium\\.vault\\plugins\\empty-files-cleaner-np\\main.js") {
            Ok(src) => src,
            Err(_) => return,
        };

        let handle = spawn_plugin_runtime(
            manifest,
            script,
            api_tx,
            snapshot,
        ).expect("failed to spawn runtime");

        std::thread::sleep(Duration::from_millis(50));

        // 1. Trigger the command "find-empty-files"
        let _ = handle.host_events_tx.unbounded_send(HostEvent::ExecuteCommand {
            command_id: "find-empty-files".to_string(),
        });

        // 2. The plugin scans files and issues VaultRead for each file:
        let mut got_empty_read = false;
        let mut got_non_empty_read = false;
        let mut view_tree_with_files = false;
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_secs(2) {
            while let Ok(call) = api_rx.try_recv() {
                match call.request {
                    ApiRequest::VaultRead { ref path } => {
                        if path == "notes/empty.md" {
                            got_empty_read = true;
                            let _ = handle.host_events_tx.unbounded_send(HostEvent::response_ok(
                                call.request_id,
                                serde_json::json!("   \n  "),
                            ));
                        } else if path == "notes/non_empty.md" {
                            got_non_empty_read = true;
                            let _ = handle.host_events_tx.unbounded_send(HostEvent::response_ok(
                                call.request_id,
                                serde_json::json!("# Title\nSome content"),
                            ));
                        }
                    }
                    ApiRequest::SetViewTree { view_id, ref tree } => {
                        if view_id == 1 {
                            let json = serde_json::to_string(tree).unwrap();
                            if json.contains("empty.md") && json.contains("Delete selected") {
                                view_tree_with_files = true;
                            }
                        }
                    }
                    _ => {}
                }
            }
            if view_tree_with_files {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        assert!(got_empty_read, "Plugin should read empty.md");
        assert!(got_non_empty_read, "Plugin should read non_empty.md");
        assert!(view_tree_with_files, "Modal view tree should contain found empty file and delete button");

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }
}


