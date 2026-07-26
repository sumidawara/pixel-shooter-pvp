//! GameCoreの試合進行、移動、射撃、当たり判定、リスポーンを処理するBevy System。

use bevy::prelude::*;
use pixel_shooter_protocol::{BULLET_RADIUS, ITEM_RADIUS, MatchPhase, PLAYER_RADIUS};

use crate::{
    arena::ArenaMap,
    model::{Bullet, MatchState, Player, ScoreItem},
    schedule::GameClock,
    settings::{GameSettings, GameplaySettings},
};

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
    mut commands: Commands,
) {
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
        despawn_all_bullets(&mut commands, &bullets);
        despawn_all_items(&mut commands, &items);
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
            despawn_all_bullets(&mut commands, &bullets);
            despawn_all_items(&mut commands, &items);
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
            despawn_all_bullets(&mut commands, &bullets);
            despawn_all_items(&mut commands, &items);
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
                despawn_all_bullets(&mut commands, &bullets);
                despawn_all_items(&mut commands, &items);
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
            state.phase_time_left = (state.phase_time_left - dt).max(0.0);
            if state.phase_time_left <= 0.0 {
                let winner_id = unique_score_winner(
                    players.iter().map(|(_, player)| (player.id, player.score)),
                );
                finish_match(&mut state, winner_id, &settings);
                despawn_all_bullets(&mut commands, &bullets);
                despawn_all_items(&mut commands, &items);
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

fn unique_score_winner(standings: impl Iterator<Item = (u64, i32)>) -> Option<u64> {
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
fn reset_empty_room(state: &mut MatchState) {
    state.phase = MatchPhase::Waiting;
    state.phase_time_left = 0.0;
    state.resume_phase = None;
    state.match_winner_id = None;
    state.item_spawn_left = 0.0;
    state.host_player_id = None;
    state.start_requested = false;
}

fn despawn_all_bullets(commands: &mut Commands, bullets: &Query<Entity, With<Bullet>>) {
    for entity in bullets {
        commands.entity(entity).despawn();
    }
}

fn despawn_all_items(commands: &mut Commands, items: &Query<Entity, With<ScoreItem>>) {
    for entity in items {
        commands.entity(entity).despawn();
    }
}

fn is_playing_phase(phase: MatchPhase) -> bool {
    phase == MatchPhase::Running
}

/// クライアントから受け取った移動入力でプレイヤーを動かすSystem。
pub(crate) fn move_players(
    clock: Res<GameClock>,
    settings: Res<GameSettings>,
    map: Res<ArenaMap>,
    state: Res<MatchState>,
    mut players: Query<&mut Player>,
) {
    if !is_playing_phase(state.phase) {
        return;
    }
    let dt = clock.delta_seconds();
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
        map.move_with_collision(&mut player.position, delta);
    }
}

/// CPUプレイヤーの入力をサーバー内で作る簡易AI。
pub(crate) fn update_cpu_players(
    state: Res<MatchState>,
    mut players: Query<&mut Player>,
    items: Query<&ScoreItem>,
) {
    if !is_playing_phase(state.phase) {
        return;
    }

    let targets: Vec<(u64, Vec2, bool)> = players
        .iter()
        .map(|player| (player.id, player.position, player.alive))
        .collect();
    let item_positions: Vec<Vec2> = items.iter().map(|item| item.position).collect();

    for mut cpu in &mut players {
        if !cpu.is_cpu || !cpu.alive {
            continue;
        }
        let Some((_, enemy_position, _)) = targets
            .iter()
            .filter(|(id, _, alive)| *id != cpu.id && *alive)
            .min_by(|left, right| {
                cpu.position
                    .distance_squared(left.1)
                    .total_cmp(&cpu.position.distance_squared(right.1))
            })
        else {
            continue;
        };
        let movement_target = item_positions
            .iter()
            .min_by(|left, right| {
                cpu.position
                    .distance_squared(**left)
                    .total_cmp(&cpu.position.distance_squared(**right))
            })
            .copied()
            .unwrap_or(*enemy_position);
        cpu.movement = (movement_target - cpu.position).normalize_or_zero();
        cpu.aim = (*enemy_position - cpu.position).normalize_or_zero();
        cpu.shooting = cpu.aim != Vec2::ZERO;
        if state.tick.is_multiple_of(180) {
            cpu.dash_requested = true;
        }
    }
}

/// 射撃入力とクールダウンを確認し、Bullet Entityを生成するSystem。
pub(crate) fn fire_bullets(
    mut commands: Commands,
    settings: Res<GameSettings>,
    map: Res<ArenaMap>,
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
        map.move_with_collision(
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
    clock: Res<GameClock>,
    settings: Res<GameSettings>,
    map: Res<ArenaMap>,
    state: Res<MatchState>,
    mut bullets: Query<(Entity, &mut Bullet)>,
    mut players: Query<&mut Player>,
) {
    if !is_playing_phase(state.phase) {
        return;
    }
    let dt = clock.delta_seconds();
    for (entity, mut bullet) in &mut bullets {
        // Rustの借用規則上、positionを変更しながらvelocityを読む式を分けている。
        let velocity = bullet.velocity;
        bullet.position += velocity * dt;
        bullet.life_left -= dt;
        if bullet.life_left <= 0.0
            || !map.bullet_in_bounds(bullet.position)
            || map.obstacle_at(bullet.position, 0.0)
        {
            // 寿命切れ、画面外、障害物への衝突のどれかなら弾を削除する。
            commands.entity(entity).despawn();
            continue;
        }

        let mut hit = false;
        let mut killed_player_id = None;
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
                    killed_player_id = Some(player.id);
                }
                break;
            }
        }
        if hit {
            // 1つの弾は1回だけダメージを与える。
            commands.entity(entity).despawn();
            if let Some(victim_id) = killed_player_id {
                // 撃破者へ加点し、死亡したプレイヤーからペナルティを引く。
                // 得点は負数も取り得るためi32で保持し、極端な設定でも飽和演算する。
                for mut player in &mut players {
                    if player.id == owner_id {
                        player.score = add_points(player.score, state.room_settings.kill_points);
                    } else if player.id == victim_id {
                        player.score =
                            subtract_points(player.score, state.room_settings.death_penalty);
                    }
                }
            }
        }
    }
}

