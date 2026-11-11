//! Guard of the requests: only this machine reaches the server, through the
//! interface it serves itself.
//!
//! In workstation mode, the server has no login: whoever reaches it runs
//! queries on the saved connections, with their passwords. A web page of
//! another site can reach it through DNS rebinding, a name that first points
//! elsewhere and then to `127.0.0.1`: the browser then sends the other site's
//! name in the `Host` header. The guard therefore accepts a request only if
//! its `Host` names this machine, and, when the browser sends an `Origin`
//! header, if that origin is the same host (`http://prisme.localhost:10000`
//! for `Host: prisme.localhost:10000`). The Vite server of the development
//! interface keeps the browser's `Host`, so it passes the same check.
//!
//! The names of this machine are its loopback addresses, `localhost` and
//! every name under `.localhost` (`prisme.localhost`): RFC 6761 reserves them
//! for the loopback, which Chrome, Edge and Firefox resolve them to without
//! asking any DNS server. Each tool of the machine can so have its own name,
//! hence its own origin in the browser (local storage, cookies).

use axum::extract::Request;
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

/// Loopback addresses accepted in the `Host` header, without the port.
const LOOPBACK_ADDRESSES: &[&str] = &["127.0.0.1", "[::1]"];

/// Name reserved for this machine, alone or as the end of a longer name.
const LOCALHOST: &str = "localhost";

/// Passes the request on if it comes from this machine, refuses it otherwise.
pub async fn local_only(request: Request, next: Next) -> Response {
    if is_local(request.headers()) {
        next.run(request).await
    } else {
        StatusCode::FORBIDDEN.into_response()
    }
}

/// Tells whether the headers come from this machine: the `Host` names this
/// machine, and the `Origin`, if present, is that same host over `http`. A
/// request without `Host` is refused, as HTTP/1.1 requires it.
fn is_local(headers: &HeaderMap) -> bool {
    let Some(host) = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    if !names_this_machine(host_name(host)) {
        return false;
    }
    match headers.get(header::ORIGIN) {
        None => true,
        Some(origin) => origin
            .to_str()
            .ok()
            .and_then(|origin| origin.strip_prefix("http://"))
            .is_some_and(|origin_host| origin_host == host),
    }
}

/// Tells whether a host name, without its port, names this machine: a
/// loopback address, `localhost` or a name under `.localhost`, in any case.
fn names_this_machine(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    LOOPBACK_ADDRESSES.contains(&name.as_str())
        || name
            .strip_suffix(LOCALHOST)
            .is_some_and(|start| start.is_empty() || start.ends_with('.'))
}

/// Name of a `Host` value without its port: `localhost:8484` gives
/// `localhost`, `[::1]:8484` gives `[::1]`.
fn host_name(host: &str) -> &str {
    if host.starts_with('[') {
        host.find(']').map_or(host, |end| &host[..=end])
    } else {
        host.split(':').next().unwrap_or(host)
    }
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::Body;
    use axum::http::{HeaderValue, Request};
    use axum::middleware::from_fn;
    use axum::routing::get;
    use tower::ServiceExt;

    use super::*;

    /// Headers of a request with the given `Host` and `Origin`.
    fn headers(host: Option<&str>, origin: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(host) = host {
            headers.insert(header::HOST, HeaderValue::from_str(host).unwrap());
        }
        if let Some(origin) = origin {
            headers.insert(header::ORIGIN, HeaderValue::from_str(origin).unwrap());
        }
        headers
    }

    #[test]
    fn accepts_loopback_hosts_with_any_port() {
        assert!(is_local(&headers(Some("localhost:8484"), None)));
        assert!(is_local(&headers(Some("127.0.0.1:8484"), None)));
        assert!(is_local(&headers(Some("[::1]:8484"), None)));
        assert!(is_local(&headers(Some("localhost:5173"), None)));
    }

    #[test]
    fn accepts_the_names_under_localhost() {
        assert!(is_local(&headers(Some("prisme.localhost:10000"), None)));
        assert!(is_local(&headers(Some("a.b.localhost:5173"), None)));
        assert!(is_local(&headers(Some("Prisme.LOCALHOST:10000"), None)));
        assert!(is_local(&headers(
            Some("prisme.localhost:5173"),
            Some("http://prisme.localhost:5173")
        )));
    }

    #[test]
    fn refuses_other_hosts_and_missing_host() {
        assert!(!is_local(&headers(Some("evil.example:8484"), None)));
        assert!(!is_local(&headers(
            Some("localhost.evil.example:8484"),
            None
        )));
        assert!(!is_local(&headers(Some("evillocalhost:8484"), None)));
        assert!(!is_local(&headers(Some("localhost.:8484"), None)));
        assert!(!is_local(&headers(Some("192.168.1.10:8484"), None)));
        assert!(!is_local(&headers(None, None)));
    }

    #[test]
    fn accepts_the_origin_of_the_same_host_only() {
        assert!(is_local(&headers(
            Some("localhost:5173"),
            Some("http://localhost:5173")
        )));
        assert!(!is_local(&headers(
            Some("localhost:8484"),
            Some("http://localhost:5173")
        )));
        assert!(!is_local(&headers(
            Some("localhost:8484"),
            Some("http://evil.example")
        )));
        assert!(!is_local(&headers(Some("localhost:8484"), Some("null"))));
    }

    #[tokio::test]
    async fn refuses_a_rebound_request_before_the_route() {
        let app = Router::new()
            .route("/api/health", get(|| async { "ok" }))
            .layer(from_fn(local_only));
        let rebound = Request::get("/api/health")
            .header(header::HOST, "evil.example:8484")
            .body(Body::empty())
            .unwrap();
        let local = Request::get("/api/health")
            .header(header::HOST, "localhost:8484")
            .body(Body::empty())
            .unwrap();

        assert_eq!(
            app.clone().oneshot(rebound).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(app.oneshot(local).await.unwrap().status(), StatusCode::OK);
    }
}
