use clap::Subcommand;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use usage_runtime::{Record, Result};
#[derive(Subcommand)]
pub enum Action {
    /// Installer coordination; only acts on this installation's executables.
    #[command(hide = true)]
    PrepareInstall {
        #[arg(long)]
        json: bool,
    },
    /// Open or focus the installed desktop client.
    Open {
        #[arg(long)]
        json: bool,
    },
    /// Inspect a running service without starting it.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Query the shared index without opening a window.
    Summary {
        #[arg(long, default_value = "today")]
        period: String,
        #[arg(long)]
        start: Option<String>,
        #[arg(long)]
        end: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Atomically export a snapshot from the shared index.
    Export {
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        redact: bool,
        #[arg(long)]
        json: bool,
    },
}
fn record(start: bool) -> Result<Option<Record>> {
    let root = crate::config::config_dir();
    if !start {
        return Ok(Record::read(&root)?.filter(|record| record.health().is_ok()));
    }
    let homes = crate::preferences::load()?.codex_homes;
    Ok(Some(
        usage_runtime::ensure_service(&std::env::current_exe()?, &root, &homes, None)?.record,
    ))
}
fn ready(record: &Record) -> Result<()> {
    let deadline = Instant::now() + usage_runtime::TIMEOUT;
    loop {
        let health = record.health()?;
        match health["index"]["phase"].as_str() {
            Some("ready") => return Ok(()),
            Some("error") => {
                return Err(format!("index failed: {}", health["index"]["error"]).into());
            }
            _ => {}
        }
        if Instant::now() > deadline {
            return Err("index timeout".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
pub fn run(action: Action) {
    let result = (|| -> Result<Value> {
        Ok(match action {
            Action::PrepareInstall { .. } => {
                let root = crate::config::config_dir();
                let binary = std::env::current_exe()?;
                let desktop = binary.with_file_name(if cfg!(windows) {
                    "cousash-desktop.exe"
                } else {
                    "cousash-desktop"
                });
                if let Ok(bytes) =
                    std::fs::read(usage_runtime::runtime_dir(&root).join("desktop.json"))
                    && let Ok(value) = serde_json::from_slice::<Value>(&bytes)
                    && value["executable"].as_str().is_some_and(|p| {
                        usage_runtime::absolute(std::path::Path::new(p))
                            == usage_runtime::absolute(&desktop)
                    })
                {
                    let _ = usage_runtime::activate_desktop(&root, "quit");
                }
                if let Some(record) = Record::read(&root)?
                    && usage_runtime::absolute(&record.executable)
                        == usage_runtime::absolute(&binary)
                    && record.health().is_ok()
                {
                    // The desktop may already be shutting down the same server.
                    if let Err(error) = record.stop()
                        && record.health().is_ok()
                    {
                        return Err(error);
                    }
                }
                json!({"prepared":true})
            }
            Action::Status { .. } => match record(false)? {
                Some(r) => r.public_status()?,
                None => json!({"state":"stopped"}),
            },
            Action::Open { .. } => {
                if usage_runtime::activate_desktop(&crate::config::config_dir(), "show").is_err() {
                    let current = std::env::current_exe()?;
                    let binary = current.with_file_name(if cfg!(windows) {
                        "cousash-desktop.exe"
                    } else {
                        "cousash-desktop"
                    });
                    if !binary.is_file() {
                        return Err("desktop_not_installed: install the desktop package or run its executable".into());
                    }
                    usage_runtime::detached(&mut Command::new(binary))
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()?;
                    let deadline = Instant::now() + Duration::from_secs(15);
                    loop {
                        if usage_runtime::activate_desktop(&crate::config::config_dir(), "show")
                            .is_ok()
                        {
                            break;
                        }
                        if Instant::now() > deadline {
                            return Err("desktop_start_timeout".into());
                        }
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
                json!({"opened":true})
            }
            Action::Summary {
                period, start, end, ..
            } => {
                let r = record(true)?.ok_or("service stopped")?;
                ready(&r)?;
                let mut url = usage_runtime::ureq_url(&format!("{}/api/summary", r.url()))?;
                url.query_pairs_mut().append_pair("period", &period);
                if let Some(start) = start {
                    url.query_pairs_mut().append_pair("start", &start);
                }
                if let Some(end) = end {
                    url.query_pairs_mut().append_pair("end", &end);
                }
                usage_runtime::get_json(url.as_str())?
            }
            Action::Export { output, redact, .. } => {
                let r = record(true)?.ok_or("service stopped")?;
                ready(&r)?;
                let output = usage_runtime::absolute(&output);
                let parent = output.parent().ok_or("invalid output path")?;
                let path = if output
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("json"))
                {
                    "json"
                } else {
                    "json.gz"
                };
                let mut response = usage_runtime::agent_with_timeout(Duration::from_secs(600))
                    .get(format!(
                        "{}/api/export.{path}{}",
                        r.url(),
                        if redact { "?mode=redacted" } else { "" }
                    ))
                    .call()?;
                let mut file = tempfile::NamedTempFile::new_in(parent)?;
                std::io::copy(&mut response.body_mut().as_reader(), &mut file)?;
                file.as_file().sync_all()?;
                file.persist(&output).map_err(|e| e.error)?;
                json!({"output":output,"redacted":redact})
            }
        })
    })();
    match result {
        Ok(data) => println!("{}", json!({"schema_version":1,"ok":true,"data":data})),
        Err(error) => {
            let message = error.to_string();
            let code = if message.starts_with("desktop_not_installed") {
                "desktop_not_installed"
            } else if message.contains("timeout") {
                "timeout"
            } else {
                "operation_failed"
            };
            println!(
                "{}",
                json!({"schema_version":1,"ok":false,"error":{"code":code,"message":message}})
            );
            std::process::exit(1);
        }
    }
}
