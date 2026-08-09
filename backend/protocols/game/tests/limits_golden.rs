//! サーバーとクライアントの両方が守る必要のある範囲の固定。
//!
//! ルーム設定の許容範囲、マップの大きさの上限、CPUの段階数は、Rust側が丸める値と
//! Godot側の入力欄・検証が同じでなければならない。別々に書かれていると、
//! 片方だけ動かしても誰も気付かない（実際、SpinBoxの`min/max`と
//! `sanitize_room_settings`の`clamp`にまったく同じ数字が並んでいた）。
//!
//! Rustを正とし、ここでJSONへ書き出す。
//! `frontend/tests/shared_limits_test.gd`が同じファイルを読んで画面側と突き合わせる。
//!
//! 期待値を更新する場合:
//!
//! ```sh
//! UPDATE_LIMITS_GOLDEN=1 cargo test -p pixel-shooter-protocol --test limits_golden
//! ```

use std::{fs, path::PathBuf};

use pixel_shooter_protocol::{map_limits, player_colors, room_settings_bounds as bounds};
use serde_json::{Value, json};

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../frontend/tests/fixtures/shared_limits_golden.json")
}

fn build_golden() -> Value {
    json!({
        "room_settings": {
            "match_seconds": { "min": bounds::MATCH_SECONDS.0, "max": bounds::MATCH_SECONDS.1 },
            "kill_points": { "min": bounds::KILL_POINTS.0, "max": bounds::KILL_POINTS.1 },
            "death_penalty": { "min": bounds::DEATH_PENALTY.0, "max": bounds::DEATH_PENALTY.1 },
            "item_points": { "min": bounds::ITEM_POINTS.0, "max": bounds::ITEM_POINTS.1 },
            "item_spawn_interval": {
                "min": bounds::ITEM_SPAWN_INTERVAL.0,
                "max": bounds::ITEM_SPAWN_INTERVAL.1
            },
            "max_items": { "min": bounds::MAX_ITEMS.0, "max": bounds::MAX_ITEMS.1 },
        },
        // CPUの強さはルームの設定ではなくCPU1体ごとの属性になったが、
        // 選べる範囲は両側で揃っている必要がある。
        "cpu": { "min_level": bounds::CPU_LEVEL.0, "max_level": bounds::CPU_LEVEL.1 },
        "player_colors": { "count": player_colors::COUNT },
        "map": {
            "max_width": map_limits::MAX_WIDTH,
            "max_height": map_limits::MAX_HEIGHT,
            "min_tile_size": map_limits::MIN_TILE_SIZE,
            "max_tile_size": map_limits::MAX_TILE_SIZE,
        },
    })
}

#[test]
fn limits_match_the_committed_fixture() {
    let golden = build_golden();
    let path = golden_path();

    if std::env::var("UPDATE_LIMITS_GOLDEN").is_ok() {
        let mut text = serde_json::to_string_pretty(&golden).expect("serialize limits");
        text.push('\n');
        fs::write(&path, text).expect("write limits fixture");
        return;
    }

    let text = fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "{} が読めない: {error}\nUPDATE_LIMITS_GOLDEN=1 で生成すること",
            path.display()
        )
    });
    let committed: Value = serde_json::from_str(&text).expect("parse limits fixture");
    assert_eq!(
        committed, golden,
        "共有している範囲が変わった。Godot側（入力欄の min/max、\n\
         arena_map_data.gd の検証、ロビーの段階数）を追従させたうえで、\n\
         UPDATE_LIMITS_GOLDEN=1 でfixtureを更新すること"
    );
}
