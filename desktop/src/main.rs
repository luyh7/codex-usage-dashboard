#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod native;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::{
    Manager, RunEvent, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::TrayIconBuilder,
};
use tauri_plugin_opener::OpenerExt;
use usage_runtime::{Result, Service};

struct Desktop {
    root: PathBuf,
    binary: PathBuf,
    service: Mutex<Option<Service>>,
    busy: AtomicBool,
    closing: AtomicBool,
    tray: AtomicBool,
    hide_on_close: AtomicBool,
    last_error: Mutex<String>,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct WindowSettings {
    hide_on_close: bool,
    width: Option<f64>,
    height: Option<f64>,
    maximized: bool,
    x: Option<i32>,
    y: Option<i32>,
}
fn read_window(root: &std::path::Path) -> WindowSettings {
    fs::read(root.join("desktop-settings.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}
fn trusted(
    window: &WebviewWindow,
    state: &Desktop,
    bootstrap: bool,
) -> std::result::Result<(), String> {
    let url = window.url().map_err(|e| e.to_string())?;
    let allowed = state
        .service
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|s| url.origin().ascii_serialization() == s.record.url());
    if window.label() == "main" && (allowed || (bootstrap && is_bootstrap(&url))) {
        Ok(())
    } else {
        Err("untrusted desktop page".into())
    }
}
async fn trusted_live(
    window: &WebviewWindow,
    app: &tauri::AppHandle,
) -> std::result::Result<(), String> {
    let state = app.state::<Desktop>();
    trusted(window, &state, false)?;
    let record = state
        .service
        .lock()
        .unwrap()
        .as_ref()
        .ok_or("service stopped")?
        .record
        .clone();
    tauri::async_runtime::spawn_blocking(move || {
        record.health().map(|_| ()).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
fn show(app: &tauri::AppHandle, settings: bool) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        if settings {
            let _ = window.eval("window.dispatchEvent(new Event('cousash-settings'))");
        }
    }
}
fn quit(app: &tauri::AppHandle) {
    app.state::<Desktop>().closing.store(true, Ordering::SeqCst);
    save_window(app);
    app.exit(0);
}
fn client_size(window: &WebviewWindow) -> Option<(f64, f64)> {
    #[cfg(target_os = "linux")]
    {
        // Tao's cached configure-event size can include GTK's CSD shadow.
        // gtk_window_get_size returns the client dimensions accepted on restore.
        let (send, recv) = std::sync::mpsc::sync_channel(1);
        let native = window.clone();
        window
            .run_on_main_thread(move || {
                use gtk::prelude::GtkWindowExt;
                let size = native.gtk_window().ok().and_then(|window| {
                    let (width, height) = window.size();
                    (width > 0 && height > 0).then_some((f64::from(width), f64::from(height)))
                });
                let _ = send.send(size);
            })
            .ok()?;
        recv.recv_timeout(Duration::from_secs(2)).ok().flatten()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let size = window.inner_size().ok()?;
        let scale = window.scale_factor().unwrap_or(1.);
        Some((
            f64::from(size.width) / scale,
            f64::from(size.height) / scale,
        ))
    }
}
fn save_window(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let state = app.state::<Desktop>();
    let mut prefs = read_window(&state.root);
    prefs.maximized = window.is_maximized().unwrap_or(false);
    if !prefs.maximized
        && let Some((width, height)) = client_size(&window)
    {
        prefs.width = Some(width);
        prefs.height = Some(height);
        if let Ok(position) = window.outer_position() {
            prefs.x = Some(position.x);
            prefs.y = Some(position.y);
        }
    }
    let _ = usage_runtime::atomic_json(&state.root.join("desktop-settings.json"), &prefs);
}
fn launch_service(app: &tauri::AppHandle) -> std::result::Result<(), String> {
    let state = app.state::<Desktop>();
    if state.busy.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let state = app.state::<Desktop>();
        let result = (|| -> Result<()> {
            let homes: Vec<PathBuf> = fs::read(state.root.join("preferences.json"))
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                .and_then(|v| serde_json::from_value(v["codex_homes"].clone()).ok())
                .unwrap_or_default();
            let mut service = usage_runtime::ensure_service(
                &state.binary,
                &state.root,
                &homes,
                Some(std::process::id()),
            )?;
            if state
                .service
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|previous| {
                    previous.owned && previous.record.instance_id == service.record.instance_id
                })
            {
                service.owned = true;
            }
            if service.record.build_id != env!("COUSASH_SERVICE_BUILD")
                || service.record.version != env!("CARGO_PKG_VERSION")
            {
                if service.owned {
                    let _ = service.record.stop();
                }
                return Err(
                    "bundled service build mismatch; reinstall the complete application".into(),
                );
            }
            let url = format!("{}/", service.record.url()).parse()?;
            app.add_capability(json!({
                "identifier":format!("service-{}",service.record.instance_id),
                "local":false,"windows":["main"],"remote":{"urls":[format!("{}/*",service.record.url())]},
                "permissions":["allow-retry-service","allow-desktop-settings","allow-save-desktop-settings","allow-choose-source","allow-export-snapshot","allow-check-update","allow-apply-sources","allow-quit-desktop"]
            }).to_string().as_str())?;
            *state.service.lock().unwrap() = Some(service);
            if let Some(window) = app.get_webview_window("main") {
                window.navigate(url)?;
            }
            Ok(())
        })();
        state.busy.store(false, Ordering::SeqCst);
        if let Err(error) = result {
            *state.last_error.lock().unwrap() = error.to_string();
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.navigate(bootstrap_url());
            }
        } else {
            state.last_error.lock().unwrap().clear();
        }
    });
    Ok(())
}
#[tauri::command]
fn retry_service(window: WebviewWindow, app: tauri::AppHandle) -> std::result::Result<(), String> {
    trusted(&window, &app.state::<Desktop>(), true)?;
    launch_service(&app)
}
#[tauri::command]
fn quit_desktop(window: WebviewWindow, app: tauri::AppHandle) -> std::result::Result<(), String> {
    trusted(&window, &app.state::<Desktop>(), true)?;
    quit(&app);
    Ok(())
}

