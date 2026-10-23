mod config;
mod http;
mod launcher;
mod network;
mod service;
mod snapshot;
use clap::Parser;
use std::{net::SocketAddr, path::PathBuf};
#[derive(Parser)]
#[command(version, about = "Local read-only Codex Usage Dashboard")]
struct Args {
    #[arg(long)]
    codex_home: Vec<PathBuf>,
    #[arg(long)]
    no_auto_windows: bool,
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    #[arg(long, default_value_t = 8765)]
    port: u16,
    /// Kept for CLI compatibility; the index uses one serialized writer.
    #[arg(long, default_value_t = 1)]
    parse_workers: usize,
    #[arg(long)]
    open: bool,
    /// Run the HTTP server in the foreground (default: launch in background).
    #[arg(long, conflicts_with_all = ["launch", "stop", "shortcut"])]
    serve: bool,
    /// Start or reuse the native dashboard, then open the browser.
    #[arg(long)]
    launch: bool,
    #[arg(long)]
    no_browser: bool,
    /// Gracefully stop the authenticated, managed dashboard on port 8765.
    #[arg(long, conflicts_with_all = ["launch", "once", "export_snapshot", "shortcut"])]
    stop: bool,
    /// Install a desktop launcher pointing at this native executable.
    #[arg(long)]
    shortcut: bool,
    #[arg(long)]
    once: bool,
    #[arg(long)]
    json: bool,
    /// Export lossless .json.gz by default; an explicit .json path writes legacy JSON.
    #[arg(long,num_args=0..=1,default_missing_value="")]
    export_snapshot: Option<String>,
    #[arg(long)]
    build_info: bool,
    #[arg(long)]
    timezone: Option<String>,
}
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if args.build_info {
        println!(
            "{}",
            serde_json::json!({"build_id":env!("COUSASH_BUILD_ID"),"version":env!("CARGO_PKG_VERSION"),"ui":env!("COUSASH_UI")})
        );
        return Ok(());
    }
    if args.port != 8765 {
        return Err("this dashboard uses fixed port 8765".into());
    }
    if !args.serve && !args.once && args.export_snapshot.is_none() {
        return launcher::run(&args);
    }
    let zone = args
        .timezone
        .or_else(|| std::env::var("TZ").ok())
        .or_else(|| iana_time_zone::get_timezone().ok())
        .unwrap_or_else(|| "UTC".into())
        .parse()?;
    let sources = config::sources(&args.codex_home, !args.no_auto_windows);
    // Claim the fixed port before opening the index. A previous Windows/WSL
    // binding can outlive its process, and waiting must not hold the index lock.
    let listener = if args.serve {
        let address: SocketAddr = format!("{}:{}", args.host, args.port).parse()?;
        Some(network::bind(address).await?)
    } else {
        None
    };
    // One-shot commands must coexist with the server's exclusive writer. They
    // share read-only sources/remotes and use a private disposable index.
    let once_index = (args.once || args.export_snapshot.is_some())
        .then(tempfile::tempdir)
        .transpose()?;
    let database = once_index
        .as_ref()
        .map(|dir| dir.path().join("index.sqlite3"));
    let (state, worker) = service::start(config::config_dir(), sources, zone, database)?;
    if args.once || args.export_snapshot.is_some() {
        loop {
            if state.current.read().unwrap().is_some() {
                break;
            }
            let status = state.status.read().unwrap().clone();
            if status["phase"] == "error" {
                return Err(status["error"].to_string().into());
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        if let Some(path) = args.export_snapshot {
            let path = if path.is_empty() {
                std::env::current_dir()?.join(format!("cousash-{}.json.gz", state.device))
            } else {
                usage_core::project::expand_home(&path)
            };
            let path = if path.is_absolute() {
                path
            } else {
                std::env::current_dir()?.join(path)
            };
            if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
            {
                config::atomic_json(&path, &state.export().map_err(|e| format!("{e:?}"))?)?;
            } else {
                let view = state.local_view().map_err(|e| format!("{e:?}"))?;
                snapshot::write_gzip(
                    &path,
                    &state
                        .export_document(&view, None)
                        .map_err(|e| format!("{e:?}"))?,
                )?;
            }
            println!("{}", path.display());
        } else {
            let (view, published) = state
                .view(
                    &std::collections::HashMap::from([("period".into(), "all".into())]),
                    false,
                )
                .map_err(|e| format!("{e:?}"))?;
            if args.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &serde_json::json!({"generated_at":view.generated_at,"codex_home":config::homes_display(&state.sources),"codex_sources":published.sources,"summary":view.summary().map_err(|e|format!("{e:?}"))?,"sessions":view.wire_summaries()?,"cache_metrics":state.status.read().unwrap()["metrics"]})
                    )?
                );
            } else {
                println!(
                    "Codex homes: {}\nSessions: {}\nTotal tokens: {}",
                    config::homes_display(&state.sources),
                    view.summaries().count(),
                    view.summary()
                        .map_err(|e| format!("{e:?}"))?
                        .usage
                        .total_tokens
                );
            }
        }
        let _ = state.sender.send(service::Command::Stop);
        let _ = worker.join();
        return Ok(());
    }
    let listener = listener.expect("server listener was bound before indexing");
    let address = listener.local_addr()?;
    if args.open && !args.no_browser {
        std::thread::spawn(|| {
            let _ = launcher::open_browser();
        });
    }
    println!("Codex Usage Dashboard: http://{address}/");
    axum::serve(listener, http::router(state.clone()))
        .with_graceful_shutdown({
            let state = state.clone();
            async move {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {},
                    _ = state.shutdown.notified() => {},
                }
                state
                    .events
                    .send_replace(serde_json::json!({"phase":"shutdown"}));
            }
        })
        .await?;
    let _ = state.sender.send(service::Command::Stop);
    let _ = worker.join();
    Ok(())
}
