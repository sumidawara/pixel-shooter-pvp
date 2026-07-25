//! 試合進行、移動、射撃、当たり判定、リスポーンを処理するBevy System。

use bevy::{prelude::*, time::Fixed};
use pixel_shooter_protocol::{BULLET_RADIUS, MatchPhase, PLAYER_RADIUS};

use crate::{
    arena::{
        bullet_in_bounds, choose_respawn_position, move_with_collision, obstacle_at, spawn_position,
    },
    config::{GameplaySettings, MatchRules, ServerSettings},
    model::{Bullet, MAX_PLAYERS, MatchState, Player},
};

/// 待機から3ラウンド先取までの状態遷移を管理するSystem。
///
/// `ResMut<MatchState>` はResourceを変更可能で借りる指定。
/// `Query<Entity, With<Bullet>>` はBulletを持つEntity番号だけを取得するフィルター。
pub(crate) fn update_match(
    time: Res<Time<Fixed>>,
    settings: Res<ServerSettings>,
    mut state: ResMut<MatchState>,
    mut players: Query<(Entity, &mut Player)>,
    bullets: Query<Entity, With<Bullet>>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();

    // 切断中のプレイヤーには猶予時間を与え、Entityと試合状態を保持する。
    let mut expired = Vec::new();
    for (entity, mut player) in &mut players {
        if player.connection_id.is_none() {
            player.reconnect_grace_left = (player.reconnect_grace_left - dt).max(0.0);
            if player.reconnect_grace_left <= 0.0 {
                expired.push((entity, player.id));
            }
        }
    }

    if !expired.is_empty() {
        let connected_winner = players
            .iter()
            .find(|(_, player)| player.connection_id.is_some())
            .map(|(_, player)| player.id);
        let match_was_active = state.phase == MatchPhase::Paused
            || matches!(
                state.phase,
                MatchPhase::Countdown
                    | MatchPhase::Running
                    | MatchPhase::Overtime
                    | MatchPhase::RoundEnd
            );
        for (entity, player_id) in expired {
            commands.entity(entity).despawn();
            println!("player {player_id} reconnect grace expired");
        }
        if match_was_active {
            state.phase = MatchPhase::MatchFinished;
            state.phase_time_left = settings.match_rules.match_finished_seconds;
            state.match_winner_id = connected_winner;
            state.round_winner_id = None;
            state.resume_phase = None;
            despawn_all_bullets(&mut commands, &bullets);
            println!("match ended by forfeit; winner: {connected_winner:?}");
        }
        state.tick += 1;
        return;
    }

    let connected_count = players
        .iter()
        .filter(|(_, player)| player.connection_id.is_some())
        .count();
    let has_disconnected_player = players
        .iter()
        .any(|(_, player)| player.connection_id.is_none());

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
            if connected_count == MAX_PLAYERS {
                start_new_match(&mut state, &mut players, &settings);
                despawn_all_bullets(&mut commands, &bullets);
            }
        }
        MatchPhase::Countdown => {
            state.phase_time_left = (state.phase_time_left - dt).max(0.0);
            if state.phase_time_left <= 0.0 {
                state.phase = MatchPhase::Running;
                state.phase_time_left = settings.match_rules.round_seconds;
                println!("round {} started", state.round_number);
            }
        }
        MatchPhase::Running => {
            state.phase_time_left = (state.phase_time_left - dt).max(0.0);
            if state.phase_time_left <= 0.0 {
                let mut standings: Vec<(u64, u32)> = players
                    .iter()
                    .map(|(_, player)| (player.id, player.score))
                    .collect();
                standings.sort_by_key(|(_, score)| *score);
                if standings.len() == 2 && standings[0].1 == standings[1].1 {
                    state.phase = MatchPhase::Overtime;
                    state.phase_time_left = settings.match_rules.overtime_seconds;
                    println!("round {} entered overtime", state.round_number);
                } else if let Some((winner_id, _)) = standings.last() {
                    award_round(&mut state, &mut players, *winner_id, &settings);
                    despawn_all_bullets(&mut commands, &bullets);
                }
            }
        }
        MatchPhase::Overtime => {
            state.phase_time_left = (state.phase_time_left - dt).max(0.0);
            if state.phase_time_left <= 0.0 {
                let mut standings: Vec<(u64, i32)> = players
                    .iter()
                    .map(|(_, player)| (player.id, player.hp))
                    .collect();
                standings.sort_by_key(|(_, hp)| *hp);
                if standings.len() == 2 && standings[0].1 == standings[1].1 {
                    // HPまで同じなら10秒ずつサドンデスを継続する。
                    state.phase_time_left = 10.0;
                } else if let Some((winner_id, _)) = standings.last() {
                    award_round(&mut state, &mut players, *winner_id, &settings);
                    despawn_all_bullets(&mut commands, &bullets);
                }
            }
        }
        MatchPhase::RoundEnd => {
            state.phase_time_left = (state.phase_time_left - dt).max(0.0);
            if state.phase_time_left <= 0.0 {
                state.round_number += 1;
                prepare_round(&mut players, &settings.gameplay);
                state.round_winner_id = None;
                state.phase = MatchPhase::Countdown;
                state.phase_time_left = settings.match_rules.countdown_seconds;
                despawn_all_bullets(&mut commands, &bullets);
            }
        }
        MatchPhase::MatchFinished => {
            state.phase_time_left = (state.phase_time_left - dt).max(0.0);
            if state.phase_time_left <= 0.0 {
                state.match_winner_id = None;
                state.round_winner_id = None;
                state.round_number = 0;
                for (_, mut player) in &mut players {
                    player.round_wins = 0;
                    player.score = 0;
                    reset_player(&mut player, &settings.gameplay);
                }
                if connected_count == MAX_PLAYERS {
                    start_new_match(&mut state, &mut players, &settings);
                } else {
                    state.phase = MatchPhase::Waiting;
                    state.phase_time_left = 0.0;
                }
            }
        }
        MatchPhase::Paused => {}
    }

    state.tick += 1;
}

