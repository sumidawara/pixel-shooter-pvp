//! 対戦用アリーナの自動生成。
//!
//! 種(seed)を1つ与えると、遊べる形のマップが必ず1つ決まる。同じ種なら何度でも
//! 同じマップになるので、面白い地形に当たったら種を控えておけば作り直せる。
//!
//! 「いい感じ」を次の5つに分解して、すべて構造で保証している。
//! どれか1つでも欠けると、生成できても遊べないマップになる。
//!
//! 1. 点対称。どのスポーン地点から見ても地形の有利不利が同じになる
//! 2. 全部つながっている。閉じ込められた床や、たどり着けない場所を作らない
//! 3. 遮蔽がある。ただの空箱でも迷路でもない量に収める
//! 4. スポーン地点が離れている。開始直後に撃ち合いにならない
//! 5. アイテムの置き場所が6箇所。全種類を並べる練習場でもそのまま使える
//!
//! 大きさは既存のマップと同じ20×11タイル(640×352px)に固定している。
//! 画面まわりがこの寸法を前提にしているため、ここだけ変えると表示が壊れる。

use pixel_shooter_protocol::MapDefinition;

use super::{ArenaMap, GridPosition};

/// ロビーで「毎回作る」を選んだことを表すマップID。
///
/// 実体のあるマップと同じ場所に置くのは、ロビーの一覧が
/// `MapSummary`の並びしか扱わないため。特別扱いはサーバー側で1箇所に閉じる。
pub const RANDOM_MAP_ID: &str = "random";

const WIDTH: usize = 20;
const HEIGHT: usize = 11;
const TILE_SIZE: u32 = 32;

/// 外周の壁のすぐ内側に必ず残す通路の幅。
///
/// ここを塞がせないことで、4隅のスポーン地点が必ず1本の輪でつながる。
/// 生成した壁でプレイヤーを閉じ込めてしまう事故を、後から直すのではなく
/// 起きない形にしている。
const CORRIDOR: usize = 1;

/// 内側を壁が占める割合の目標。種ごとにこの範囲から1つ選ぶ。
///
/// 塊の個数ではなく割合で止めるのは、塊の大きさが毎回違うため。
/// 個数で決めると、大きい塊ばかり引いた種だけ迷路になる。
const MIN_WALL_RATIO: f32 = 0.10;
const MAX_WALL_RATIO: f32 = 0.22;
/// 目標に届かないまま回り続けないための上限。
///
/// 隙間の条件で弾かれる試行が多いので、目標の塊数よりかなり大きく取る。
const MAX_BLOCK_ATTEMPTS: usize = 200;

/// アイテムの置き場所の数。練習場が全6種類を並べるのでそれに合わせる。
const ITEM_SPAWN_COUNT: usize = 6;
/// アイテムの置き場所どうしを離すタイル数。近すぎると1歩で複数拾える。
const ITEM_SPACING_TILES: usize = 3;

impl ArenaMap {
    /// 種から遊べるアリーナを1つ作る。同じ種なら必ず同じマップになる。
    pub fn generate(seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let mut tiles = vec![Tile::Floor; WIDTH * HEIGHT];

        // 外周は壁。ここは可変にしない。抜けられる外周があると場外へ出られる。
        for x in 0..WIDTH {
            tiles[index(x, 0)] = Tile::Wall;
            tiles[index(x, HEIGHT - 1)] = Tile::Wall;
        }
        for y in 0..HEIGHT {
            tiles[index(0, y)] = Tile::Wall;
            tiles[index(WIDTH - 1, y)] = Tile::Wall;
        }

        place_cover(&mut tiles, &mut rng);

        let spawn_points = spawn_points();
        let item_spawn_points = item_spawn_points(&tiles, &spawn_points, &mut rng);

        let definition = MapDefinition {
            schema_version: 1,
            id: RANDOM_MAP_ID.into(),
            // 種を版として持たせる。同じ「random」でも中身が違うことを、
            // 受け取った側が見分けられるようにする。
            revision: format!("{seed:016x}"),
            // 種全体を畳んで短い名前にする。上位だけ使うと、
            // 小さい種がどれも同じ名前になる。
            name: format!(
                "RANDOM {:04X}",
                (seed ^ (seed >> 16) ^ (seed >> 32) ^ (seed >> 48)) as u16
            ),
            width: WIDTH,
            height: HEIGHT,
            tile_size: TILE_SIZE,
            tiles: rows(&tiles),
            spawn_points: spawn_points.iter().map(|p| [p.x, p.y]).collect(),
            item_spawn_points: item_spawn_points.iter().map(|p| [p.x, p.y]).collect(),
        };

        // 手書きのマップとまったく同じ検査を通す。生成物だけ緩い基準で通すと、
        // 手書きなら弾かれる形のマップが遊びの場に出てしまう。
        Self::validate(definition)
            .unwrap_or_else(|error| panic!("generated map for seed {seed:#x} is invalid: {error}"))
    }
}

