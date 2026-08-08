//! サンドボックス（練習場）の規則。
//!
//! 対戦では、アイテムの効果を確かめようとしても相手が撃ち返してくるし、
//! 出てくるアイテムも選べない。ここでは的と道具だけを揃えて、
//! 「この武器は何発で倒せるか」「このアイテムは何が起きるか」を落ち着いて試せる場を作る。
//!
//! 対戦と違うのは次の4点で、それ以外はすべて通常の試合と同じ計算を通す。
//! 練習場だけ挙動が違うと、確かめた結果が本番で当てにならない。
//!
//! - 空きスロットに、動かず撃ち返さないダミーが入る
//! - 全種類のアイテムが常に置かれ、取られてもすぐ戻る
//! - 試合が時間で終わらない
//! - ダミーを倒しても得点は動かない（勝敗を決める場ではない）

use bevy::prelude::*;
use pixel_shooter_protocol::{ITEM_RADIUS, ItemKind};

use crate::{
    arena::{ArenaMap, GridPosition},
    model::{HeldItem, MatchState, Player, ScoreItem},
};

use super::is_playing_phase;

/// 練習場に置くアイテムの種類。ここに並べた順に置き場所が決まる。
pub(super) const SANDBOX_ITEM_KINDS: [ItemKind; 6] = [
    ItemKind::EnergyCell,
    ItemKind::Dash,
    ItemKind::Shield,
    ItemKind::Berserk,
    ItemKind::LarokinPoppos,
    ItemKind::Ghost,
];

/// 取られたアイテムが戻ってくるまでの時間。
///
/// 短いのは、続けて何度も試せるようにするため。対戦の`item_spawn_interval`は
/// 奪い合いを作るための値なので、練習場では使わない。
pub(super) const ITEM_RESTOCK_SECONDS: f32 = 1.0;

/// ダミーが倒れてから起き上がるまでの時間。
///
/// 通常の`respawn_seconds`より短くする。何発で倒せるかを繰り返し数えるとき、
/// 待ち時間が長いと確かめること自体が面倒になる。
const DUMMY_RESPAWN_SECONDS: f32 = 1.0;

/// 置き場所どうしを離す最小距離。近すぎると1回歩いただけで複数拾ってしまう。
const ITEM_SPACING: f32 = 64.0;

/// 練習場のダミーを、試せる状態に保つSystem。
///
/// ダミーは動かない・撃たないというだけでなく、Ghostの的にもなる必要がある。
/// Ghostは「アイテムを持っている他人」からしか奪えないため、何も持たせないと
/// 全種類のうちGhostだけ試せない。
pub(crate) fn update_sandbox_dummies(state: Res<MatchState>, mut players: Query<&mut Player>) {
    if !state.room_settings.sandbox || !is_playing_phase(state.phase) {
        return;
    }
    for mut player in &mut players {
        if !player.is_dummy {
            continue;
        }
        // 撃ち返さない。CPUのSystemは触らないが、入力が残っていると動いてしまう。
        player.movement = Vec2::ZERO;
        player.shooting = false;
        player.dash_requested = false;
        player.use_item_requested = false;
        player.reload_requested = false;

        if !player.alive {
            player.respawn_left = player.respawn_left.min(DUMMY_RESPAWN_SECONDS);
            continue;
        }
        // 奪われたら補充する。奪えるものが無い状態が続くとGhostを試せない。
        if player.held_item.is_none() {
            player.held_item = Some(HeldItem {
                kind: dummy_item_for_slot(player.slot),
                charges: 1,
            });
        }
    }
}

/// 場に無い種類のアイテムを置き直す。
///
/// 対戦の抽選（`choose_score_item_spawn`）は使わない。あちらは1個ずつ順番に
/// 出す仕組みで、狙った種類を試せるようにはできていない。
pub(super) fn restock_items(
    commands: &mut Commands,
    map: &ArenaMap,
    state: &mut MatchState,
    present_kinds: &[ItemKind],
) {
    let positions = item_positions(map, SANDBOX_ITEM_KINDS.len());
    for (index, kind) in SANDBOX_ITEM_KINDS.iter().enumerate() {
        if present_kinds.contains(kind) {
            continue;
        }
        // 置き場所が足りないマップでは、入りきらない種類は出ない。
        let Some(position) = positions.get(index) else {
            continue;
        };
        state.next_item_id += 1;
        commands.spawn(ScoreItem {
            id: state.next_item_id,
            position: *position,
            kind: *kind,
        });
    }
}

