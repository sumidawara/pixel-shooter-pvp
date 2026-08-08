//! Godotクライアントが解釈する通信メッセージの、フィールド名まで含めた固定。
//!
//! Rustの型を正としつつ、消費側（Godot、Adminデバッグ画面、Nodeの試験スクリプト）は
//! すべて手書きでJSONを読んでいる。フィールド名を変えてもRustはコンパイルが通るため、
//! ここでゴールデンJSONを固定し、`frontend/tests/snapshot_contract_test.gd` が
//! 同じファイルをクライアントの必須キー一覧と突き合わせる。
//!
//! 期待値を更新する場合:
//!
//! ```sh
//! UPDATE_WIRE_GOLDEN=1 cargo test -p pixel-shooter-protocol --test wire_golden
//! ```
//!
//! 更新後は必ず、Godot側の読み取り箇所とAdminデバッグ画面の型定義を追従させること。

use std::{fs, path::PathBuf};

use pixel_shooter_protocol::{
    BulletSnapshot, GhostThiefSnapshot, HeldItemSnapshot, ItemKind, ItemSnapshot,
    LarokinPopposSnapshot, MapDefinition, MapSummary, MatchPhase, PlayerSnapshot, RoomSettings,
    RoomSnapshot, ServerMessage, Snapshot, Vec2,
};
use serde_json::{Value, json};

fn vec2(x: f32, y: f32) -> Vec2 {
    Vec2 { x, y }
}

/// クライアントが解釈しうるフィールドをすべて埋めた代表メッセージ。
///
/// `Option`のフィールドは、`None`側だけを固定すると値ありの形が抜けるため、
/// 必ず片方は値の入った状態にしておく。
fn representative_messages() -> Vec<(&'static str, ServerMessage)> {
    vec![
        (
            "welcome",
            ServerMessage::Welcome {
                player_id: 1,
                reconnect_token: "0123456789abcdef0123456789abcdef".into(),
                reconnected: false,
            },
        ),
        (
            "rejected",
            ServerMessage::Rejected {
                reason: "The match has already started.".into(),
                retryable: true,
            },
        ),
        (
            "map_definition",
            ServerMessage::MapDefinition {
                map: MapDefinition {
                    schema_version: 1,
                    id: "wire_golden".into(),
                    revision: "1".into(),
                    name: "Wire Golden".into(),
                    width: 4,
                    height: 3,
                    tile_size: 32,
                    tiles: vec!["####".into(), "#..#".into(), "####".into()],
                    spawn_points: vec![[1, 1], [2, 1], [1, 1], [2, 1]],
                    item_spawn_points: vec![[1, 1]],
                },
            },
        ),
        (
            "map_catalog",
            ServerMessage::MapCatalog {
                maps: vec![
                    MapSummary {
                        id: "classic_arena".into(),
                        name: "Classic Arena".into(),
                    },
                    MapSummary {
                        id: "crossroads".into(),
                        name: "Crossroads".into(),
                    },
                ],
            },
        ),
        (
            "snapshot",
            ServerMessage::Snapshot(Box::new(Snapshot {
                tick: 1234,
                phase: MatchPhase::Running,
                time_left: 87.5,
                winner_id: Some(2),
                reconnect_grace_left: 12.25,
                move_speed: 150.0,
                dash_speed: 520.0,
                dash_duration: 0.13,
                dash_cooldown: 1.1,
                players: vec![
                    PlayerSnapshot {
                        id: 1,
                        name: "Player 1".into(),
                        position: vec2(80.0, 80.0),
                        aim: vec2(1.0, 0.0),
                        hp: 4,
                        max_hp: 5,
                        score: 120,
                        is_cpu: false,
                        is_dummy: false,
                        connected: true,
                        reconnect_grace_left: 0.0,
                        alive: true,
                        respawn_left: 0.0,
                        invulnerable_left: 0.5,
                        ammo: 3,
                        max_ammo: 6,
                        reloading: false,
                        reload_left: 0.0,
                        dash_cooldown_left: 0.75,
                        dashing: true,
                        dash_time_left: 0.05,
                        // 所持アイテムありの形を固定する。
                        held_item: Some(HeldItemSnapshot {
                            kind: ItemKind::Dash,
                            charges: 4,
                        }),
                        berserk_left: 1.5,
                        shield_hp: 2,
                        last_input_sequence: 987,
                    },
                    PlayerSnapshot {
                        id: 2,
                        name: "DUMMY-2".into(),
                        position: vec2(560.0, 272.0),
                        aim: vec2(-1.0, 0.0),
                        hp: 0,
                        max_hp: 5,
                        score: -25,
                        is_cpu: true,
                        is_dummy: true,
                        connected: true,
                        reconnect_grace_left: 0.0,
                        alive: false,
                        respawn_left: 1.75,
                        invulnerable_left: 0.0,
                        ammo: 0,
                        max_ammo: 6,
                        reloading: true,
                        reload_left: 1.2,
                        dash_cooldown_left: 0.0,
                        dashing: false,
                        dash_time_left: 0.0,
                        // 所持アイテムなしの形も同時に固定する。
                        held_item: None,
                        berserk_left: 0.0,
                        shield_hp: 0,
                        last_input_sequence: 0,
                    },
                ],
                bullets: vec![BulletSnapshot {
                    id: 42,
                    owner_id: 1,
                    position: vec2(120.5, 80.0),
                    velocity: vec2(340.0, 0.0),
                    damage: 2,
                }],
                items: vec![ItemSnapshot {
                    id: 7,
                    position: vec2(320.0, 144.0),
                    points: 20,
                    kind: ItemKind::EnergyCell,
                }],
                larokin_poppos: vec![LarokinPopposSnapshot {
                    id: 3,
                    owner_id: 1,
                    position: vec2(40.0, 176.0),
                    velocity: vec2(230.0, 0.0),
                    telegraph_left: 0.35,
                }],
                ghost_thieves: vec![GhostThiefSnapshot {
                    id: 5,
                    owner_id: 1,
                    target_id: 2,
                    from: vec2(80.0, 80.0),
                    to: vec2(200.0, 120.0),
                    stolen_kind: ItemKind::Shield,
                    progress: 0.25,
                }],
                room: RoomSnapshot {
                    host_player_id: Some(1),
                    can_start: false,
                    max_players: 4,
                    // 練習場のON側を固定する。既定のOFFだけだと、
                    // 設定が届かなくなっても気付けない。
                    settings: RoomSettings {
                        sandbox: true,
                        ..RoomSettings::default()
                    },
                },
            })),
        ),
    ]
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../frontend/tests/fixtures/wire_messages_golden.json")
}

