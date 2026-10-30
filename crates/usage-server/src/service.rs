use crate::config;
use chrono::Utc;
use chrono_tz::Tz;
use notify::Watcher;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::{Arc, Mutex, RwLock, mpsc},
    time::{Duration, Instant},
};
use tokio::sync::{oneshot, watch};
use usage_core::{
    Error, PriceSchedule,
    fsinfo::{LOCAL_POLL, SourcePolicy},
    index::{Index, IndexConfig, Snapshot},
    parser::LogSource,
    query::Period,
    remote::{Metadata, Store},
    view::{QueryError, View},
};
#[derive(Clone)]
pub struct Published {
    pub snapshot: Arc<Snapshot>,
    pub local: Arc<Snapshot>,
    pub local_sources: Vec<LogSource>,
    pub sources: Vec<Value>,
    pub remotes: Vec<Metadata>,
}
pub enum Command {
    Refresh(oneshot::Sender<Result<Value, Error>>),
    Wake(Vec<PathBuf>),
    Remote(String, Value, oneshot::Sender<Result<Value, Error>>),
    Sources(
        Vec<PathBuf>,
        Vec<PathBuf>,
        oneshot::Sender<Result<Value, Error>>,
    ),
    /// Source polling settings changed; reload them from preferences.
    Reconfigure,
    Stop,
}
type CachedView = (String, Arc<View>, Arc<Published>);
pub struct State {
    redaction_key: [u8; 32],
    pub shutdown: tokio::sync::Notify,
    pub current: RwLock<Option<Arc<Published>>>,
    pub status: RwLock<Value>,
    pub events: watch::Sender<Value>,
    pub sender: mpsc::SyncSender<Command>,
    pub device: String,
    pub hostname: String,
    pub sources: Vec<LogSource>,
    /// Effective polling policy for each active source, keyed by its home.
    pub policies: RwLock<HashMap<PathBuf, SourcePolicy>>,
    pub zone: Tz,
    views: Mutex<VecDeque<CachedView>>,
    pub query_slots: Arc<tokio::sync::Semaphore>,
    pub exports: Mutex<VecDeque<Arc<crate::snapshot::ExportJob>>>,
}
impl State {
    pub fn active_sources(&self) -> Vec<LogSource> {
        self.current
            .read()
            .unwrap()
            .as_ref()
            .map(|publication| publication.local_sources.clone())
            .unwrap_or_else(|| self.sources.clone())
    }
    pub fn redact(&self, value: &Value) -> Value {
        crate::redact::Redactor(&self.redaction_key).document(value)
    }
    pub fn publication(&self) -> Result<Arc<Published>, QueryError> {
        self.current
            .read()
            .unwrap()
            .clone()
            .ok_or_else(|| QueryError::Invalid("index is loading".into()))
    }
    pub fn view(
        &self,
        params: &std::collections::HashMap<String, String>,
        require_revision: bool,
    ) -> Result<(Arc<View>, Arc<Published>), QueryError> {
        let current = self.publication()?;
        let mut cache = self.views.lock().unwrap();
        let revision = params
            .get("snapshot_token")
            .or_else(|| params.get("revision"))
            .filter(|s| !s.is_empty());
        if require_revision {
            let token =
                revision.ok_or_else(|| QueryError::Invalid("snapshot_token is required".into()))?;
            let view = cache
                .iter()
                .find(|(_, v, _)| &v.revision == token)
                .map(|(_, v, publication)| (v.clone(), publication.clone()))
                .ok_or(QueryError::Stale)?;
            return Ok(view);
        }
        let zone = params
            .get("timezone")
            .map(|s| {
                s.parse::<Tz>()
                    .map_err(|_| QueryError::Invalid("invalid timezone".into()))
            })
            .transpose()?
            .unwrap_or(self.zone);
        let now = Utc::now();
        let key = serde_json::to_string(&(
            current.snapshot.revision,
            &current.snapshot.generated_at,
            params.get("period").map(String::as_str).unwrap_or("today"),
            params.get("start"),
            params.get("end"),
            params.get("model"),
            zone.name(),
            now.with_timezone(&zone).date_naive(),
        ))?;
        if let Some((_, view, _)) = cache.iter().find(|(k, _, _)| k == &key) {
            return Ok((view.clone(), current));
        }
        let period = Period::bounds(
            params.get("period").map(String::as_str).unwrap_or("today"),
            params.get("start").map(String::as_str),
            params.get("end").map(String::as_str),
            zone,
            now,
        )?;
        let view = Arc::new(View::with_models(
            &current.snapshot,
            period,
            zone,
            &PriceSchedule::bundled(),
            params.get("model").map(String::as_str).unwrap_or(""),
        )?);
        cache.push_back((key, view.clone(), current.clone()));
        while cache.len() > 8 {
            cache.pop_front();
        }
        Ok((view, current))
    }
    pub fn local_view(&self) -> Result<(Arc<View>, Arc<Published>), QueryError> {
        let current = self.publication()?;
        let period = Period::bounds(
            "all",
            None,
            None,
            self.zone,
            usage_core::timestamp::parse(&current.local.generated_at).unwrap_or_else(Utc::now),
        )?;
        let view = Arc::new(View::new(
            &current.local,
            period,
            self.zone,
            &PriceSchedule::bundled(),
        )?);
        Ok((view, current))
    }
    pub fn export(&self) -> Result<Value, QueryError> {
        let (view, published) = self.local_view()?;
        let details: serde_json::Map<String, Value> = view
            .details()
            .map(|d| Ok((d.identity.uid.clone(), serde_json::to_value(d)?)))
            .collect::<Result<_, serde_json::Error>>()?;
        Ok(
            json!({"schema":usage_core::remote::SCHEMA,"version":1,"exported_at":usage_core::timestamp::format(Utc::now()),"device":{"short_code":self.device,"label":self.hostname,"platform":config::platform(),"hostname":self.hostname},"snapshot":{"generated_at":view.generated_at,"codex_home":config::homes_display(&published.local_sources),"codex_sources":config::source_rows(&published.local_sources),"summary":view.summary()?,"sessions":view.summaries().collect::<Vec<_>>(),"details_by_uid":details,"daily_usage":view.daily_usage}}),
        )
    }
}
impl State {
    pub fn export_document<'a>(
        &self,
        view: &'a View,
        sources: &[LogSource],
        progress: Option<&'a std::sync::atomic::AtomicUsize>,
    ) -> Result<crate::snapshot::Document<'a>, QueryError> {
        crate::snapshot::document(
            view,
            json!({"short_code":self.device,"label":self.hostname,"platform":config::platform(),"hostname":self.hostname}),
            sources,
            progress,
        )
    }
}
fn update_status(state: &State, status: Value, notify: bool) {
    let mut current = state.status.write().unwrap();
    if *current != status {
        let phase_changed = current["phase"] != status["phase"];
        *current = status.clone();
        if notify || phase_changed {
            state.events.send_replace(status);
        }
    }
}
pub fn start(
    root: PathBuf,
    sources: Vec<LogSource>,
    zone: Tz,
    database: Option<PathBuf>,
    auto_windows: bool,
    managed: bool,
) -> Result<(Arc<State>, std::thread::JoinHandle<()>), Error> {
    let device = config::device_code(&root)?;
    let hostname = config::hostname();
    let (sender, receiver) = mpsc::sync_channel(128);
    let initial = json!({"phase":"loading","completed":0,"total":0,"revision":0});
    let (events, _) = watch::channel(initial.clone());
    let state = Arc::new(State {
        redaction_key: crate::redact::load_key(&root)?,
        shutdown: tokio::sync::Notify::new(),
        current: RwLock::new(None),
        status: RwLock::new(initial),
        events,
        sender,
        device,
        hostname,
        sources: sources.clone(),
        policies: RwLock::new(HashMap::new()),
        zone,
        views: Mutex::new(VecDeque::new()),
        query_slots: Arc::new(tokio::sync::Semaphore::new(4)),
        exports: Mutex::new(VecDeque::new()),
    });
    let shared = state.clone();
    let handle = std::thread::Builder::new()
        .name("usage-index-writer".into())
        .spawn(move || {
            let options = WorkerOptions {
                database,
                auto_windows,
                managed,
            };
            if let Err(error) = run_worker(&shared, root, sources, zone, receiver, options) {
                update_status(
                    &shared,
                    json!({"phase":"error","error":error.to_string(),"revision":0}),
                    true,
                );
            }
        })?;
    Ok((state, handle))
}
fn relevant_event(path: &std::path::Path, sources: &[LogSource]) -> bool {
    sources.iter().any(|source| {
        path == source.home
            || path.starts_with(source.home.join("sessions"))
            || path.starts_with(source.home.join("archived_sessions"))
            || (path.parent() == Some(source.home.as_path())
                && path.file_name().is_some_and(|name| {
                    ["session_index.jsonl", "logs_2.sqlite", "logs_2.sqlite-wal"]
                        .iter()
                        .any(|allowed| name == *allowed)
                }))
    })
}
fn refresh_watches(
    watcher: &mut Option<notify::RecommendedWatcher>,
    sources: &[LogSource],
    watched: &mut HashMap<PathBuf, usage_core::inventory::FileStamp>,
) -> usize {
    let Some(watcher) = watcher.as_mut() else {
        return 0;
    };
    let desired: std::collections::HashSet<_> = sources
        .iter()
        .flat_map(|source| {
            [
                source.home.clone(),
                source.home.join("sessions"),
                source.home.join("archived_sessions"),
            ]
        })
        .collect();
    let removed: Vec<_> = watched
        .keys()
        .filter(|path| !desired.contains(*path))
        .cloned()
        .collect();
    for path in removed {
        let _ = watcher.unwatch(&path);
        watched.remove(&path);
    }
    for source in sources {
        for (path, mode) in [
            (source.home.clone(), notify::RecursiveMode::NonRecursive),
            (
                source.home.join("sessions"),
                notify::RecursiveMode::Recursive,
            ),
            (
                source.home.join("archived_sessions"),
                notify::RecursiveMode::Recursive,
            ),
        ] {
            let stamp = usage_core::inventory::FileStamp::read(&path).ok();
            if stamp.as_ref().is_some_and(|stamp| {
                watched
                    .get(&path)
                    .is_some_and(|old| old.same_identity(stamp))
            }) {
                continue;
            }
            if watched.remove(&path).is_some() {
                let _ = watcher.unwatch(&path);
            }
            if let Some(stamp) = stamp
                && watcher.watch(&path, mode).is_ok()
            {
                watched.insert(path, stamp);
            }
        }
    }
    watched.len()
}
type RemoteReply = oneshot::Sender<Result<Value, Error>>;
pub const SOURCE_CONFLICT: &str = "log sources changed in another window; reload and retry";

