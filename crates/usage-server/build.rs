use sha2::{Digest, Sha256};
use std::{env, fs, path::Path};
fn collect(path: &Path, hash: &mut Sha256) {
    println!("cargo:rerun-if-changed={}", path.display());
    if path.is_dir() {
        let mut paths: Vec<_> = fs::read_dir(path)
            .unwrap()
            .map(|p| p.unwrap().path())
            .collect();
        paths.sort();
        for path in paths {
            collect(&path, hash);
        }
    } else {
        hash.update(path.to_string_lossy().as_bytes());
        hash.update(fs::read(path).unwrap());
    }
}
fn main() {
    let root = Path::new("../..");
    let mut hash = Sha256::new();
    for path in [
        "Cargo.toml",
        "Cargo.lock",
        ".cargo/config.toml",
        "crates/usage-core/src",
        "crates/usage-core/Cargo.toml",
        "crates/usage-core/data",
        "crates/usage-server/src",
        "crates/usage-server/build.rs",
        "crates/usage-server/Cargo.toml",
    ] {
        collect(&root.join(path), &mut hash);
    }
    collect(&root.join("web/shared"), &mut hash);
    let out = env::var("OUT_DIR").unwrap();
    web_assets(root, Path::new(&out), &mut hash);
    println!("cargo:rustc-env=COUSASH_BUILD_ID={:x}", hash.finalize());
}

fn web_assets(root: &Path, out: &Path, hash: &mut Sha256) {
    let web = root.join("web");
    let dist = web.join("dist");
    println!("cargo:rerun-if-changed={}", dist.display());
    assert!(
        dist.join("index.html").is_file(),
        "build the dashboard first: npm ci --prefix web && npm run build --prefix web"
    );
    let mut rows = String::from("pub static ASSETS: &[(&str,&str,&[u8])] = &[\n");
    if dist.join("index.html").is_file() {
        for path in [
            "src",
            "package.json",
            "package-lock.json",
            "vite.config.ts",
            "svelte.config.js",
            "tsconfig.json",
            "index.html",
        ] {
            collect(&web.join(path), hash);
        }
        collect(&dist, hash);
        fn files(path: &Path, list: &mut Vec<std::path::PathBuf>) {
            for entry in fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    files(&path, list)
                } else {
                    list.push(path)
                }
            }
        }
        let mut paths = Vec::new();
        files(&dist, &mut paths);
        paths.sort();
        for path in paths {
            let url = format!(
                "/{}",
                path.strip_prefix(&dist)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            );
            let mime = match path.extension().and_then(|s| s.to_str()).unwrap_or("") {
                "html" => "text/html; charset=utf-8",
                "js" => "text/javascript; charset=utf-8",
                "css" => "text/css; charset=utf-8",
                "json" | "map" => "application/json",
                "svg" => "image/svg+xml",
                _ => "application/octet-stream",
            };
            rows.push_str(&format!(
                "({url:?},{mime:?},include_bytes!({:?})),\n",
                path.canonicalize().unwrap().to_string_lossy()
            ));
        }
    }
    println!("cargo:rustc-env=COUSASH_UI=svelte");
    rows.push_str("];\n");
    fs::write(out.join("assets.rs"), rows).unwrap();
}
