//! AdminServerのRouter構築とリスナー起動。

use axum::{
    Router,
    response::Redirect,
    routing::{get, post},
};
use tokio::net::TcpListener;

use crate::{
    routes::{
        assets::{app_css, app_js, index},
        controls::{debug_snapshot, pause, resume, server_state, step},
        health,
        registry::{allocate, heartbeat, list_servers, register},
    },
    state::AppState,
};

pub(crate) async fn run() {
    let bind_address =
        std::env::var("PIXEL_SHOOTER_ADMIN_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8081".into());
    let app = Router::new()
        .route("/", get(|| async { Redirect::temporary("/debug/") }))
        .route("/debug", get(|| async { Redirect::temporary("/debug/") }))
        .route("/debug/", get(index))
        .route("/debug/assets/app.js", get(app_js))
        .route("/debug/assets/app.css", get(app_css))
        .route("/debug/api/health", get(health))
        .route("/debug/api/state", get(debug_snapshot))
        .route("/api/servers", get(list_servers))
        .route("/api/servers/{server_id}/state", get(server_state))
        .route("/api/servers/{server_id}/pause", post(pause))
        .route("/api/servers/{server_id}/step", post(step))
        .route("/api/servers/{server_id}/resume", post(resume))
        .route("/internal/health", get(health))
        .route("/internal/game-servers/register", post(register))
        .route("/internal/game-servers/heartbeat", post(heartbeat))
        .route("/internal/allocate", post(allocate))
        .with_state(AppState::new());

    let listener = TcpListener::bind(&bind_address)
        .await
        .expect("bind AdminServer");
    println!("AdminServer listening on http://{bind_address}/debug/");
    axum::serve(listener, app).await.expect("serve AdminServer");
}
