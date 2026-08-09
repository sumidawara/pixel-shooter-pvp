// 統合試験がサーバーへ送るルーム設定の土台。
//
// 正は backend/protocols/game/src/lib.rs の `RoomSettings`。あちらの項目が
// 増えると、`#[serde(default)]` の付いていない項目は必須になるため、
// 書き漏らした試験だけが「設定が届かない」形で落ちる。
//
// 各試験がそれぞれ全項目を書き写していると、項目が増えるたびに全部を直すことになり、
// 直し漏れに気付くのは落ちたときになる。ここへまとめ、必要な項目だけ上書きして使う。

/// サーバーの既定と同じ値。試験ごとに変えたい項目だけ上書きする。
export const DEFAULT_ROOM_SETTINGS = Object.freeze({
  map_id: "classic_arena",
  match_seconds: 120.0,
  kill_points: 100,
  death_penalty: 25,
  item_points: 20,
  item_spawn_interval: 5.0,
  max_items: 3,
  sandbox: false,
  cpu_level: 3,
});

/// 既定へ `overrides` を重ねたルーム設定を作る。
export function roomSettings(overrides = {}) {
  return { ...DEFAULT_ROOM_SETTINGS, ...overrides };
}
