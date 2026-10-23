fn main() {
    let manifest = format!("service-build-{}.json", std::env::var("TARGET").unwrap());
    println!("cargo:rerun-if-changed={manifest}");
    let info: serde_json::Value = serde_json::from_slice(
        &std::fs::read(manifest).expect("run node tools/desktop.mjs prepare first"),
    )
    .unwrap();
    assert_eq!(
        info["version"].as_str(),
        Some(env!("CARGO_PKG_VERSION")),
        "desktop and service versions must match"
    );
    println!(
        "cargo:rustc-env=COUSASH_SERVICE_BUILD={}",
        info["build_id"].as_str().unwrap()
    );
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "retry_service",
            "quit_desktop",
            "desktop_settings",
            "save_desktop_settings",
            "choose_source",
            "apply_sources",
            "export_snapshot",
            "check_update",
        ]),
    ))
    .expect("desktop build configuration");
}
