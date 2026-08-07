//! GameServer登録、heartbeat、ルーム割り当て。

use std::{collections::HashMap, time::Instant};

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use pixel_shooter_admin_protocol::{
    AllocateRoomRequest, AllocationResponse, GameServerHeartbeat, GameServerRegistration,
    GameServerStatus, GameServerView,
};

use crate::{
    routes::error,
    state::{
        AppState, HEALTH_TIMEOUT, MAX_PLAYERS, ServerRecord, allocation_response, server_view,
    },
};

pub(crate) async fn register(
    State(state): State<AppState>,
    Json(registration): Json<GameServerRegistration>,
) -> StatusCode {
    let mut servers = state.servers.write().await;
    let previous = servers.get(&registration.server_id).cloned();
    servers.insert(
        registration.server_id.clone(),
        ServerRecord {
            registration,
            status: previous
                .as_ref()
                .map_or(GameServerStatus::Available, |server| server.status),
            room_id: previous.as_ref().and_then(|server| server.room_id.clone()),
            player_count: previous.as_ref().map_or(0, |server| server.player_count),
            accepting_players: previous
                .as_ref()
                .is_some_and(|server| server.accepting_players),
            reservations: previous
                .as_ref()
                .map_or_else(Vec::new, |server| server.reservations.clone()),
            tick: previous.as_ref().map_or(0, |server| server.tick),
            simulation_mode: previous.map_or(
                pixel_shooter_admin_protocol::SimulationMode::Realtime,
                |server| server.simulation_mode,
            ),
            last_seen: Instant::now(),
        },
    );
    StatusCode::NO_CONTENT
}

pub(crate) async fn heartbeat(
    State(state): State<AppState>,
    Json(heartbeat): Json<GameServerHeartbeat>,
) -> Response {
    let mut servers = state.servers.write().await;
    let Some(server) = servers.get_mut(&heartbeat.server_id) else {
        return error(StatusCode::NOT_FOUND, "game_server_not_registered");
    };
    server.status = heartbeat.status;
    server.room_id = heartbeat.room_id;
    server.player_count = heartbeat.player_count;
    server.accepting_players = heartbeat.accepting_players;
    if heartbeat.status == GameServerStatus::Available {
        // ルームが空へ戻ったので、残っている割当も破棄する。
        server.reservations.clear();
    } else {
        server.prune_reservations(Instant::now());
    }
    server.tick = heartbeat.tick;
    server.simulation_mode = heartbeat.simulation_mode;
    server.last_seen = Instant::now();
    StatusCode::NO_CONTENT.into_response()
}

pub(crate) async fn list_servers(State(state): State<AppState>) -> Json<Vec<GameServerView>> {
    let servers = state.servers.read().await;
    let mut views = servers.values().map(server_view).collect::<Vec<_>>();
    views.sort_by(|left, right| left.server_id.cmp(&right.server_id));
    Json(views)
}

/// 割当先の選択を再現可能にするため、常に同じ順序でGameServerを見る。
///
/// HashMapの列挙順は不定なので、順序が揺れるとほぼ同時に来た2人が
/// 別々のルームへ入り、いつまでも出会えなくなる。
fn sorted_server_ids(servers: &HashMap<String, ServerRecord>) -> Vec<String> {
    let mut server_ids: Vec<String> = servers.keys().cloned().collect();
    server_ids.sort();
    server_ids
}

/// 合流できるルームのserver_idを選ぶ。ついでに期限切れの割当を返す。
///
/// `accepting_players` はGameServerが報告する「今すぐJoinを受理できるか」であり、
/// 試合が始まったルームでは false になる。これを見ないと、空きGameServerが
/// あるのに走行中のルームへ案内し、GameServerに拒否されて行き止まりになる。
fn find_joinable_room(servers: &mut HashMap<String, ServerRecord>, now: Instant) -> Option<String> {
    for server_id in sorted_server_ids(servers) {
        let Some(server) = servers.get_mut(&server_id) else {
            continue;
        };
        if now.duration_since(server.last_seen) > HEALTH_TIMEOUT
            || server.status != GameServerStatus::Allocated
            || !server.accepting_players
            || server.room_id.is_none()
        {
            continue;
        }
        // 期限切れの割当を返してから空き枠を数える。
        server.prune_reservations(now);
        if server.occupied_seats() >= MAX_PLAYERS {
            continue;
        }
        return Some(server_id);
    }
    None
}

