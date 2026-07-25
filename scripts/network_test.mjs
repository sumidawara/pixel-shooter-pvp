// 起動中のサーバーへ2クライアントを接続し、通信と第2段階の状態を確認する。
// 例:
// PIXEL_SHOOTER_LATENCY_MS=120 PIXEL_SHOOTER_PACKET_LOSS_PERCENT=20 \
//   cargo run -p pixel-shooter-server
// node scripts/network_test.mjs

const TEST_SECONDS = 8;
const SERVER_URL = process.env.PIXEL_SHOOTER_SERVER_URL ?? "ws://127.0.0.1:9001";
const clients = [];
let finished = false;

for (let index = 0; index < 2; index += 1) {
  const client = {
    index,
    socket: new WebSocket(SERVER_URL),
    id: 0,
    sequence: 0,
    snapshots: 0,
    runningSnapshots: 0,
    firstSnapshotAt: 0,
    lastSnapshotAt: 0,
    maxGapMs: 0,
    sawStageTwoFields: false,
    sawAmmoDecrease: false,
    sawReload: false,
    sawDashCooldown: false,
    sawInvulnerability: false,
    sawBulletVelocity: false,
    sawCountdown: false,
  };
  clients.push(client);

  client.socket.addEventListener("open", () => {
    client.socket.send(
      JSON.stringify({ type: "join", name: `NetworkTest${index + 1}` }),
    );
  });

  client.socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.type === "welcome") {
      client.id = message.player_id;
      return;
    }
    if (message.type !== "snapshot") return;

    const now = performance.now();
    if (client.lastSnapshotAt > 0) {
      client.maxGapMs = Math.max(client.maxGapMs, now - client.lastSnapshotAt);
    } else {
      client.firstSnapshotAt = now;
    }
    client.lastSnapshotAt = now;
    client.snapshots += 1;
    if (message.phase === "countdown") client.sawCountdown = true;
    if (message.phase === "running") client.runningSnapshots += 1;
    const me = message.players.find((player) => player.id === client.id);
    client.sawStageTwoFields ||= Boolean(
      me &&
        "ammo" in me &&
        "reload_left" in me &&
        "invulnerable_left" in me &&
        "dash_cooldown_left" in me,
    );
    if (me) {
      client.sawAmmoDecrease ||= me.ammo < me.max_ammo;
      client.sawReload ||= me.reloading;
      client.sawDashCooldown ||= me.dash_cooldown_left > 0;
      client.sawInvulnerability ||= me.invulnerable_left > 0;
    }
    client.sawBulletVelocity ||= message.bullets.some(
      (bullet) => bullet.velocity && Number.isFinite(bullet.velocity.x),
    );
  });
}

const inputTimer = setInterval(() => {
  for (const client of clients) {
    if (!client.id || client.socket.readyState !== WebSocket.OPEN) continue;
    client.sequence += 1;
    const direction = client.index === 0 ? 1 : -1;
    client.socket.send(
      JSON.stringify({
        type: "input",
        sequence: client.sequence,
        move_x: direction,
        move_y: 0,
        aim_x: direction,
        aim_y: 0,
        shooting: true,
        reload_pressed: client.sequence === 45,
        dash_pressed: client.sequence === 10,
      }),
    );
  }
}, 1000 / 60);

setTimeout(() => {
  if (finished) return;
  finished = true;
  clearInterval(inputTimer);

  let failed = false;
  for (const client of clients) {
    client.socket.close();
    const result = {
      client: client.index + 1,
      snapshots: client.snapshots,
      runningSnapshots: client.runningSnapshots,
      maxGapMs: Math.round(client.maxGapMs),
      stageTwoFields: client.sawStageTwoFields,
      ammoDecrease: client.sawAmmoDecrease,
      reload: client.sawReload,
      dash: client.sawDashCooldown,
      invulnerability: client.sawInvulnerability,
      bulletVelocity: client.sawBulletVelocity,
      countdown: client.sawCountdown,
    };
    console.log(JSON.stringify(result));
    if (
      client.runningSnapshots === 0 ||
      client.snapshots < 5 ||
      !client.sawStageTwoFields ||
      !client.sawAmmoDecrease ||
      !client.sawReload ||
      !client.sawDashCooldown ||
      !client.sawInvulnerability ||
      !client.sawBulletVelocity ||
      !client.sawCountdown
    ) {
      failed = true;
    }
  }
  process.exit(failed ? 1 : 0);
}, TEST_SECONDS * 1000);

setTimeout(() => {
  console.error("Network test timed out.");
  process.exit(1);
}, (TEST_SECONDS + 2) * 1000);
