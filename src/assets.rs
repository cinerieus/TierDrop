use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "static/"]
pub struct StaticAssets;

pub async fn serve_static(
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Response {
    match StaticAssets::get(&path) {
        Some(file) => {
            let mime = if path.ends_with(".svg") {
                "image/svg+xml"
            } else {
                mime_guess::from_path(&path)
                    .first_raw()
                    .unwrap_or("application/octet-stream")
            };

            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, HeaderValue::from_str(mime).unwrap())
                .header(
                    header::CACHE_CONTROL,
                    HeaderValue::from_static("public, max-age=31536000, immutable"),
                )
                .body(axum::body::Body::from(file.data.to_vec()))
                .unwrap()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
