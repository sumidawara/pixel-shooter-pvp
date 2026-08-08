// ロビーで RANDOM を選ぶと地形が自動生成され、試合ごとに作り直されることを確認する。
//
// 生成そのものの質（対称性・連結性・遮蔽の量）はRust側の単体試験が種400通りで
// 見ている。ここで見るのは繋ぎ込み: 一覧に出るか、選んだら届くか、
// 開始でもう一度作り直されるか。ここが切れると、選べるのに何も起きない。
const SERVER_URL = process.env.PIXEL_SHOOTER_SERVER_URL ?? "ws://127.0.0.1:9001";
const RANDOM_MAP_ID = "random";

const host = {
  socket: new WebSocket(SERVER_URL),
  id: 0,
  phase: "",
  mapId: "",
  catalogIds: [],
  definitions: [],
};

host.socket.addEventListener("open", () => {
  host.socket.send(
    JSON.stringify({ type: "join", name: "RandomMapHost", reconnect_token: "" }),
  );
});

host.socket.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  switch (message.type) {
    case "welcome":
      host.id = message.player_id;
      break;
    case "map_catalog":
      host.catalogIds = message.maps.map((map) => map.id);
      break;
    case "map_definition":
      host.definitions.push(message.map);
      break;
    case "snapshot":
      host.phase = message.phase;
      host.mapId = message.room?.settings?.map_id ?? "";
      break;
  }
});

/// 地形が実際に遊べる形になっているかを、届いた定義だけから確かめる。
function inspect(definition) {
  const rows = definition.tiles;
  const width = definition.width;
  const height = definition.height;

  // 点対称。どのスポーンから見ても地形の有利不利が同じであることの根拠。
  let symmetric = true;
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      if (rows[y][x] !== rows[height - 1 - y][width - 1 - x]) {
        symmetric = false;
      }
    }
  }

  // 全部つながっているか。スポーン0から塗りつぶして床の数と比べる。
  const floors = rows.join("").split("").filter((tile) => tile === ".").length;
  const [startX, startY] = definition.spawn_points[0];
  const seen = new Set([`${startX},${startY}`]);
  const stack = [[startX, startY]];
  while (stack.length > 0) {
    const [x, y] = stack.pop();
    for (const [nx, ny] of [
      [x - 1, y],
      [x + 1, y],
      [x, y - 1],
      [x, y + 1],
    ]) {
      if (nx < 0 || ny < 0 || nx >= width || ny >= height) continue;
      const key = `${nx},${ny}`;
      if (rows[ny][nx] === "." && !seen.has(key)) {
        seen.add(key);
        stack.push([nx, ny]);
      }
    }
  }

  return {
    symmetric,
    connected: seen.size === floors,
    itemSpawns: definition.item_spawn_points.length,
    spawns: definition.spawn_points.length,
  };
}

let settingsSent = false;
let startSent = false;
let selectedRevision = "";

const poll = setInterval(() => {
  if (!settingsSent && host.id && host.phase === "waiting") {
    settingsSent = true;
    host.socket.send(
      JSON.stringify({
        type: "update_room_settings",
        settings: {
          map_id: RANDOM_MAP_ID,
          match_seconds: 120.0,
          kill_points: 100,
          death_penalty: 25,
          item_points: 20,
          item_spawn_interval: 5.0,
          max_items: 3,
          sandbox: false,
        },
      }),
    );
    return;
  }

  // 選んだ時点で1枚届いているはず。届いてから開始する。
  const generated = host.definitions.filter((map) => map.id === RANDOM_MAP_ID);
  if (settingsSent && !startSent && generated.length > 0 && host.mapId === RANDOM_MAP_ID) {
    startSent = true;
    selectedRevision = generated[generated.length - 1].revision;
    host.socket.send(JSON.stringify({ type: "start_match" }));
    return;
  }

  if (!startSent || host.phase === "waiting") {
    return;
  }

  const problems = [];
  if (!host.catalogIds.includes(RANDOM_MAP_ID)) {
    problems.push(`一覧に RANDOM が出ない: ${host.catalogIds.join(", ")}`);
  }
  if (generated.length < 2) {
    problems.push(`試合開始で作り直していない: ${generated.length}枚`);
  } else if (generated[generated.length - 1].revision === selectedRevision) {
    problems.push("開始しても同じ地形のまま。毎回違う場所で遊べない");
  }
  for (const definition of generated) {
    const report = inspect(definition);
    if (!report.symmetric) {
      problems.push(`地形が非対称: ${definition.revision}`);
    }
    if (!report.connected) {
      problems.push(`行けない床がある: ${definition.revision}`);
    }
    if (report.spawns < 4) {
      problems.push(`スポーンが足りない: ${report.spawns}`);
    }
    if (report.itemSpawns < 6) {
      problems.push(`アイテムの置き場所が足りない: ${report.itemSpawns}`);
    }
  }

  clearInterval(poll);
  host.socket.close();
  if (problems.length > 0) {
    console.error(JSON.stringify({ error: problems }));
    process.exit(1);
  }
  console.log(
    JSON.stringify({
      catalogHasRandom: true,
      generated: generated.length,
      revisions: generated.map((map) => map.revision),
      ...inspect(generated[generated.length - 1]),
    }),
  );
  process.exit(0);
}, 20);

setTimeout(() => {
  clearInterval(poll);
  host.socket.close();
  console.error(
    JSON.stringify({
      error: "自動生成マップで試合が始まらなかった",
      settingsSent,
      startSent,
      phase: host.phase,
      mapId: host.mapId,
      definitions: host.definitions.map((map) => `${map.id}@${map.revision}`),
    }),
  );
  process.exit(1);
}, 12000);
