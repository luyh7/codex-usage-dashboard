//! Local browser boundary. Local processes remain trusted; remote listeners are
//! unsupported. Never expose this service through an unauthenticated proxy.
use axum::{
    Json, Router,
    extract::{Request, State},
    http::{HeaderMap, Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::json;
use std::{io, net::SocketAddr, sync::Arc};

pub fn require_loopback(address: SocketAddr) -> io::Result<()> {
    if address.ip().is_loopback() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "only loopback --host addresses are supported; remote access requires a separately secured gateway",
        ))
    }
}

struct Security {
    authorities: Vec<String>,
    token: String,
}

pub fn protect(router: Router, address: SocketAddr) -> io::Result<Router> {
    require_loopback(address)?;
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(io::Error::other)?;
    let security = Arc::new(Security {
        authorities: vec![address.to_string(), format!("localhost:{}", address.port())],
        token: bytes.iter().map(|b| format!("{b:02x}")).collect(),
    });
    let session = security.clone();
    Ok(router
        .route(
            "/api/security/token",
            get(move || async move { Json(json!({"token": session.token})) }),
        )
        .layer(middleware::from_fn_with_state(security, guard)))
}

fn single<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let mut values = headers.get_all(name).iter();
    let value = values.next()?.to_str().ok()?;
    values.next().is_none().then_some(value)
}

fn validate(security: &Security, request: &Request) -> Result<(), (StatusCode, &'static str)> {
    let headers = request.headers();
    let host = single(headers, "host").ok_or((StatusCode::FORBIDDEN, "invalid Host"))?;
    if !security
        .authorities
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(host))
    {
        return Err((StatusCode::FORBIDDEN, "untrusted Host"));
    }
    if headers.contains_key(header::ORIGIN)
        && single(headers, "origin").is_none_or(|origin| origin != format!("http://{host}"))
    {
        return Err((StatusCode::FORBIDDEN, "untrusted Origin"));
    }
    if headers.contains_key("sec-fetch-site")
        && !matches!(
            single(headers, "sec-fetch-site"),
            Some("same-origin" | "none")
        )
    {
        return Err((StatusCode::FORBIDDEN, "cross-site request blocked"));
    }
    if !matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    ) {
        // Shutdown has its own launcher-only bearer token, never sent to the UI.
        if request.uri().path() == "/api/shutdown" {
            return Ok(());
        }
        if single(headers, "x-codex-write-token") != Some(security.token.as_str()) {
            return Err((StatusCode::FORBIDDEN, "missing or invalid write token"));
        }
        let content_type = single(headers, "content-type")
            .and_then(|s| s.split(';').next())
            .unwrap_or("")
            .trim();
        let expected = if request.uri().path() == "/api/remotes/import-file" {
            "application/octet-stream"
        } else {
            "application/json"
        };
        if !content_type.eq_ignore_ascii_case(expected) {
            return Err((
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "unexpected Content-Type",
            ));
        }
    }
    Ok(())
}

async fn guard(State(security): State<Arc<Security>>, request: Request, next: Next) -> Response {
    let mut response = match validate(&security, &request) {
        Ok(()) => next.run(request).await,
        Err((status, error)) => (status, Json(json!({"error": error}))).into_response(),
    };
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    headers.insert(header::X_FRAME_OPTIONS, "DENY".parse().unwrap());
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        "frame-ancestors 'none'".parse().unwrap(),
    );
    headers.insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;

    fn security() -> Security {
        Security {
            authorities: vec![
                "127.0.0.1:9876".into(),
                "localhost:9876".into(),
                "[::1]:9876".into(),
            ],
            token: "secret".into(),
        }
    }
    fn check(method: &str, path: &str, headers: &[(&str, &str)]) -> Result<(), StatusCode> {
        let mut request = Request::builder().method(method).uri(path);
        for (key, value) in headers {
            request = request.header(*key, *value);
        }
        validate(&security(), &request.body(Body::empty()).unwrap()).map_err(|e| e.0)
    }
    #[test]
    fn rejects_remote_binds_and_untrusted_read_origins() {
        for address in ["0.0.0.0:9876", "192.168.1.1:9876", "[::]:9876"] {
            assert!(require_loopback(address.parse().unwrap()).is_err());
        }
        for host in [
            "evil.test:9876",
            "127.0.0.1:8765",
            "localhost.evil.test:9876",
            "127.0.0.1:9876@evil.test",
        ] {
            assert_eq!(
                check("GET", "/api/export.json", &[("host", host)]),
                Err(StatusCode::FORBIDDEN)
            );
        }
        assert!(check("GET", "/api/security/token", &[]).is_err());
        for origin in [
            "null",
            "http://evil.test",
            "http://127.0.0.1:9877",
            "https://127.0.0.1:9876",
        ] {
            assert!(
                check(
                    "GET",
                    "/api/security/token",
                    &[("host", "127.0.0.1:9876"), ("origin", origin)]
                )
                .is_err()
            );
        }
        for site in ["cross-site", "same-site"] {
            assert!(
                check(
                    "GET",
                    "/api/export.json",
                    &[("host", "127.0.0.1:9876"), ("sec-fetch-site", site)]
                )
                .is_err()
            );
        }
    }
    #[test]
    fn supports_loopback_and_requires_token_and_content_type_for_mutations() {
        for host in ["127.0.0.1:9876", "localhost:9876", "[::1]:9876"] {
            assert!(
                check(
                    "GET",
                    "/api/health",
                    &[("host", host), ("origin", &format!("http://{host}"))]
                )
                .is_ok()
            );
        }
        let mut headers = vec![
            ("host", "127.0.0.1:9876"),
            ("content-type", "application/json"),
        ];
        assert_eq!(
            check("POST", "/api/remotes/delete", &headers),
            Err(StatusCode::FORBIDDEN)
        );
        headers.push(("x-codex-write-token", "wrong"));
        assert!(check("POST", "/api/remotes/delete", &headers).is_err());
        headers.pop();
        headers.push(("x-codex-write-token", "secret"));
        for path in [
            "/api/remotes/import",
            "/api/remotes/rename",
            "/api/remotes/delete",
            "/api/export/start",
            "/api/sources/choose",
        ] {
            assert!(check("POST", path, &headers).is_ok());
            let mut bad = headers.clone();
            bad[1].1 = "text/plain";
            assert_eq!(
                check("POST", path, &bad),
                Err(StatusCode::UNSUPPORTED_MEDIA_TYPE)
            );
        }
        headers[1].1 = "application/octet-stream";
        assert!(check("POST", "/api/remotes/import-file", &headers).is_ok());
        headers.push(("host", "evil.test"));
        assert!(check("GET", "/api/security/token", &headers).is_err());
    }
}
