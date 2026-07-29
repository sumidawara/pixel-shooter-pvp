//! GameServerの内部Control APIをAdmin APIへ中継する。

use axum::{
    Json,
    body::Body,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::Response,
};
use pixel_shooter_admin_protocol::{GameServerView, StepRequest};
use serde::Deserialize;

use crate::{
    routes::error,
    state::{AppState, server_view},
};

#[derive(Deserialize)]
pub(crate) struct ServerQuery {
    server_id: Option<String>,
}

pub(crate) async fn debug_snapshot(
    State(state): State<AppState>,
    Query(query): Query<ServerQuery>,
) -> Response {
    let server = match select_server(&state, query.server_id.as_deref()).await {
        Ok(server) => server,
        Err(response) => return response,
    };
    proxy_get(&state, format!("{}/internal/snapshot", server.control_url)).await
}

pub(crate) async fn pause(
    State(state): State<AppState>,
    Path(server_id): Path<String>,
) -> Response {
    proxy_control(&state, &server_id, "pause", None).await
}

pub(crate) async fn server_state(
    State(state): State<AppState>,
    Path(server_id): Path<String>,
) -> Response {
    let server = match select_server(&state, Some(&server_id)).await {
        Ok(server) => server,
        Err(response) => return response,
    };
    proxy_get(&state, format!("{}/internal/state", server.control_url)).await
}

pub(crate) async fn step(
    State(state): State<AppState>,
    Path(server_id): Path<String>,
    Json(request): Json<StepRequest>,
) -> Response {
    proxy_control(&state, &server_id, "step", Some(request)).await
}

pub(crate) async fn resume(
    State(state): State<AppState>,
    Path(server_id): Path<String>,
) -> Response {
    proxy_control(&state, &server_id, "resume", None).await
}

pub(crate) async fn proxy_control(
    state: &AppState,
    server_id: &str,
    action: &str,
    step: Option<StepRequest>,
) -> Response {
    let server = match select_server(state, Some(server_id)).await {
        Ok(server) => server,
        Err(response) => return response,
    };
    let url = format!("{}/internal/debug/{action}", server.control_url);
    let request = state.client.post(url);
    let response = match step {
        Some(step) => request.json(&step).send().await,
        None => request.send().await,
    };
    proxy_response(response).await
}

pub(crate) async fn select_server(
    state: &AppState,
    requested_id: Option<&str>,
) -> Result<GameServerView, Response> {
    let servers = state.servers.read().await;
    let record = requested_id.and_then(|id| servers.get(id)).or_else(|| {
        servers
            .values()
            .min_by_key(|server| &server.registration.server_id)
    });
    record
        .map(server_view)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "game_server_not_found"))
}

pub(crate) async fn proxy_get(state: &AppState, url: String) -> Response {
    proxy_response(state.client.get(url).send().await).await
}

pub(crate) async fn proxy_response(result: Result<reqwest::Response, reqwest::Error>) -> Response {
    let response = match result {
        Ok(response) => response,
        Err(_) => return error(StatusCode::BAD_GATEWAY, "game_server_unreachable"),
    };
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .cloned()
        .unwrap_or_else(|| "application/json".parse().expect("content type"));
    let body = response.bytes().await.unwrap_or_default();
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(body))
        .expect("proxy response")
}
