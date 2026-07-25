// PIXEL_SHOOTER_RECONNECT_GRACE_SECONDS=1 で起動したサーバーに対し、
// 猶予切れ後に残ったプレイヤーが途中離脱勝利になることを確認する。
const SERVER_URL = process.env.PIXEL_SHOOTER_SERVER_URL ?? "ws://127.0.0.1:9001";

function connect(name) {
  const state = { socket: new WebSocket(SERVER_URL), id: 0, phase: "" };
  state.socket.addEventListener("open", () => {
    state.socket.send(
      JSON.stringify({ type: "join", name, reconnect_token: "" }),
    );
  });
  state.socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.type === "welcome") {
      state.id = message.player_id;
    } else if (message.type === "snapshot") {
      state.phase = message.phase;
      if (
        state === survivor &&
        message.phase === "match_finished" &&
        message.winner_id === survivor.id
      ) {
        quitter.socket.close();
        survivor.socket.close();
        console.log(
          JSON.stringify({ paused: sawPaused, forfeitWinner: survivor.id }),
        );
        process.exit(sawPaused ? 0 : 1);
      }
    }
  });
  return state;
}

let sawPaused = false;
const quitter = connect("ForfeitTest1");
const survivor = connect("ForfeitTest2");

const poll = setInterval(() => {
  if (quitter.id && survivor.id && survivor.phase === "countdown") {
    quitter.socket.close();
  }
  if (survivor.phase === "paused") sawPaused = true;
}, 50);

setTimeout(() => {
  clearInterval(poll);
  quitter.socket.close();
  survivor.socket.close();
  console.error(JSON.stringify({ paused: sawPaused, forfeitWinner: null }));
  process.exit(1);
}, 5000);