fn start_new_match(
    state: &mut MatchState,
    players: &mut Query<(Entity, &mut Player)>,
    settings: &ServerSettings,
) {
    state.round_number = 1;
    state.round_winner_id = None;
    state.match_winner_id = None;
    state.resume_phase = None;
    state.phase = MatchPhase::Countdown;
    state.phase_time_left = settings.match_rules.countdown_seconds;
    for (_, mut player) in players.iter_mut() {
        player.round_wins = 0;
        player.score = 0;
        reset_player(&mut player, &settings.gameplay);
    }
    println!("new best-of-five match started");
}

fn prepare_round(players: &mut Query<(Entity, &mut Player)>, gameplay: &GameplaySettings) {
    for (_, mut player) in players.iter_mut() {
        player.score = 0;
        reset_player(&mut player, gameplay);
    }
}

fn award_round(
    state: &mut MatchState,
    players: &mut Query<(Entity, &mut Player)>,
    winner_id: u64,
    settings: &ServerSettings,
) {
    let mut match_won = false;
    for (_, mut player) in players.iter_mut() {
        if player.id == winner_id {
            player.round_wins += 1;
            match_won = player.round_wins >= settings.match_rules.rounds_to_win;
            break;
        }
    }
    set_round_result(state, winner_id, match_won, &settings.match_rules);
}

fn set_round_result(state: &mut MatchState, winner_id: u64, match_won: bool, rules: &MatchRules) {
    state.round_winner_id = Some(winner_id);
    if match_won {
        state.phase = MatchPhase::MatchFinished;
        state.phase_time_left = rules.match_finished_seconds;
        state.match_winner_id = Some(winner_id);
        println!("player {winner_id} won the match");
    } else {
        state.phase = MatchPhase::RoundEnd;
        state.phase_time_left = rules.round_interval_seconds;
        println!("player {winner_id} won round {}", state.round_number);
    }
}

fn despawn_all_bullets(commands: &mut Commands, bullets: &Query<Entity, With<Bullet>>) {
    for entity in bullets {
        commands.entity(entity).despawn();
    }
}

fn is_playing_phase(phase: MatchPhase) -> bool {
    matches!(phase, MatchPhase::Running | MatchPhase::Overtime)
}