/// 生成中だけ使う、床か壁かの2値。`TileKind`は壊せる壁も表すが、
/// 破壊の処理がまだ無いので生成では使わない。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tile {
    Floor,
    Wall,
}

fn index(x: usize, y: usize) -> usize {
    y * WIDTH + x
}

/// 点対称の相手側の座標。(x, y) と (W-1-x, H-1-y) が対になる。
fn mirrored(x: usize, y: usize) -> (usize, usize) {
    (WIDTH - 1 - x, HEIGHT - 1 - y)
}

/// 遮蔽の塊を左半分へ置き、同じものを点対称の位置へも置く。
///
/// 線対称ではなく点対称にしているのは、向かい合って始まる対戦で
/// 「自分から見た地形」が両者で一致するのがこちらだから。
fn place_cover(tiles: &mut [Tile], rng: &mut Rng) {
    // 外周の内側1マスは通路として残す。塊はさらにその内側にだけ置く。
    let low_x = 1 + CORRIDOR;
    let low_y = 1 + CORRIDOR;
    let high_y = HEIGHT - 2 - CORRIDOR;
    // 左半分だけを対象にする。右半分は点対称で埋まる。
    let high_x = WIDTH / 2 - 1;

    // 目標の密度を種から選び、そこへ届くまで塊を足す。
    let steps = ((MAX_WALL_RATIO - MIN_WALL_RATIO) * 100.0) as usize;
    let target = MIN_WALL_RATIO + rng.range(0, steps) as f32 / 100.0;

    for _ in 0..MAX_BLOCK_ATTEMPTS {
        if inner_wall_ratio(tiles) >= target {
            break;
        }
        let width = rng.range(1, 3);
        let height = rng.range(1, 2);
        let x0 = rng.range(low_x, high_x.saturating_sub(width - 1).max(low_x));
        let y0 = rng.range(low_y, high_y.saturating_sub(height - 1).max(low_y));
        let width = width.min(high_x + 1 - x0);
        let height = height.min(high_y + 1 - y0);
        // 既にある壁と隙間なく並べない。くっつけると塊どうしが融合して
        // 一枚の大きな壁になり、遮蔽ではなく行き止まりだらけの地形になる。
        if !has_room_for_block(tiles, x0, y0, width, height) {
            continue;
        }
        for y in y0..y0 + height {
            for x in x0..x0 + width {
                tiles[index(x, y)] = Tile::Wall;
                let (mx, my) = mirrored(x, y);
                tiles[index(mx, my)] = Tile::Wall;
            }
        }
    }
}

/// 塊とその周囲1マスが床であること。対称side側も同じ条件で見る。
///
/// 周囲まで見るのは、隣り合った塊のあいだに必ず1マスの通り道を残すため。
fn has_room_for_block(tiles: &[Tile], x0: usize, y0: usize, width: usize, height: usize) -> bool {
    let clear = |x0: usize, y0: usize, width: usize, height: usize| {
        for y in y0.saturating_sub(1)..=(y0 + height).min(HEIGHT - 2) {
            for x in x0.saturating_sub(1)..=(x0 + width).min(WIDTH - 2) {
                if tiles[index(x, y)] != Tile::Floor {
                    return false;
                }
            }
        }
        true
    };
    let (mx, my) = mirrored(x0 + width - 1, y0 + height - 1);
    clear(x0, y0, width, height) && clear(mx, my, width, height)
}

/// 外周の壁を除いた範囲で、壁が占める割合。
fn inner_wall_ratio(tiles: &[Tile]) -> f32 {
    let walls = (1..HEIGHT - 1)
        .flat_map(|y| (1..WIDTH - 1).map(move |x| (x, y)))
        .filter(|(x, y)| tiles[index(*x, *y)] == Tile::Wall)
        .count();
    walls as f32 / ((WIDTH - 2) * (HEIGHT - 2)) as f32
}

