//! ビルド済みSvelteデバッグ画面の配信。

use axum::{
    body::Body,
    http::{StatusCode, header},
    response::Response,
};

const INDEX_HTML: &str = include_str!("../../../../tools/debug-web/dist/index.html");
const APP_JS: &[u8] = include_bytes!("../../../../tools/debug-web/dist/assets/app.js");
const APP_CSS: &[u8] = include_bytes!("../../../../tools/debug-web/dist/assets/app.css");

pub(crate) async fn index() -> Response {
    asset("text/html; charset=utf-8", INDEX_HTML.as_bytes())
}

pub(crate) async fn app_js() -> Response {
    asset("text/javascript; charset=utf-8", APP_JS)
}

pub(crate) async fn app_css() -> Response {
    asset("text/css; charset=utf-8", APP_CSS)
}

fn asset(content_type: &'static str, body: &'static [u8]) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .body(Body::from(body))
        .expect("asset response")
}
