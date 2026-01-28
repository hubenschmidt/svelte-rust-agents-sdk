use axum::{
    body::Body,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "client/dist"]
struct Assets;

pub fn routes() -> Router {
    Router::new()
        .route("/", get(index))
        .route("/*path", get(static_handler))
}

async fn index() -> impl IntoResponse {
    static_handler(axum::extract::Path("index.html".to_string())).await
}

async fn static_handler(axum::extract::Path(path): axum::extract::Path<String>) -> Response {
    let path = path.trim_start_matches('/');

    let asset = Assets::get(path).or_else(|| Assets::get("index.html"));

    let Some(content) = asset else {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    };

    let mime = mime_guess::from_path(path).first_or_octet_stream();

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime.as_ref())
        .body(Body::from(content.data.into_owned()))
        .unwrap()
}
