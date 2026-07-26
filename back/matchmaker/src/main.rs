//! プレイヤーを空きルームへ割り当て、短命なJoin Ticketを発行する。

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use pixel_shooter_admin_protocol::{
    AllocateRoomRequest, AllocationResponse, JoinTicketClaims, MatchmakeRequest, MatchmakeResponse,
    encode_join_ticket,
};
use serde::Serialize;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;

const TICKET_LIFETIME_SECONDS: u64 = 60;

#[derive(Clone)]
struct AppState {
    admin_url: String,
    join_secret: Arc<String>,
    client: reqwest::Client,
    room_sequence: Arc<AtomicU64>,
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

#[tokio::main]
async fn main() {
    let bind_address = std::env::var("PIXEL_SHOOTER_MATCHMAKER_BIND_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:8080".into());
    let state = AppState {
        admin_url: std::env::var("PIXEL_SHOOTER_ADMIN_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8081".into()),
        join_secret: Arc::new(
            std::env::var("PIXEL_SHOOTER_JOIN_SECRET")
                .unwrap_or_else(|_| "development-only-secret".into()),
        ),
        client: reqwest::Client::new(),
        room_sequence: Arc::new(AtomicU64::new(1)),
    };
    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/matchmake", post(matchmake))
        .with_state(state)
        .layer(CorsLayer::permissive());
    let listener = TcpListener::bind(&bind_address)
        .await
        .expect("bind Matchmaker");
    println!("Matchmaker listening on http://{bind_address}");
    axum::serve(listener, app).await.expect("serve Matchmaker");
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}

async fn matchmake(
    State(state): State<AppState>,
    Json(request): Json<MatchmakeRequest>,
) -> Response {
    let now = unix_time();
    let room_id = format!(
        "room-{now}-{}",
        state.room_sequence.fetch_add(1, Ordering::Relaxed)
    );
    let allocation = state
        .client
        .post(format!("{}/internal/allocate", state.admin_url))
        .json(&AllocateRoomRequest { room_id })
        .send()
        .await;
    let allocation = match allocation {
        Ok(response) if response.status().is_success() => {
            match response.json::<AllocationResponse>().await {
                Ok(allocation) => allocation,
                Err(_) => return error(StatusCode::BAD_GATEWAY, "invalid_admin_response"),
            }
        }
        Ok(response) => {
            return error(
                if response.status() == StatusCode::SERVICE_UNAVAILABLE {
                    StatusCode::SERVICE_UNAVAILABLE
                } else {
                    StatusCode::BAD_GATEWAY
                },
                "no_game_server_available",
            );
        }
        Err(_) => return error(StatusCode::BAD_GATEWAY, "admin_server_unreachable"),
    };

    let player_name = sanitize_name(&request.player_name);
    let expires_at_unix = now + TICKET_LIFETIME_SECONDS;
    let join_ticket = encode_join_ticket(
        state.join_secret.as_bytes(),
        &JoinTicketClaims {
            room_id: allocation.room_id.clone(),
            player_name,
            expires_at_unix,
        },
    );
    Json(MatchmakeResponse {
        server_id: allocation.server_id,
        room_id: allocation.room_id,
        game_url: allocation.game_url,
        join_ticket,
        expires_at_unix,
    })
    .into_response()
}

fn sanitize_name(name: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        "Player".into()
    } else {
        name.chars().take(16).collect()
    }
}

fn error(status: StatusCode, message: &str) -> Response {
    (
        status,
        Json(ErrorBody {
            error: message.into(),
        }),
    )
        .into_response()
}

fn unix_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
