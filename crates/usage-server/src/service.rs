use crate::config;
use chrono::Utc;
use chrono_tz::Tz;
use notify::Watcher;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{Arc, Mutex, RwLock, mpsc},
    time::Duration,
};
use tokio::sync::{oneshot, watch};
use usage_core::{
    Error, PriceSchedule,
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
    pub sources: Vec<Value>,
    pub remotes: Vec<Metadata>,
}
pub enum Command {
    Wake(Vec<PathBuf>),
    Remote(String, Value, oneshot::Sender<Result<Value, Error>>),
    Stop,
}
type CachedView = (String, Arc<View>, Arc<Published>);
pub struct State {
    pub shutdown: tokio::sync::Notify,
    pub current: RwLock<Option<Arc<Published>>>,
    pub status: RwLock<Value>,
    pub events: watch::Sender<Value>,
    pub sender: mpsc::SyncSender<Command>,
    pub device: String,
    pub hostname: String,
    pub sources: Vec<LogSource>,
    pub zone: Tz,
    views: Mutex<VecDeque<CachedView>>,
    pub query_slots: Arc<tokio::sync::Semaphore>,
}
impl State {
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
        let view = Arc::new(View::new(
            &current.snapshot,
            period,
            zone,
            &PriceSchedule::bundled(),
        )?);
        cache.push_back((key, view.clone(), current.clone()));
        while cache.len() > 8 {
            cache.pop_front();
        }
        Ok((view, current))
    }
    pub fn local_view(&self) -> Result<Arc<View>, QueryError> {
        let current = self.publication()?;
        let period = Period::bounds(
            "all",
            None,
            None,
            self.zone,
            usage_core::timestamp::parse(&current.local.generated_at).unwrap_or_else(Utc::now),
        )?;
        Ok(Arc::new(View::new(
            &current.local,
            period,
            self.zone,
            &PriceSchedule::bundled(),
        )?))
    }
    pub fn export(&self) -> Result<Value, QueryError> {
        let view = self.local_view()?;
        let details: serde_json::Map<String, Value> = view
            .details()
            .map(|d| Ok((d.identity.uid.clone(), serde_json::to_value(d)?)))
            .collect::<Result<_, serde_json::Error>>()?;
        Ok(
            json!({"schema":usage_core::remote::SCHEMA,"version":1,"exported_at":usage_core::timestamp::format(Utc::now()),"device":{"short_code":self.device,"label":self.hostname,"platform":config::platform(),"hostname":self.hostname},"snapshot":{"generated_at":view.generated_at,"codex_home":config::homes_display(&self.sources),"codex_sources":config::source_rows(&self.sources),"summary":view.summary()?,"sessions":view.summaries().collect::<Vec<_>>(),"details_by_uid":details,"daily_usage":view.daily_usage}}),
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
) -> Result<(Arc<State>, std::thread::JoinHandle<()>), Error> {
    let device = config::device_code(&root)?;
    let hostname = config::hostname();
    let (sender, receiver) = mpsc::sync_channel(128);
    let initial = json!({"phase":"loading","completed":0,"total":0,"revision":0});
    let (events, _) = watch::channel(initial.clone());
    let state = Arc::new(State {
        shutdown: tokio::sync::Notify::new(),
        current: RwLock::new(None),
        status: RwLock::new(initial),
        events,
        sender,
        device,
        hostname,
        sources: sources.clone(),
        zone,
        views: Mutex::new(VecDeque::new()),
        query_slots: Arc::new(tokio::sync::Semaphore::new(4)),
    });
    let shared = state.clone();
    let handle = std::thread::Builder::new()
        .name("usage-index-writer".into())
        .spawn(move || {
            if let Err(error) = run_worker(&shared, root, sources, zone, receiver, database) {
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
fn run_worker(
    shared: &Arc<State>,
    root: PathBuf,
    sources: Vec<LogSource>,
    zone: Tz,
    receiver: mpsc::Receiver<Command>,
    database: Option<PathBuf>,
) -> Result<(), Error> {
    let mut index = Index::open(IndexConfig {
        database: database.unwrap_or_else(|| config::index_path(&root, &sources)),
        sources: sources.clone(),
    })?;
    let mut store = Store::new(root.join("remotes"), shared.device.clone(), zone);
    let prices = PriceSchedule::bundled();
    let mut publication = 0u64;
    let tx = shared.sender.clone();
    let watch_sources = sources.clone();
    let mut watcher =
        notify::recommended_watcher(move |result: Result<notify::Event, notify::Error>| {
            if let Ok(event) = result
                && !matches!(event.kind, notify::EventKind::Access(_))
            {
                let paths: Vec<_> = event
                    .paths
                    .into_iter()
                    .filter(|path| relevant_event(path, &watch_sources))
                    .collect();
                if !paths.is_empty() {
                    let _ = tx.try_send(Command::Wake(paths));
                }
            }
        })
        .ok();
    let mut watched = HashMap::new();
    let mut pending: Vec<(RemoteReply, Result<Value, Error>)> = Vec::new();
    loop {
        let watch_count = refresh_watches(&mut watcher, &sources, &mut watched);
        let mut progressed = false;
        let result=index.refresh(|progress|{if !["inventory","ready"].contains(&progress.phase){progressed=true;update_status(shared,json!({"phase":progress.phase,"completed":progress.completed,"total":progress.total,"revision":publication}),true);}});
        let remotes = store.refresh(&prices);
        match (result, remotes) {
            (Ok(local_changed), Ok((remote_changed, remotes))) => {
                if local_changed.is_some() || remote_changed || publication == 0 {
                    publication += 1;
                    let local = index.snapshot();
                    let mut sessions = local.sessions.clone();
                    let mut sources = config::source_rows(&sources);
                    let mut metadata = Vec::new();
                    for remote in remotes {
                        sessions.extend(remote.sessions.iter().cloned());
                        let meta = &remote.metadata;
                        sources.push(json!({"id":format!("remote-{}",meta.device_short_code),"label":meta.label,"codex_home":format!("remote:{}",meta.device_short_code),"is_remote":true}));
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
                        sources,
                        remotes: metadata,
                    }));
                    progressed = true;
                }
                update_status(
                    shared,
                    json!({"phase":"ready","revision":publication,"metrics":index.metrics,"watch_roots":watch_count,"poll_interval_ms":2000}),
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
        // mounted Windows histories converge on the same inventory.
        let command = receiver.recv_timeout(Duration::from_secs(2));
        let mut commands = Vec::new();
        if let Ok(command) = command {
            commands.push(command);
            commands.extend(receiver.try_iter());
        }
        let mut stop = false;
        for command in commands {
            match command {
                Command::Stop => stop = true,
                Command::Wake(paths) => {
                    for path in paths {
                        index.invalidate(&path)
                    }
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
        if stop {
            break;
        }
    }
    Ok(())
}