fn build_golden() -> Value {
    let mut messages = serde_json::Map::new();
    for (name, message) in representative_messages() {
        messages.insert(
            name.to_string(),
            serde_json::to_value(&message).expect("serialize wire message"),
        );
    }
    json!({
        "schema_version": 1,
        "generated_by": "backend/protocols/game/tests/wire_golden.rs",
        "messages": Value::Object(messages),
    })
}

#[test]
fn server_messages_round_trip_through_their_own_wire_format() {
    for (name, message) in representative_messages() {
        let text = serde_json::to_string(&message).expect("serialize");
        let decoded: ServerMessage = serde_json::from_str(&text).unwrap_or_else(|error| {
            panic!("{name} could not be decoded from its own JSON: {error}")
        });
        assert_eq!(
            message, decoded,
            "{name} の Serialize と Deserialize が食い違っている"
        );
    }
}

/// ファイルへ書く形のJSONと、それを読み直した値を返す。
///
/// f32はJSONへ最短表記で書き出されるため、メモリ上のf64表現とは末尾の桁が
/// 食い違う。比較の前に必ず「書き出して読み直した」形へ揃える。
fn canonicalize(value: &Value) -> (String, Value) {
    let mut text = serde_json::to_string_pretty(value).expect("serialize golden");
    text.push('\n');
    let normalized = serde_json::from_str(&text).expect("re-read generated golden");
    (text, normalized)
}

#[test]
fn wire_format_matches_the_committed_fixture() {
    let (text, generated) = canonicalize(&build_golden());
    let path = golden_path();

    if std::env::var("UPDATE_WIRE_GOLDEN").is_ok() {
        fs::create_dir_all(path.parent().expect("fixture directory"))
            .expect("create fixture directory");
        fs::write(&path, text).expect("write golden fixture");
        return;
    }

    let committed = fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "could not read {}: {error}\n\
             UPDATE_WIRE_GOLDEN=1 を付けて再実行すると生成できる",
            path.display()
        )
    });
    let committed: Value = serde_json::from_str(&committed).expect("parse committed golden");

    assert_eq!(
        committed, generated,
        "通信フォーマットが変わった。Godotの読み取り箇所\n\
         (frontend/src/networking/snapshot_contract.gd ほか) と\n\
         Adminデバッグ画面の型 (tools/debug-web/src/types.ts) を追従させたうえで、\n\
         UPDATE_WIRE_GOLDEN=1 でfixtureを更新すること"
    );
}