/// 得点アイテムの生成と取得判定を処理するSystem。
pub(crate) fn update_score_items(
    mut commands: Commands,
    clock: Res<GameClock>,
    map: Res<ArenaMap>,
    mut state: ResMut<MatchState>,
    mut players: Query<&mut Player>,
    items: Query<(Entity, &ScoreItem)>,
) {
    if !is_playing_phase(state.phase) {
        return;
    }

    // 一定間隔で候補地点を巡回し、マップ上の個数が上限未満なら1個生成する。
    state.item_spawn_left = (state.item_spawn_left - clock.delta_seconds()).max(0.0);
    if state.item_spawn_left <= 0.0 {
        if items.iter().len() < state.room_settings.max_items as usize {
            let player_positions: Vec<_> = players
                .iter()
                .filter(|player| player.alive)
                .map(|player| player.position)
                .collect();
            let item_positions: Vec<_> = items.iter().map(|(_, item)| item.position).collect();
            if let Some((id, position)) = choose_score_item_spawn(
                &map,
                state.next_item_id,
                &player_positions,
                &item_positions,
            ) {
                state.next_item_id = id;
                commands.spawn(ScoreItem { id, position });
            }
        }
        state.item_spawn_left = state.room_settings.item_spawn_interval;
    }

    // 1つのアイテムを同じtickに2人が取得しないよう、アイテム単位で判定してbreakする。
    let pickup_distance = PLAYER_RADIUS + ITEM_RADIUS;
    for (entity, item) in &items {
        for mut player in &mut players {
            if player.alive
                && player.position.distance_squared(item.position)
                    <= pickup_distance * pickup_distance
            {
                player.score = add_points(player.score, state.room_settings.item_points);
                commands.entity(entity).despawn();
                break;
            }
        }
    }
}

