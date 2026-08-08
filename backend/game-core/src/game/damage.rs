//! すべてのダメージ源が共有する被弾・撃破処理。
//!
//! 弾、ラロキンポッポス、今後追加される攻撃手段は、被弾の適用をここへ集約する。
//! シールドの消費、無敵時間の付与、死亡判定、スコアの増減を1箇所に置くことで、
//! ダメージ源ごとに条件がずれる事故を防ぐ。

use bevy::prelude::*;

use crate::{
    model::{MatchState, Player},
    settings::GameplaySettings,
};

use super::score::{add_points, subtract_points};

/// 1回の被弾を適用した結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct HitOutcome {
    /// この被弾でプレイヤーが倒れたか。
    pub killed: bool,
}

/// ダメージ判定の対象になるか。自分の攻撃と、無敵中・死亡中には当たらない。
pub(super) fn can_be_hit(player: &Player, attacker_id: u64) -> bool {
    player.alive && player.id != attacker_id && player.invulnerable_left <= 0.0
}

/// 被弾を1回適用する。ダメージ源はこの関数だけを呼ぶ。
pub(super) fn apply_damage(
    player: &mut Player,
    damage: i32,
    gameplay: &GameplaySettings,
) -> HitOutcome {
    // シールドがある間はHPへ通さず、シールド側だけを削る。
    if player.shield_hp > 0 {
        player.shield_hp = (player.shield_hp - damage).max(0);
    } else {
        player.hp -= damage;
    }

    // シールドが吸収した場合も無敵時間を与える。ここを分岐させると、
    // 同時に複数の攻撃体が触れたときシールドが1tickで消し飛ぶ。
    player.invulnerable_left = gameplay.hit_invulnerable_seconds;

    let killed = player.hp <= 0;
    if killed {
        player.alive = false;
        player.respawn_left = gameplay.respawn_seconds;
        player.shooting = false;
    }
    HitOutcome { killed }
}

/// 撃破者へ加点し、倒されたプレイヤーからペナルティを引く。
///
/// 得点は負数も取り得るためi32で保持し、極端なルール設定でも飽和演算する。
pub(super) fn award_kill(
    players: &mut Query<&mut Player>,
    killer_id: u64,
    victim_id: u64,
    state: &MatchState,
) {
    // 練習場の的を倒しても得点は動かさない。勝敗を決める場ではないのに
    // 数字が増えると、稼ぐための場所に見えてしまう。
    if players
        .iter()
        .any(|player| player.id == victim_id && player.is_dummy)
    {
        return;
    }
    for mut player in players.iter_mut() {
        if player.id == killer_id {
            player.score = add_points(player.score, state.room_settings.kill_points);
        } else if player.id == victim_id {
            player.score = subtract_points(player.score, state.room_settings.death_penalty);
        }
    }
}

#[cfg(test)]
mod tests {
    use pixel_shooter_protocol::MatchPhase;

    use super::*;
    use crate::{
        arena::ArenaMap,
        game::test_support::{test_app, test_player},
        model::{Bullet, LarokinPoppos},
        schedule::advance_one_tick,
    };

    /// 比較対象にする、被弾後のプレイヤー状態。
    #[derive(Debug, PartialEq)]
    struct HitState {
        hp: i32,
        shield_hp: i32,
        alive: bool,
        invulnerable: bool,
        respawn_left: f32,
        score: i32,
    }

    fn hit_state(player: &Player) -> HitState {
        HitState {
            hp: player.hp,
            shield_hp: player.shield_hp,
            alive: player.alive,
            invulnerable: player.invulnerable_left > 0.0,
            respawn_left: player.respawn_left,
            score: player.score,
        }
    }

    /// 攻撃側と被弾側を1体ずつ置き、1tick進めた後の被弾側の状態を返す。
    ///
    /// `spawn_attack`にはダメージ源のEntityを生成させる。被弾側は必ず床タイルの
    /// スポーン地点へ置く。壁の上に置くと弾は当たり判定の前に消えてしまい、
    /// ダメージ源どうしを比較できない。
    fn hit_once(
        shield_hp: i32,
        target_hp: i32,
        spawn_attack: impl FnOnce(&mut App, Vec2),
    ) -> HitState {
        let mut app = test_app(MatchPhase::Running, 60.0);
        let map = app.world().resource::<ArenaMap>().clone();

        let mut attacker = test_player(1, Some(101));
        attacker.position = map.spawn_position(0);
        app.world_mut().spawn(attacker);

        let mut target = test_player(2, Some(102));
        target.position = map.spawn_position(1);
        target.hp = target_hp;
        target.shield_hp = shield_hp;
        let target_position = target.position;
        let target_entity = app.world_mut().spawn(target).id();

        spawn_attack(&mut app, target_position);
        advance_one_tick(app.world_mut());

        hit_state(app.world().get::<Player>(target_entity).expect("target"))
    }

