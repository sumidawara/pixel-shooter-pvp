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
    AllocateRoomRequest, AllocationResponse, GameServerHeartbeat, GameServerRegistration,
    GameServerView, JoinTicketClaims, MatchmakeRequest, MatchmakeResponse, RoomListEntry,
    RoomListResponse, encode_join_ticket,
};
use pixel_shooter_protocol::MAX_PLAYERS;
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

impl AppState {
    /// 接続先から、その部屋を持っているサーバーを引き当てる。
    ///
    /// `server_id` は制御面のエンドポイントの鍵になるのでクライアントへ配っていない。
    /// クライアントが知っているのは繋ぎ先だけなので、対応付けはここで行う。
    async fn server_id_for(&self, game_url: &str) -> Option<String> {
        let response = self
            .client
            .get(format!("{}/api/servers", self.admin_url))
            .send()
            .await
            .ok()?;
        let servers = response.json::<Vec<GameServerView>>().await.ok()?;
        servers
            .into_iter()
            .find(|server| server.public_url == game_url)
            .map(|server| server.server_id)
    }
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
        .route("/v1/rooms", get(list_rooms))
        .route("/v1/game-servers/register", post(relay_register))
        .route("/v1/game-servers/heartbeat", post(relay_heartbeat))
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

/// プレイヤーへ見せるルームの一覧。
///
/// AdminServerの`/api/servers`をそのまま転送してはいけない。あちらの
/// `GameServerView`には`control_url`が入っており、それは試合を止める・1tick進める
/// 操作の宛先そのものになる。運用のための面と、参加先を選ぶための面は分ける。
///
/// 並びは「入れる部屋が先、その中では空いている順」。押せない行が上に溜まると、
/// 一覧を見る意味が薄れる。
async fn list_rooms(State(state): State<AppState>) -> Response {
    let response = state
        .client
        .get(format!("{}/api/servers", state.admin_url))
        .send()
        .await;
    let Ok(response) = response else {
        return error(StatusCode::BAD_GATEWAY, "lobby_unreachable");
    };
    let Ok(servers) = response.json::<Vec<GameServerView>>().await else {
        return error(StatusCode::BAD_GATEWAY, "lobby_returned_garbage");
    };
    Json(RoomListResponse {
        rooms: visible_rooms(servers),
    })
    .into_response()
}

/// 運用向けのサーバー一覧を、プレイヤーへ見せるルーム一覧へ落とす。
///
/// 落とすことが目的の関数である。`GameServerView` の `control_url` や
/// `server_id` はここで捨てる。
///
/// 並びは「入れる部屋が先、その中では空いている順」。押せない行が上に溜まると、
/// 一覧を見る意味が薄れる。
fn visible_rooms(servers: Vec<GameServerView>) -> Vec<RoomListEntry> {
    let mut rooms: Vec<RoomListEntry> = servers
        .into_iter()
        // 応答が途絶えたサーバーは載せない。押しても繋がらない行が並ぶだけになる。
        .filter(|server| server.healthy)
        .map(|server| RoomListEntry {
            game_url: server.public_url,
            host_name: server.host_name,
            player_count: server.player_count,
            max_players: MAX_PLAYERS,
            accepting_players: server.accepting_players,
        })
        .collect();
    rooms.sort_by(|left, right| {
        right
            .accepting_players
            .cmp(&left.accepting_players)
            .then(left.player_count.cmp(&right.player_count))
            .then(left.host_name.cmp(&right.host_name))
    });
    rooms
}

/// GameServerの登録をAdminServerへ中継する。
///
/// 手元で開いた部屋を一覧へ載せるには、GameServerがロビーへ名乗る必要がある。
/// そのためにAdminServerのURLをクライアントへ配ると、`/api/servers/{id}/pause` も
/// 一緒に配ることになる。窓口をこちらに一本化し、制御面は外から見えないままにする。
async fn relay_register(
    State(state): State<AppState>,
    Json(registration): Json<GameServerRegistration>,
) -> Response {
    relay(&state, "/internal/game-servers/register", &registration).await
}

async fn relay_heartbeat(
    State(state): State<AppState>,
    Json(heartbeat): Json<GameServerHeartbeat>,
) -> Response {
    relay(&state, "/internal/game-servers/heartbeat", &heartbeat).await
}

async fn relay<T: Serialize>(state: &AppState, path: &str, body: &T) -> Response {
    match state
        .client
        .post(format!("{}{path}", state.admin_url))
        .json(body)
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => StatusCode::NO_CONTENT.into_response(),
        Ok(response) => error(
            StatusCode::BAD_GATEWAY,
            &format!("lobby_rejected_{}", response.status().as_u16()),
        ),
        Err(_) => error(StatusCode::BAD_GATEWAY, "lobby_unreachable"),
    }
}

