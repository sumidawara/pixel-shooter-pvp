// PIXEL_SHOOTER_RECONNECT_GRACE_SECONDS=1 で起動したサーバーに対し、
// 最後の人間が切断したCPU戦が空のルームへ戻り、再入室できることを確認する。
const SERVER_URL =
  process.env.PIXEL_SHOOTER_SERVER_URL ?? "ws://127.0.0.1:9001";

const host = {
  socket: new WebSocket(SERVER_URL),
  id: 0,
  phase: "",
  sawCpu: false,
};
const observer = {
  socket: new WebSocket(SERVER_URL),
  id: 0,
  phase: "",
  players: [],
  hostPlayerId: null,
  canStart: false,
};

host.socket.addEventListener("open", () => {
  host.socket.send(
    JSON.stringify({
      type: "join",
      name: "CpuOrphanHost",
      reconnect_token: "",
    }),
  );
});
host.socket.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (message.type === "welcome") {
    host.id = message.player_id;
  } else if (message.type === "snapshot") {
    host.phase = message.phase;
    host.sawCpu ||= message.players.some((player) => player.is_cpu);
  }
});

observer.socket.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (message.type === "welcome") {
    observer.id = message.player_id;
  } else if (message.type === "snapshot") {
    observer.phase = message.phase;
    observer.players = message.players;
    observer.hostPlayerId = message.room.host_player_id;
    observer.canStart = message.room.can_start;
  }
});

let startSent = false;
let hostClosed = false;
let rejoinSent = false;

const poll = setInterval(() => {
  if (!startSent && host.id && host.phase === "waiting") {
    startSent = true;
    host.socket.send(JSON.stringify({ type: "start_match" }));
  }

  if (!hostClosed && host.phase === "countdown" && host.sawCpu) {
    hostClosed = true;
    host.socket.close();
  }

  if (
    hostClosed &&
    !rejoinSent &&
    observer.phase === "waiting" &&
    observer.players.length === 0 &&
    observer.hostPlayerId === null &&
    observer.canStart === false
  ) {
    rejoinSent = true;
    observer.socket.send(
      JSON.stringify({
        type: "join",
        name: "ReplacementHost",
        reconnect_token: "",
      }),
    );
  }

  if (
    rejoinSent &&
    observer.id &&
    observer.phase === "waiting" &&
    observer.players.length === 1 &&
    observer.players[0].id === observer.id &&
    observer.hostPlayerId === observer.id &&
    observer.canStart
  ) {
    clearInterval(poll);
    observer.socket.close();
    console.log(
      JSON.stringify({
        cpuRemoved: true,
        emptyRoomObserved: true,
        replacementHost: observer.id,
        canStart: observer.canStart,
      }),
    );
    process.exit(0);
  }
}, 20);

setTimeout(() => {
  clearInterval(poll);
  host.socket.close();
  observer.socket.close();
  console.error(
    JSON.stringify({
      error: "CPU-only room did not reset or accept a replacement host",
      startSent,
      hostClosed,
      rejoinSent,
      observer: {
        id: observer.id,
        phase: observer.phase,
        players: observer.players.length,
        hostPlayerId: observer.hostPlayerId,
        canStart: observer.canStart,
      },
    }),
  );
  process.exit(1);
}, 6000);