/// スポーン地点。外周通路の上なので、生成した壁と重なることはない。
///
/// 並び順は既存のマップに合わせ、スロット0と1が対角になるようにする。
/// 2人で始めたときに一番遠い組み合わせから始まる。
/// 4箇所より多いのは、復活地点が相手や弾から遠い所を選べるようにするため。
fn spawn_points() -> Vec<GridPosition> {
    let low = CORRIDOR;
    let high_x = WIDTH - 1 - CORRIDOR;
    let high_y = HEIGHT - 1 - CORRIDOR;
    [
        (low, low),
        (high_x, high_y),
        (high_x, low),
        (low, high_y),
        (WIDTH / 2 - 1, low),
        (WIDTH / 2, high_y),
    ]
    .into_iter()
    .map(|(x, y)| GridPosition { x, y })
    .collect()
}

/// アイテムの置き場所を左半分から選び、点対称の位置と対にする。
///
/// 対にするのは、片側にだけ多く湧く地形にしないため。
fn item_spawn_points(
    tiles: &[Tile],
    spawn_points: &[GridPosition],
    rng: &mut Rng,
) -> Vec<GridPosition> {
    let mut candidates: Vec<GridPosition> = Vec::new();
    for y in 1..HEIGHT - 1 {
        for x in 1..WIDTH / 2 {
            if tiles[index(x, y)] != Tile::Floor {
                continue;
            }
            let position = GridPosition { x, y };
            // スポーン地点の上に置くと、開始と同時に勝手に拾ってしまう。
            if spawn_points
                .iter()
                .any(|spawn| grid_distance(*spawn, position) < 2)
            {
                continue;
            }
            candidates.push(position);
        }
    }

    let mut chosen: Vec<GridPosition> = Vec::new();
    // 選ぶ順を種で散らす。同じ位置から詰めると、どの種でも似た配置になる。
    let offset = rng.range(0, candidates.len().saturating_sub(1).max(1));
    for spacing in (1..=ITEM_SPACING_TILES).rev() {
        for step in 0..candidates.len() {
            if chosen.len() >= ITEM_SPAWN_COUNT / 2 {
                break;
            }
            let candidate = candidates[(offset + step) % candidates.len()];
            let (mx, my) = mirrored(candidate.x, candidate.y);
            let mirror = GridPosition { x: mx, y: my };
            // 自分の対称位置とも離れている必要がある。中央付近だと重なる。
            // 既に選んだ場所、その対称位置、そして自分の対称位置のすべてから離す。
            let far_enough = chosen.iter().all(|other| {
                let (ox, oy) = mirrored(other.x, other.y);
                grid_distance(*other, candidate) >= spacing
                    && grid_distance(GridPosition { x: ox, y: oy }, candidate) >= spacing
            }) && grid_distance(mirror, candidate) >= spacing;
            if far_enough {
                chosen.push(candidate);
            }
        }
        if chosen.len() >= ITEM_SPAWN_COUNT / 2 {
            break;
        }
    }

    // 間隔を詰めても足りない場合は、残りを埋める。置き場所が減るより近い方がまし。
    for candidate in &candidates {
        if chosen.len() >= ITEM_SPAWN_COUNT / 2 {
            break;
        }
        if !chosen.contains(candidate) {
            chosen.push(*candidate);
        }
    }

    let mut points = Vec::with_capacity(ITEM_SPAWN_COUNT);
    for point in chosen {
        let (mx, my) = mirrored(point.x, point.y);
        points.push(point);
        points.push(GridPosition { x: mx, y: my });
    }
    points
}

fn grid_distance(left: GridPosition, right: GridPosition) -> usize {
    left.x.abs_diff(right.x) + left.y.abs_diff(right.y)
}

fn rows(tiles: &[Tile]) -> Vec<String> {
    (0..HEIGHT)
        .map(|y| {
            (0..WIDTH)
                .map(|x| match tiles[index(x, y)] {
                    Tile::Floor => '.',
                    Tile::Wall => '#',
                })
                .collect()
        })
        .collect()
}

