//! Files of the interface, served by the server itself, to deliver Prisme as
//! a single executable.
//!
//! The compiled interface is the `web/dist` folder, produced by
//! `npm run build` (Vite): `index.html`, then the JavaScript and the CSS in
//! `assets/`.
//!
//! - In a release build (`cargo build --release`), `rust-embed` copies
//!   `web/dist` into the executable at compile time: running this single file
//!   is enough, without Node.js.
//! - In development (`cargo run`), `rust-embed` reads the files from disk on
//!   each request: a new `npm run build` is served without recompiling. To
//!   work on the interface, Vite (`npm run dev`) stays more convenient: it
//!   reloads the page on each change and forwards the API to this server.
//!
//! If `web/dist` does not exist (interface never compiled), the server starts
//! anyway, and the root says how to compile the interface.

use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

/// Interface compiled by Vite (`web/dist`, path relative to the crate).
#[derive(RustEmbed)]
#[folder = "../../web/dist/"]
#[allow_missing = true]
struct WebDist;

/// Page served at the root of the site.
const INDEX: &str = "index.html";

/// Response of the root when the interface was not compiled.
const NOT_BUILT: &str = "Prisme's interface is not compiled: run \"npm run build\" in \
    the web/ folder, then reload the page. In development, the interface is also served \
    by Vite (npm run dev) on http://localhost:5173.";

/// How long the browser keeps the files of `assets/`: one year. Their name
/// contains the hash of their content (`index-BzxA2zj2.js`): a new build
/// changes the name, never the content of a name already served.
const IMMUTABLE: &str = "public, max-age=31536000, immutable";

/// The other files (`index.html`, icon) are revalidated on each visit, so
/// that the page always points to the `assets/` of the last build.
const REVALIDATE: &str = "no-cache";

/// Fallback handler of the router: serves the interface file requested by the
/// URL, or a 404 error.
pub async fn serve(uri: Uri) -> Response {
    serve_from::<WebDist>(uri.path())
}

/// Serves a file of an embedded interface, from the path of the URL.
///
/// The root gives `index.html`. An unknown API path (`/api/...`) stays a 404
/// error, like any missing file.
fn serve_from<E: RustEmbed>(path: &str) -> Response {
    let file = match path.trim_start_matches('/') {
        "" => INDEX,
        file => file,
    };
    if file.starts_with("api/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    match E::get(file) {
        Some(content) => {
            let headers = [
                (
                    header::CONTENT_TYPE,
                    content.metadata.mimetype().to_string(),
                ),
                (header::CACHE_CONTROL, cache_control(file).to_string()),
            ];
            (headers, content.data).into_response()
        }
        None if file == INDEX => (StatusCode::SERVICE_UNAVAILABLE, NOT_BUILT).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Cache policy of an interface file.
fn cache_control(file: &str) -> &'static str {
    if file.starts_with("assets/") {
        IMMUTABLE
    } else {
        REVALIDATE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Small, stable compiled interface, for the tests.
    #[derive(RustEmbed)]
    #[folder = "testdata/web_dist/"]
    struct Fixture;

    /// Interface never compiled: missing folder.
    #[derive(RustEmbed)]
    #[folder = "testdata/absent/"]
    #[allow_missing = true]
    struct Absent;

    /// Status, useful headers and body of a response.
    async fn read(response: Response) -> (StatusCode, String, String, String) {
        let status = response.status();
        let header = |name| {
            response
                .headers()
                .get(name)
                .map_or(String::new(), |value| value.to_str().unwrap().to_string())
        };
        let (content_type, cache) = (header(header::CONTENT_TYPE), header(header::CACHE_CONTROL));
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body = String::from_utf8(bytes.to_vec()).unwrap();
        (status, content_type, cache, body)
    }

    #[tokio::test]
    async fn serves_index_at_the_root() {
        let (status, content_type, cache, body) = read(serve_from::<Fixture>("/")).await;
        assert_eq!(status, StatusCode::OK);
        assert!(content_type.starts_with("text/html"));
        assert_eq!(cache, REVALIDATE);
        assert!(body.contains("<div id=\"app\">"));
    }

    #[tokio::test]
    async fn serves_assets_with_a_long_cache() {
        let (status, content_type, cache, body) =
            read(serve_from::<Fixture>("/assets/index-test.js")).await;
        assert_eq!(status, StatusCode::OK);
        assert!(content_type.contains("javascript"));
        assert_eq!(cache, IMMUTABLE);
        assert!(body.contains("console.log"));
    }

    #[tokio::test]
    async fn unknown_paths_give_not_found() {
        for path in ["/absent.js", "/api/absent", "/api/"] {
            let (status, ..) = read(serve_from::<Fixture>(path)).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        }
    }

    #[tokio::test]
    async fn explains_how_to_build_a_missing_interface() {
        let (status, _, _, body) = read(serve_from::<Absent>("/")).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(body.contains("npm run build"));
    }
}