/// ダミーに持たせるアイテム。スロットごとに固定する。
///
/// 毎回変えると、Ghostで何を奪ったのかが分からなくなる。
fn dummy_item_for_slot(slot: usize) -> ItemKind {
    const ROTATION: [ItemKind; 3] = [ItemKind::Shield, ItemKind::Berserk, ItemKind::Dash];
    ROTATION[slot % ROTATION.len()]
}

/// 種類ごとの置き場所を決める。同じマップなら毎回同じ場所になる。
///
/// 対戦用の`item_spawn_points`はマップによって3〜6個しかなく、それだけでは
/// 全種類を並べられない。足りない分は床タイルから決定的に拾う。
/// 毎回同じ場所に置くのは、どこに何があるかを覚えて試せるようにするため。
fn item_positions(map: &ArenaMap, count: usize) -> Vec<Vec2> {
    let mut positions: Vec<Vec2> = Vec::new();
    // マップが用意した場所を先に使う。障害物や通路を考えて置かれている。
    for index in 0..map.item_spawn_count().min(count) {
        positions.push(map.item_spawn_position(index));
    }
    if positions.len() >= count {
        return positions;
    }

    // 出現位置に置くと、試合開始と同時に勝手に拾ってしまう。
    let spawn_positions: Vec<Vec2> = (0..crate::model::MAX_PLAYERS)
        .map(|slot| map.spawn_position(slot))
        .collect();
    for y in 0..map.height() {
        for x in 0..map.width() {
            if positions.len() >= count {
                return positions;
            }
            let center = map.tile_center(GridPosition { x, y });
            if map.obstacle_at(center, ITEM_RADIUS) {
                continue;
            }
            let too_close = positions
                .iter()
                .chain(spawn_positions.iter())
                .any(|other| other.distance_squared(center) < ITEM_SPACING * ITEM_SPACING);
            if too_close {
                continue;
            }
            positions.push(center);
        }
    }
    positions
}

#[cfg(test)]
mod tests {
    use super::*;
    use pixel_shooter_protocol::MatchPhase;

    use crate::{
        game::test_support::{test_app, test_player},
        schedule::advance_one_tick,
    };

    fn sandbox_app() -> App {
        let mut app = test_app(MatchPhase::Running, 60.0);
        let mut state = app.world_mut().resource_mut::<MatchState>();
        state.room_settings.sandbox = true;
        // 試合開始時と同じ状態にする。start_new_match はここを0にするので、
        // アイテムは最初のtickで並ぶ。test_app の既定値は他の試験を
        // 邪魔しないよう出現を止めてあり、そのままでは何も置かれない。
        state.item_spawn_left = 0.0;
        app
    }

    fn dummy(id: u64, slot: usize) -> Player {
        let mut player = test_player(id, None);
        player.is_cpu = true;
        player.is_dummy = true;
        player.slot = slot;
        player
    }

    /// 置き場所は種類ごとに離れていること。
    ///
    /// 重なっていると、1歩動いただけで全部拾ってしまい、1つずつ試せない。
    #[test]
    fn every_item_kind_gets_its_own_spot() {
        for map_name in [
            "classic_arena",
            "crossroads",
            "four_fortresses",
            "open_range",
        ] {
            let map = ArenaMap::load(format!("../maps/{map_name}.json"))
                .unwrap_or_else(|error| panic!("{map_name}: {error}"));
            let positions = item_positions(&map, SANDBOX_ITEM_KINDS.len());

            assert_eq!(
                positions.len(),
                SANDBOX_ITEM_KINDS.len(),
                "{map_name}: 全種類ぶんの置き場所が取れていない"
            );
            for (index, position) in positions.iter().enumerate() {
                assert!(
                    !map.obstacle_at(*position, ITEM_RADIUS),
                    "{map_name}: {index}番目が壁に埋まっている"
                );
                for other in &positions[index + 1..] {
                    assert!(
                        position.distance(*other) >= 1.0,
                        "{map_name}: 置き場所が重なっている"
                    );
                }
            }
        }
    }

    /// 同じマップなら毎回同じ場所。どこに何があるかを覚えて試せるように。
    #[test]
    fn the_layout_does_not_move_between_runs() {
        let map = ArenaMap::default();
        assert_eq!(
            item_positions(&map, SANDBOX_ITEM_KINDS.len()),
            item_positions(&map, SANDBOX_ITEM_KINDS.len())
        );
    }

    fn field_kinds(app: &mut App) -> Vec<ItemKind> {
        let world = app.world_mut();
        let mut query = world.query::<&ScoreItem>();
        query.iter(world).map(|item| item.kind).collect()
    }