fn save_sources(
    root: &std::path::Path,
    homes: &[PathBuf],
    sources: &[LogSource],
    managed: bool,
) -> Result<(), Error> {
    let previous = crate::preferences::load().map_err(Error::InvalidData)?;
    let runtime = usage_runtime::runtime_dir(root);
    let _lock = if managed {
        Some(
            usage_runtime::lock(&runtime, "startup.lock", Duration::from_secs(10))
                .map_err(|e| Error::InvalidData(e.to_string()))?,
        )
    } else {
        None
    };
    let mut record = if managed {
        let record = usage_runtime::Record::read(root)
            .map_err(|e| Error::InvalidData(e.to_string()))?
            .ok_or_else(|| Error::InvalidData("managed service record is missing".into()))?;
        if record.pid != std::process::id()
            || std::env::var("COUSASH_INSTANCE_ID").ok().as_deref()
                != Some(record.instance_id.as_str())
        {
            return Err(Error::InvalidData(
                "managed service identity mismatch".into(),
            ));
        }
        Some(record)
    } else {
        None
    };
    crate::preferences::save_patch(json!({"codex_homes":homes})).map_err(|error| match error {
        crate::preferences::SaveError::Invalid(e) | crate::preferences::SaveError::Storage(e) => {
            Error::InvalidData(e)
        }
    })?;
    if let Some(record) = record.as_mut() {
        record.codex_homes = sources.iter().map(|source| source.home.clone()).collect();
        if let Err(error) = usage_runtime::atomic_json(&runtime.join("instance.json"), record) {
            let rollback =
                crate::preferences::save_patch(json!({"codex_homes":previous.codex_homes}));
            return Err(Error::InvalidData(match rollback {
                Ok(_) => error.to_string(),
                Err(
                    crate::preferences::SaveError::Invalid(e)
                    | crate::preferences::SaveError::Storage(e),
                ) => format!("{error}; could not restore log sources: {e}"),
            }));
        }
    }
    Ok(())
}

