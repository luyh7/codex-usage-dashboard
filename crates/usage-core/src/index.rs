//! Single-writer, transactional, rebuildable index. Queries consume immutable
//! published snapshots; only the host's background worker calls refresh().
use crate::{
    Error, PriceSchedule, Usage,
    inventory::{FileStamp, Inventory, InventoryFile},
    normalize,
    parser::{self, LogSource, ParsedFile, SessionDetail},
    runtime, timestamp, usage,
};
use chrono::Utc;
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use fs2::FileExt;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

const SCHEMA_VERSION: &str = "1";
const PARSER_VERSION: &str = "3";
const MARKER_BYTES: u64 = 4096;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventIndex {
    pub timestamp_us: i64,
    pub source_order: usize,
    pub delta: Usage,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedSession {
    #[serde(default)]
    pub wire: Option<Arc<crate::wire::RemoteWire>>,
    #[serde(default)]
    pub summary: Option<Arc<crate::query::SessionSummary>>,
    #[serde(default = "yes")]
    pub has_detail: bool,
    pub detail: Arc<SessionDetail>,
    pub events: Arc<Vec<EventIndex>>,
}
fn yes() -> bool {
    true
}
impl IndexedSession {
    pub fn from_detail(detail: Arc<SessionDetail>) -> Result<Self, Error> {
        let events = usage::normalize(&detail.timeline)?
            .into_iter()
            .map(|e| EventIndex {
                timestamp_us: e.timestamp.timestamp_micros(),
                source_order: e.source_order,
                delta: e.delta,
            })
            .collect();
        Ok(Self {
            wire: None,
            summary: None,
            has_detail: true,
            detail,
            events: Arc::new(events),
        })
    }
}
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub revision: u64,
    pub generated_at: String,
    pub sessions: Vec<IndexedSession>,
}
impl Snapshot {
    pub fn empty() -> Self {
        Self {
            revision: 0,
            generated_at: timestamp::format(Utc::now()),
            sessions: Vec::new(),
        }
    }
}
#[derive(Debug, Default, Clone, Serialize)]
pub struct IndexMetrics {
    pub full_parses: u64,
    pub incremental_parses: u64,
    pub persistent_hits: u64,
    pub unchanged_files: u64,
    pub normalized_components: u64,
    pub component_hits: u64,
    pub transaction_ms: f64,
    pub last_refresh_ms: f64,
    pub events_written: u64,
}
#[derive(Debug, Clone, Serialize)]
pub struct IndexProgress {
    pub phase: &'static str,
    pub completed: usize,
    pub total: usize,
    pub revision: u64,
}
#[derive(Debug, Clone)]
pub struct IndexConfig {
    pub database: PathBuf,
    pub sources: Vec<LogSource>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Markers {
    #[serde(default)]
    append_safe: bool,
    pub prefix_len: u64,
    pub prefix: String,
    pub tail: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedFile {
    stamp: FileStamp,
    kind: String,
    source_id: String,
    parsed: Arc<ParsedFile>,
    markers: Markers,
}
#[derive(Clone)]
struct Component {
    signature: String,
    sessions: Vec<IndexedSession>,
}

pub struct Index {
    config: IndexConfig,
    connection: Connection,
    _lock: File,
    inventory: Inventory,
    files: BTreeMap<String, CachedFile>,
    components: HashMap<String, Component>,
    auxiliary: String,
    pricing_fingerprint: String,
    pricing: PriceSchedule,
    snapshot: Arc<Snapshot>,
    pub metrics: IndexMetrics,
    #[cfg(test)]
    fail_at: Option<&'static str>,
}
impl Drop for Index {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self._lock);
    }
}
#[cfg(test)]
fn crash_checkpoint(stage: &str) {
    if std::env::var("COUSASH_TEST_CRASH_STAGE").ok().as_deref() == Some(stage) {
        std::process::exit(93);
    }
}
fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, Error> {
    let mut writer = ZlibEncoder::new(Vec::new(), Compression::fast());
    // Feed the compressor in bulk; JSON token-sized writes significantly
    // increase cold-index overhead. This transient byte buffer is per record.
    writer.write_all(&serde_json::to_vec(value)?)?;
    Ok(writer.finish()?)
}
fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, Error> {
    let mut raw = Vec::new();
    ZlibDecoder::new(bytes).read_to_end(&mut raw)?;
    Ok(serde_json::from_slice(&raw)?)
}
fn fingerprint<T: Serialize>(value: &T) -> Result<String, Error> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}
fn read_hash(file: &mut File, start: u64, len: u64) -> Result<String, Error> {
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = vec![0; len as usize];
    file.read_exact(&mut bytes)?;
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}
fn markers(path: &Path, consumed: u64) -> Result<Markers, Error> {
    // Decoded offsets must never be used to seek within a Zstandard file.
    // Unchanged archives use their physical FileStamp and persistent cache.
    if crate::reader::is_compressed(path) {
        return Ok(Markers {
            append_safe: false,
            prefix_len: 0,
            prefix: String::new(),
            tail: String::new(),
        });
    }
    let mut file = File::open(path)?;
    let len = consumed.min(MARKER_BYTES);
    let append_safe = if consumed == 0 {
        true
    } else {
        file.seek(SeekFrom::Start(consumed - 1))?;
        let mut last = [0u8; 1];
        file.read_exact(&mut last)?;
        last[0] == b'\n'
    };
    Ok(Markers {
        append_safe,
        prefix_len: len,
        prefix: read_hash(&mut file, 0, len)?,
        tail: read_hash(&mut file, consumed - len, len)?,
    })
}
fn matches_prefix(path: &Path, cached: &CachedFile) -> bool {
    if !cached.markers.append_safe {
        return false;
    }
    let consumed = cached.parsed.state.parsed_end_offset;
    markers(path, consumed).is_ok_and(|current| {
        current.prefix == cached.markers.prefix
            && current.tail == cached.markers.tail
            && current.prefix_len == cached.markers.prefix_len
    })
}
fn file_key(file: &InventoryFile) -> String {
    format!("{}:{}", file.source.id, file.path.display())
}
fn initialize(connection: &Connection) -> Result<(), Error> {
    connection.busy_timeout(std::time::Duration::from_secs(2))?;
    connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
        CREATE TABLE IF NOT EXISTS metadata (key TEXT PRIMARY KEY,value TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS files (path_key TEXT PRIMARY KEY,payload BLOB NOT NULL);
        CREATE TABLE IF NOT EXISTS sessions (uid TEXT PRIMARY KEY,position INTEGER NOT NULL,payload BLOB NOT NULL);
        CREATE TABLE IF NOT EXISTS components (identity TEXT PRIMARY KEY,signature TEXT NOT NULL,uids TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS events (session_uid TEXT NOT NULL,ordinal INTEGER NOT NULL,timestamp_us INTEGER NOT NULL,input_tokens INTEGER NOT NULL,cached_input_tokens INTEGER NOT NULL,cache_write_tokens INTEGER NOT NULL,output_tokens INTEGER NOT NULL,reasoning_output_tokens INTEGER NOT NULL,total_tokens INTEGER NOT NULL,PRIMARY KEY(session_uid,ordinal));
        CREATE INDEX IF NOT EXISTS events_by_session_time ON events(session_uid,timestamp_us,ordinal);
        CREATE INDEX IF NOT EXISTS events_by_time ON events(timestamp_us,session_uid);")?;
    Ok(())
}
fn meta(connection: &Connection, key: &str) -> Option<String> {
    connection
        .query_row("SELECT value FROM metadata WHERE key=?", [key], |row| {
            row.get(0)
        })
        .ok()
}
fn open_rebuildable(path: &Path) -> Result<Connection, Error> {
    let connection = Connection::open(path)?;
    match initialize(&connection) {
        Ok(()) => Ok(connection),
        Err(Error::Database(rusqlite::Error::SqliteFailure(error, _)))
            if matches!(
                error.code,
                rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase
            ) =>
        {
            drop(connection);
            let suffix = format!("corrupt-{}", Utc::now().timestamp_micros());
            fs::rename(path, path.with_extension(&suffix))?;
            for ext in ["-wal", "-shm"] {
                let side = PathBuf::from(format!("{}{ext}", path.display()));
                if side.exists() {
                    fs::rename(
                        &side,
                        PathBuf::from(format!("{}.{}", side.display(), suffix)),
                    )?;
                }
            }
            let connection = Connection::open(path)?;
            initialize(&connection)?;
            Ok(connection)
        }
        Err(error) => Err(error),
    }
}
impl Index {
    pub fn open(config: IndexConfig) -> Result<Self, Error> {
        if let Some(parent) = config.database.parent() {
            fs::create_dir_all(parent)?;
            #[cfg(unix)]
            if parent.file_name().is_some_and(|n| n == "index-v1") {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
            }
        }
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(config.database.with_extension("lock"))?;
        lock.try_lock_exclusive()
            .map_err(|e| Error::InvalidData(format!("index already has a writer: {e}")))?;
        let mut connection = open_rebuildable(&config.database)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&config.database, fs::Permissions::from_mode(0o600))?;
        }
        let scope = fingerprint(&config.sources)?;
        if meta(&connection, "schema_version").as_deref() != Some(SCHEMA_VERSION)
            || meta(&connection, "parser_version").as_deref() != Some(PARSER_VERSION)
            || meta(&connection, "scope").as_deref() != Some(&scope)
        {
            let tx = connection.transaction()?;
            tx.execute_batch("DELETE FROM events; DELETE FROM sessions; DELETE FROM files; DELETE FROM components; DELETE FROM metadata;")?;
            for (key, value) in [
                ("schema_version", SCHEMA_VERSION),
                ("parser_version", PARSER_VERSION),
                ("scope", scope.as_str()),
            ] {
                tx.execute("INSERT INTO metadata VALUES (?,?)", [key, value])?;
            }
            tx.commit()?;
        }
        let pricing = PriceSchedule::bundled();
        let pricing_fingerprint = format!(
            "validity-v2:{:x}",
            Sha256::digest(include_bytes!("../data/prices-v1.json"))
        );
        let mut files = BTreeMap::new();
        let mut metrics = IndexMetrics::default();
        {
            let mut statement = connection.prepare("SELECT path_key,payload FROM files")?;
            for row in statement.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
            })? {
                let (key, bytes) = row?;
                if let Ok(file) = decode::<CachedFile>(&bytes) {
                    files.insert(key, file);
                    metrics.persistent_hits += 1;
                }
            }
        }
        let mut snapshot = Snapshot::empty();
        if meta(&connection, "pricing").as_deref() == Some(&pricing_fingerprint) {
            snapshot.revision = meta(&connection, "revision")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            snapshot.generated_at =
                meta(&connection, "generated_at").unwrap_or(snapshot.generated_at);
            let mut statement =
                connection.prepare("SELECT payload FROM sessions ORDER BY position")?;
            for row in statement.query_map([], |row| row.get::<_, Vec<u8>>(0))? {
                match decode::<IndexedSession>(&row?) {
                    Ok(session) => snapshot.sessions.push(session),
                    Err(_) => {
                        snapshot = Snapshot::empty();
                        break;
                    }
                }
            }
        }
        let mut components = HashMap::new();
        let by_uid: HashMap<_, _> = snapshot
            .sessions
            .iter()
            .map(|s| (s.detail.identity.uid.as_str(), s))
            .collect();
        {
            let mut statement =
                connection.prepare("SELECT identity,signature,uids FROM components")?;
            for row in statement.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })? {
                let (key, signature, uids) = row?;
                if let Ok(uids) = serde_json::from_str::<Vec<String>>(&uids) {
                    let sessions: Option<Vec<_>> = uids
                        .iter()
                        .map(|uid| by_uid.get(uid.as_str()).map(|s| (*s).clone()))
                        .collect();
                    if let Some(sessions) = sessions {
                        components.insert(
                            key,
                            Component {
                                signature,
                                sessions,
                            },
                        );
                    }
                }
            }
        }
        let auxiliary = if snapshot.revision > 0 {
            meta(&connection, "auxiliary").unwrap_or_default()
        } else {
            String::new()
        };
        Ok(Self {
            inventory: Inventory::new(config.sources.clone()),
            config,
            connection,
            _lock: lock,
            files,
            components,
            auxiliary,
            pricing_fingerprint,
            pricing,
            snapshot: Arc::new(snapshot),
            metrics,
            #[cfg(test)]
            fail_at: None,
        })
    }
    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.snapshot.clone()
    }
    pub fn invalidate(&mut self, path: &Path) {
        self.inventory.invalidate(path);
    }
    pub fn replace_pricing(&mut self, json: &str) -> Result<(), Error> {
        self.pricing = PriceSchedule::from_json(json)?;
        self.pricing_fingerprint = format!("validity-v2:{:x}", Sha256::digest(json.as_bytes()));
        self.auxiliary.clear();
        Ok(())
    }
    fn auxiliary_signature(&self) -> Result<String, Error> {
        let mut stamps = Vec::new();
        for source in &self.config.sources {
            for name in ["session_index.jsonl", "logs_2.sqlite", "logs_2.sqlite-wal"] {
                let path = source.home.join(name);
                stamps.push((
                    path.to_string_lossy().into_owned(),
                    FileStamp::read(&path).ok(),
                ));
            }
        }
        fingerprint(&(stamps, &self.pricing_fingerprint))
    }
    pub fn refresh(
        &mut self,
        mut progress: impl FnMut(IndexProgress),
    ) -> Result<Option<Arc<Snapshot>>, Error> {
        let started = Instant::now();
        progress(IndexProgress {
            phase: "inventory",
            completed: 0,
            total: 0,
            revision: self.snapshot.revision,
        });
        let inventory = self.inventory.discover()?;
        let auxiliary = self.auxiliary_signature()?;
        let unchanged = auxiliary == self.auxiliary
            && inventory.len() == self.files.len()
            && inventory.iter().all(|file| {
                self.files
                    .get(&file_key(file))
                    .is_some_and(|cached| cached.stamp == file.stamp && cached.kind == file.kind)
            });
        if unchanged {
            self.metrics.last_refresh_ms = started.elapsed().as_secs_f64() * 1000.;
            progress(IndexProgress {
                phase: "ready",
                completed: self.snapshot.sessions.len(),
                total: self.snapshot.sessions.len(),
                revision: self.snapshot.revision,
            });
            return Ok(None);
        }
        let mut next = BTreeMap::new();
        let mut changed = HashSet::new();
        for (i, file) in inventory.iter().enumerate() {
            let key = file_key(file);
            let cached = self.files.get(&key);
            if let Some(cached) =
                cached.filter(|cached| cached.stamp == file.stamp && cached.kind == file.kind)
            {
                next.insert(key, cached.clone());
                self.metrics.unchanged_files += 1;
                continue;
            }
            progress(IndexProgress {
                phase: "parsing",
                completed: i,
                total: inventory.len(),
                revision: self.snapshot.revision,
            });
            let append = cached.filter(|cached| {
                !crate::reader::is_compressed(&file.path)
                    && cached.stamp.same_identity(&file.stamp)
                    && file.stamp.size > cached.stamp.size
                    && matches_prefix(&file.path, cached)
            });
            let base = append.map(|cached| cached.parsed.as_ref().clone());
            let parsed = parser::parse_file(
                &file.path,
                &file.kind,
                &file.source,
                &self.pricing,
                base,
                Some(file.stamp.size),
            )?;
            let after = FileStamp::read(&file.path)?;
            if after != file.stamp
                && (crate::reader::is_compressed(&file.path)
                    || !after.same_identity(&file.stamp)
                    || after.size <= file.stamp.size)
            {
                self.inventory.invalidate(&file.path);
                return Err(Error::InvalidData(
                    "log changed during parse; retry scheduled".into(),
                ));
            }
            if append.is_some() {
                self.metrics.incremental_parses += 1;
            } else {
                self.metrics.full_parses += 1;
            }
            let markers = markers(&file.path, parsed.state.parsed_end_offset)?;
            changed.insert(key.clone());
            next.insert(
                key,
                CachedFile {
                    stamp: file.stamp.clone(),
                    kind: file.kind.clone(),
                    source_id: file.source.id.clone(),
                    parsed: Arc::new(parsed),
                    markers,
                },
            );
        }
        progress(IndexProgress {
            phase: "normalizing",
            completed: 0,
            total: next.len(),
            revision: self.snapshot.revision,
        });
        let present: Vec<_> = inventory
            .iter()
            .filter(|f| next.contains_key(&file_key(f)))
            .collect();
        let refs: Vec<_> = present
            .iter()
            .map(|f| next[&file_key(f)].parsed.as_ref())
            .collect();
        let projects = crate::project::Resolver::default();
        let groups = normalize::dependency_components(&refs);
        let mut tiers = HashMap::new();
        let mut titles = HashMap::new();
        for source in &self.config.sources {
            let ids = refs
                .iter()
                .filter(|f| f.detail.environment_id == source.id)
                .map(|f| f.detail.identity.session_id.clone())
                .collect::<BTreeSet<_>>();
            tiers.insert(
                source.id.clone(),
                runtime::service_tiers(&source.home, &ids),
            );
            let mut source_titles = BTreeMap::new();
            if let Ok(file) = File::open(source.home.join("session_index.jsonl")) {
                use std::io::BufRead;
                for line in std::io::BufReader::new(file).lines().map_while(Result::ok) {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line)
                        && let (Some(id), Some(title)) =
                            (value["id"].as_str(), value["thread_name"].as_str())
                        && !title.trim().is_empty()
                    {
                        source_titles.insert(id.to_owned(), parser::clean(title, 180));
                    }
                }
            }
            titles.insert(source.id.clone(), source_titles);
        }
        let mut entries = Vec::new();
        let mut live_components = HashSet::new();
        for group in groups {
            let mut identities: Vec<_> = group
                .iter()
                .map(|i| {
                    (
                        &refs[*i].detail.environment_id,
                        &refs[*i].detail.identity.session_id,
                    )
                })
                .collect();
            identities.sort();
            let key = fingerprint(&identities)?;
            live_components.insert(key.clone());
            let mut stamps: Vec<_> = group
                .iter()
                .map(|i| (&present[*i].path, &present[*i].stamp, &present[*i].kind))
                .collect();
            stamps.sort_by_key(|(path, _, _)| *path);
            let evidence: Vec<_> = identities
                .iter()
                .map(|(source, id)| {
                    (
                        tiers.get(*source).and_then(|t| t.get(*id)),
                        titles.get(*source).and_then(|t| t.get(*id)),
                    )
                })
                .collect();
            let signature = fingerprint(&(stamps, evidence, &self.pricing_fingerprint))?;
            let sessions = if let Some(cached) = self
                .components
                .get(&key)
                .filter(|c| c.signature == signature)
            {
                self.metrics.component_hits += 1;
                cached.sessions.clone()
            } else {
                self.metrics.normalized_components += 1;
                let mut raw = Vec::new();
                for i in &group {
                    let mut file = refs[*i].clone();
                    projects.get(&file.detail.cwd).apply(&mut file.detail);
                    if let Some(events) = tiers
                        .get(&file.detail.environment_id)
                        .and_then(|t| t.get(&file.detail.identity.session_id))
                    {
                        normalize::apply_runtime_tiers(&mut file, events.clone());
                    }
                    raw.push(file);
                }
                let normalized = normalize::normalize_sessions(raw, &self.pricing)?;
                let mut sessions = Vec::new();
                for mut file in normalized {
                    if let Some(title) = titles
                        .get(&file.detail.environment_id)
                        .and_then(|t| t.get(&file.detail.identity.session_id))
                    {
                        file.detail.title = title.clone();
                    }
                    sessions.push(IndexedSession::from_detail(Arc::new(file.detail))?);
                }
                self.components.insert(
                    key,
                    Component {
                        signature,
                        sessions: sessions.clone(),
                    },
                );
                sessions
            };
            for session in sessions {
                let position = group
                    .iter()
                    .find(|i| {
                        refs[**i].detail.identity.session_id == session.detail.identity.session_id
                            && refs[**i].detail.environment_id == session.detail.environment_id
                    })
                    .copied()
                    .unwrap_or(group[0]);
                entries.push((position, session));
            }
        }
        self.components
            .retain(|key, _| live_components.contains(key));
        entries.sort_by_key(|(i, _)| *i);
        entries.sort_by_key(|(_, s)| std::cmp::Reverse(s.detail.total_token_usage.total_tokens));
        let published_changed = self.snapshot.revision == 0
            || entries.len() != self.snapshot.sessions.len()
            || entries
                .iter()
                .zip(&self.snapshot.sessions)
                .any(|((_, next), old)| !Arc::ptr_eq(&next.detail, &old.detail));
        let snapshot = if published_changed {
            Arc::new(Snapshot {
                revision: self.snapshot.revision.max(
                    meta(&self.connection, "revision")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0),
                ) + 1,
                generated_at: timestamp::format(Utc::now()),
                sessions: entries.into_iter().map(|(_, s)| s).collect(),
            })
        } else {
            self.snapshot.clone()
        };
        progress(IndexProgress {
            phase: "committing",
            completed: 0,
            total: snapshot.sessions.len(),
            revision: self.snapshot.revision,
        });
        let commit_started = Instant::now();
        let tx = self.connection.transaction()?;
        for (key, file) in &next {
            if changed.contains(key) {
                tx.execute(
                    "INSERT OR REPLACE INTO files VALUES (?,?)",
                    params![key, encode(file)?],
                )?;
            }
        }
        let stored_keys = {
            let mut statement = tx.prepare("SELECT path_key FROM files")?;
            statement
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        for key in &stored_keys {
            if !next.contains_key(key) {
                tx.execute("DELETE FROM files WHERE path_key=?", [key])?;
            }
        }
        #[cfg(test)]
        crash_checkpoint("files");
        #[cfg(test)]
        if self.fail_at == Some("files") {
            return Err(Error::InvalidData("simulated crash after files".into()));
        }
        let previous: HashMap<_, _> = self
            .snapshot
            .sessions
            .iter()
            .map(|s| (s.detail.identity.uid.as_str(), s))
            .collect();
        let mut live = HashSet::new();
        for (position, session) in snapshot.sessions.iter().enumerate() {
            let uid = &session.detail.identity.uid;
            live.insert(uid.as_str());
            if previous
                .get(uid.as_str())
                .is_some_and(|old| Arc::ptr_eq(&old.detail, &session.detail))
            {
                tx.execute(
                    "UPDATE sessions SET position=? WHERE uid=?",
                    params![position as i64, uid],
                )?;
                continue;
            }
            tx.execute(
                "INSERT OR REPLACE INTO sessions VALUES (?,?,?)",
                params![uid, position as i64, encode(session)?],
            )?;
            tx.execute("DELETE FROM events WHERE session_uid=?", [uid])?;
            let mut statement =
                tx.prepare_cached("INSERT INTO events VALUES (?,?,?,?,?,?,?,?,?)")?;
            for event in session.events.iter() {
                let u = event.delta;
                statement.execute(params![
                    uid,
                    event.source_order as i64,
                    event.timestamp_us,
                    u.input_tokens,
                    u.cached_input_tokens,
                    u.cache_write_tokens,
                    u.output_tokens,
                    u.reasoning_output_tokens,
                    u.total_tokens
                ])?;
                self.metrics.events_written += 1;
            }
        }
        let stored_uids = {
            let mut statement = tx.prepare("SELECT uid FROM sessions")?;
            statement
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        for uid in &stored_uids {
            if !live.contains(uid.as_str()) {
                tx.execute("DELETE FROM sessions WHERE uid=?", [uid])?;
                tx.execute("DELETE FROM events WHERE session_uid=?", [uid])?;
            }
        }
        #[cfg(test)]
        crash_checkpoint("sessions");
        #[cfg(test)]
        if self.fail_at == Some("sessions") {
            return Err(Error::InvalidData("simulated crash after sessions".into()));
        }
        for (key, value) in [
            ("revision", snapshot.revision.to_string()),
            ("generated_at", snapshot.generated_at.clone()),
            ("pricing", self.pricing_fingerprint.clone()),
            ("auxiliary", auxiliary.clone()),
        ] {
            tx.execute(
                "INSERT OR REPLACE INTO metadata VALUES (?,?)",
                [key, &value],
            )?;
        }
        tx.execute("DELETE FROM components", [])?;
        for (key, component) in &self.components {
            let uids: Vec<_> = component
                .sessions
                .iter()
                .map(|s| &s.detail.identity.uid)
                .collect();
            tx.execute(
                "INSERT INTO components VALUES (?,?,?)",
                params![key, &component.signature, serde_json::to_string(&uids)?],
            )?;
        }
        #[cfg(test)]
        crash_checkpoint("revision");
        #[cfg(test)]
        if self.fail_at == Some("revision") {
            return Err(Error::InvalidData("simulated crash before commit".into()));
        }
        tx.commit()?;
        self.metrics.transaction_ms = commit_started.elapsed().as_secs_f64() * 1000.;
        self.files = next;
        self.auxiliary = auxiliary;
        self.snapshot = snapshot.clone();
        self.metrics.last_refresh_ms = started.elapsed().as_secs_f64() * 1000.;
        progress(IndexProgress {
            phase: "ready",
            completed: snapshot.sessions.len(),
            total: snapshot.sessions.len(),
            revision: snapshot.revision,
        });
        Ok(published_changed.then_some(snapshot))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;
    struct Fixture {
        dir: tempfile::TempDir,
        config: IndexConfig,
        path: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let home = dir.path().join("logs");
            fs::create_dir_all(home.join("sessions")).unwrap();
            let config = IndexConfig {
                database: dir.path().join("config/index.sqlite3"),
                sources: vec![LogSource {
                    id: "fixture".into(),
                    label: "Synthetic".into(),
                    home: home.clone(),
                }],
            };
            let path = home.join("sessions/rollout-synthetic.jsonl");
            fs::write(&path,format!("{}\n{}\n",json!({"timestamp":"2026-10-01T00:00:00Z","type":"session_meta","payload":{"id":"synthetic","model":"gpt-5"}}),event(10,1))).unwrap();
            Self { dir, config, path }
        }
        fn append(&self, total: i64, second: u32) {
            let mut file = OpenOptions::new().append(true).open(&self.path).unwrap();
            writeln!(file, "{}", event(total, second)).unwrap();
        }
    }
    fn event(total: i64, second: u32) -> serde_json::Value {
        json!({"timestamp":format!("2026-10-01T00:00:{second:02}Z"),"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":total,"total_tokens":total},"last_token_usage":{"input_tokens":10,"total_tokens":10}}}})
    }
    fn total(index: &Index) -> i64 {
        index
            .snapshot
            .sessions
            .iter()
            .map(|s| s.detail.total_token_usage.total_tokens)
            .sum()
    }
    #[test]
    fn append_restart_and_immutable_revision() {
        let f = Fixture::new();
        let mut index = Index::open(f.config.clone()).unwrap();
        index.refresh(|_| {}).unwrap();
        let old = index.snapshot();
        assert_eq!(total(&index), 10);
        let generation = old.revision;
        assert!(index.refresh(|_| {}).unwrap().is_none());
        assert_eq!(index.metrics.full_parses, 1);
        f.append(20, 2);
        index.refresh(|_| {}).unwrap();
        assert_eq!(total(&index), 20);
        assert_eq!(old.sessions[0].detail.total_token_usage.total_tokens, 10);
        assert_eq!(index.snapshot.revision, generation + 1);
        assert_eq!(index.metrics.incremental_parses, 1);
        let generation = index.snapshot.revision;
        drop(index);
        let mut restarted = Index::open(f.config).unwrap();
        assert_eq!(total(&restarted), 20);
        assert!(restarted.refresh(|_| {}).unwrap().is_none());
        assert_eq!(restarted.snapshot.revision, generation);
        assert_eq!(restarted.metrics.full_parses, 0);
        assert_eq!(restarted.metrics.persistent_hits, 1);
    }
    #[test]
    fn parser_upgrade_reextracts_cached_prompts_without_changing_logs() {
        let f = Fixture::new();
        let context =
            "<external_codex_apps_open_page>{\"page_id\":null}</external_codex_apps_open_page>";
        let request = "修复登录流程。";
        let mut file = OpenOptions::new().append(true).open(&f.path).unwrap();
        for message in [context, request] {
            writeln!(
                file,
                "{}",
                json!({"type":"event_msg","payload":{"type":"user_message","message":message}})
            )
            .unwrap();
        }
        let bytes = fs::read(&f.path).unwrap();
        let mut index = Index::open(f.config.clone()).unwrap();
        index.refresh(|_| {}).unwrap();
        assert_eq!(index.snapshot.sessions[0].detail.first_user_prompt, request);

        // Emulate a persisted v2 extraction of this unchanged source file.
        let mut cached = index.files.values().next().unwrap().clone();
        Arc::make_mut(&mut cached.parsed).detail.first_user_prompt = context.into();
        index
            .connection
            .execute("UPDATE files SET payload=?", [encode(&cached).unwrap()])
            .unwrap();
        let mut session = index.snapshot.sessions[0].clone();
        Arc::make_mut(&mut session.detail).first_user_prompt = context.into();
        index
            .connection
            .execute("UPDATE sessions SET payload=?", [encode(&session).unwrap()])
            .unwrap();
        index
            .connection
            .execute(
                "UPDATE metadata SET value='2' WHERE key='parser_version'",
                [],
            )
            .unwrap();
        drop(index);

        let mut restarted = Index::open(f.config).unwrap();
        assert!(restarted.snapshot.sessions.is_empty());
        restarted.refresh(|_| {}).unwrap();
        assert_eq!(
            restarted.snapshot.sessions[0].detail.first_user_prompt,
            request
        );
        assert_eq!(restarted.metrics.full_parses, 1);
        assert_eq!(restarted.metrics.persistent_hits, 0);
        assert_eq!(total(&restarted), 10);
        assert_eq!(fs::read(&f.path).unwrap(), bytes);
    }
    #[test]
    fn compression_resume_archive_and_restart_preserve_usage() {
        let f = Fixture::new();
        f.append(20, 2);
        let mut index = Index::open(f.config.clone()).unwrap();
        index.refresh(|_| {}).unwrap();
        let uid = index.snapshot.sessions[0].detail.identity.uid.clone();
        let compressed = f.path.with_extension("jsonl.zst");
        let bytes = fs::read(&f.path).unwrap();
        fs::write(
            &compressed,
            zstd::stream::encode_all(bytes.as_slice(), 1).unwrap(),
        )
        .unwrap();
        // Both files coexist during publication; count only one copy.
        index.refresh(|_| {}).unwrap();
        assert_eq!(total(&index), 20);
        assert_eq!(index.files.len(), 1);
        fs::remove_file(&f.path).unwrap();
        index.refresh(|_| {}).unwrap();
        assert_eq!(total(&index), 20);
        assert_eq!(index.snapshot.sessions[0].detail.identity.uid, uid);
        assert_eq!(
            index.snapshot.sessions[0].detail.file_size,
            fs::metadata(&compressed).unwrap().len() as i64
        );
        assert_eq!(index.snapshot.sessions[0].detail.token_event_count, 2);
        let days = crate::query::daily_usage(
            index.snapshot.sessions.iter().map(|s| s.detail.as_ref()),
            chrono_tz::Asia::Shanghai,
        )
        .unwrap();
        assert_eq!(days[0].usage.total_tokens, 20);
        // A changed compressed stream is fully decoded, never byte-appended.
        let frame = zstd::stream::encode_all(format!("{}\n", event(30, 3)).as_bytes(), 1).unwrap();
        OpenOptions::new()
            .append(true)
            .open(&compressed)
            .unwrap()
            .write_all(&frame)
            .unwrap();
        index.refresh(|_| {}).unwrap();
        assert_eq!(total(&index), 30);
        assert_eq!(index.metrics.incremental_parses, 0);
        // Resuming restores a plain log, which can then use incremental reads.
        let decoded = zstd::stream::decode_all(File::open(&compressed).unwrap()).unwrap();
        fs::write(&f.path, decoded).unwrap();
        index.refresh(|_| {}).unwrap();
        assert_eq!(total(&index), 30);
        fs::remove_file(&compressed).unwrap();
        f.append(40, 4);
        index.refresh(|_| {}).unwrap();
        assert_eq!(total(&index), 40);
        assert_eq!(index.metrics.incremental_parses, 1);
        assert_eq!(index.snapshot.sessions[0].detail.identity.uid, uid);
        let archive = f.config.sources[0].home.join("archived_sessions");
        fs::create_dir_all(&archive).unwrap();
        let archived = archive.join("rollout-synthetic.jsonl.zst");
        fs::write(
            &archived,
            zstd::stream::encode_all(fs::read(&f.path).unwrap().as_slice(), 1).unwrap(),
        )
        .unwrap();
        fs::remove_file(&f.path).unwrap();
        index.refresh(|_| {}).unwrap();
        assert_eq!(total(&index), 40);
        assert_eq!(index.snapshot.sessions[0].detail.source, "archived");
        drop(index);
        let mut index = Index::open(f.config).unwrap();
        assert_eq!(total(&index), 40);
        assert!(index.refresh(|_| {}).unwrap().is_none());
        assert_eq!(index.metrics.full_parses, 0);
        assert_eq!(index.metrics.persistent_hits, 1);
    }
    #[test]
    fn migrated_compressed_children_match_raw_parent_history() {
        let f = Fixture::new();
        let mut first = event(100, 1);
        first["payload"]["info"]["last_token_usage"] =
            json!({"input_tokens":100,"total_tokens":100});
        let mut second = event(200, 2);
        // Codex's recorded request usage need not equal the cumulative delta.
        second["payload"]["info"]["last_token_usage"] =
            json!({"input_tokens":70,"total_tokens":70});
        let parent = format!(
            "{}\n{}\n{}\n",
            json!({"timestamp":"2026-10-01T00:00:00Z","type":"session_meta","payload":{"id":"synthetic"}}),
            first,
            second
        );
        let mut inherited_first = first.clone();
        let mut inherited_second = second.clone();
        // Paginated migration can assign copied events the child's timestamp.
        inherited_first["timestamp"] = json!("2026-10-01T00:00:04Z");
        inherited_second["timestamp"] = json!("2026-10-01T00:00:04Z");
        let mut own = event(250, 5);
        own["payload"]["info"]["last_token_usage"] = json!({"input_tokens":50,"total_tokens":50});
        let child = format!(
            "{}\n{}\n{}\n{}\n",
            json!({"timestamp":"2026-10-01T00:00:03Z","type":"session_meta","payload":{"id":"child","forked_from_id":"synthetic","parent_thread_id":"synthetic"}}),
            inherited_first,
            inherited_second,
            own
        );
        fs::write(
            f.path.with_extension("jsonl.zst"),
            zstd::stream::encode_all(parent.as_bytes(), 1).unwrap(),
        )
        .unwrap();
        fs::remove_file(&f.path).unwrap();
        fs::write(
            f.path.with_file_name("child.jsonl.zst"),
            zstd::stream::encode_all(child.as_bytes(), 1).unwrap(),
        )
        .unwrap();
        let mut index = Index::open(f.config).unwrap();
        index.refresh(|_| {}).unwrap();
        assert_eq!(total(&index), 250);
        let child = &index
            .snapshot
            .sessions
            .iter()
            .find(|s| s.detail.identity.session_id == "child")
            .unwrap()
            .detail;
        assert!(child.fork_usage_resolved);
        assert_eq!(child.inherited_token_event_count, 2);
        assert_eq!(child.inherited_token_usage.total_tokens, 200);
        assert_eq!(child.total_token_usage.total_tokens, 50);
    }
    #[test]
    fn rollback_at_every_commit_stage_does_not_advance_offset() {
        for stage in ["files", "sessions", "revision"] {
            let f = Fixture::new();
            let mut index = Index::open(f.config.clone()).unwrap();
            index.refresh(|_| {}).unwrap();
            let revision = index.snapshot.revision;
            f.append(20, 2);
            index.fail_at = Some(stage);
            assert!(index.refresh(|_| {}).is_err());
            assert_eq!(total(&index), 10);
            drop(index);
            let mut recovered = Index::open(f.config).unwrap();
            assert_eq!(recovered.snapshot.revision, revision);
            assert_eq!(total(&recovered), 10);
            recovered.refresh(|_| {}).unwrap();
            assert_eq!(total(&recovered), 20);
            assert_eq!(recovered.snapshot.revision, revision + 1);
            assert_eq!(
                recovered
                    .files
                    .values()
                    .next()
                    .unwrap()
                    .parsed
                    .detail
                    .token_event_count,
                2
            );
            let (count, sum): (i64, i64) = recovered
                .connection
                .query_row("SELECT COUNT(*),SUM(total_tokens) FROM events", [], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })
                .unwrap();
            assert_eq!((count, sum), (2, 20));
        }
    }
    #[test]
    fn incomplete_line_then_completion_is_consumed_once() {
        let f = Fixture::new();
        let mut index = Index::open(f.config).unwrap();
        index.refresh(|_| {}).unwrap();
        let row = event(20, 2).to_string();
        let split = row.len() / 2;
        OpenOptions::new()
            .append(true)
            .open(&f.path)
            .unwrap()
            .write_all(&row.as_bytes()[..split])
            .unwrap();
        index.refresh(|_| {}).unwrap();
        assert_eq!(total(&index), 10);
        let mut file = OpenOptions::new().append(true).open(&f.path).unwrap();
        writeln!(file, "{}", &row[split..]).unwrap();
        index.refresh(|_| {}).unwrap();
        assert_eq!(total(&index), 20);
        assert_eq!(index.snapshot.sessions[0].detail.token_event_count, 2);
    }
    #[test]
    fn rewrite_with_preserved_mtime_and_archive_move() {
        let f = Fixture::new();
        let mut index = Index::open(f.config.clone()).unwrap();
        index.refresh(|_| {}).unwrap();
        let before = fs::metadata(&f.path).unwrap().modified().unwrap();
        let changed = fs::read_to_string(&f.path).unwrap().replace(":10", ":30");
        fs::write(&f.path, changed).unwrap();
        File::options()
            .write(true)
            .open(&f.path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(before))
            .unwrap();
        index.refresh(|_| {}).unwrap();
        assert_eq!(total(&index), 30);
        let old_uid = index.snapshot.sessions[0].detail.identity.uid.clone();
        let archive = f.config.sources[0].home.join("archived_sessions");
        fs::create_dir_all(&archive).unwrap();
        fs::rename(&f.path, archive.join("rollout-synthetic.jsonl")).unwrap();
        index.refresh(|_| {}).unwrap();
        assert_eq!(total(&index), 30);
        assert_eq!(index.snapshot.sessions.len(), 1);
        assert_eq!(index.snapshot.sessions[0].detail.source, "archived");
        assert_ne!(index.snapshot.sessions[0].detail.identity.uid, old_uid);
    }
    #[test]
    fn writer_lock_and_corrupt_cache_rebuild_preserve_logs() {
        let f = Fixture::new();
        let bytes = fs::read(&f.path).unwrap();
        let index = Index::open(f.config.clone()).unwrap();
        assert!(Index::open(f.config.clone()).is_err());
        drop(index);
        fs::write(&f.config.database, b"not a sqlite database").unwrap();
        let mut recovered = Index::open(f.config).unwrap();
        recovered.refresh(|_| {}).unwrap();
        assert_eq!(total(&recovered), 10);
        assert_eq!(fs::read(&f.path).unwrap(), bytes);
        assert!(
            f.dir.path().join("config").read_dir().unwrap().any(|p| p
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("corrupt"))
        );
    }
    #[test]
    fn repricing_does_not_reread_rollouts() {
        let f = Fixture::new();
        let mut index = Index::open(f.config).unwrap();
        index.refresh(|_| {}).unwrap();
        let before = index.snapshot.sessions[0].detail.pricing.estimated_cost_usd;
        let mut asset: serde_json::Value =
            serde_json::from_str(include_str!("../data/prices-v1.json")).unwrap();
        for schedule in asset["schedules"].as_array_mut().unwrap() {
            schedule[1]["gpt-5"]["input"] = json!(10.0);
        }
        index.replace_pricing(&asset.to_string()).unwrap();
        index.refresh(|_| {}).unwrap();
        assert_ne!(
            index.snapshot.sessions[0].detail.pricing.estimated_cost_usd,
            before
        );
        assert_eq!(index.metrics.full_parses, 1);
        assert_eq!(index.metrics.incremental_parses, 0);
    }
    #[test]
    fn a_changed_session_reuses_an_independent_component_after_restart() {
        let f = Fixture::new();
        let other = f.path.with_file_name("rollout-other.jsonl");
        fs::write(
            &other,
            fs::read_to_string(&f.path)
                .unwrap()
                .replace("synthetic", "other"),
        )
        .unwrap();
        let mut index = Index::open(f.config.clone()).unwrap();
        index.refresh(|_| {}).unwrap();
        drop(index);
        let mut index = Index::open(f.config.clone()).unwrap();
        f.append(20, 2);
        index.refresh(|_| {}).unwrap();
        assert_eq!(index.metrics.full_parses, 0);
        assert_eq!(index.metrics.incremental_parses, 1);
        assert_eq!(index.metrics.component_hits, 1);
        assert_eq!(index.metrics.normalized_components, 1);
        assert_eq!(total(&index), 30);
    }
    #[test]
    fn crash_child() {
        let Ok(root) = std::env::var("COUSASH_TEST_CRASH_ROOT") else {
            return;
        };
        let root = PathBuf::from(root);
        let config = IndexConfig {
            database: root.join("config/index.sqlite3"),
            sources: vec![LogSource {
                id: "fixture".into(),
                label: "Synthetic".into(),
                home: root.join("logs"),
            }],
        };
        let mut index = Index::open(config).unwrap();
        index.refresh(|_| {}).unwrap();
        panic!("fault checkpoint was not reached");
    }
    #[test]
    fn process_exit_at_each_write_stage_recovers_committed_state() {
        for stage in ["files", "sessions", "revision"] {
            let f = Fixture::new();
            let mut index = Index::open(f.config.clone()).unwrap();
            index.refresh(|_| {}).unwrap();
            let revision = index.snapshot.revision;
            drop(index);
            f.append(20, 2);
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "index::tests::crash_child", "--nocapture"])
                .env("COUSASH_TEST_CRASH_ROOT", f.dir.path())
                .env("COUSASH_TEST_CRASH_STAGE", stage)
                .stdout(std::process::Stdio::null())
                .status()
                .unwrap();
            assert_eq!(status.code(), Some(93));
            let mut recovered = Index::open(f.config).unwrap();
            assert_eq!(recovered.snapshot.revision, revision);
            assert_eq!(total(&recovered), 10);
            recovered.refresh(|_| {}).unwrap();
            assert_eq!(total(&recovered), 20);
            assert_eq!(recovered.snapshot.revision, revision + 1);
            assert!(recovered.refresh(|_| {}).unwrap().is_none());
        }
    }
    #[test]
    fn a_complete_non_newline_tail_must_be_reparsed_after_append() {
        let f = Fixture::new();
        let mut bytes = fs::read(&f.path).unwrap();
        bytes.pop();
        fs::write(&f.path, bytes).unwrap();
        let mut index = Index::open(f.config.clone()).unwrap();
        index.refresh(|_| {}).unwrap();
        assert_eq!(total(&index), 10);
        // The next JSON object joins the previous line; it is one malformed
        // complete record, not two independent events at an arbitrary offset.
        f.append(20, 2);
        index.refresh(|_| {}).unwrap();
        assert_eq!(total(&index), 0);
        assert_eq!(index.metrics.incremental_parses, 0);
        assert_eq!(index.snapshot.sessions[0].detail.parse_errors, 1);
    }
}
