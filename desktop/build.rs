fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        // Tests that link Muda need the same Common Controls version as the app.
        println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-tests=/MANIFESTDEPENDENCY:type='win32' \
             name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' \
             publicKeyToken='6595b64144ccf1df' language='*'"
        );
    }

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
