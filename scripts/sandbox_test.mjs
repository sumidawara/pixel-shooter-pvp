// 練習場（サンドボックス）が、1人でアイテムと威力を試せる状態になることを確認する。
//
// ルーム設定を送る → 開始する → 的とアイテムが揃う、という一連の流れは
// プロトコル・ルーム設定・試合進行・アイテム出現をまたぐ。単体試験では
// それぞれの部品しか見られないため、ここで通しで確かめる。
const SERVER_URL = process.env.PIXEL_SHOOTER_SERVER_URL ?? "ws://127.0.0.1:9001";

/// 練習場に必ず置かれている必要があるアイテム。1つでも欠けると試せない種類が出る。
const REQUIRED_ITEM_KINDS = [
  "energy_cell",
  "dash",
  "shield",
  "berserk",
  "larokin_poppos",
  "ghost",
];

const host = {
  socket: new WebSocket(SERVER_URL),
  id: 0,
  phase: "",
  players: [],
  items: [],
  bullets: [],
  sandbox: false,
  timeLeftSamples: [],
  dummyPositions: new Map(),
  dummiesMoved: false,
  dummiesFired: false,
};

host.socket.addEventListener("open", () => {
  host.socket.send(
    JSON.stringify({ type: "join", name: "SandboxHost", reconnect_token: "" }),
  );
});

host.socket.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (message.type === "welcome") {
    host.id = message.player_id;
    return;
  }
  if (message.type !== "snapshot") {
    return;
  }
  host.phase = message.phase;
  host.players = message.players;
  host.items = message.items;
  host.sandbox = Boolean(message.room?.settings?.sandbox);

  if (message.phase !== "running") {
    return;
  }
  host.timeLeftSamples.push(message.time_left);

  const dummyIds = new Set(
    message.players.filter((player) => player.is_dummy).map((p) => p.id),
  );
  // 的が撃ち返していないこと。弾の持ち主で見る。
  if (message.bullets.some((bullet) => dummyIds.has(bullet.owner_id))) {
    host.dummiesFired = true;
  }
  // 的が動いていないこと。復活で位置が変わるため、生きている間だけ見る。
  for (const player of message.players) {
    if (!player.is_dummy || !player.alive) {
      host.dummyPositions.delete(player.id);
      continue;
    }
    const previous = host.dummyPositions.get(player.id);
    if (
      previous &&
      (Math.abs(previous.x - player.position.x) > 0.01 ||
        Math.abs(previous.y - player.position.y) > 0.01)
    ) {
      host.dummiesMoved = true;
    }
    host.dummyPositions.set(player.id, player.position);
  }
});

let settingsSent = false;
let startSent = false;
let runningSince = 0;

const poll = setInterval(() => {
  if (!settingsSent && host.id && host.phase === "waiting") {
    settingsSent = true;
    host.socket.send(
      JSON.stringify({
        type: "update_room_settings",
        settings: {
          map_id: "classic_arena",
          match_seconds: 120.0,
          kill_points: 100,
          death_penalty: 25,
          item_points: 20,
          item_spawn_interval: 5.0,
          max_items: 3,
          sandbox: true,
        },
      }),
    );
    return;
  }

  // 設定が反映されたことを確かめてから開始する。届く前に開始すると、
  // 通常の対戦として始まってしまい、何を試したのか分からない失敗になる。
  if (settingsSent && !startSent && host.sandbox && host.phase === "waiting") {
    startSent = true;
    host.socket.send(JSON.stringify({ type: "start_match" }));
    return;
  }

  if (host.phase !== "running") {
    return;
  }
  if (runningSince === 0) {
    runningSince = Date.now();
    return;
  }
  // 残り時間が減らないことを見るため、しばらく観察してから判定する。
  if (Date.now() - runningSince < 1500) {
    return;
  }

  const dummies = host.players.filter((player) => player.is_dummy);
  const presentKinds = new Set(host.items.map((item) => item.kind));
  const missingKinds = REQUIRED_ITEM_KINDS.filter((k) => !presentKinds.has(k));
  const dummiesHoldingItems = dummies.filter((d) => d.held_item !== null).length;
  const timeLeftMoved =
    host.timeLeftSamples.length > 1 &&
    Math.abs(host.timeLeftSamples[0] - host.timeLeftSamples.at(-1)) > 0.01;

  const problems = [];
  if (dummies.length !== 3) {
    problems.push(`空きスロットが的で埋まっていない: ${dummies.length}/3`);
  }
  if (host.dummiesMoved) {
    problems.push("的が動いている");
  }
  if (host.dummiesFired) {
    problems.push("的が撃ち返している");
  }
  if (missingKinds.length > 0) {
    problems.push(`置かれていないアイテム: ${missingKinds.join(", ")}`);
  }
  if (dummiesHoldingItems !== dummies.length) {
    problems.push(
      `持ち物のない的がいる（Ghostを試せない）: ${dummiesHoldingItems}/${dummies.length}`,
    );
  }
  if (timeLeftMoved) {
    problems.push("残り時間が減っている。練習中に試合が終わる");
  }

  clearInterval(poll);
  host.socket.close();
  if (problems.length > 0) {
    console.error(JSON.stringify({ error: problems }));
    process.exit(1);
  }
  console.log(
    JSON.stringify({
      dummies: dummies.length,
      itemKinds: [...presentKinds].sort(),
      dummiesArmed: dummiesHoldingItems,
      timeLeftHeld: true,
    }),
  );
  process.exit(0);
}, 20);

setTimeout(() => {
  clearInterval(poll);
  host.socket.close();
  console.error(
    JSON.stringify({
      error: "練習場が開始状態にならなかった",
      settingsSent,
      startSent,
      phase: host.phase,
      sandbox: host.sandbox,
      players: host.players.length,
    }),
  );
  process.exit(1);
}, 12000);
