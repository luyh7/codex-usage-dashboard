use super::*;
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_updater::UpdaterExt;

#[tauri::command]
pub async fn desktop_settings(
    window: WebviewWindow,
    app: tauri::AppHandle,
) -> std::result::Result<Value, String> {
    trusted_live(&window, &app).await?;
    let state = app.state::<Desktop>();
    Ok(
        json!({"hide_on_close":state.hide_on_close.load(Ordering::SeqCst),"can_hide":state.tray.load(Ordering::SeqCst)&&cfg!(windows),"autostart":app.autolaunch().is_enabled().map_err(|e|e.to_string())?,"version":env!("CARGO_PKG_VERSION"),"channel":if cfg!(target_os="linux")&&std::env::var_os("APPIMAGE").is_none(){"package"}else{"direct"}}),
    )
}
#[tauri::command]
pub async fn save_desktop_settings(
    window: WebviewWindow,
    app: tauri::AppHandle,
    hide_on_close: bool,
    autostart: bool,
) -> std::result::Result<(), String> {
    trusted_live(&window, &app).await?;
    let state = app.state::<Desktop>();
    if hide_on_close && (!state.tray.load(Ordering::SeqCst) || !cfg!(windows)) {
        return Err("this desktop session does not support hiding to tray".into());
    }
    if std::env::var_os("COUSASH_CONFIG_DIR").is_some() && autostart {
        return Err("autostart is available only for the installed application".into());
    }
    let was_enabled = app.autolaunch().is_enabled().map_err(|e| e.to_string())?;
    if autostart != was_enabled {
        if autostart {
            app.autolaunch().enable()
        } else {
            app.autolaunch().disable()
        }
        .map_err(|e| e.to_string())?;
    }
    let mut value = read_window(&state.root);
    value.hide_on_close = hide_on_close;
    if let Err(error) =
        usage_runtime::atomic_json(&state.root.join("desktop-settings.json"), &value)
    {
        if autostart != was_enabled {
            let _ = if was_enabled {
                app.autolaunch().enable()
            } else {
                app.autolaunch().disable()
            };
        }
        return Err(error.to_string());
    }
    state.hide_on_close.store(hide_on_close, Ordering::SeqCst);
    Ok(())
}
#[tauri::command]
pub async fn choose_source(
    window: WebviewWindow,
    app: tauri::AppHandle,
) -> std::result::Result<Option<String>, String> {
    trusted_live(&window, &app).await?;
    let path = tauri::async_runtime::spawn_blocking(move || {
        let mut picker = app.dialog().file().set_parent(&window);
        if let Ok(home) = app.path().home_dir() {
            picker = picker.set_directory(home);
        }
        picker.blocking_pick_folder()
    })
    .await
    .map_err(|e| e.to_string())?;
    path.map(|p| {
        p.into_path()
            .map(|p| p.to_string_lossy().into_owned())
            .map_err(|e| e.to_string())
    })
    .transpose()
}
#[tauri::command]
pub async fn apply_sources(
    window: WebviewWindow,
    app: tauri::AppHandle,
) -> std::result::Result<(), String> {
    trusted_live(&window, &app).await?;
    tauri::async_runtime::spawn_blocking(move || -> std::result::Result<(), String> {
        let state = app.state::<Desktop>();
        let mut service = state.service.lock().unwrap();
        if let Some(current) = service.as_ref() {
            if !current.owned {
                return Err(
                    "Quit the independently running dashboard before changing its log sources."
                        .into(),
                );
            }
            current.record.stop().map_err(|e| e.to_string())?;
        }
        *service = None;
        drop(service);
        window
            .navigate(bootstrap_url())
            .map_err(|e| e.to_string())?;
        launch_service(&app)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn export_snapshot(
    window: WebviewWindow,
    app: tauri::AppHandle,
    redacted: bool,
) -> std::result::Result<bool, String> {
    trusted_live(&window, &app).await?;
    tauri::async_runtime::spawn_blocking(move||->Result<bool>{
        let mut picker=app.dialog().file().set_parent(&window).set_file_name(if redacted{"cousash-shared.json.gz"}else{"cousash-snapshot.json.gz"}).add_filter("Snapshot",&["json.gz"]);
        // AppImage changes cwd to its temporary AppDir. Never offer that as a
        // user's export destination; it disappears when the application exits.
        if let Some(directory)=app.path().document_dir().ok().filter(|path|path.is_dir()).or_else(||app.path().home_dir().ok()) {picker=picker.set_directory(directory);}
        let Some(path)=picker.blocking_save_file() else{return Ok(false);};
        let path=path.into_path()?;
        let base=app.state::<Desktop>().service.lock().unwrap().as_ref().ok_or("service stopped")?.record.url();
        let token=usage_runtime::get_json(&format!("{base}/api/security/token"))?;
        let mode=if redacted{"?mode=redacted"}else{""};
        let mut job:Value=usage_runtime::agent().post(format!("{base}/api/export/start{mode}")).header("x-codex-write-token",token["token"].as_str().ok_or("missing write token")?).send_json(json!({}))?.body_mut().read_json()?;
        let deadline=std::time::Instant::now()+Duration::from_secs(600);
        loop {
            let _=window.eval(format!("window.dispatchEvent(new CustomEvent('cousash-export-progress',{{detail:{}}}))",json!({"percent":job["percent"],"phase":if job["phase"]=="ready"{"downloading"}else{job["phase"].as_str().unwrap_or("preparing")}})));
            if job["phase"]=="error" {return Err(format!("export failed: {}",job["error"]).into());}
            if job["phase"]=="ready"{break;}
            if std::time::Instant::now()>deadline{return Err("export timed out".into());}
            std::thread::sleep(Duration::from_millis(150));
            job=usage_runtime::get_json(&format!("{base}/api/export/progress?id={}",job["id"].as_str().ok_or("invalid export job")?))?;
        }
        let mut url=usage_runtime::ureq_url(&format!("{base}/api/export.json.gz{mode}"))?;
        url.query_pairs_mut().append_pair("job",job["id"].as_str().ok_or("invalid export job")?);
        let mut response=usage_runtime::agent_with_timeout(Duration::from_secs(600)).get(url.as_str()).call()?;
        let mut file=tempfile::NamedTempFile::new_in(path.parent().ok_or("invalid destination")?)?;
        std::io::copy(&mut response.body_mut().as_reader(),&mut file)?;file.as_file().sync_all()?;file.persist(path).map_err(|e|e.error)?;Ok(true)
    }).await.map_err(|e|e.to_string())?.map_err(|e|e.to_string())
}
#[tauri::command]
pub async fn check_update(
    window: WebviewWindow,
    app: tauri::AppHandle,
    install: bool,
) -> std::result::Result<Value, String> {
    trusted_live(&window, &app).await?;
    if cfg!(target_os = "linux") && std::env::var_os("APPIMAGE").is_none() {
        return Ok(
            json!({"state":"package","message":"Update using your package manager or install the newer .deb package."}),
        );
    }
    let config = &app.config().plugins.0;
    if config
        .get("updater")
        .and_then(|v| v["pubkey"].as_str())
        .is_none_or(str::is_empty)
    {
        return Ok(
            json!({"state":"unavailable","message":"This development package has no signed update channel."}),
        );
    }
    let updater = app
        .updater_builder()
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?;
    match updater.check().await.map_err(|e| e.to_string())? {
        None => Ok(json!({"state":"current"})),
        Some(update) => {
            if !install {
                return Ok(
                    json!({"state":"available","version":update.version,"notes":update.body}),
                );
            }
            let bytes=update.download(|received,total|{let _=window.eval(format!("window.dispatchEvent(new CustomEvent('cousash-update-progress',{{detail:{}}}))",json!({"received":received,"total":total})));},||{}).await.map_err(|e|e.to_string())?;
            let state = app.state::<Desktop>();
            if state.busy.swap(true, Ordering::SeqCst) {
                return Err(
                    "The application is still starting or reconnecting. Retry the update shortly."
                        .into(),
                );
            }
            save_window(&app);
            let record = {
                let service = state.service.lock().unwrap();
                if service.as_ref().is_some_and(|s| !s.owned) {
                    state.busy.store(false, Ordering::SeqCst);
                    return Err("Quit the independently running dashboard before updating this application.".into());
                }
                service.as_ref().map(|s| s.record.clone())
            };
            if let Some(record) = record
                && let Err(error) = record.stop()
            {
                state.busy.store(false, Ordering::SeqCst);
                return Err(error.to_string());
            }
            *state.service.lock().unwrap() = None;
            if let Err(error) = update.install(bytes) {
                state.busy.store(false, Ordering::SeqCst);
                let _ = launch_service(&app);
                return Err(error.to_string());
            }
            app.state::<Desktop>().closing.store(true, Ordering::SeqCst);
            app.restart();
        }
    }
}
