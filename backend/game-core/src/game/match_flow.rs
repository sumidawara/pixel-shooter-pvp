//! 待機、試合開始、終了、切断復帰を含むMatchの状態遷移。

use bevy::prelude::*;
use pixel_shooter_protocol::MatchPhase;

use crate::{
    arena::ArenaMap,
    model::{Bullet, GhostThief, LarokinPoppos, MatchState, Player, ScoreItem},
    schedule::GameClock,
    settings::GameSettings,
};

use super::respawn::reset_player;

/// 待機から時間制ポイントマッチ終了までの状態遷移を管理するSystem。
///
/// `ResMut<MatchState>` はResourceを変更可能で借りる指定。
/// `Query<Entity, With<Bullet>>` はBulletを持つEntity番号だけを取得するフィルター。
#[allow(clippy::too_many_arguments)]
pub(crate) fn update_match(
    clock: Res<GameClock>,
    settings: Res<GameSettings>,
    map: Res<ArenaMap>,
    mut state: ResMut<MatchState>,
    mut players: Query<(Entity, &mut Player)>,
    bullets: Query<Entity, With<Bullet>>,
    items: Query<Entity, With<ScoreItem>>,
    attackers: Query<Entity, With<LarokinPoppos>>,
    thieves: Query<Entity, With<GhostThief>>,
    mut commands: Commands,
) {
    // 試合中だけ存在するEntityをまとめて片付ける。
    // 種類ごとに書き並べていると、増やしたときに消し忘れる箇所が出る。
    let clear_arena = |commands: &mut Commands| {
        for entity in &bullets {
            commands.entity(entity).despawn();
        }
        for entity in &items {
            commands.entity(entity).despawn();
        }
        for entity in &attackers {
            commands.entity(entity).despawn();
        }
        for entity in &thieves {
            commands.entity(entity).despawn();
        }
    };
    let dt = clock.delta_seconds();

    // ロビーまたは結果画面で最後の人間が退出した場合、CPUだけのルームを残さない。
    // CPUは接続を持たないため、この状態を放置するとホスト不在のまま開始不能になる。
    let has_player = players.iter().next().is_some();
    let has_human_player = players.iter().any(|(_, player)| !player.is_cpu);
    if has_player
        && !has_human_player
        && matches!(state.phase, MatchPhase::Waiting | MatchPhase::MatchFinished)
    {
        for (entity, _) in &players {
            commands.entity(entity).despawn();
        }
        clear_arena(&mut commands);
        reset_empty_room(&mut state);
        state.tick += 1;
        println!("room reset because no human players remain");
        return;
    }

    // 切断中のプレイヤーには猶予時間を与え、Entityと試合状態を保持する。
    let mut expired = Vec::new();
    for (entity, mut player) in &mut players {
        if !player.is_cpu && player.connection_id.is_none() {
            player.reconnect_grace_left = (player.reconnect_grace_left - dt).max(0.0);
            if player.reconnect_grace_left <= 0.0 {
                expired.push((entity, player.id));
            }
        }
    }

    if !expired.is_empty() {
        let expired_ids: Vec<u64> = expired.iter().map(|(_, id)| *id).collect();
        let remaining_human_count = players
            .iter()
            .filter(|(_, player)| !player.is_cpu && !expired_ids.contains(&player.id))
            .count();

        // 人間+CPUの対戦で最後の人間の再接続猶予が切れた場合は、
        // CPUを勝者として結果画面に残さず、ルーム全体を空へ戻す。
        if remaining_human_count == 0 {
            for (entity, player) in &players {
                commands.entity(entity).despawn();
                if expired_ids.contains(&player.id) {
                    println!("player {} reconnect grace expired", player.id);
                }
            }
            clear_arena(&mut commands);
            reset_empty_room(&mut state);
            state.tick += 1;
            println!("room reset because no human players remain");
            return;
        }

        let remaining_ids: Vec<u64> = players
            .iter()
            .filter(|(_, player)| {
                !expired_ids.contains(&player.id)
                    && (player.is_cpu || player.connection_id.is_some())
            })
            .map(|(_, player)| player.id)
            .collect();
        let match_was_active = state.phase == MatchPhase::Paused
            || matches!(state.phase, MatchPhase::Countdown | MatchPhase::Running);
        for (entity, player_id) in expired {
            commands.entity(entity).despawn();
            println!("player {player_id} reconnect grace expired");
        }
        if state
            .host_player_id
            .is_some_and(|id| expired_ids.contains(&id))
        {
            state.host_player_id = players
                .iter()
                .find(|(_, player)| {
                    !player.is_cpu
                        && !expired_ids.contains(&player.id)
                        && player.connection_id.is_some()
                })
                .map(|(_, player)| player.id);
        }
        if match_was_active && remaining_ids.len() < 2 {
            finish_match(&mut state, remaining_ids.first().copied(), &settings);
            clear_arena(&mut commands);
            println!("match ended because fewer than two players remain");
        } else if state.phase == MatchPhase::Paused {
            state.phase = state.resume_phase.take().unwrap_or(MatchPhase::Waiting);
        }
        state.tick += 1;
        return;
    }

    let active_player_count = players
        .iter()
        .filter(|(_, player)| player.is_cpu || player.connection_id.is_some())
        .count();
    let has_disconnected_player = players
        .iter()
        .any(|(_, player)| !player.is_cpu && player.connection_id.is_none());

    // 対戦中の切断はタイマーとゲーム計算を止める。復帰後は同じフェーズへ戻る。
    if has_disconnected_player
        && !matches!(
            state.phase,
            MatchPhase::Waiting | MatchPhase::MatchFinished | MatchPhase::Paused
        )
    {
        state.resume_phase = Some(state.phase);
        state.phase = MatchPhase::Paused;
        for (_, mut player) in &mut players {
            player.movement = Vec2::ZERO;
            player.shooting = false;
        }
    } else if !has_disconnected_player && state.phase == MatchPhase::Paused {
        state.phase = state.resume_phase.take().unwrap_or(MatchPhase::Countdown);
        println!("match resumed after reconnect");
    }

    match state.phase {
        MatchPhase::Waiting => {
            if state.start_requested && active_player_count >= 2 {
                start_new_match(&mut state, &mut players, &settings, &map);
                clear_arena(&mut commands);
            }
        }
        MatchPhase::Countdown => {
            state.phase_time_left = (state.phase_time_left - dt).max(0.0);
            if state.phase_time_left <= 0.0 {
                state.phase = MatchPhase::Running;
                state.phase_time_left = state.room_settings.match_seconds;
                println!("timed score match started");
            }
        }
        MatchPhase::Running => {
            // 練習場は時間で終わらせない。試し終わる時機は人によって違うし、
            // 途中で打ち切られると、数えている最中の結果が無駄になる。
            // 抜けるときはルームを出る。
            if state.room_settings.sandbox {
                state.tick += 1;
                return;
            }
            state.phase_time_left = (state.phase_time_left - dt).max(0.0);
            if state.phase_time_left <= 0.0 {
                let winner_id = unique_score_winner(
                    players.iter().map(|(_, player)| (player.id, player.score)),
                );
                finish_match(&mut state, winner_id, &settings);
                clear_arena(&mut commands);
            }
        }
        MatchPhase::MatchFinished => {
            state.phase_time_left = (state.phase_time_left - dt).max(0.0);
            if state.phase_time_left <= 0.0 {
                state.match_winner_id = None;
                for (_, mut player) in &mut players {
                    player.score = 0;
                    reset_player(&mut player, &settings.gameplay, &map);
                }
                state.phase = MatchPhase::Waiting;
                state.phase_time_left = 0.0;
                state.start_requested = false;
            }
        }
        MatchPhase::Paused => {}
    }

    state.tick += 1;
}

