//! Godotのクライアント予測と突き合わせるための、移動規則のゴールデンベクタ。
//!
//! クライアント予測がある以上、移動・ダッシュ・壁判定の規則はRust（権威サーバー）と
//! GDScript（予測）の2箇所に存在する。両者が一致していることは型では保証できないため、
//! ここで「入力列 → 位置列」をfixtureとして固定し、
//! `frontend/tests/movement_prediction_golden_test.gd` が同じ入力を予測側へ流して
//! 一致を検証する。どちらかの規則を変えると、必ずもう一方のテストが落ちる。
//!
//! 期待値を更新する場合:
//!
//! ```sh
//! UPDATE_MOVEMENT_GOLDEN=1 cargo test -p pixel-shooter-game-core --test movement_prediction_golden
//! ```
//!
//! 更新後は必ずGodot側のテストも実行し、予測実装を追従させること。

use std::{fs, path::PathBuf};

use bevy::prelude::*;
use pixel_shooter_game_core::{
    ArenaMap, GameCorePlugin, GameSettings, HeldItem, MatchState, Player, advance_one_tick,
    apply_network_player_input,
};
use pixel_shooter_protocol::{ItemKind, MatchPhase, PlayerInput};
use serde_json::{Value, json};

const TICK_RATE: f64 = 60.0;
/// 死亡中に復活してしまうと位置が飛ぶため、十分長い復活待ち時間を固定で入れる。
const FROZEN_RESPAWN_SECONDS: f32 = 9_999.0;

/// 1tick分の指示。`state`はスナップショット由来でクライアントも知っている値だけを使う。
struct ScriptedFrame {
    note: &'static str,
    alive: bool,
    berserk_left: f32,
    /// 所持しているダッシュアイテムの残チャージ。0なら未所持。
    dash_charges: u32,
    move_x: f32,
    move_y: f32,
    dash_pressed: bool,
    use_item_pressed: bool,
}

impl ScriptedFrame {
    fn walking(note: &'static str, move_x: f32, move_y: f32) -> Self {
        Self {
            note,
            alive: true,
            berserk_left: 0.0,
            dash_charges: 0,
            move_x,
            move_y,
            dash_pressed: false,
            use_item_pressed: false,
        }
    }
}