struct WorkerOptions {
    database: Option<PathBuf>,
    auto_windows: bool,
    managed: bool,
}

fn load_policies(shared: &State, index: &mut Index, sources: &[LogSource]) -> Vec<SourcePolicy> {
    let polling = crate::preferences::load()
        .map(|p| p.source_polling)
        .unwrap_or_default();
    let policies = config::source_policies(sources, &polling);
    index.set_network_sources(
        sources
            .iter()
            .zip(&policies)
            .filter(|(_, policy)| policy.network)
            .map(|(source, _)| source.id.clone()),
    );
    *shared.policies.write().unwrap() = sources
        .iter()
        .map(|source| source.home.clone())
        .zip(policies.iter().copied())
        .collect();
    policies
}
fn polling_status(sources: &[LogSource], policies: &[SourcePolicy]) -> Value {
    sources
        .iter()
        .zip(policies)
        .map(|(source, policy)| json!({"id":source.id,"network":policy.network,"poll_interval_ms":policy.poll_interval.as_millis() as u64}))
        .collect()
}
fn run_worker(
    shared: &Arc<State>,
    root: PathBuf,
    mut sources: Vec<LogSource>,
    zone: Tz,
    receiver: mpsc::Receiver<Command>,
    options: WorkerOptions,
) -> Result<(), Error> {
    let mut index = Index::open(IndexConfig {
        database: options
            .database
            .unwrap_or_else(|| config::index_path(&root, &sources)),
        sources: sources.clone(),
    })?;
    let mut store = Store::new(root.join("remotes"), shared.device.clone(), zone);
    let prices = PriceSchedule::bundled();
    let mut publication = 0u64;
    let tx = shared.sender.clone();
    let watch_sources = Arc::new(RwLock::new(sources.clone()));
    let callback_sources = watch_sources.clone();
    let mut watcher =
        notify::recommended_watcher(move |result: Result<notify::Event, notify::Error>| {
            if let Ok(event) = result
                && !matches!(event.kind, notify::EventKind::Access(_))
            {
                let paths: Vec<_> = event
                    .paths
                    .into_iter()
                    .filter(|path| relevant_event(path, &callback_sources.read().unwrap()))
                    .collect();
                if !paths.is_empty() {
                    let _ = tx.try_send(Command::Wake(paths));
                }
            }
        })
        .ok();
    let mut watched = HashMap::new();
    let mut pending: Vec<(RemoteReply, Result<Value, Error>)> = Vec::new();
    let mut policies = load_policies(shared, &mut index, &sources);
    // Each source refreshes on its own schedule: local sources every 2 s as
    // before, network shares only when due or explicitly refreshed/woken.
    let mut due_at = vec![Instant::now(); sources.len()];
    loop {
        let now = Instant::now();
        let due: Vec<bool> = due_at.iter().map(|at| *at <= now).collect();
        let due_sources: Vec<LogSource> = sources
            .iter()
            .zip(&due)
            .filter(|(_, due)| **due)
            .map(|(source, _)| source.clone())
            .collect();
        let watch_count = refresh_watches(&mut watcher, &due_sources, &mut watched);
        let mut progressed = false;
        let result = if due_sources.is_empty() {
            Ok(None)
        } else {
            let ids: HashSet<String> = due_sources.iter().map(|s| s.id.clone()).collect();
            let scope = (due_sources.len() < sources.len()).then_some(&ids);
            index.refresh_sources(scope, |progress|{if !["inventory","ready"].contains(&progress.phase){progressed=true;update_status(shared,json!({"phase":progress.phase,"completed":progress.completed,"total":progress.total,"revision":publication}),true);}})
        };
        let refreshed = Instant::now();
        for ((at, due), policy) in due_at.iter_mut().zip(&due).zip(&policies) {
            if *due {
                *at = refreshed + policy.poll_interval;
            }
        }
        let poll_interval = policies
            .iter()
            .map(|policy| policy.poll_interval)
            .min()
            .unwrap_or(LOCAL_POLL);
        let remotes = store.refresh(&prices);
        match (result, remotes) {
            (Ok(local_changed), Ok((remote_changed, remotes))) => {
                if local_changed.is_some() || remote_changed || publication == 0 {
                    publication += 1;
                    let local = index.snapshot();
                    let mut sessions = local.sessions.clone();
                    let mut source_rows = config::source_rows(&sources);
                    let mut metadata = Vec::new();
                    for remote in remotes {
                        sessions.extend(remote.sessions.iter().cloned());
                        let meta = &remote.metadata;
                        source_rows.push(json!({"id":format!("remote-{}",meta.device_short_code),"label":meta.label,"codex_home":format!("remote:{}",meta.device_short_code),"is_remote":true}));
                        metadata.push(meta.clone());
                    }
                    metadata.sort_by(|a, b| a.label.cmp(&b.label));
                    let snapshot = Arc::new(Snapshot {
                        revision: publication,
                        generated_at: usage_core::timestamp::format(Utc::now()),
                        sessions,
                    });
                    *shared.current.write().unwrap() = Some(Arc::new(Published {
                        snapshot,
                        local,
                        local_sources: sources.clone(),
                        sources: source_rows,
                        remotes: metadata,
                    }));
                    progressed = true;
                }
                update_status(
                    shared,
                    json!({"phase":"ready","revision":publication,"metrics":index.metrics,"watch_roots":watch_count,"poll_interval_ms":poll_interval.as_millis() as u64,"source_polling":polling_status(&sources,&policies)}),
                    progressed,
                );
                for (reply, response) in pending.drain(..) {
                    let _: Result<_, _> = reply.send(response);
                }
            }
            (Err(error), _) | (_, Err(error)) => {
                update_status(
                    shared,
                    json!({"phase":"error","revision":publication,"error":error.to_string()}),
                    true,
                );
                for (reply, _) in pending.drain(..) {
                    let _ = reply.send(Err(Error::InvalidData(error.to_string())));
                }
            }
        }
        // Poll even without watcher events; dropped notifications and
        // mounted Windows histories converge on the same inventory. Wake at
        // least every 2 s so imported remote snapshots keep their cadence.
        let wait = due_at
            .iter()
            .min()
            .map(|at| at.saturating_duration_since(Instant::now()))
            .unwrap_or(LOCAL_POLL)
            .min(LOCAL_POLL);
        let command = receiver.recv_timeout(wait);
        let mut commands = Vec::new();
        if let Ok(command) = command {
            commands.push(command);
            commands.extend(receiver.try_iter());
        }
        let mut stop = false;
        let woken = !commands.is_empty();
        for command in commands {
            match command {
                Command::Refresh(reply) => {
                    index.force_refresh();
                    due_at.fill(Instant::now());
                    store = Store::new(root.join("remotes"), shared.device.clone(), zone);
                    pending.push((reply, Ok(json!({"ok":true}))));
                }
                Command::Stop => stop = true,
                Command::Wake(paths) => {
                    for path in paths {
                        for (source, at) in sources.iter().zip(due_at.iter_mut()) {
                            if relevant_event(&path, std::slice::from_ref(source)) {
                                *at = Instant::now();
                            }
                        }
                        index.invalidate(&path)
                    }
                }
                Command::Sources(homes, expected, reply) => {
                    let result = (|| -> Result<Value, Error> {
                        let current: Vec<_> =
                            sources.iter().map(|source| source.home.clone()).collect();
                        if current != expected {
                            return Err(Error::InvalidData(SOURCE_CONFLICT.into()));
                        }
                        let next_sources = config::sources(&homes, options.auto_windows);
                        let changed = config::index_path(&root, &next_sources)
                            != config::index_path(&root, &sources);
                        let next_index = if changed {
                            let mut next = Index::open(IndexConfig {
                                database: config::index_path(&root, &next_sources),
                                sources: next_sources.clone(),
                            })?;
                            next.refresh(|_| {})?;
                            Some(next)
                        } else {
                            None
                        };
                        save_sources(&root, &homes, &next_sources, options.managed)?;
                        if let Some(next) = next_index {
                            index = next;
                            sources = next_sources;
                            *watch_sources.write().unwrap() = sources.clone();
                            let previous = shared.current.read().unwrap().clone();
                            let local = index.snapshot();
                            let mut sessions = local.sessions.clone();
                            let mut source_rows = config::source_rows(&sources);
                            let mut remotes = Vec::new();
                            if let Some(previous) = previous {
                                sessions.extend(
                                    previous
                                        .snapshot
                                        .sessions
                                        .iter()
                                        .skip(previous.local.sessions.len())
                                        .cloned(),
                                );
                                source_rows.extend(
                                    previous
                                        .sources
                                        .iter()
                                        .filter(|source| source["is_remote"] == true)
                                        .cloned(),
                                );
                                remotes = previous.remotes.clone();
                            }
                            publication += 1;
                            let snapshot = Arc::new(Snapshot {
                                revision: publication,
                                generated_at: usage_core::timestamp::format(Utc::now()),
                                sessions,
                            });
                            *shared.current.write().unwrap() = Some(Arc::new(Published {
                                snapshot,
                                local,
                                local_sources: sources.clone(),
                                sources: source_rows,
                                remotes,
                            }));
                            policies = load_policies(shared, &mut index, &sources);
                            due_at = vec![Instant::now(); sources.len()];
                            let watch_count = refresh_watches(&mut watcher, &sources, &mut watched);
                            update_status(
                                shared,
                                json!({"phase":"ready","revision":publication,"metrics":index.metrics,"watch_roots":watch_count,"poll_interval_ms":policies.iter().map(|policy| policy.poll_interval).min().unwrap_or(LOCAL_POLL).as_millis() as u64,"source_polling":polling_status(&sources,&policies)}),
                                true,
                            );
                        }
                        Ok(json!({"ok":true}))
                    })();
                    let _ = reply.send(result);
                }
                Command::Reconfigure => {
                    policies = load_policies(shared, &mut index, &sources);
                    due_at.fill(Instant::now());
                }
                Command::Remote(action, mut body, reply) => {
                    let result = match action.as_str() {
                        "import" => store.import(
                            body["snapshot"].take(),
                            body["label"].as_str(),
                            body["allow_current_device"].as_bool().unwrap_or(false),
                        ),
                        "rename" => store.rename(
                            body["device_short_code"].as_str().unwrap_or(""),
                            body["label"].as_str().unwrap_or(""),
                        ),
                        "delete" => {
                            if body["confirm"] != true {
                                Err(Error::InvalidData("delete requires confirmation".into()))
                            } else {
                                store
                                    .delete(body["device_short_code"].as_str().unwrap_or(""))
                                    .map(|_| json!({"ok":true}))
                            }
                        }
                        _ => Err(Error::InvalidData("not found".into())),
                    };
                    // Reply only after the associated publication is ready.
                    pending.push((reply, result));
                }
            }
        }
        if woken {
            // Any command refreshed every source before; local ones still do.
            for (at, policy) in due_at.iter_mut().zip(&policies) {
                if !policy.network {
                    *at = Instant::now();
                }
            }
        }
        if stop {
            break;
        }
    }
    Ok(())
}
