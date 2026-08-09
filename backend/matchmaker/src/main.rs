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
    let room_id = next_room_id(now, state.room_sequence.fetch_add(1, Ordering::Relaxed));
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
            let (status, message) = allocation_failure(response.status());
            return error(status, message);
        }
        Err(_) => return error(StatusCode::BAD_GATEWAY, "admin_server_unreachable"),
    };

    Json(issue_ticket(
        state.join_secret.as_bytes(),
        allocation,
        &request.player_name,
        now,
    ))
    .into_response()
}

/// 割り当てられた部屋への入場券を作る。
///
/// 券に載せる部屋IDは、こちらが希望した番号ではなく **AdminServerが実際に割り当てた**
/// 方を使う。取り違えると、券は正しいのにGameServer側で別の部屋と判定され、
/// 入場できない。
fn issue_ticket(
    secret: &[u8],
    allocation: AllocationResponse,
    player_name: &str,
    now_unix: u64,
) -> MatchmakeResponse {
    let expires_at_unix = now_unix + TICKET_LIFETIME_SECONDS;
    let join_ticket = encode_join_ticket(
        secret,
        &JoinTicketClaims {
            room_id: allocation.room_id.clone(),
            player_name: sanitize_name(player_name),
            expires_at_unix,
        },
    );
    MatchmakeResponse {
        server_id: allocation.server_id,
        room_id: allocation.room_id,
        game_url: allocation.game_url,
        join_ticket,
        expires_at_unix,
    }
}

/// 割り当てに失敗したときに、クライアントへ返す状態と理由。
///
/// 空きが無いだけ(503)なら、待って試せば入れる。それ以外はこちらの
/// 構成の問題なので、区別して返す。
fn allocation_failure(status: StatusCode) -> (StatusCode, &'static str) {
    if status == StatusCode::SERVICE_UNAVAILABLE {
        (StatusCode::SERVICE_UNAVAILABLE, "no_game_server_available")
    } else {
        (StatusCode::BAD_GATEWAY, "no_game_server_available")
    }
}

/// 希望する部屋ID。同時に来た要求どうしがぶつからないようにする。
fn next_room_id(now_unix: u64, sequence: u64) -> String {
    format!("room-{now_unix}-{sequence}")
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

#[cfg(test)]
mod tests {
    use super::*;
    use pixel_shooter_admin_protocol::{TicketError, decode_join_ticket};

    const SECRET: &[u8] = b"matchmaker-test-secret";

    fn allocation() -> AllocationResponse {
        AllocationResponse {
            server_id: "game-server-2".into(),
            room_id: "room-assigned-by-admin".into(),
            game_url: "ws://127.0.0.1:9002".into(),
        }
    }

    /// 券に載る部屋IDが、AdminServerの割り当てた方であること。
    ///
    /// こちらが希望した番号を載せると、券は正しいのにGameServerで別の部屋と
    /// 判定されて入場できない。要求と結果が食い違い得る以上、結果を使う。
    #[test]
    fn the_ticket_names_the_room_the_admin_server_assigned() {
        let response = issue_ticket(SECRET, allocation(), "Player", 1_000);

        assert_eq!(response.room_id, "room-assigned-by-admin");
        let claims = decode_join_ticket(SECRET, &response.join_ticket, 1_000).expect("ticket");
        assert_eq!(claims.room_id, "room-assigned-by-admin");
        assert_eq!(response.server_id, "game-server-2");
        assert_eq!(response.game_url, "ws://127.0.0.1:9002");
    }

    /// 券が短命であること。期限は応答にも載せ、券の中身とも一致すること。
    #[test]
    fn the_ticket_expires_and_says_when() {
        let now = 1_700_000_000;
        let response = issue_ticket(SECRET, allocation(), "Player", now);

        assert_eq!(response.expires_at_unix, now + TICKET_LIFETIME_SECONDS);
        let claims = decode_join_ticket(SECRET, &response.join_ticket, now).expect("ticket");
        assert_eq!(claims.expires_at_unix, response.expires_at_unix);

        // 期限を過ぎたら弾かれる。
        assert!(matches!(
            decode_join_ticket(SECRET, &response.join_ticket, response.expires_at_unix + 1),
            Err(TicketError::Expired)
        ));
    }

    /// 別の秘密鍵で作った券が通らないこと。
    ///
    /// ここが通ると、誰でも入場券を自作できる。
    #[test]
    fn a_ticket_signed_with_another_secret_is_rejected() {
        let response = issue_ticket(b"someone-elses-secret", allocation(), "Player", 1_000);

        assert!(matches!(
            decode_join_ticket(SECRET, &response.join_ticket, 1_000),
            Err(TicketError::InvalidSignature)
        ));
    }

    /// 名前が整えられてから券に載ること。
    ///
    /// 券の中身はGameServerがそのまま表示名に使う。整える前に載せると、
    /// 空欄や極端に長い名前がそのまま入る。
    #[test]
    fn the_name_is_tidied_before_it_goes_into_the_ticket() {
        let response = issue_ticket(SECRET, allocation(), "   ", 1_000);
        let claims = decode_join_ticket(SECRET, &response.join_ticket, 1_000).expect("ticket");
        assert_eq!(claims.player_name, "Player", "空の名前が空のまま載る");

        let long = "0123456789ABCDEFGHIJ";
        let response = issue_ticket(SECRET, allocation(), long, 1_000);
        let claims = decode_join_ticket(SECRET, &response.join_ticket, 1_000).expect("ticket");
        assert_eq!(claims.player_name.chars().count(), 16);
        assert_eq!(sanitize_name("  Ada  "), "Ada");
    }

    /// 空きが無いだけの失敗と、それ以外を区別して返すこと。
    ///
    /// 待てば入れるのか、こちらの構成が壊れているのかで、クライアントの
    /// 取るべき行動が変わる。
    #[test]
    fn a_full_pool_is_reported_differently_from_a_broken_admin_server() {
        assert_eq!(
            allocation_failure(StatusCode::SERVICE_UNAVAILABLE).0,
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            allocation_failure(StatusCode::INTERNAL_SERVER_ERROR).0,
            StatusCode::BAD_GATEWAY
        );
    }

    /// 続けて来た要求が、別の部屋IDを希望すること。
    #[test]
    fn each_request_asks_for_a_different_room() {
        assert_ne!(next_room_id(1_000, 1), next_room_id(1_000, 2));
        assert!(next_room_id(1_000, 7).contains("1000"));
    }
}