/// 検証したい規則を順に通す入力列を組み立てる。
///
/// 各区間が1つの規則に対応しており、規則を1つ壊すとその区間から期待値がずれる。
fn scripted_frames() -> Vec<ScriptedFrame> {
    let mut frames = Vec::new();
    let diagonal = std::f32::consts::FRAC_1_SQRT_2;

    // 無入力では動かない。
    for _ in 0..5 {
        frames.push(ScriptedFrame::walking("idle", 0.0, 0.0));
    }
    // 通常移動。
    for _ in 0..30 {
        frames.push(ScriptedFrame::walking("walk right", 1.0, 0.0));
    }
    // 入力長は1へクランプされるため、斜め入力でも速度は上がらない。
    for _ in 0..10 {
        frames.push(ScriptedFrame::walking(
            "walk diagonal (length is clamped to 1)",
            diagonal,
            diagonal,
        ));
    }
    // ダッシュ開始。移動入力方向へ dash_duration のあいだ高速移動する。
    frames.push(ScriptedFrame {
        dash_pressed: true,
        ..ScriptedFrame::walking("dash starts", 1.0, 0.0)
    });
    for _ in 0..14 {
        frames.push(ScriptedFrame::walking("dash continues then ends", 1.0, 0.0));
    }
    // クールダウン中の再ダッシュは無視される。
    frames.push(ScriptedFrame {
        dash_pressed: true,
        ..ScriptedFrame::walking("dash rejected while on cooldown", 1.0, 0.0)
    });
    for _ in 0..9 {
        frames.push(ScriptedFrame::walking("walk right", 1.0, 0.0));
    }
    // 障害物ブロックの手前まで斜めに降りる。
    for _ in 0..40 {
        frames.push(ScriptedFrame::walking(
            "approach the obstacle block",
            diagonal,
            diagonal,
        ));
    }
    // 壁へ正面から押し込む。X方向が止まり、位置が変化しなくなる。
    for _ in 0..20 {
        frames.push(ScriptedFrame::walking("push into the wall", 1.0, 0.0));
    }
    // 壁沿いに斜め移動する。X軸だけが止まり、Y軸は進む（軸ごとの衝突解決）。
    // 壁の下端を抜けるとX軸も再び進み始める。
    for _ in 0..60 {
        frames.push(ScriptedFrame::walking(
            "slide along the wall (x blocked, y free)",
            diagonal,
            diagonal,
        ));
    }
    // バーサク中は移動速度が半分になる。
    for _ in 0..15 {
        frames.push(ScriptedFrame {
            berserk_left: 1.0,
            ..ScriptedFrame::walking("berserk halves move speed", 0.0, -1.0)
        });
    }
    // 死亡中は一切動かず、ダッシュのクールダウンも進まない。
    for _ in 0..15 {
        frames.push(ScriptedFrame {
            alive: false,
            dash_pressed: true,
            ..ScriptedFrame::walking("dead players do not move", 1.0, 0.0)
        });
    }
    // アイテムのダッシュはクールダウンを消費せずに発動する。
    frames.push(ScriptedFrame {
        dash_charges: 5,
        use_item_pressed: true,
        ..ScriptedFrame::walking("item dash ignores the cooldown", 0.0, -1.0)
    });
    for _ in 0..14 {
        frames.push(ScriptedFrame {
            dash_charges: 4,
            ..ScriptedFrame::walking("item dash continues then ends", 0.0, -1.0)
        });
    }
    frames
}

fn build_app(settings: &GameSettings) -> App {
    let mut app = App::new();
    app.insert_resource(settings.clone())
        .insert_resource(MatchState {
            phase: MatchPhase::Running,
            // 試合終了やアイテム出現が混ざらないよう、期間は十分長く取る。
            phase_time_left: 3_600.0,
            item_spawn_left: 3_600.0,
            reconnect_grace_seconds: settings.match_rules.reconnect_grace_seconds,
            room_settings: settings.room_settings(),
            ..Default::default()
        })
        .add_plugins(GameCorePlugin::new(TICK_RATE));
    app
}

fn golden_player(
    id: u64,
    position: Vec2,
    gameplay: &pixel_shooter_game_core::GameplaySettings,
) -> Player {
    Player {
        id,
        connection_id: Some(1),
        is_cpu: false,
        reconnect_token: String::new(),
        reconnect_grace_left: 0.0,
        slot: 0,
        name: "Golden".into(),
        position,
        aim: Vec2::X,
        movement: Vec2::ZERO,
        shooting: false,
        hp: gameplay.max_hp,
        score: 0,
        alive: true,
        respawn_left: 0.0,
        shot_cooldown: 0.0,
        ammo: gameplay.max_ammo,
        reload_left: 0.0,
        reload_requested: false,
        // 無敵時間は位置計算に影響しないため0から始める。
        invulnerable_left: 0.0,
        dash_cooldown_left: 0.0,
        dash_time_left: 0.0,
        dash_direction: Vec2::ZERO,
        dash_requested: false,
        use_item_requested: false,
        held_item: None,
        berserk_left: 0.0,
        shield_hp: 0,
        last_input_sequence: 0,
    }
}

