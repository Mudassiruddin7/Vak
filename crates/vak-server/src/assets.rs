use axum::body::Body;
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../web/"]
struct Assets;

/// Serves the embedded front end. Unknown paths fall back to the app shell so deep links work; files
/// with a dot in the last segment that don't exist are a real 404.
pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    if path.contains("..") {
        return StatusCode::NOT_FOUND.into_response();
    }
    let (file, name) = match Assets::get(path) {
        Some(f) => (f, path.to_string()),
        None if !path.rsplit('/').next().unwrap_or("").contains('.') && !path.starts_with("api/") => match Assets::get("index.html") {
            Some(f) => (f, "index.html".to_string()),
            None => return StatusCode::NOT_FOUND.into_response(),
        },
        None => return StatusCode::NOT_FOUND.into_response(),
    };
    let mime = mime_guess::from_path(&name).first_or_octet_stream();
    let cache = if name.ends_with(".woff2") || name.starts_with("fonts/") || name.starts_with("img/") {
        "public, max-age=31536000, immutable"
    } else if name == "index.html" {
        "no-cache"
    } else {
        "public, max-age=300, must-revalidate"
    };
    Response::builder()
        .header(header::CONTENT_TYPE, mime.as_ref())
        .header(header::CACHE_CONTROL, cache)
        .body(Body::from(file.data.into_owned()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