fn choose_score_item_spawn(
    map: &ArenaMap,
    current_id: u64,
    player_positions: &[Vec2],
    item_positions: &[Vec2],
) -> Option<(u64, Vec2)> {
    (1..=map.item_spawn_count()).find_map(|offset| {
        let id = current_id.saturating_add(offset as u64);
        let position = map.item_spawn_position(id.saturating_sub(1) as usize);
        let away_from_players = player_positions
            .iter()
            .all(|other| other.distance_squared(position) > 48.0 * 48.0);
        let away_from_items = item_positions
            .iter()
            .all(|other| other.distance_squared(position) > ITEM_RADIUS * ITEM_RADIUS * 4.0);
        (away_from_players && away_from_items).then_some((id, position))
    })
}

fn add_points(score: i32, points: i32) -> i32 {
    score.saturating_add(points)
}

fn subtract_points(score: i32, penalty: i32) -> i32 {
    score.saturating_sub(penalty)
}

/// 死亡したプレイヤーの復活カウントを進めるSystem。
pub(crate) fn update_respawns(
    clock: Res<GameClock>,
    settings: Res<GameSettings>,
    map: Res<ArenaMap>,
    state: Res<MatchState>,
    mut players: Query<&mut Player>,
    bullets: Query<&Bullet>,
) {
    if !is_playing_phase(state.phase) {
        return;
    }
    let dt = clock.delta_seconds();
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
                map.choose_respawn_position(player.id, &positions, &bullet_positions, state.tick);
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
fn reset_player(player: &mut Player, gameplay: &GameplaySettings, map: &ArenaMap) {
    player.position = map.spawn_position(player.slot);
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
    fn highest_unique_score_wins() {
        assert_eq!(
            unique_score_winner([(1, 75), (2, 120)].into_iter()),
            Some(2)
        );
        assert_eq!(unique_score_winner([(1, -25), (2, -25)].into_iter()), None);
    }

    #[test]
    fn score_item_positions_do_not_overlap_obstacles() {
        let map = ArenaMap::default();
        for index in 0..map.item_spawn_count() {
            assert!(!map.obstacle_at(map.item_spawn_position(index), ITEM_RADIUS));
        }
    }

    #[test]
    fn score_item_spawn_avoids_occupied_candidate() {
        let map = ArenaMap::default();
        let occupied = map.item_spawn_position(0);
        let (_, position) =
            choose_score_item_spawn(&map, 0, &[occupied], &[]).expect("another candidate");
        assert_ne!(position, occupied);
    }

    #[test]
    fn score_events_include_kill_death_and_item_values() {
        let rules = crate::settings::MatchRules::default();
        assert_eq!(add_points(0, rules.kill_points), 100);
        assert_eq!(subtract_points(0, rules.death_penalty), -25);
        assert_eq!(add_points(-25, rules.item_points), -5);
    }

    #[test]
    fn empty_room_reset_removes_host_and_pending_match_state() {
        let mut state = MatchState {
            phase: MatchPhase::MatchFinished,
            phase_time_left: 4.0,
            resume_phase: Some(MatchPhase::Running),
            match_winner_id: Some(2),
            item_spawn_left: 1.0,
            host_player_id: Some(1),
            start_requested: true,
            ..default()
        };

        reset_empty_room(&mut state);

        assert_eq!(state.phase, MatchPhase::Waiting);
        assert_eq!(state.phase_time_left, 0.0);
        assert_eq!(state.resume_phase, None);
        assert_eq!(state.match_winner_id, None);
        assert_eq!(state.item_spawn_left, 0.0);
        assert_eq!(state.host_player_id, None);
        assert!(!state.start_requested);
    }
}
