use crate::service::State;
use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Query, State as Extract},
    http::{StatusCode, header},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{collections::HashMap, convert::Infallible, sync::Arc};
use tokio_stream::{StreamExt, wrappers::WatchStream};
use usage_core::{
    Error,
    view::{Filter, QueryError},
};
fn response(status: StatusCode, value: &impl Serialize) -> Response {
    match serde_json::to_vec(value) {
        Ok(bytes) => (
            status,
            [
                (header::CONTENT_TYPE, "application/json; charset=utf-8"),
                (header::CACHE_CONTROL, "no-store"),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "serialization failed").into_response(),
    }
}
fn error(error: QueryError) -> Response {
    let (status, body) = match error {
        QueryError::Stale => (
            StatusCode::CONFLICT,
            json!({"error":"snapshot stale","code":"snapshot_stale"}),
        ),
        QueryError::Missing => (
            StatusCode::NOT_FOUND,
            json!({"error":"session not found","code":"session_not_found"}),
        ),
        QueryError::DetailsMissing => (
            StatusCode::NOT_FOUND,
            json!({"error":"snapshot does not contain session details","code":"details_missing"}),
        ),
        QueryError::Invalid(e) => (
            if e == "index is loading" {
                StatusCode::SERVICE_UNAVAILABLE
            } else {
                StatusCode::BAD_REQUEST
            },
            json!({"error":e}),
        ),
        QueryError::Core(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"error":e.to_string()}),
        ),
    };
    response(status, &body)
}
async fn blocking(
    state: Arc<State>,
    task: impl FnOnce() -> Result<Response, QueryError> + Send + 'static,
) -> Response {
    let Ok(permit) = state.query_slots.clone().acquire_owned().await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match tokio::task::spawn_blocking(move || {
        let _permit = permit;
        task()
    })
    .await
    {
        Ok(Ok(response)) => response,
        Ok(Err(e)) => error(e),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
async fn health(Extract(state): Extract<Arc<State>>) -> Response {
    response(
        StatusCode::OK,
        &json!({"ok":true,"app":"codex-usage-dashboard","protocol_version":1,"implementation":"rust","ui":env!("COUSASH_UI"),"timezone":state.zone.name(),"source_sha256":env!("COUSASH_BUILD_ID"),"build_id":env!("COUSASH_BUILD_ID"),"version":env!("CARGO_PKG_VERSION"),"pid":std::process::id(),"codex_homes":state.sources.iter().map(|s|s.home.to_string_lossy()).collect::<Vec<_>>(),"instance_id":std::env::var("COUSASH_INSTANCE_ID").unwrap_or_default(),"features":["paged-events-v1","separate-chart-v1","indexed-search-v1","periods","period-deltas","all-period","multi-codex-home","remote-snapshot-v1","background-index-v1","sse-revision-v1"],"device_short_code":state.device,"index":*state.status.read().unwrap()}),
    )
}
async fn shutdown(Extract(state): Extract<Arc<State>>, headers: axum::http::HeaderMap) -> Response {
    let token = std::env::var("COUSASH_SHUTDOWN_TOKEN").unwrap_or_default();
    if token.len() < 32
        || headers
            .get(header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            != Some(format!("Bearer {token}").as_str())
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    state.events.send_replace(json!({"phase":"shutdown"}));
    state.shutdown.notify_one();
    StatusCode::ACCEPTED.into_response()
}
fn text<'a>(p: &'a HashMap<String, String>, key: &str) -> &'a str {
    p.get(key).map(String::as_str).unwrap_or("")
}
fn limit(p: &HashMap<String, String>) -> Result<usize, QueryError> {
    p.get("limit")
        .map(|s| {
            s.parse()
                .map_err(|_| QueryError::Invalid("invalid limit".into()))
        })
        .unwrap_or(Ok(100))
}
fn filter(p: &HashMap<String, String>) -> Filter {
    Filter {
        search: text(p, "search").into(),
        model: text(p, "model").into(),
        source: text(p, "source").into(),
        environment: text(p, "environment").into(),
        project: text(p, "project").into(),
        agents: text(p, "agents").into(),
        sort: text(p, "sort").into(),
        ascending: text(p, "ascending") == "true",
    }
}
fn current_device(state: &State, published: &crate::service::Published) -> Value {
    let mut counts = HashMap::<&str, usize>::new();
    for session in &published.local.sessions {
        *counts
            .entry(session.detail.environment_id.as_str())
            .or_default() += 1;
    }
    let sources: Vec<_> = state.sources.iter().map(|source| {
        json!({"id":source.id,"label":source.label,"session_count":counts.get(source.id.as_str()).copied().unwrap_or(0)})
    }).collect();
    json!({"device_short_code":state.device,"label":state.hostname,"session_count":published.local.sessions.len(),"sources":sources})
}
async fn paged(
    Extract(state): Extract<Arc<State>>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    blocking(state.clone(),move||{
    let(view,published)=state.view(&params,!text(&params,"cursor").is_empty()||!text(&params,"revision").is_empty())?;
    let mut page=view.query_sessions(&filter(&params),text(&params,"cursor"),limit(&params)?)?;
    if text(&params,"cursor").is_empty()&&let Ok(offset)=text(&params,"offset").parse::<usize>()&&let Some(cursor)=usage_core::view::anchor_cursor(&page,offset){page=view.query_sessions(&filter(&params),&cursor,limit(&params)?)?;}
    if text(&params,"if_query_token")==page.query_token{return Ok(StatusCode::NO_CONTENT.into_response())}
    Ok(response(StatusCode::OK,&json!({"page":page,"period":view.period,"generated_at":view.generated_at,"codex_sources":published.sources,"remotes":published.remotes,"current_device_short_code":state.device,"current_device":current_device(&state,&published),"facets":view.facets()?,"metric_trend":view.query_metric_trend(&filter(&params))?})))
}).await
}
async fn project_list(
    Extract(state): Extract<Arc<State>>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    blocking(state.clone(),move||{
    let(view,published)=state.view(&params,!text(&params,"cursor").is_empty()||!text(&params,"revision").is_empty())?;
    let mut page=view.query_projects(&filter(&params),text(&params,"cursor"),limit(&params)?)?;
    if text(&params,"cursor").is_empty()&&let Ok(offset)=text(&params,"offset").parse::<usize>()&&let Some(cursor)=usage_core::view::anchor_cursor(&page,offset){page=view.query_projects(&filter(&params),&cursor,limit(&params)?)?;}
    if text(&params,"if_query_token")==page.query_token{return Ok(StatusCode::NO_CONTENT.into_response())}
    Ok(response(StatusCode::OK,&json!({"page":page,"period":view.period,"generated_at":view.generated_at,"codex_sources":published.sources,"remotes":published.remotes,"current_device_short_code":state.device,"current_device":current_device(&state,&published),"facets":view.facets()?,"metric_trend":view.query_metric_trend(&filter(&params))?})))
}).await
}
async fn projects(
    Extract(state): Extract<Arc<State>>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    blocking(state.clone(), move || {
        let (view, _) = state.view(&params, true)?;
        let mut page = view.query_project_tasks(
            &filter(&params),
            text(&params, "project"),
            text(&params, "cursor"),
            limit(&params)?,
        )?;
        if text(&params, "cursor").is_empty()
            && let Ok(offset) = text(&params, "offset").parse::<usize>()
            && let Some(cursor) = usage_core::view::anchor_cursor(&page, offset)
        {
            page = view.query_project_tasks(
                &filter(&params),
                text(&params, "project"),
                &cursor,
                limit(&params)?,
            )?;
        }
        Ok(response(StatusCode::OK, &page))
    })
    .await
}
async fn metadata(
    Extract(state): Extract<Arc<State>>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    blocking(state.clone(), move || {
        let (view, _) = state.view(&params, true)?;
        if text(&params, "check_filters") == "true"
            && !view.session_matches(&filter(&params), text(&params, "id"))?
        {
            return Ok(response(
                StatusCode::NOT_FOUND,
                &json!({
                    "error":"session excluded by filters", "code":"session_filtered_out"
                }),
            ));
        }
        let detail = view.query_session_detail(text(&params, "id"))?;
        let has_subagents = view.summaries().any(|session| {
            session.is_subagent
                && session.environment_id == detail.environment_id
                && session.root_session_id == detail.identity.session_id
        });
        if let Some(mut value) = view.legacy_detail(text(&params, "id"))? {
            value.as_object_mut().unwrap().remove("timeline");
            value["revision"] = json!(view.revision);
            value["has_subagents"] = json!(has_subagents);
            return Ok(response(StatusCode::OK, &value));
        }
        let mut meta = detail.metadata();
        meta.tasks = detail.tasks.clone();
        let mut value = serde_json::to_value(meta)?;
        value.as_object_mut().unwrap().remove("timeline");
        value["revision"] = json!(view.revision);
        value["has_subagents"] = json!(has_subagents);
        Ok(response(StatusCode::OK, &value))
    })
    .await
}
async fn events(
    Extract(state): Extract<Arc<State>>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    blocking(state.clone(), move || {
        let (view, _) = state.view(&params, true)?;
        let mut cursor = text(&params, "cursor").to_string();
        if cursor.is_empty()
            && let Ok(offset) = text(&params, "offset").parse::<usize>()
        {
            cursor = view.event_anchor(text(&params, "id"), offset, limit(&params)?)?;
        }
        if let Some(value) = view.legacy_events(text(&params, "id"), &cursor, limit(&params)?)? {
            return Ok(response(StatusCode::OK, &value));
        }
        Ok(response(
            StatusCode::OK,
            &view.query_events(text(&params, "id"), &cursor, limit(&params)?)?,
        ))
    })
    .await
}
async fn chart(
    Extract(state): Extract<Arc<State>>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    blocking(state.clone(), move || {
        let (view, _) = state.view(&params, true)?;
        Ok(response(
            StatusCode::OK,
            &view.query_chart(text(&params, "id"))?,
        ))
    })
    .await
}
async fn daily(
    Extract(state): Extract<Arc<State>>,
    Query(mut params): Query<HashMap<String, String>>,
) -> Response {
    blocking(state.clone(),move||{params.insert("period".into(),"all".into());let(view,_)=state.view(&params,false)?;Ok(response(StatusCode::OK,&json!({"generated_at":view.generated_at,"daily_usage":view.daily_usage,"daily_usage_complete":true}))) }).await
}
async fn export(Extract(state): Extract<Arc<State>>) -> Response {
    blocking(state.clone(), move || {
        let mut result = response(StatusCode::OK, &state.export()?);
        result.headers_mut().insert(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"cousash-{}.json\"", state.device)
                .parse()
                .unwrap(),
        );
        Ok(result)
    })
    .await
}
async fn remotes(Extract(state): Extract<Arc<State>>) -> Response {
    match state.publication() {
        Ok(p) => response(
            StatusCode::OK,
            &json!({"current_device_short_code":state.device,"remotes":p.remotes}),
        ),
        Err(e) => error(e),
    }
}
fn gzip_response(state: &State, bytes: Bytes) -> Response {
    (
        [
            (header::CONTENT_TYPE, "application/gzip".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"cousash-{}.json.gz\"", state.device),
            ),
            (header::CACHE_CONTROL, "no-store".to_owned()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
        ],
        bytes,
    )
        .into_response()
}
async fn export_gzip(
    Extract(state): Extract<Arc<State>>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    if let Some(id) = params.get("job") {
        let jobs = state.exports.lock().unwrap();
        if let Some(job) = jobs.iter().find(|job| &job.id == id) {
            if let Some(Ok(bytes)) = job.result.lock().unwrap().as_ref() {
                return gzip_response(&state, bytes.clone());
            }
            return response(
                StatusCode::CONFLICT,
                &json!({"error":"export is not ready"}),
            );
        }
        return response(
            StatusCode::NOT_FOUND,
            &json!({"error":"export expired; please retry"}),
        );
    }
    blocking(state.clone(), move || {
        let view = state.local_view()?;
        let bytes = crate::snapshot::gzip(&state.export_document(&view, None)?)?;
        Ok(gzip_response(&state, bytes.into()))
    })
    .await
}
async fn start_export(Extract(state): Extract<Arc<State>>) -> Response {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let published = match state.publication() {
        Ok(p) => p,
        Err(e) => return error(e),
    };
    let mut jobs = state.exports.lock().unwrap();
    if let Some(job) = jobs.iter().rev().find(|job| {
        let result = job.result.lock().unwrap();
        result.is_none()
            || (job.revision == published.local.revision && matches!(result.as_ref(), Some(Ok(_))))
    }) {
        return response(StatusCode::OK, &job.status());
    }
    let job = Arc::new(crate::snapshot::ExportJob {
        id: format!(
            "{}-{}",
            published.local.revision,
            chrono::Utc::now().timestamp_micros()
        ),
        revision: published.local.revision,
        percent: AtomicUsize::new(1),
        result: std::sync::Mutex::new(None),
    });
    jobs.push_back(job.clone());
    while jobs.len() > 3 {
        jobs.pop_front();
    }
    let status = job.status();
    drop(jobs);
    tokio::task::spawn_blocking(move || {
        let result = (|| -> Result<Bytes, QueryError> {
            // Capture one publication so concurrent log updates cannot mix export revisions.
            let period = usage_core::query::Period::bounds(
                "all",
                None,
                None,
                state.zone,
                usage_core::timestamp::parse(&published.local.generated_at)
                    .unwrap_or_else(chrono::Utc::now),
            )?;
            let view = usage_core::view::View::new(
                &published.local,
                period,
                state.zone,
                &usage_core::PriceSchedule::bundled(),
            )?;
            Ok(crate::snapshot::gzip(&state.export_document(&view, Some(&job.percent))?)?.into())
        })();
        let mut completed = job.result.lock().unwrap();
        *completed = Some(result.map_err(|e| format!("{e:?}")));
        job.percent.store(95, Ordering::Relaxed);
    });
    response(StatusCode::OK, &status)
}
async fn export_progress(
    Extract(state): Extract<Arc<State>>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let jobs = state.exports.lock().unwrap();
    match jobs.iter().find(|job| Some(&job.id) == params.get("id")) {
        Some(job) => response(StatusCode::OK, &job.status()),
        None => response(
            StatusCode::NOT_FOUND,
            &json!({"error":"export expired; please retry"}),
        ),
    }
}
async fn mutate(
    state: Arc<State>,
    action: &str,
    body: Bytes,
    file_options: Option<HashMap<String, String>>,
) -> Response {
    let Ok(permit) = state.query_slots.clone().acquire_owned().await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let payload: Value = match tokio::task::spawn_blocking(move || {
        let _permit = permit;
        if let Some(options) = file_options {
            let snapshot = crate::snapshot::decode(&body)?;
            Ok::<_, Error>(json!({"snapshot": snapshot, "label": options.get("label"), "allow_current_device": options.get("allow_current_device").is_some_and(|s| s == "true")}))
        } else {
            Ok(serde_json::from_slice(body.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&body))?)
        }
    })
    .await
    {
        Ok(Ok(value)) => value,
        result => {
            return response(
                StatusCode::BAD_REQUEST,
                &json!({"ok":false,"error": match result { Ok(Err(e)) => e.to_string(), _ => "invalid snapshot body".into() }}),
            );
        }
    };
    if !payload.is_object() {
        return response(
            StatusCode::BAD_REQUEST,
            &json!({"ok":false,"error":"JSON body must be an object"}),
        );
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    if state
        .sender
        .try_send(crate::service::Command::Remote(action.into(), payload, tx))
        .is_err()
    {
        return response(
            StatusCode::SERVICE_UNAVAILABLE,
            &json!({"ok":false,"error":"index worker is busy"}),
        );
    }
    match rx.await {
        Ok(Ok(value)) => response(
            if value["ok"] == false {
                StatusCode::CONFLICT
            } else {
                StatusCode::OK
            },
            &value,
        ),
        Ok(Err(e)) => {
            let status = if matches!(&e,Error::Io(e) if e.kind()==std::io::ErrorKind::NotFound) {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::BAD_REQUEST
            };
            response(
                status,
                &json!({"ok":false,"error":if status==StatusCode::NOT_FOUND{"remote data not found".into()}else{e.to_string()}}),
            )
        }
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}
async fn subscribe(Extract(state): Extract<Arc<State>>) -> impl IntoResponse {
    let stream = WatchStream::new(state.events.subscribe())
        .take_while(|status| status["phase"] != "shutdown")
        .map(|status| {
            let id = status["revision"].to_string();
            Ok::<_, Infallible>(
                Event::default()
                    .event("revision")
                    .id(id)
                    .data(status.to_string()),
            )
        });
    Sse::new(stream).keep_alive(KeepAlive::default())
}
include!(concat!(env!("OUT_DIR"), "/assets.rs"));
async fn static_file(uri: axum::http::Uri) -> Response {
    let path = if uri.path() == "/" {
        "/index.html"
    } else {
        uri.path()
    };
    if let Some((_, mime, bytes)) = ASSETS.iter().find(|(name, _, _)| *name == path) {
        return (
            [
                (header::CONTENT_TYPE, *mime),
                (header::CACHE_CONTROL, "no-cache"),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            ],
            *bytes,
        )
            .into_response();
    }
    StatusCode::NOT_FOUND.into_response()
}
async fn previous_entry(uri: axum::http::Uri) -> impl IntoResponse {
    let location = uri
        .query()
        .map_or_else(|| "/".into(), |query| format!("/?{query}"));
    axum::response::Redirect::permanent(&location)
}
pub fn router(state: Arc<State>) -> Router {
    Router::new()
        .route("/", get(static_file))
        .route("/index.html", get(static_file))
        .route("/svelte", get(previous_entry))
        .route("/svelte/", get(previous_entry))
        .fallback(static_file)
        .route("/api/health", get(health))
        .route("/api/shutdown", post(shutdown))
        .route("/api/daily-usage", get(daily))
        .route("/api/v2/sessions", get(paged))
        .route("/api/v2/project-tasks", get(projects))
        .route("/api/v2/projects", get(project_list))
        .route("/api/v2/session", get(metadata))
        .route("/api/v2/events", get(events))
        .route("/api/v2/chart", get(chart))
        .route("/api/revisions", get(subscribe))
        .route("/api/export.json", get(export))
        .route("/api/export.json.gz", get(export_gzip))
        .route("/api/export/start", post(start_export))
        .route("/api/export/progress", get(export_progress))
        .route("/api/remotes", get(remotes))
        .route(
            "/api/remotes/import-file",
            post(
                |Extract(s): Extract<Arc<State>>,
                 Query(options): Query<HashMap<String, String>>,
                 b: Bytes| async { mutate(s, "import", b, Some(options)).await },
            )
            .layer(DefaultBodyLimit::max(crate::snapshot::MAX_BYTES)),
        )
        .route(
            "/api/remotes/import",
            post(|Extract(s): Extract<Arc<State>>, b: Bytes| async {
                mutate(s, "import", b, None).await
            }),
        )
        .route(
            "/api/remotes/rename",
            post(|Extract(s): Extract<Arc<State>>, b: Bytes| async {
                mutate(s, "rename", b, None).await
            }),
        )
        .route(
            "/api/remotes/delete",
            post(|Extract(s): Extract<Arc<State>>, b: Bytes| async {
                mutate(s, "delete", b, None).await
            }),
        )
        .layer(DefaultBodyLimit::max(100 * 1024 * 1024))
        .with_state(state)
}