    /// 練習場では最初から全種類が置いてある。
    ///
    /// 対戦の抽選任せだと、試したい種類が出るまで待つことになる。
    #[test]
    fn every_kind_is_on_the_field_from_the_first_tick() {
        let mut app = sandbox_app();

        advance_one_tick(app.world_mut());

        let kinds = field_kinds(&mut app);
        for kind in SANDBOX_ITEM_KINDS {
            assert!(
                kinds.contains(&kind),
                "{kind:?} が置かれていない: {kinds:?}"
            );
        }
    }

    /// 取られた種類はすぐ戻る。続けて何度も試せるように。
    #[test]
    fn a_taken_kind_comes_back_quickly() {
        let mut app = sandbox_app();
        advance_one_tick(app.world_mut());

        // 1つ拾われた状態を作る。
        let taken = ItemKind::Shield;
        let entity = {
            let world = app.world_mut();
            let mut query = world.query::<(Entity, &ScoreItem)>();
            query
                .iter(world)
                .find(|(_, item)| item.kind == taken)
                .map(|(entity, _)| entity)
                .expect("shield on the field")
        };
        app.world_mut().despawn(entity);
        assert!(
            !field_kinds(&mut app).contains(&taken),
            "検査の前提が崩れている"
        );

        // 補充間隔ぶんだけ進める。対戦の既定間隔(5秒)では戻ってこない長さ。
        for _ in 0..(ITEM_RESTOCK_SECONDS * 60.0).ceil() as i32 + 2 {
            advance_one_tick(app.world_mut());
        }

        assert!(
            field_kinds(&mut app).contains(&taken),
            "取られたアイテムが戻ってこない"
        );
    }

    /// 練習場は時間で終わらない。数えている最中に打ち切られない。
    #[test]
    fn the_sandbox_does_not_end_on_its_own() {
        let mut app = sandbox_app();
        {
            let mut state = app.world_mut().resource_mut::<MatchState>();
            // 対戦ならこの残り時間で終了する。
            state.phase_time_left = 0.01;
        }

        for _ in 0..30 {
            advance_one_tick(app.world_mut());
        }

        assert_eq!(
            app.world().resource::<MatchState>().phase,
            MatchPhase::Running,
            "練習中に試合が終わっている"
        );
    }

    /// ダミーは持ち物を補充される。Ghostは持っている相手からしか奪えない。
    #[test]
    fn dummies_always_carry_something_to_steal() {
        let mut app = sandbox_app();
        let entity = app.world_mut().spawn(dummy(2, 1)).id();

        advance_one_tick(app.world_mut());

        assert!(
            app.world()
                .get::<Player>(entity)
                .expect("dummy")
                .held_item
                .is_some(),
            "何も持っていないとGhostだけ試せない"
        );
    }

    /// ダミーは撃ち返さず、その場から動かず、こちらを向きもしない。
    ///
    /// 狙いまで見るのは、CPUのAIが的を掴んでいないことを確かめるため。
    /// 動きと射撃だけを止めても、AIに動かされていれば銃口はこちらを追ってくる。
    /// 撃ってこない相手が狙いだけ合わせてくるのは、何が起きているのか分からない。
    #[test]
    fn dummies_neither_move_nor_shoot_nor_take_aim() {
        let mut app = sandbox_app();
        let mut target = dummy(2, 1);
        // CPUのSystemが動かした後のような入力を、あらかじめ入れておく。
        target.movement = Vec2::new(1.0, 0.0);
        target.shooting = true;
        let start = target.position;
        let start_aim = target.aim;
        let entity = app.world_mut().spawn(target).id();
        // AIが狙う相手として人間を1人置く。CPUは近くの敵を狙って動く。
        app.world_mut().spawn(test_player(1, Some(101)));

        for _ in 0..30 {
            advance_one_tick(app.world_mut());
        }

        let dummy = app.world().get::<Player>(entity).expect("dummy");
        assert_eq!(dummy.position, start, "的が動いている");
        assert!(!dummy.shooting, "的が撃ち返している");
        assert_eq!(dummy.aim, start_aim, "的が銃口をこちらへ向けている");
    }

    /// ダミーは通常より早く起き上がる。何度も数え直せるように。
    #[test]
    fn a_downed_dummy_comes_back_quickly() {
        let mut app = sandbox_app();
        let mut target = dummy(2, 1);
        target.alive = false;
        // 通常の復活時間（既定2.0秒）を入れておく。
        target.respawn_left = 2.0;
        let entity = app.world_mut().spawn(target).id();

        advance_one_tick(app.world_mut());

        let respawn_left = app
            .world()
            .get::<Player>(entity)
            .expect("dummy")
            .respawn_left;
        assert!(
            respawn_left <= DUMMY_RESPAWN_SECONDS,
            "復活待ちが縮んでいない: {respawn_left}"
        );
    }
}