fn generate_golden() -> Value {
    let settings = GameSettings::default();
    let mut app = build_app(&settings);
    let map = app.world().resource::<ArenaMap>().clone();
    let start_position = map.spawn_position(0);
    let entity = app
        .world_mut()
        .spawn(golden_player(1, start_position, &settings.gameplay))
        .id();

    let mut frames = Vec::new();
    for frame in scripted_frames() {
        {
            let world = app.world_mut();
            let mut player = world.get_mut::<Player>(entity).expect("golden player");
            player.alive = frame.alive;
            // 復活処理で位置が飛ばないよう、死亡中は復活待ちを固定する。
            player.respawn_left = if frame.alive {
                0.0
            } else {
                FROZEN_RESPAWN_SECONDS
            };
            player.berserk_left = frame.berserk_left;
            player.held_item = (frame.dash_charges > 0).then_some(HeldItem {
                kind: ItemKind::Dash,
                charges: frame.dash_charges,
            });
            apply_network_player_input(
                &mut player,
                PlayerInput {
                    move_x: frame.move_x,
                    move_y: frame.move_y,
                    aim_x: 1.0,
                    aim_y: 0.0,
                    shooting: false,
                    reload_pressed: false,
                    dash_pressed: frame.dash_pressed,
                    use_item_pressed: frame.use_item_pressed,
                },
            );
        }

        advance_one_tick(app.world_mut());

        let player = app.world().get::<Player>(entity).expect("golden player");
        frames.push(json!({
            "note": frame.note,
            "alive": frame.alive,
            "berserk_left": frame.berserk_left,
            "dash_charges": frame.dash_charges,
            "move_x": frame.move_x,
            "move_y": frame.move_y,
            "dash_pressed": frame.dash_pressed,
            "use_item_pressed": frame.use_item_pressed,
            "expected": {
                "position": { "x": player.position.x, "y": player.position.y },
                "dash_time_left": player.dash_time_left,
                "dash_cooldown_left": player.dash_cooldown_left,
            },
        }));
    }

    json!({
        "schema_version": 1,
        "generated_by": "backend/game-core/tests/movement_prediction_golden.rs",
        "tick_rate": TICK_RATE,
        "gameplay": {
            "move_speed": settings.gameplay.move_speed,
            "dash_speed": settings.gameplay.dash_speed,
            "dash_duration": settings.gameplay.dash_duration,
            "dash_cooldown": settings.gameplay.dash_cooldown,
        },
        // マップは接続時にサーバーが送る表現そのままを埋め込む。
        // Godot側はres://の外を読めないため、fixtureだけで再現できるようにする。
        "map": map.definition(),
        "start_position": { "x": start_position.x, "y": start_position.y },
        "frames": frames,
    })
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../frontend/tests/fixtures/movement_prediction_golden.json")
}

/// fixtureが検証したい規則を実際に通っていることを確かめる。
///
/// 入力列を書き換えた拍子に「壁に一度も当たらない」「ダッシュが発動しない」といった
/// 空振りのfixtureになると、Godot側のテストが通っても何も保証しなくなる。
fn assert_golden_covers_every_rule(golden: &Value) {
    let frames = golden["frames"].as_array().expect("frames array");
    let position = |frame: &Value| {
        (
            frame["expected"]["position"]["x"].as_f64().expect("x"),
            frame["expected"]["position"]["y"].as_f64().expect("y"),
        )
    };

    let mut blocked_on_x_only = false;
    let mut fully_blocked = false;
    let mut dashed = false;
    let mut moved_while_dead = false;

    for (index, frame) in frames.iter().enumerate() {
        let (x, y) = position(frame);
        let (previous_x, previous_y) = if index == 0 {
            (
                golden["start_position"]["x"].as_f64().expect("start x"),
                golden["start_position"]["y"].as_f64().expect("start y"),
            )
        } else {
            position(&frames[index - 1])
        };
        let requested_x = frame["move_x"].as_f64().expect("move_x").abs() > 0.0;
        let requested_y = frame["move_y"].as_f64().expect("move_y").abs() > 0.0;
        let held_x = (x - previous_x).abs() < f64::EPSILON;
        let held_y = (y - previous_y).abs() < f64::EPSILON;

        if requested_x && requested_y && held_x && !held_y {
            blocked_on_x_only = true;
        }
        if requested_x && !requested_y && held_x {
            fully_blocked = true;
        }
        if frame["expected"]["dash_time_left"].as_f64().expect("dash") > 0.0 {
            dashed = true;
        }
        if !frame["alive"].as_bool().expect("alive") && (!held_x || !held_y) {
            moved_while_dead = true;
        }
    }

    assert!(
        fully_blocked,
        "壁へ正面から当たって停止するフレームがfixtureに含まれていない"
    );
    assert!(
        blocked_on_x_only,
        "軸ごとの衝突解決（X軸だけ止まりY軸は進む）がfixtureに含まれていない"
    );
    assert!(dashed, "ダッシュ中のフレームがfixtureに含まれていない");
    assert!(
        !moved_while_dead,
        "死亡中に位置が動いている。サーバー側の規則が変わった可能性がある"
    );
}