/// 種から決まる乱数列(splitmix64)。
///
/// 外部の乱数生成器を使わないのは、同じ種が将来も同じマップを生むことを
/// この実装だけで保証したいから。依存先の更新で地形が変わると、
/// 控えておいた種が意味を失う。
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// `low`以上`high`以下の値を1つ返す。
    fn range(&mut self, low: usize, high: usize) -> usize {
        if high <= low {
            return low;
        }
        low + (self.next_u64() % (high - low + 1) as u64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 上下左右のマス。塗りつぶしで「つながっているか」を測るのに使う。
    fn neighbors(position: GridPosition) -> Vec<(usize, usize)> {
        let mut result = Vec::with_capacity(4);
        if position.x > 0 {
            result.push((position.x - 1, position.y));
        }
        if position.y > 0 {
            result.push((position.x, position.y - 1));
        }
        if position.x + 1 < WIDTH {
            result.push((position.x + 1, position.y));
        }
        if position.y + 1 < HEIGHT {
            result.push((position.x, position.y + 1));
        }
        result
    }

    use crate::{arena::TileKind, model::MAX_PLAYERS};
    use pixel_shooter_protocol::PLAYER_RADIUS;

    /// 検査に使う種。1つ2つでは、たまたま通っただけの可能性が消えない。
    const SEEDS: std::ops::Range<u64> = 0..400;

    fn generated(seed: u64) -> ArenaMap {
        ArenaMap::generate(seed.wrapping_mul(0x1234_5678_9ABC_DEF1).wrapping_add(seed))
    }

    /// 同じ種なら同じマップ。控えた種で作り直せることの土台。
    #[test]
    fn the_same_seed_always_makes_the_same_map() {
        for seed in [0, 1, 42, u64::MAX] {
            assert_eq!(
                ArenaMap::generate(seed).definition(),
                ArenaMap::generate(seed).definition(),
                "種{seed}で結果が違う"
            );
        }
    }

    /// 種を変えれば地形も変わる。同じものしか出ないなら自動生成の意味がない。
    #[test]
    fn different_seeds_make_different_maps() {
        let mut shapes = std::collections::HashSet::new();
        for seed in SEEDS {
            shapes.insert(generated(seed).definition().tiles);
        }
        let seed_count = SEEDS.end - SEEDS.start;
        assert!(
            shapes.len() as u64 > seed_count / 2,
            "{seed_count}種から{}通りしか出ていない。似たマップばかりになる",
            shapes.len()
        );
    }

    /// 地形が点対称であること。どのスポーンから見ても地形の有利不利が同じになる。
    #[test]
    fn the_terrain_is_point_symmetric() {
        for seed in SEEDS {
            let map = generated(seed);
            for y in 0..map.height() {
                for x in 0..map.width() {
                    let (mx, my) = mirrored(x, y);
                    assert_eq!(
                        map.tile(GridPosition { x, y }),
                        map.tile(GridPosition { x: mx, y: my }),
                        "種{seed}: ({x},{y})と({mx},{my})が非対称"
                    );
                }
            }
        }
    }

    /// 床が全部つながっていること。
    ///
    /// 分断された床が残ると、そこへ湧いたアイテムを誰も取れず、
    /// CPUは行けない場所を目指して止まる。
    #[test]
    fn every_floor_tile_is_reachable_from_every_other() {
        for seed in SEEDS {
            let map = generated(seed);
            let floor_count = (0..map.width() * map.height())
                .filter(|i| {
                    map.tile(GridPosition {
                        x: i % map.width(),
                        y: i / map.width(),
                    }) == Some(TileKind::Floor)
                })
                .count();

            let start = map.spawn_points[0];
            let mut reached = std::collections::HashSet::new();
            let mut stack = vec![start];
            reached.insert(start);
            while let Some(current) = stack.pop() {
                for (x, y) in neighbors(current) {
                    let next = GridPosition { x, y };
                    if map.tile(next) == Some(TileKind::Floor) && reached.insert(next) {
                        stack.push(next);
                    }
                }
            }
            assert_eq!(
                reached.len(),
                floor_count,
                "種{seed}: たどり着けない床が{}マス残っている",
                floor_count - reached.len()
            );
        }
    }

    /// 遮蔽があり、かつ迷路でないこと。
    ///
    /// 壁が無いと隠れる場所の無い撃ち合いになり、多すぎると相手を見つけられない。
    #[test]
    fn there_is_cover_without_turning_into_a_maze() {
        for seed in SEEDS {
            let map = generated(seed);
            let inner_total = (map.width() - 2) * (map.height() - 2);
            let inner_walls = (1..map.height() - 1)
                .flat_map(|y| (1..map.width() - 1).map(move |x| (x, y)))
                .filter(|(x, y)| {
                    map.tile(GridPosition { x: *x, y: *y }) == Some(TileKind::SolidWall)
                })
                .count();
            let ratio = inner_walls as f32 / inner_total as f32;
            assert!(
                (0.08..=0.32).contains(&ratio),
                "種{seed}: 内側の壁が{:.0}%。遮蔽が無いか、迷路になっている",
                ratio * 100.0
            );
        }
    }

    /// 壁の塊が大きく育たないこと。
    ///
    /// 塊どうしをくっつけて置けるようにすると、重なりが連鎖して一枚の大きな壁になる。
    /// そうなると遮蔽ではなく行き止まりだらけの地形になり、相手を見つけられない。
    /// 置ける最大の塊は3×2で、点対称の相手と接した場合でも12マス。
    #[test]
    fn cover_never_grows_into_one_big_mass() {
        for seed in SEEDS {
            let map = generated(seed);
            let mut seen: std::collections::HashSet<GridPosition> = Default::default();
            let mut largest = 0;
            for y in 1..map.height() - 1 {
                for x in 1..map.width() - 1 {
                    let start = GridPosition { x, y };
                    if map.tile(start) != Some(TileKind::SolidWall) || seen.contains(&start) {
                        continue;
                    }
                    let mut size = 0;
                    let mut stack = vec![start];
                    seen.insert(start);
                    while let Some(current) = stack.pop() {
                        size += 1;
                        for (nx, ny) in neighbors(current) {
                            // 外周の壁はつながっていて当然なので数えない。
                            if nx == 0 || ny == 0 || nx == WIDTH - 1 || ny == HEIGHT - 1 {
                                continue;
                            }
                            let next = GridPosition { x: nx, y: ny };
                            if map.tile(next) == Some(TileKind::SolidWall) && seen.insert(next) {
                                stack.push(next);
                            }
                        }
                    }
                    largest = largest.max(size);
                }
            }
            assert!(
                largest <= 12,
                "種{seed}: 壁の塊が{largest}マスに育っている。遮蔽ではなく壁になっている"
            );
        }
    }

    /// スポーン地点が壁と重ならず、互いに離れていること。
    ///
    /// 近いと開始直後に撃ち合いになり、地形を確かめる前に試合が始まる。
    #[test]
    fn spawn_points_are_clear_and_far_apart() {
        for seed in SEEDS {
            let map = generated(seed);
            assert!(map.spawn_points.len() >= MAX_PLAYERS);
            for (index, point) in map.spawn_points.iter().enumerate() {
                assert!(
                    !map.obstacle_at(map.tile_center(*point), PLAYER_RADIUS),
                    "種{seed}: スポーン{index}が壁に埋まっている"
                );
            }
            // スロット0〜3が実際に使われる組。ここが近いと開始直後に接触する。
            for left in 0..MAX_PLAYERS {
                for right in left + 1..MAX_PLAYERS {
                    let distance = grid_distance(map.spawn_points[left], map.spawn_points[right]);
                    assert!(
                        distance >= 8,
                        "種{seed}: スポーン{left}と{right}が近すぎる({distance}マス)"
                    );
                }
            }
        }
    }

    /// アイテムの置き場所が6箇所あり、床の上で、互いに離れていること。
    ///
    /// 6箇所なのは練習場が全6種類を並べるから。重なっていると1歩で複数拾える。
    #[test]
    fn item_spawns_cover_every_kind_without_overlapping() {
        for seed in SEEDS {
            let map = generated(seed);
            assert_eq!(
                map.item_spawn_count(),
                ITEM_SPAWN_COUNT,
                "種{seed}: 置き場所が足りない"
            );
            for point in &map.item_spawn_points {
                assert_eq!(
                    map.tile(*point),
                    Some(TileKind::Floor),
                    "種{seed}: 置き場所({},{})が床でない",
                    point.x,
                    point.y
                );
            }
            for (index, left) in map.item_spawn_points.iter().enumerate() {
                for right in &map.item_spawn_points[index + 1..] {
                    assert!(left != right, "種{seed}: 置き場所が重なっている");
                }
            }
        }
    }

    /// スポーン地点どうしを歩いて行き来できること。
    ///
    /// つながっているだけでなく、CPUの経路探索が実際に道を見つけられる必要がある。
    /// 見つけられないとCPUはその場から動かなくなる。
    #[test]
    fn cpu_pathfinding_can_walk_between_spawns() {
        for seed in SEEDS {
            let map = generated(seed);
            for index in 1..MAX_PLAYERS {
                assert!(
                    map.find_player_path(map.spawn_position(0), map.spawn_position(index))
                        .is_some(),
                    "種{seed}: スポーン0から{index}への道が無い"
                );
            }
        }
    }
}
