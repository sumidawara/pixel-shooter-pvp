// ロビーで選んだCPUの強さが、サーバーへ届いて保たれることを確認する。
//
// 強さの中身（視界や反応）はRust側の単体試験が段階ごとに見ている。ここで見るのは
// 繋ぎ込み: 選んだ番号が届くか、範囲外を送っても壊れないか、その状態でCPUを
// 追加できるか。ここが切れると、選んでも何も変わらない。
const SERVER_URL = process.env.PIXEL_SHOOTER_SERVER_URL ?? "ws://127.0.0.1:9001";

const BASE_SETTINGS = {
  map_id: "classic_arena",
  match_seconds: 120.0,
  kill_points: 100,
  death_penalty: 25,
  item_points: 20,
  item_spawn_interval: 5.0,
  max_items: 3,
  sandbox: false,
};

const host = {
  socket: new WebSocket(SERVER_URL),
  id: 0,
  phase: "",
  cpuLevel: null,
  players: [],
};

host.socket.addEventListener("open", () => {
  host.socket.send(
    JSON.stringify({ type: "join", name: "CpuLevelHost", reconnect_token: "" }),
  );
});

host.socket.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (message.type === "welcome") {
    host.id = message.player_id;
  } else if (message.type === "snapshot") {
    host.phase = message.phase;
    host.cpuLevel = message.room?.settings?.cpu_level ?? null;
    host.players = message.players;
  }
});

function sendLevel(level) {
  host.socket.send(
    JSON.stringify({
      type: "update_room_settings",
      settings: { ...BASE_SETTINGS, cpu_level: level },
    }),
  );
}

/// 「この段階を送ったら、こう返ってくるはず」の並び。
/// 範囲外は近い方へ寄せて返す（弾かずに丸める）。
const STEPS = [
  { send: 1, expect: 1 },
  { send: 4, expect: 4 },
  { send: 9, expect: 4 },
  { send: 0, expect: 1 },
  { send: 2, expect: 2 },
];

let index = 0;
/// 送った設定が返ってくるのを待つ期限。これを過ぎても変わらなければ失敗。
///
/// 「期待した値以外なら失敗」とすると、届く前の1つ前の値まで失敗に数えてしまう。
let deadline = 0;
let cpuAdded = false;
const problems = [];

const poll = setInterval(() => {
  if (!host.id || host.phase !== "waiting") {
    return;
  }

  if (index < STEPS.length) {
    const step = STEPS[index];
    if (deadline === 0) {
      sendLevel(step.send);
      deadline = Date.now() + 1500;
      return;
    }
    if (host.cpuLevel === step.expect) {
      index += 1;
      deadline = 0;
      return;
    }
    if (Date.now() > deadline) {
      problems.push(
        `${step.send} を送ったが ${host.cpuLevel} のまま（期待 ${step.expect}）`,
      );
      index += 1;
      deadline = 0;
    }
    return;
  }

  if (!cpuAdded) {
    cpuAdded = true;
    host.socket.send(JSON.stringify({ type: "add_cpu" }));
    return;
  }
  if (!host.players.some((player) => player.is_cpu)) {
    return;
  }

  clearInterval(poll);
  host.socket.close();
  if (problems.length > 0) {
    console.error(JSON.stringify({ error: problems }));
    process.exit(1);
  }
  console.log(
    JSON.stringify({ levelsAccepted: STEPS.length, finalLevel: host.cpuLevel, cpuAdded: true }),
  );
  process.exit(0);
}, 20);

setTimeout(() => {
  clearInterval(poll);
  host.socket.close();
  console.error(
    JSON.stringify({
      error: "CPUの段階が届かなかった",
      stepIndex: index,
      cpuLevel: host.cpuLevel,
      phase: host.phase,
      problems,
    }),
  );
  process.exit(1);
}, 12000);