/// 2つのJSONで最初に食い違った位置を、パス付きで返す。
fn first_difference(expected: &Value, actual: &Value, path: &str) -> Option<String> {
    match (expected, actual) {
        (Value::Object(expected), Value::Object(actual)) => {
            for (key, expected_value) in expected {
                let Some(actual_value) = actual.get(key) else {
                    return Some(format!("{path}.{key} が生成結果に無い"));
                };
                if let Some(difference) =
                    first_difference(expected_value, actual_value, &format!("{path}.{key}"))
                {
                    return Some(difference);
                }
            }
            actual
                .keys()
                .find(|key| !expected.contains_key(*key))
                .map(|key| format!("{path}.{key} がfixtureに無い"))
        }
        (Value::Array(expected), Value::Array(actual)) => {
            if expected.len() != actual.len() {
                return Some(format!(
                    "{path} の要素数が違う (fixture={}, 生成={})",
                    expected.len(),
                    actual.len()
                ));
            }
            expected
                .iter()
                .zip(actual)
                .enumerate()
                .find_map(|(index, (expected, actual))| {
                    first_difference(expected, actual, &format!("{path}[{index}]"))
                })
        }
        (expected, actual) if expected == actual => None,
        (expected, actual) => Some(format!("{path}: fixture={expected} 生成={actual}")),
    }
}

/// ファイルへ書く形のJSONと、それを読み直した値を返す。
///
/// 位置はf32で計算しているが、JSONへはf32として最短表記で書き出される。
/// メモリ上のf64表現とは末尾の桁が食い違うため、比較する前に必ず
/// 「書き出して読み直した」形へ揃える。
fn canonicalize(value: &Value) -> (String, Value) {
    let mut text = serde_json::to_string_pretty(value).expect("serialize golden");
    text.push('\n');
    let normalized = serde_json::from_str(&text).expect("re-read generated golden");
    (text, normalized)
}

#[test]
fn movement_golden_vector_matches_the_committed_fixture() {
    let raw = generate_golden();
    assert_golden_covers_every_rule(&raw);
    let (text, generated) = canonicalize(&raw);
    let path = golden_path();

    if std::env::var("UPDATE_MOVEMENT_GOLDEN").is_ok() {
        let parent = path.parent().expect("fixture directory");
        fs::create_dir_all(parent).expect("create fixture directory");
        fs::write(&path, text).expect("write golden fixture");
        return;
    }

    let committed = fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "could not read {}: {error}\n\
             UPDATE_MOVEMENT_GOLDEN=1 を付けて再実行すると生成できる",
            path.display()
        )
    });
    let committed: Value = serde_json::from_str(&committed).expect("parse committed golden");

    // fixtureは数百フレームあるため、値をまるごと突き合わせると差分が読めない。
    // 最初に食い違った場所だけを示す。
    if let Some(difference) = first_difference(&committed, &generated, "$") {
        panic!(
            "移動規則が変わった: {difference}\n\
             Godot側の予測 (frontend/src/game_modes/match/movement_predictor.gd) を\n\
             同じ規則へ追従させたうえで、UPDATE_MOVEMENT_GOLDEN=1 でfixtureを更新すること"
        );
    }
}
