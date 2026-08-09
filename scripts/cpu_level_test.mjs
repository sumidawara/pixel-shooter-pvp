// ロビーで選んだCPUの強さと色が、サーバーへ届いて保たれることを確認する。
//
// 強さの中身（視界や反応）はRust側の単体試験が段階ごとに見ている。ここで見るのは
// 繋ぎ込み: 足すときに指定した番号が届くか、後から変えられるか、範囲外を送っても
// 壊れないか、色が重ならないか。ここが切れると、選んでも何も変わらない。
//
// 強さはルームの設定ではなくCPU1体ごとの属性なので、確認先はSnapshotの
// players[].cpu_level になる。設定を経由しないぶん、送り先を間違えると
// 黙って無視されるだけになる。

const SERVER_URL = process.env.PIXEL_SHOOTER_SERVER_URL ?? "ws://127.0.0.1:9001";

const host = {
  socket: new WebSocket(SERVER_URL),
  id: 0,
  phase: "",
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
    host.players = message.players;
  }
});

const cpus = () => host.players.filter((player) => player.is_cpu && !player.is_dummy);
const me = () => host.players.find((player) => player.id === host.id);

/// 送った強さと、返ってくるはずの強さ。範囲外は近い方へ寄せて返す。
const STEPS = [
  { send: 1, expect: 1 },
  { send: 4, expect: 4 },
  { send: 9, expect: 4 },
  { send: 0, expect: 1 },
  { send: 2, expect: 2 },
];

const problems = [];
let stage = "add";
let index = 0;
let deadline = 0;
let cpuId = 0;
let myFirstColor = -1;

const poll = setInterval(() => {
  if (!host.id || host.phase !== "waiting") {
    return;
  }

  // 1) 強さを指定して1体足す。指定がそのまま乗ること。
  if (stage === "add") {
    if (deadline === 0) {
      host.socket.send(JSON.stringify({ type: "add_cpu", level: 1 }));
      deadline = Date.now() + 2000;
      return;
    }
    if (cpus().length > 0) {
      cpuId = cpus()[0].id;
      if (cpus()[0].cpu_level !== 1) {
        problems.push(`足すときの強さが乗らない: ${cpus()[0].cpu_level}`);
      }
      if (cpus()[0].color === me()?.color) {
        problems.push("CPUの色がホストと重なっている");
      }
      myFirstColor = me()?.color ?? -1;
      stage = "levels";
      deadline = 0;
    } else if (Date.now() > deadline) {
      problems.push("CPUが増えない");
      stage = "colors";
      deadline = 0;
    }
    return;
  }

  // 2) 後から強さを変えられること。
  if (stage === "levels") {
    if (index >= STEPS.length) {
      stage = "colors";
      deadline = 0;
      return;
    }
    const step = STEPS[index];
    if (deadline === 0) {
      host.socket.send(
        JSON.stringify({ type: "set_cpu_level", player_id: cpuId, level: step.send }),
      );
      deadline = Date.now() + 1500;
      return;
    }
    const current = cpus()[0]?.cpu_level ?? null;
    if (current === step.expect) {
      index += 1;
      deadline = 0;
      return;
    }
    if (Date.now() > deadline) {
      problems.push(`${step.send} を送ったが ${current} のまま（期待 ${step.expect}）`);
      index += 1;
      deadline = 0;
    }
    return;
  }

  // 3) 空いている色へは移れ、埋まっている色へは移れないこと。
  if (stage === "colors") {
    if (deadline === 0) {
      const cpuColor = cpus()[0]?.color ?? 1;
      // 空いている色を1つ選ぶ。
      const free = [0, 1, 2, 3].find(
        (color) => color !== cpuColor && color !== myFirstColor,
      );
      host.socket.send(JSON.stringify({ type: "set_color", color: free }));
      deadline = Date.now() + 1500;
      poll.freeColor = free;
      return;
    }
    if (me()?.color === poll.freeColor) {
      // 埋まっている色（CPUの色）へは移れないこと。
      host.socket.send(JSON.stringify({ type: "set_color", color: cpus()[0]?.color }));
      setTimeout(finish, 600);
      clearInterval(poll);
      return;
    }
    if (Date.now() > deadline) {
      problems.push(`色が ${poll.freeColor} へ移らない（今 ${me()?.color}）`);
      clearInterval(poll);
      finish();
    }
  }
}, 20);

function finish() {
  if (me()?.color === cpus()[0]?.color) {
    problems.push("埋まっている色を取れてしまった");
  }
  host.socket.close();
  if (problems.length > 0) {
    console.error(JSON.stringify({ error: problems }));
    process.exit(1);
  }
  console.log(
    JSON.stringify({
      levelsAccepted: STEPS.length,
      finalLevel: cpus()[0]?.cpu_level ?? null,
      colorMoved: true,
    }),
  );
  process.exit(0);
}

setTimeout(() => {
  clearInterval(poll);
  host.socket.close();
  console.error(
    JSON.stringify({
      error: "CPUの強さと色が届かなかった",
      stage,
      stepIndex: index,
      players: host.players.map((p) => ({ id: p.id, cpu_level: p.cpu_level, color: p.color })),
      phase: host.phase,
      problems,
    }),
  );
  process.exit(1);
}, 15000);