async fn matchmake(
    State(state): State<AppState>,
    Json(request): Json<MatchmakeRequest>,
) -> Response {
    let now = unix_time();
    let room_id = next_room_id(now, state.room_sequence.fetch_add(1, Ordering::Relaxed));
    // 一覧から選んだ部屋の接続先が来ていれば、その1台へ入れる。
    // クライアントは server_id を知らない（制御面の鍵になるので配っていない）ので、
    // ここで接続先から引き当てる。
    let server_id = match &request.game_url {
        Some(game_url) => match state.server_id_for(game_url).await {
            Some(server_id) => Some(server_id),
            None => return error(StatusCode::NOT_FOUND, "room_not_found"),
        },
        None => None,
    };
    let allocation = state
        .client
        .post(format!("{}/internal/allocate", state.admin_url))
        .json(&AllocateRoomRequest { room_id, server_id })
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
    use pixel_shooter_admin_protocol::{
        GameServerStatus, SimulationMode, TicketError, decode_join_ticket,
    };

    const SECRET: &[u8] = b"matchmaker-test-secret";

    fn server(host: &str, players: usize, accepting: bool, healthy: bool) -> GameServerView {
        GameServerView {
            server_id: format!("game-server-{host}"),
            public_url: format!("ws://127.0.0.1:9001/{host}"),
            // これが外へ漏れてはいけない。試合を止める操作の宛先そのもの。
            control_url: "http://127.0.0.1:9101".into(),
            status: GameServerStatus::Allocated,
            room_id: Some("room-1".into()),
            player_count: players,
            accepting_players: accepting,
            host_name: host.into(),
            reserved_players: players,
            tick: 100,
            simulation_mode: SimulationMode::Realtime,
            healthy,
        }
    }

    /// 制御面のURLが一覧へ混ざらないこと。
    ///
    /// `GameServerView`をそのまま返すと、誰でも他人の試合を止められる。
    /// 型を分けてあるので取り違えは起きにくいが、落とすこと自体が目的なので
    /// ここで固定する。
    #[test]
    fn the_room_list_never_carries_the_control_plane() {
        let rooms = visible_rooms(vec![server("A", 1, true, true)]);
        let json = serde_json::to_string(&rooms).expect("serialize rooms");
        assert!(!json.contains("9101"), "制御APIのURLが漏れている: {json}");
        assert!(
            !json.contains("control"),
            "制御面の項目が漏れている: {json}"
        );
        assert!(
            !json.contains("game-server-A"),
            "server_id が漏れている: {json}"
        );
    }

    #[test]
    fn rooms_you_can_enter_come_first() {
        let rooms = visible_rooms(vec![
            server("Full", 4, false, true),
            server("Open", 2, true, true),
        ]);
        assert_eq!(rooms[0].host_name, "Open");
        assert_eq!(rooms[1].host_name, "Full");
    }

    /// 入れる部屋どうしは、空いている順。
    #[test]
    fn emptier_rooms_come_first_among_the_open_ones() {
        let rooms = visible_rooms(vec![
            server("Crowded", 3, true, true),
            server("Quiet", 1, true, true),
        ]);
        assert_eq!(rooms[0].host_name, "Quiet");
    }

    /// 応答が途絶えたサーバーは載せない。
    ///
    /// 押しても繋がらない行を並べると、一覧そのものが信用されなくなる。
    #[test]
    fn a_server_that_stopped_answering_is_not_listed() {
        let rooms = visible_rooms(vec![server("Gone", 1, true, false)]);
        assert!(rooms.is_empty());
    }

    /// 分母はサーバーと同じ値を使うこと。
    #[test]
    fn the_capacity_matches_the_server() {
        let rooms = visible_rooms(vec![server("A", 1, true, true)]);
        assert_eq!(rooms[0].max_players, MAX_PLAYERS);
    }

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
