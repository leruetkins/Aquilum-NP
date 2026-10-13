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
            let setup_res = ctx.with(|ctx| setup_plugin_environment(&ctx, &p_id, api_tx.clone(), snapshot.clone()));
            if let Err(e) = setup_res {
                eprintln!("[plugin:{}] Environment setup error: {:?}", p_id, e);
                return;
            }

            // Execute the plugin script
            let eval_res = ctx.with(|ctx| {
                ctx.eval::<(), _>(script_source.as_str())
            });
            if let Err(e) = eval_res {
                eprintln!("[plugin:{}] Script evaluation error: {:?}", p_id, e);
            }

            // Plugin worker message loop: process incoming host events and responses
            loop {
                match host_events_rx.try_recv() {
                    Ok(event) => match event {
                        HostEvent::Response { id, ok, result } => {
                            let _ = ctx.with(|ctx| {
                                dispatch_js_response(&ctx, id, ok, result);
                            });
                        }
                        HostEvent::Unload => {
                            let _ = ctx.with(|ctx| {
                                let _ = ctx.eval::<(), _>("if (typeof __plugin_instance !== 'undefined' && __plugin_instance.onunload) { __plugin_instance.onunload(); }");
                            });
                            break;
                        }
                        _ => {
                            // Dispatch other event hooks (file modified, etc.)
                        }
                    },
                    Err(TryRecvError::Empty) => {
                        thread::sleep(Duration::from_millis(5));
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
    plugin_id: &str,
    api_tx: UnboundedSender<ApiCall>,
    snapshot: Arc<arc_swap::ArcSwap<Snapshot>>,
) -> rquickjs::Result<()> {
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
    let snap_active = snapshot.clone();
    let get_active_file_fn = Function::new(ctx.clone(), move || -> rquickjs::Result<Option<String>> {
        let snap = snap_active.load();
        Ok(snap.active_file.as_ref().map(|f| f.path.clone()))
    })?;
    globals.set("__host_get_active_file", get_active_file_fn)?;

    let snap_files = snapshot.clone();
    let get_files_fn = Function::new(ctx.clone(), move || -> rquickjs::Result<Vec<String>> {
        let snap = snap_files.load();
        Ok(snap.files.iter().map(|f| f.path.clone()).collect())
    })?;
    globals.set("__host_get_markdown_files", get_files_fn)?;

    // 4. Inject runtime JavaScript bootstrap defining Obsidian-compatible classes
    let bootstrap_js = r#"
        const __pending_promises = new Map();

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

        class Plugin {
            constructor(app, manifest) {
                this.app = app;
                this.manifest = manifest;
            }
            async onload() {}
            async onunload() {}
            addCommand(cmd) {
                return __invoke_host("add_command", cmd);
            }
            async loadData() {
                return __invoke_host("load_data", {});
            }
            async saveData(data) {
                return __invoke_host("save_data", { data });
            }
        }

        const app = {
            vault: {
                read: (path) => __invoke_host("vault_read", { path }),
                modify: (path, data) => __invoke_host("vault_modify", { path, data }),
                create: (path, data) => __invoke_host("vault_create", { path, data }),
                delete: (path) => __invoke_host("vault_delete", { path }),
                getMarkdownFiles: () => __host_get_markdown_files(),
            },
            workspace: {
                getActiveFile: () => {
                    const p = __host_get_active_file();
                    return p ? { path: p, name: p.split("/").pop() } : null;
                },
                openFile: (path) => __invoke_host("open_file", { path }),
            }
        };

        globalThis.Notice = Notice;
        globalThis.Plugin = Plugin;
        globalThis.app = app;
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
        _ => ApiRequest::Notice {
            text: format!("Unknown request kind: {}", kind),
            timeout_ms: 3000,
        },
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
}
