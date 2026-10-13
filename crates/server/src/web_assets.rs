//! Fichiers de l'interface, servis par le serveur lui-même, pour livrer
//! Prisme en un seul exécutable.
//!
//! L'interface compilée est le dossier `web/dist`, produit par `npm run build`
//! (Vite) : `index.html`, puis le JavaScript et le CSS dans `assets/`.
//!
//! - En version de production (`cargo build --release`), `rust-embed` copie
//!   `web/dist` dans l'exécutable à la compilation : il suffit de lancer ce
//!   seul fichier, sans Node.js.
//! - En développement (`cargo run`), `rust-embed` lit les fichiers sur le
//!   disque à chaque requête : un nouveau `npm run build` est servi sans
//!   recompiler. Pour travailler sur l'interface, Vite (`npm run dev`) reste
//!   plus pratique : il recharge la page à chaque modification et redirige
//!   l'API vers ce serveur.
//!
//! Si `web/dist` n'existe pas (interface jamais compilée), le serveur démarre
//! quand même, et la racine dit comment compiler l'interface.

use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

/// Interface compilée par Vite (`web/dist`, chemin relatif au crate).
#[derive(RustEmbed)]
#[folder = "../../web/dist/"]
#[allow_missing = true]
struct WebDist;

/// Page servie à la racine du site.
const INDEX: &str = "index.html";

/// Réponse de la racine quand l'interface n'a pas été compilée.
const NOT_BUILT: &str = "Interface de Prisme non compilée : lancez « npm run build » dans \
le dossier web/, puis rechargez la page. En développement, l'interface est aussi servie \
par Vite (npm run dev) sur http://localhost:5173.";

/// Durée de conservation des fichiers de `assets/` par le navigateur : un an.
/// Leur nom contient l'empreinte de leur contenu (`index-BzxA2zj2.js`) : un
/// nouveau build change le nom, jamais le contenu d'un nom déjà servi.
const IMMUTABLE: &str = "public, max-age=31536000, immutable";

/// Les autres fichiers (`index.html`, icône) sont revalidés à chaque visite,
/// pour que la page désigne toujours les `assets/` du dernier build.
const REVALIDATE: &str = "no-cache";

/// Handler de repli du routeur : sert le fichier de l'interface demandé par
/// l'URL, ou une erreur 404.
pub async fn serve(uri: Uri) -> Response {
    serve_from::<WebDist>(uri.path())
}

/// Sert un fichier d'une interface embarquée, d'après le chemin de l'URL.
///
/// La racine donne `index.html`. Un chemin de l'API inconnu (`/api/...`)
/// reste une erreur 404, comme tout fichier absent.
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

/// Politique de cache d'un fichier de l'interface.
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

    /// Petite interface compilée, stable, pour les tests.
    #[derive(RustEmbed)]
    #[folder = "testdata/web_dist/"]
    struct Fixture;

    /// Interface jamais compilée : dossier absent.
    #[derive(RustEmbed)]
    #[folder = "testdata/absent/"]
    #[allow_missing = true]
    struct Absent;

    /// Statut, en-têtes utiles et corps d'une réponse.
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
