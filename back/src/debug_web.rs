//! 読み取り専用のWebデバッグ画面と状態API。

use std::sync::{Arc, RwLock};

use axum::{
    Router,
    body::Body,
    extract::State,
    http::{StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use tokio::net::TcpListener;

const INDEX_HTML: &str = include_str!("../../tools/debug-web/dist/index.html");
const APP_JS: &[u8] = include_bytes!("../../tools/debug-web/dist/assets/app.js");
const APP_CSS: &[u8] = include_bytes!("../../tools/debug-web/dist/assets/app.css");

/// Bevyが作った最新SnapshotのJSONをHTTPタスクと共有する。
pub(crate) type SharedDebugSnapshot = Arc<RwLock<Option<String>>>;

#[derive(Clone)]
struct DebugWebState {
    snapshot: SharedDebugSnapshot,
}

pub(crate) fn empty_snapshot() -> SharedDebugSnapshot {
    Arc::new(RwLock::new(None))
}

/// 既存のTokio runtime上でデバッグ用HTTPサーバーを開始する。
pub(crate) async fn serve(bind_address: String, snapshot: SharedDebugSnapshot) {
    let listener = match TcpListener::bind(&bind_address).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("could not bind debug web server to {bind_address}: {error}");
            return;
        }
    };
    let state = DebugWebState { snapshot };
    let app = Router::new()
        .route("/", get(|| async { Redirect::temporary("/debug/") }))
        .route("/debug", get(|| async { Redirect::temporary("/debug/") }))
        .route("/debug/", get(index))
        .route("/debug/assets/app.js", get(app_js))
        .route("/debug/assets/app.css", get(app_css))
        .route("/debug/api/health", get(health))
        .route("/debug/api/state", get(current_state))
        .with_state(state);

    println!("Debug observer available at http://{bind_address}/debug/");
    if let Err(error) = axum::serve(listener, app).await {
        eprintln!("debug web server stopped: {error}");
    }
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn app_js() -> Response {
    static_asset(APP_JS, "text/javascript; charset=utf-8")
}

async fn app_css() -> Response {
    static_asset(APP_CSS, "text/css; charset=utf-8")
}

async fn health() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        r#"{"status":"ok","read_only":true}"#,
    )
}

async fn current_state(State(state): State<DebugWebState>) -> Response {
    let snapshot = state
        .snapshot
        .read()
        .ok()
        .and_then(|snapshot| snapshot.clone());
    match snapshot {
        Some(json) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json; charset=utf-8")
            .header(header::CACHE_CONTROL, "no-store")
            .body(Body::from(json))
            .expect("valid debug state response"),
        None => Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .header(header::CONTENT_TYPE, "application/json; charset=utf-8")
            .header(header::CACHE_CONTROL, "no-store")
            .body(Body::from(r#"{"error":"snapshot_not_ready"}"#))
            .expect("valid debug state unavailable response"),
    }
}

fn static_asset(content: &'static [u8], content_type: &'static str) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .body(Body::from(content))
        .expect("valid static asset response")
}