fn bootstrap_url() -> tauri::Url {
    if cfg!(windows) {
        "http://tauri.localhost/index.html"
    } else {
        "tauri://localhost/index.html"
    }
    .parse()
    .unwrap()
}
fn is_bootstrap(url: &tauri::Url) -> bool {
    (url.scheme() == "tauri" && url.host_str() == Some("localhost"))
        || (cfg!(windows)
            && url.scheme() == "http"
            && url.host_str() == Some("tauri.localhost")
            && url.port().is_none())
}
fn fatal(error: impl std::fmt::Display) {
    let message = error.to_string();
    eprintln!("{message}");
    rfd::MessageDialog::new()
        .set_title("Codex Usage Dashboard")
        .set_description(&message)
        .set_level(rfd::MessageLevel::Error)
        .show();
}
fn main() {
    let root = usage_runtime::config_dir();
    if let Err(error) = fs::create_dir_all(&root) {
        fatal(error);
        return;
    }
    let runtime = usage_runtime::runtime_dir(&root);
    if let Err(e) = usage_runtime::private_dir(&runtime)
        .and_then(|_| usage_runtime::private_dir(&usage_runtime::cache_dir(&root)))
    {
        fatal(e);
        return;
    }
    let deadline = Instant::now() + usage_runtime::TIMEOUT;
    let _lock = loop {
        if usage_runtime::activate_desktop(&root, "show").is_ok() {
            return;
        }
        match usage_runtime::lock(&runtime, "desktop.lock", Duration::ZERO) {
            Ok(lock) => break lock,
            Err(error) if Instant::now() >= deadline => {
                fatal(error);
                return;
            }
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    };
    let listener = TcpListener::bind("127.0.0.1:0").expect("desktop activation listener");
    let token = usage_runtime::random().unwrap();
    let record = json!({"schema_version":1,"address":listener.local_addr().unwrap().to_string(),"pid":std::process::id(),"token":token,"config_dir":usage_runtime::absolute(&root),"executable":std::env::current_exe().unwrap()});
    let executable = std::env::current_exe().unwrap();
    let binary = executable.with_file_name(if cfg!(windows) {
        "codex-usage-dashboard.exe"
    } else {
        "codex-usage-dashboard"
    });
    let prefs = read_window(&root);
    let chinese = fs::read(root.join("preferences.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .is_some_and(|v| v["language"] == "zh");
    let background = std::env::args().any(|a| a == "--background");
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .args(["--background"])
                .build(),
        )
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(Desktop {
            root: root.clone(),
            binary,
            service: Mutex::new(None),
            busy: AtomicBool::new(false),
            closing: AtomicBool::new(false),
            tray: AtomicBool::new(false),
            hide_on_close: AtomicBool::new(prefs.hide_on_close && cfg!(windows)),
            last_error: Mutex::new(String::new()),
        })
        .invoke_handler(tauri::generate_handler![
            retry_service,
            quit_desktop,
            native::desktop_settings,
            native::save_desktop_settings,
            native::choose_source,
            native::apply_sources,
            native::export_snapshot,
            native::check_update
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            let navigation = handle.clone();
            let initial_width = prefs.width.unwrap_or(1380.).clamp(760., 2400.);
            let initial_height = prefs.height.unwrap_or(900.).clamp(560., 1600.);
            let builder =
                WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                    .title("Codex Usage Dashboard")
                    .inner_size(initial_width, initial_height)
                    .min_inner_size(400., 500.)
                    .data_directory(usage_runtime::cache_dir(&root).join("webview"))
                    .on_page_load(|window, payload| {
                        if payload.event() == tauri::webview::PageLoadEvent::Finished
                            && is_bootstrap(payload.url())
                        {
                            let state = window.state::<Desktop>();
                            let error = state.last_error.lock().unwrap();
                            if !error.is_empty() {
                                let _ = window.eval(format!("showError({})", json!(*error)));
                            }
                        }
                    })
                    .visible(!background || cfg!(target_os = "linux"))
                    .on_navigation(move |url| {
                        let state = navigation.state::<Desktop>();
                        let allowed = is_bootstrap(url)
                            || state.service.lock().unwrap().as_ref().is_some_and(|s| {
                                url.origin().ascii_serialization() == s.record.url()
                            });
                        if !allowed && matches!(url.scheme(), "https" | "http") {
                            let _ = navigation.opener().open_url(url.as_str(), None::<&str>);
                        }
                        allowed
                    });
            #[cfg(target_os = "macos")]
            let builder = {
                let hash = usage_runtime::scope_id(&root);
                let mut identifier = [0u8; 16];
                for (index, byte) in identifier.iter_mut().enumerate() {
                    *byte = u8::from_str_radix(&hash[index * 2..index * 2 + 2], 16).unwrap();
                }
                builder.data_store_identifier(identifier)
            };
            let window = builder.build()?;
            if let (Some(x), Some(y)) = (prefs.x, prefs.y) {
                let visible = window.available_monitors()?.iter().any(|monitor| {
                    let position = monitor.position();
                    let size = monitor.size();
                    x >= position.x
                        && y >= position.y
                        && i64::from(x) + 120 < i64::from(position.x) + i64::from(size.width)
                        && i64::from(y) + 80 < i64::from(position.y) + i64::from(size.height)
                });
                if visible {
                    window.set_position(tauri::PhysicalPosition::new(x, y))?;
                } else {
                    window.center()?;
                }
            }
            if prefs.maximized {
                window.maximize()?;
            }
            let show_item = MenuItem::with_id(
                app,
                "show",
                if chinese { "显示窗口" } else { "Show" },
                true,
                Some("CmdOrCtrl+1"),
            )?;
            let settings_item = MenuItem::with_id(
                app,
                "settings",
                if chinese { "设置" } else { "Settings" },
                true,
                Some("CmdOrCtrl+,"),
            )?;
            let retry = MenuItem::with_id(
                app,
                "retry",
                if chinese { "重新连接" } else { "Reconnect" },
                true,
                Some("CmdOrCtrl+R"),
            )?;
            let exit = MenuItem::with_id(
                app,
                "quit",
                if chinese { "退出" } else { "Quit" },
                true,
                Some("CmdOrCtrl+Q"),
            )?;
            let menu = Menu::with_items(app, &[&show_item, &settings_item, &retry, &exit])?;
            let appmenu = Submenu::with_items(
                app,
                "Codex Usage Dashboard",
                true,
                &[&show_item, &settings_item, &retry, &exit],
            )?;
            #[cfg(target_os = "macos")]
            {
                appmenu.prepend(&PredefinedMenuItem::about(app, None, None)?)?;
                appmenu.insert(&PredefinedMenuItem::separator(app)?, 1)?;
                appmenu.insert(&PredefinedMenuItem::hide(app, None)?, 2)?;
                appmenu.insert(&PredefinedMenuItem::hide_others(app, None)?, 3)?;
                appmenu.insert(&PredefinedMenuItem::show_all(app, None)?, 4)?;
                appmenu.insert(&PredefinedMenuItem::separator(app)?, 5)?;
            }
            let edit = Submenu::with_items(
                app,
                if chinese { "编辑" } else { "Edit" },
                true,
                &[
                    &PredefinedMenuItem::undo(app, None)?,
                    &PredefinedMenuItem::redo(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::cut(app, None)?,
                    &PredefinedMenuItem::copy(app, None)?,
                    &PredefinedMenuItem::paste(app, None)?,
                    &PredefinedMenuItem::select_all(app, None)?,
                ],
            )?;
            let window_menu = Submenu::with_items(
                app,
                if chinese { "窗口" } else { "Window" },
                true,
                &[
                    &MenuItem::with_id(
                        app,
                        "minimize",
                        if chinese { "最小化" } else { "Minimize" },
                        true,
                        Some("CmdOrCtrl+M"),
                    )?,
                    &MenuItem::with_id(
                        app,
                        "maximize",
                        if chinese {
                            "最大化 / 还原"
                        } else {
                            "Maximize / Restore"
                        },
                        true,
                        None::<&str>,
                    )?,
                    &MenuItem::with_id(
                        app,
                        "fullscreen",
                        if chinese { "全屏" } else { "Full screen" },
                        true,
                        Some(if cfg!(target_os = "macos") {
                            "Ctrl+Cmd+F"
                        } else {
                            "F11"
                        }),
                    )?,
                    &MenuItem::with_id(
                        app,
                        "close",
                        if chinese {
                            "关闭窗口"
                        } else {
                            "Close window"
                        },
                        true,
                        Some("CmdOrCtrl+W"),
                    )?,
                ],
            )?;
            app.set_menu(Menu::with_items(app, &[&appmenu, &edit, &window_menu])?)?;
            // Adding the Win32 menu changes the client area. Restore the saved
            // client size after attaching it, so every reopen does not lose a row.
            #[cfg(windows)]
            if !prefs.maximized {
                window.set_size(tauri::LogicalSize::new(initial_width, initial_height))?;
            }
            app.on_menu_event(|app, event| match event.id().as_ref() {
                "show" => show(app, false),
                "settings" => show(app, true),
                "quit" => quit(app),
                "minimize" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.minimize();
                    }
                }
                "maximize" => {
                    if let Some(window) = app.get_webview_window("main") {
                        if window.is_maximized().unwrap_or(false) {
                            let _ = window.unmaximize();
                        } else {
                            let _ = window.maximize();
                        }
                    }
                }
                "fullscreen" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.set_fullscreen(!window.is_fullscreen().unwrap_or(false));
                    }
                }
                "close" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.close();
                    }
                }
                "retry" => {
                    let _ = launch_service(app);
                }
                _ => {}
            });
            if let Some(icon) = app.default_window_icon().cloned() {
                match TrayIconBuilder::new()
                    .icon(icon)
                    .menu(&menu)
                    .tooltip("Codex Usage Dashboard")
                    .on_tray_icon_event(|tray, event| {
                        if matches!(
                            event,
                            tauri::tray::TrayIconEvent::Click {
                                button: tauri::tray::MouseButton::Left,
                                button_state: tauri::tray::MouseButtonState::Up,
                                ..
                            }
                        ) {
                            show(tray.app_handle(), false);
                        }
                    })
                    .build(app)
                {
                    Ok(_) => app.state::<Desktop>().tray.store(true, Ordering::SeqCst),
                    Err(e) => eprintln!("tray unavailable: {e}"),
                }
            }
            if !app.state::<Desktop>().tray.load(Ordering::SeqCst) {
                window.show()?;
            }
            usage_runtime::atomic_json(
                &usage_runtime::runtime_dir(&root).join("desktop.json"),
                &record,
            )
            .map_err(|e| std::io::Error::other(e.to_string()))?;
            std::thread::spawn(move || {
                for mut stream in listener.incoming().flatten() {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
                    let mut line = String::new();
                    let value = BufReader::new(&mut stream)
                        .take(4096)
                        .read_line(&mut line)
                        .ok()
                        .and_then(|_| serde_json::from_str::<Value>(&line).ok());
                    if let Some(value) = value
                        && value["token"] == token
                    {
                        match value["action"].as_str() {
                            #[cfg(feature = "desktop-tests")]
                            Some("test_update_check" | "test_update_install") => {
                                let result = if let Some(window) = handle.get_webview_window("main")
                                {
                                    tauri::async_runtime::block_on(native::check_update(
                                        window,
                                        handle.clone(),
                                        value["action"] == "test_update_install",
                                    ))
                                } else {
                                    Err("main window missing".into())
                                };
                                let reply = match result {
                                    Ok(value) => json!({"ok":true,"result":value}),
                                    Err(error) => json!({"ok":false,"error":error}),
                                };
                                let _ = writeln!(stream, "{reply}");
                                continue;
                            }
                            Some("show") => show(&handle, false),
                            Some("settings") => show(&handle, true),
                            Some("quit") => {
                                let _ = stream.write_all(b"ok\n");
                                quit(&handle);
                                continue;
                            }
                            _ => continue,
                        }
                        let _ = stream.write_all(b"ok\n");
                    }
                }
            });
            launch_service(app.handle()).map_err(std::io::Error::other)?;
            let monitor = app.handle().clone();
            std::thread::spawn(move || {
                let mut failures = 0;
                loop {
                    std::thread::sleep(Duration::from_secs(3));
                    let state = monitor.state::<Desktop>();
                    if state.closing.load(Ordering::SeqCst) {
                        break;
                    }
                    let record = state
                        .service
                        .lock()
                        .unwrap()
                        .as_ref()
                        .map(|s| s.record.clone());
                    let Some(record) = record else {
                        continue;
                    };
                    if record.health().is_ok() {
                        failures = 0;
                        continue;
                    }
                    failures += 1;
                    if failures < 2 || state.busy.load(Ordering::SeqCst) {
                        continue;
                    }
                    *state.service.lock().unwrap() = None;
                    let _ = launch_service(&monitor);
                    failures = 0;
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                save_window(window.app_handle());
                let state = window.state::<Desktop>();
                if cfg!(target_os = "macos")
                    || (state.tray.load(Ordering::SeqCst)
                        && state.hide_on_close.load(Ordering::SeqCst)
                        && !cfg!(target_os = "linux"))
                {
                    api.prevent_close();
                    let _ = window.hide();
                } else {
                    quit(window.app_handle());
                }
            }
        })
        .build(tauri::generate_context!());
    let app = match app {
        Ok(app) => app,
        Err(error) => {
            fatal(error);
            return;
        }
    };
    app.run(|app, event| match event {
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { .. } => show(app, false),
        #[cfg(target_os = "macos")]
        RunEvent::ExitRequested { .. } => {
            // Window close already hides without destroying the window. Allow
            // explicit Dock/system Quit, while saving state and stopping recovery.
            app.state::<Desktop>().closing.store(true, Ordering::SeqCst);
            save_window(app);
        }
        RunEvent::Exit => {
            let state = app.state::<Desktop>();
            if let Some(service) = state.service.lock().unwrap().take()
                && service.owned
            {
                let _ = service.record.stop();
            }
            let _ = fs::remove_file(usage_runtime::runtime_dir(&state.root).join("desktop.json"));
        }
        _ => {}
    });
}
