// 切断中に試合が一時停止し、同じトークンで同じPlayerへ復帰できることを確認する。
const SERVER_URL = process.env.PIXEL_SHOOTER_SERVER_URL ?? "ws://127.0.0.1:9001";

function connect(name, reconnectToken = "") {
  const state = {
    socket: new WebSocket(SERVER_URL),
    id: 0,
    token: reconnectToken,
    reconnected: false,
    phases: new Set(),
    sawPaused: false,
    sawResumed: false,
  };
  state.socket.addEventListener("open", () => {
    state.socket.send(
      JSON.stringify({
        type: "join",
        name,
        reconnect_token: reconnectToken,
      }),
    );
  });
  state.socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.type === "welcome") {
      state.id = message.player_id;
      state.token = message.reconnect_token;
      state.reconnected = message.reconnected;
    } else if (message.type === "snapshot") {
      state.phases.add(message.phase);
      if (message.phase === "paused") {
        state.sawPaused = true;
      } else if (state.sawPaused) {
        state.sawResumed = true;
      }
    }
  });
  return state;
}

const first = connect("ReconnectTest1");
const second = connect("ReconnectTest2");
let replacement = null;
let originalId = 0;

const poll = setInterval(() => {
  if (!originalId && first.id && second.id && first.phases.has("countdown")) {
    originalId = first.id;
    first.socket.close();
    setTimeout(() => {
      replacement = connect("ReconnectTest1", first.token);
    }, 400);
  }

  if (
    replacement?.reconnected &&
    replacement.id === originalId &&
    second.sawPaused &&
    second.sawResumed
  ) {
    clearInterval(poll);
    replacement.socket.close();
    second.socket.close();
    console.log(
      JSON.stringify({
        samePlayerId: true,
        paused: true,
        reconnected: true,
      }),
    );
    process.exit(0);
  }
}, 50);

setTimeout(() => {
  clearInterval(poll);
  first.socket.close();
  second.socket.close();
  replacement?.socket.close();
  console.error(
    JSON.stringify({
      samePlayerId: replacement?.id === originalId,
      paused: second.sawPaused,
      resumed: second.sawResumed,
      reconnected: replacement?.reconnected ?? false,
    }),
  );
  process.exit(1);
}, 7000);
