// ロビー退出が再接続猶予を待たず、次のSnapshotで反映されることを確認する。
const SERVER_URL =
  process.env.PIXEL_SHOOTER_SERVER_URL ?? "ws://127.0.0.1:9001";
const MAX_REFLECTION_MS = 750;

function connect(name) {
  const state = {
    socket: new WebSocket(SERVER_URL),
    id: 0,
    latestPlayers: [],
  };
  state.socket.addEventListener("open", () => {
    state.socket.send(
      JSON.stringify({
        type: "join",
        name,
        reconnect_token: "",
      }),
    );
  });
  state.socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.type === "welcome") {
      state.id = message.player_id;
    } else if (message.type === "snapshot" && message.phase === "waiting") {
      state.latestPlayers = message.players;
    }
  });
  return state;
}

const host = connect("LobbyHost");
const guest = connect("LobbyGuest");
let guestClosedAt = 0;

const poll = setInterval(() => {
  if (
    !guestClosedAt &&
    host.id &&
    guest.id &&
    host.latestPlayers.length === 2
  ) {
    guestClosedAt = performance.now();
    guest.socket.close();
    return;
  }

  if (
    guestClosedAt &&
    host.latestPlayers.length === 1 &&
    host.latestPlayers[0].id === host.id
  ) {
    const leaveReflectedMs = Math.round(performance.now() - guestClosedAt);
    clearInterval(poll);
    host.socket.close();
    if (leaveReflectedMs > MAX_REFLECTION_MS) {
      console.error(
        JSON.stringify({
          leaveReflectedMs,
          maximumMs: MAX_REFLECTION_MS,
          players: host.latestPlayers.length,
        }),
      );
      process.exit(1);
    }
    console.log(
      JSON.stringify({
        leaveReflectedMs,
        players: host.latestPlayers.length,
      }),
    );
    process.exit(0);
  }
}, 10);

setTimeout(() => {
  clearInterval(poll);
  host.socket.close();
  guest.socket.close();
  console.error(
    JSON.stringify({
      error: "Lobby leave was not reflected in time",
      players: host.latestPlayers.length,
    }),
  );
  process.exit(1);
}, 4000);
