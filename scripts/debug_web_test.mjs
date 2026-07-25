// Svelteデバッグ画面と読み取り専用Snapshot APIの統合確認。
const SERVER_URL =
  process.env.PIXEL_SHOOTER_SERVER_URL ?? "ws://127.0.0.1:9001";
const DEBUG_URL =
  process.env.PIXEL_SHOOTER_DEBUG_URL ?? "http://127.0.0.1:9101";

const socket = new WebSocket(SERVER_URL);
let playerId = 0;
let startSent = false;

socket.addEventListener("open", () => {
  socket.send(
    JSON.stringify({
      type: "join",
      name: "DebugWebTest",
      reconnect_token: "",
    }),
  );
});

socket.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (message.type === "welcome") {
    playerId = message.player_id;
  } else if (
    message.type === "snapshot" &&
    message.phase === "waiting" &&
    playerId &&
    !startSent
  ) {
    startSent = true;
    socket.send(JSON.stringify({ type: "start_match" }));
  }
});

const deadline = Date.now() + 6000;
while (Date.now() < deadline) {
  const [pageResponse, healthResponse, stateResponse] = await Promise.all([
    fetch(`${DEBUG_URL}/debug/`),
    fetch(`${DEBUG_URL}/debug/api/health`),
    fetch(`${DEBUG_URL}/debug/api/state`),
  ]);
  if (pageResponse.ok && healthResponse.ok && stateResponse.ok) {
    const [page, health, state] = await Promise.all([
      pageResponse.text(),
      healthResponse.json(),
      stateResponse.json(),
    ]);
    const observedPlayer = state.players?.find(
      (player) => player.id === playerId,
    );
    if (
      page.includes("Server Observer") &&
      health.status === "ok" &&
      health.read_only === true &&
      state.type === "snapshot" &&
      observedPlayer &&
      ["countdown", "running"].includes(state.phase)
    ) {
      socket.close();
      console.log(
        JSON.stringify({
          page: true,
          health: true,
          readOnly: true,
          phase: state.phase,
          playerId,
          tick: state.tick,
        }),
      );
      process.exit(0);
    }
  }
  await new Promise((resolve) => setTimeout(resolve, 100));
}

socket.close();
console.error(
  JSON.stringify({
    error: "Debug page did not expose the live match in time",
    playerId,
    startSent,
  }),
);
process.exit(1);