fn start_new_match(
    state: &mut MatchState,
    players: &mut Query<(Entity, &mut Player)>,
    settings: &GameSettings,
    map: &ArenaMap,
) {
    state.match_winner_id = None;
    state.resume_phase = None;
    state.item_spawn_left = 0.0;
    state.start_requested = false;
    state.phase = MatchPhase::Countdown;
    state.phase_time_left = settings.match_rules.countdown_seconds;
    for (_, mut player) in players.iter_mut() {
        player.score = 0;
        reset_player(&mut player, &settings.gameplay, map);
    }
    println!("new timed score match is ready");
}

pub(super) fn unique_score_winner(standings: impl Iterator<Item = (u64, i32)>) -> Option<u64> {
    let mut standings: Vec<_> = standings.collect();
    standings.sort_by_key(|standing| std::cmp::Reverse(standing.1));
    match standings.as_slice() {
        [(winner_id, winner_score), (_, second_score), ..] if winner_score > second_score => {
            Some(*winner_id)
        }
        [(winner_id, _)] => Some(*winner_id),
        _ => None,
    }
}

fn finish_match(state: &mut MatchState, winner_id: Option<u64>, settings: &GameSettings) {
    state.phase = MatchPhase::MatchFinished;
    state.phase_time_left = settings.match_rules.match_finished_seconds;
    state.match_winner_id = winner_id;
    state.resume_phase = None;
    println!("match finished; winner: {winner_id:?}");
}

/// 人間がいなくなったルームを、次の参加者がホストになれる空状態へ戻す。
pub(super) fn reset_empty_room(state: &mut MatchState) {
    state.phase = MatchPhase::Waiting;
    state.phase_time_left = 0.0;
    state.resume_phase = None;
    state.match_winner_id = None;
    state.item_spawn_left = 0.0;
    state.host_player_id = None;
    state.start_requested = false;
}