/// まだルームを持っていないGameServerを選ぶ。
fn find_available_server(
    servers: &HashMap<String, ServerRecord>,
    now: Instant,
) -> Option<ServerRecord> {
    sorted_server_ids(servers)
        .into_iter()
        .filter_map(|server_id| servers.get(&server_id))
        .filter(|server| now.duration_since(server.last_seen) <= HEALTH_TIMEOUT)
        .find(|server| server.status == GameServerStatus::Available)
        .cloned()
}

pub(crate) async fn allocate(
    State(state): State<AppState>,
    Json(request): Json<AllocateRoomRequest>,
) -> Response {
    // 2つの同時マッチング要求が同じ空きサーバーを奪わないよう直列化する。
    let _allocation_guard = state.allocation_lock.lock().await;
    let now = Instant::now();

    // 先に、参加枠が残っている既存ルームへ合流させる。
    if let Some(response) = {
        let mut servers = state.servers.write().await;
        find_joinable_room(&mut servers, now).map(|server_id| {
            let server = servers.get_mut(&server_id).expect("joinable room");
            server.reservations.push(now);
            allocation_response(server)
        })
    } {
        return Json(response).into_response();
    }

    let candidate = {
        let servers = state.servers.read().await;
        find_available_server(&servers, now)
    };
    let Some(candidate) = candidate else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "no_game_server_available");
    };
    let url = format!("{}/internal/allocate", candidate.registration.control_url);
    let response = match state.client.post(url).json(&request).send().await {
        Ok(response) if response.status().is_success() => response,
        _ => return error(StatusCode::BAD_GATEWAY, "game_server_allocation_failed"),
    };
    drop(response);

    let mut servers = state.servers.write().await;
    let Some(server) = servers.get_mut(&candidate.registration.server_id) else {
        return error(StatusCode::CONFLICT, "game_server_disappeared");
    };
    server.status = GameServerStatus::Allocated;
    server.room_id = Some(request.room_id.clone());
    // 割り当てたばかりのルームは、次のheartbeatが届くまで受付中として扱う。
    server.accepting_players = true;
    server.reservations = vec![now];
    Json(AllocationResponse {
        server_id: server.registration.server_id.clone(),
        room_id: request.room_id,
        game_url: server.registration.public_url.clone(),
    })
    .into_response()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use pixel_shooter_admin_protocol::SimulationMode;

    use super::*;
    use crate::state::RESERVATION_TTL;

    fn record(
        server_id: &str,
        status: GameServerStatus,
        accepting_players: bool,
        player_count: usize,
        now: Instant,
    ) -> ServerRecord {
        ServerRecord {
            registration: GameServerRegistration {
                server_id: server_id.into(),
                public_url: format!("ws://{server_id}"),
                control_url: format!("http://{server_id}"),
            },
            status,
            room_id: (status == GameServerStatus::Allocated).then(|| format!("room-{server_id}")),
            player_count,
            accepting_players,
            reservations: vec![now; player_count],
            tick: 0,
            simulation_mode: SimulationMode::Realtime,
            last_seen: now,
        }
    }

    fn registry(records: Vec<ServerRecord>) -> HashMap<String, ServerRecord> {
        records
            .into_iter()
            .map(|record| (record.registration.server_id.clone(), record))
            .collect()
    }

    #[test]
    fn a_running_match_is_not_offered_even_when_it_has_free_seats() {
        let now = Instant::now();
        let mut servers = registry(vec![
            // 試合中のルーム。席は空いているがJoinは受理されない。
            record("game-1", GameServerStatus::Allocated, false, 2, now),
        ]);

        assert_eq!(
            find_joinable_room(&mut servers, now),
            None,
            "試合中のルームへ案内すると、GameServerに拒否されて行き止まりになる"
        );
        assert_eq!(
            find_available_server(&servers, now).map(|server| server.registration.server_id),
            None
        );
    }

    #[test]
    fn a_waiting_room_is_preferred_over_starting_a_new_one() {
        let now = Instant::now();
        let mut servers = registry(vec![
            record("game-1", GameServerStatus::Allocated, true, 1, now),
            record("game-2", GameServerStatus::Available, false, 0, now),
        ]);

        assert_eq!(
            find_joinable_room(&mut servers, now).as_deref(),
            Some("game-1"),
            "待機中のルームがあれば、そこへ合流させて対戦を成立させる"
        );
    }

    #[test]
    fn a_full_room_is_skipped_and_a_new_server_is_used() {
        let now = Instant::now();
        let mut servers = registry(vec![
            record(
                "game-1",
                GameServerStatus::Allocated,
                true,
                MAX_PLAYERS,
                now,
            ),
            record("game-2", GameServerStatus::Available, false, 0, now),
        ]);

        assert_eq!(find_joinable_room(&mut servers, now), None);
        assert_eq!(
            find_available_server(&servers, now).map(|server| server.registration.server_id),
            Some("game-2".into())
        );
    }

    #[test]
    fn expired_reservations_release_their_seat() {
        let now = Instant::now();
        let stale = now - RESERVATION_TTL - Duration::from_secs(1);
        let mut servers = registry(vec![record(
            "game-1",
            GameServerStatus::Allocated,
            true,
            0,
            now,
        )]);
        // 接続してこなかった割当で満席になっている状態。
        servers.get_mut("game-1").expect("record").reservations = vec![stale; MAX_PLAYERS];

        assert_eq!(
            find_joinable_room(&mut servers, now).as_deref(),
            Some("game-1"),
            "接続されないまま期限切れになった割当は席を返す"
        );
        assert_eq!(servers.get("game-1").expect("record").reservations.len(), 0);
    }

    #[test]
    fn a_reservation_holds_a_seat_before_the_player_connects() {
        let now = Instant::now();
        let mut servers = registry(vec![record(
            "game-1",
            GameServerStatus::Allocated,
            true,
            0,
            now,
        )]);
        // まだ誰も接続していないが、4人ぶんのTicketを発行済み。
        servers.get_mut("game-1").expect("record").reservations = vec![now; MAX_PLAYERS];

        assert_eq!(
            find_joinable_room(&mut servers, now),
            None,
            "発行済みのJoin Ticketぶんは席を確保し、定員超過を防ぐ"
        );
    }

    #[test]
    fn unhealthy_servers_are_never_selected() {
        let now = Instant::now();
        let long_ago = now - HEALTH_TIMEOUT - Duration::from_secs(1);
        let mut stale_allocated = record("game-1", GameServerStatus::Allocated, true, 1, now);
        stale_allocated.last_seen = long_ago;
        let mut stale_available = record("game-2", GameServerStatus::Available, false, 0, now);
        stale_available.last_seen = long_ago;
        let mut servers = registry(vec![stale_allocated, stale_available]);

        assert_eq!(find_joinable_room(&mut servers, now), None);
        assert_eq!(
            find_available_server(&servers, now).map(|s| s.registration.server_id),
            None
        );
    }

    #[test]
    fn selection_does_not_depend_on_hash_map_ordering() {
        let now = Instant::now();
        // 同じ条件のルームが複数ある場合、常に同じ1つを選ばないと
        // ほぼ同時に来た2人が別々のルームへ散ってしまう。
        for _ in 0..16 {
            let mut servers = registry(vec![
                record("game-3", GameServerStatus::Allocated, true, 1, now),
                record("game-1", GameServerStatus::Allocated, true, 1, now),
                record("game-2", GameServerStatus::Allocated, true, 1, now),
            ]);
            assert_eq!(
                find_joinable_room(&mut servers, now).as_deref(),
                Some("game-1")
            );
        }
    }
}