/// クライアントから受け取った移動入力でプレイヤーを動かすSystem。
pub(crate) fn move_players(
    time: Res<Time<Fixed>>,
    settings: Res<ServerSettings>,
    state: Res<MatchState>,
    mut players: Query<&mut Player>,
) {
    if !is_playing_phase(state.phase) {
        return;
    }
    let dt = time.delta_secs();
    for mut player in &mut players {
        if !player.alive {
            player.reload_requested = false;
            player.dash_requested = false;
            continue;
        }
        player.shot_cooldown = (player.shot_cooldown - dt).max(0.0);
        player.invulnerable_left = (player.invulnerable_left - dt).max(0.0);
        player.dash_cooldown_left = (player.dash_cooldown_left - dt).max(0.0);

        // Rキーが押され、まだ弾が残っていない場合だけ手動リロードを始める。
        if player.reload_requested
            && player.reload_left <= 0.0
            && player.ammo < settings.gameplay.max_ammo
        {
            player.reload_left = settings.gameplay.reload_seconds;
        }
        player.reload_requested = false;

        if player.reload_left > 0.0 {
            player.reload_left = (player.reload_left - dt).max(0.0);
            if player.reload_left <= 0.0 {
                player.ammo = settings.gameplay.max_ammo;
            }
        }

        // Spaceが押された瞬間に、現在の移動入力方向へダッシュを開始する。
        if player.dash_requested
            && player.dash_cooldown_left <= 0.0
            && player.movement.length_squared() > 0.001
        {
            player.dash_direction = player.movement.normalize();
            player.dash_time_left = settings.gameplay.dash_duration;
            player.dash_cooldown_left = settings.gameplay.dash_cooldown;
        }
        player.dash_requested = false;

        // ダッシュ中は通常入力ではなく、開始時に保存した方向へ高速移動する。
        let (direction, speed) = if player.dash_time_left > 0.0 {
            player.dash_time_left = (player.dash_time_left - dt).max(0.0);
            (player.dash_direction, settings.gameplay.dash_speed)
        } else {
            (player.movement, settings.gameplay.move_speed)
        };

        // 速度(px/秒) × 経過秒で、このtickに進む距離を求める。
        let delta = direction * speed * dt;

        // X軸とY軸を別々に判定する。
        // まとめて移動すると、片方の軸が壁に当たっただけで両方向とも止まってしまう。
        move_with_collision(&mut player.position, delta);
    }
}

/// 射撃入力とクールダウンを確認し、Bullet Entityを生成するSystem。
pub(crate) fn fire_bullets(
    mut commands: Commands,
    settings: Res<ServerSettings>,
    mut state: ResMut<MatchState>,
    mut players: Query<&mut Player>,
) {
    if !is_playing_phase(state.phase) {
        return;
    }
    for mut player in &mut players {
        if !player.alive
            || !player.shooting
            || player.shot_cooldown > 0.0
            || player.reload_left > 0.0
            || player.dash_time_left > 0.0
        {
            continue;
        }
        if player.ammo == 0 {
            player.reload_left = settings.gameplay.reload_seconds;
            continue;
        }

        player.shot_cooldown = settings.gameplay.shot_interval;
        player.ammo -= 1;
        state.next_bullet_id += 1;
        let aim = player.aim;
        // プレイヤー中心に弾を置くと自分と重なるため、照準方向へ少し前に出す。
        commands.spawn(Bullet {
            id: state.next_bullet_id,
            owner_id: player.id,
            position: player.position + aim * (PLAYER_RADIUS + 6.0),
            velocity: aim * settings.gameplay.bullet_speed,
            life_left: 2.0,
        });

        // 射撃方向と反対へ少し押し戻す。サーバーで計算するので全員に同じ結果になる。
        move_with_collision(
            &mut player.position,
            -aim * settings.gameplay.recoil_distance,
        );

        // 最後の1発を撃った直後から自動リロードを開始する。
        if player.ammo == 0 {
            player.reload_left = settings.gameplay.reload_seconds;
        }
    }
}