    fn spawn_bullet(app: &mut App, position: Vec2) {
        app.world_mut().spawn(Bullet {
            id: 1,
            owner_id: 1,
            position,
            velocity: Vec2::ZERO,
            life_left: 1.0,
            damage: 1,
        });
    }

    fn spawn_larokin(app: &mut App, position: Vec2) {
        app.world_mut().spawn(LarokinPoppos {
            id: 1,
            owner_id: 1,
            position,
            velocity: Vec2::ZERO,
            telegraph_left: 0.0,
            life_left: 1.0,
        });
    }

    #[test]
    fn bullet_and_larokin_apply_the_same_hit_to_an_unshielded_player() {
        assert_eq!(
            hit_once(0, 5, spawn_bullet),
            hit_once(0, 5, spawn_larokin),
            "ダメージ源が違っても、無防備なプレイヤーへの被弾結果は同じでなければならない"
        );
    }

    #[test]
    fn bullet_and_larokin_grant_invulnerability_when_a_shield_absorbs() {
        let by_bullet = hit_once(2, 5, spawn_bullet);
        let by_larokin = hit_once(2, 5, spawn_larokin);

        assert_eq!(
            by_bullet, by_larokin,
            "シールドが吸収したときの扱いもダメージ源によらず同じでなければならない"
        );
        assert_eq!(by_bullet.hp, 5, "シールドがある間はHPへ通さない");
        assert_eq!(by_bullet.shield_hp, 1);
        assert!(
            by_bullet.invulnerable,
            "シールドが吸収した場合も無敵時間を与える"
        );
    }

    #[test]
    fn bullet_and_larokin_award_the_same_kill_and_death_score() {
        let by_bullet = hit_once(0, 1, spawn_bullet);
        let by_larokin = hit_once(0, 1, spawn_larokin);

        assert_eq!(by_bullet, by_larokin);
        assert!(!by_bullet.alive);
        assert_eq!(
            by_bullet.score, -25,
            "倒されたプレイヤーには死亡ペナルティが入る"
        );
    }

    /// 練習場の的を倒しても得点は動かない。
    ///
    /// 勝敗を決める場ではないのに数字が増えると、稼ぐための場所に見えてしまう。
    /// 的の側もペナルティで沈み続け、順位表が読めなくなる。
    #[test]
    fn downing_a_sandbox_dummy_leaves_every_score_untouched() {
        let mut app = test_app(MatchPhase::Running, 60.0);
        app.world_mut()
            .resource_mut::<MatchState>()
            .room_settings
            .sandbox = true;
        let map = app.world().resource::<ArenaMap>().clone();

        let mut attacker = test_player(1, Some(101));
        attacker.position = map.spawn_position(0);
        let attacker_entity = app.world_mut().spawn(attacker).id();

        let mut target = test_player(2, None);
        target.is_cpu = true;
        target.is_dummy = true;
        target.position = map.spawn_position(1);
        target.hp = 1;
        let target_position = target.position;
        let target_entity = app.world_mut().spawn(target).id();

        spawn_bullet(&mut app, target_position);
        advance_one_tick(app.world_mut());

        let target = app.world().get::<Player>(target_entity).expect("dummy");
        assert!(!target.alive, "検査の前提が崩れている: 的が倒れていない");
        assert_eq!(target.score, 0, "的に死亡ペナルティが入っている");
        assert_eq!(
            app.world()
                .get::<Player>(attacker_entity)
                .expect("attacker")
                .score,
            0,
            "的を倒して得点が入っている"
        );
    }

    #[test]
    fn shield_survives_a_burst_of_attackers_in_the_same_tick() {
        let mut app = test_app(MatchPhase::Running, 60.0);
        let map = app.world().resource::<ArenaMap>().clone();
        app.world_mut().spawn(test_player(1, Some(101)));

        let mut target = test_player(2, Some(102));
        target.position = map.spawn_position(1);
        target.shield_hp = 2;
        let target_position = target.position;
        let target_entity = app.world_mut().spawn(target).id();

        // ラロキンポッポスは1回の使用で10体が同じ地点へ収束する。
        for id in 0..10 {
            app.world_mut().spawn(LarokinPoppos {
                id,
                owner_id: 1,
                position: target_position,
                velocity: Vec2::ZERO,
                telegraph_left: 0.0,
                life_left: 1.0,
            });
        }

        advance_one_tick(app.world_mut());

        let target = app.world().get::<Player>(target_entity).expect("target");
        assert_eq!(
            target.shield_hp, 1,
            "無敵時間があるため、同じtickに消費されるシールドは1つだけ"
        );
        assert_eq!(target.hp, 5);
        assert!(target.alive);
    }
}