/// 弾の移動、壁との衝突、プレイヤーへのダメージを処理するSystem。
pub(crate) fn move_and_hit_bullets(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    settings: Res<ServerSettings>,
    mut state: ResMut<MatchState>,
    mut bullets: Query<(Entity, &mut Bullet)>,
    mut players: Query<&mut Player>,
) {
    if !is_playing_phase(state.phase) {
        return;
    }
    let dt = time.delta_secs();
    for (entity, mut bullet) in &mut bullets {
        // Rustの借用規則上、positionを変更しながらvelocityを読む式を分けている。
        let velocity = bullet.velocity;
        bullet.position += velocity * dt;
        bullet.life_left -= dt;
        if bullet.life_left <= 0.0
            || !bullet_in_bounds(bullet.position)
            || obstacle_at(bullet.position, 0.0)
        {
            // 寿命切れ、画面外、障害物への衝突のどれかなら弾を削除する。
            commands.entity(entity).despawn();
            continue;
        }

        let mut hit = false;
        let mut killed = false;
        let owner_id = bullet.owner_id;
        for mut player in &mut players {
            if !player.alive || player.id == owner_id || player.invulnerable_left > 0.0 {
                continue;
            }
            // 円同士の当たり判定。sqrtを避けるため距離も半径も二乗して比較する。
            let hit_distance = PLAYER_RADIUS + BULLET_RADIUS;
            if player.position.distance_squared(bullet.position) <= hit_distance * hit_distance {
                player.hp -= 1;
                player.invulnerable_left = settings.gameplay.hit_invulnerable_seconds;
                hit = true;
                if player.hp <= 0 {
                    player.alive = false;
                    player.respawn_left = settings.gameplay.respawn_seconds;
                    player.shooting = false;
                    killed = true;
                }
                break;
            }
        }
        if hit {
            // 1つの弾は1回だけダメージを与える。
            commands.entity(entity).despawn();
            if killed {
                // 撃破した弾のowner_idと一致するプレイヤーへ1点追加する。
                let overtime_kill = state.phase == MatchPhase::Overtime;
                let mut match_won = false;
                for mut player in &mut players {
                    if player.id == owner_id {
                        player.score += 1;
                        if overtime_kill {
                            player.round_wins += 1;
                            match_won = player.round_wins >= settings.match_rules.rounds_to_win;
                        }
                        break;
                    }
                }
                if overtime_kill {
                    set_round_result(&mut state, owner_id, match_won, &settings.match_rules);
                }
            }
        }
    }
}

/// 死亡したプレイヤーの復活カウントを進めるSystem。
pub(crate) fn update_respawns(
    time: Res<Time<Fixed>>,
    settings: Res<ServerSettings>,
    state: Res<MatchState>,
    mut players: Query<&mut Player>,
    bullets: Query<&Bullet>,
) {
    if !is_playing_phase(state.phase) {
        return;
    }
    let dt = time.delta_secs();
    // 下のループではPlayerを変更可能で借りるため、先に全員の位置だけコピーしておく。
    let positions: Vec<(u64, Vec2)> = players.iter().map(|p| (p.id, p.position)).collect();
    let bullet_positions: Vec<Vec2> = bullets.iter().map(|bullet| bullet.position).collect();
    for mut player in &mut players {
        if player.alive {
            continue;
        }
        player.respawn_left = (player.respawn_left - dt).max(0.0);
        if player.respawn_left <= 0.0 {
            // 複数候補から相手と弾に最も近づきにくい地点を選ぶ。
            player.position =
                choose_respawn_position(player.id, &positions, &bullet_positions, state.tick);
            player.hp = settings.gameplay.max_hp;
            player.alive = true;
            player.shot_cooldown = 0.3;
            player.ammo = settings.gameplay.max_ammo;
            player.reload_left = 0.0;
            player.invulnerable_left = settings.gameplay.respawn_invulnerable_seconds;
            player.dash_time_left = 0.0;
        }
    }
}

/// 新しい試合の開始時にプレイヤー状態を初期化する。
fn reset_player(player: &mut Player, gameplay: &GameplaySettings) {
    player.position = spawn_position(player.slot);
    player.hp = gameplay.max_hp;
    player.alive = true;
    player.respawn_left = 0.0;
    player.shot_cooldown = 0.0;
    player.ammo = gameplay.max_ammo;
    player.reload_left = 0.0;
    player.reload_requested = false;
    player.invulnerable_left = gameplay.respawn_invulnerable_seconds;
    player.dash_cooldown_left = 0.0;
    player.dash_time_left = 0.0;
    player.dash_requested = false;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_result_moves_to_interval_or_match_result() {
        let mut state = MatchState::default();
        let rules = MatchRules::default();
        set_round_result(&mut state, 7, false, &rules);
        assert_eq!(state.phase, MatchPhase::RoundEnd);
        assert_eq!(state.round_winner_id, Some(7));

        set_round_result(&mut state, 7, true, &rules);
        assert_eq!(state.phase, MatchPhase::MatchFinished);
        assert_eq!(state.match_winner_id, Some(7));
    }
}
