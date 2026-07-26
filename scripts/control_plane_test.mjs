// AdminServer、Matchmaker、GameServerを起動した状態で制御面全体を検証する。
const MATCHMAKER_URL =
  process.env.PIXEL_SHOOTER_MATCHMAKER_URL ?? "http://127.0.0.1:8080";
const ADMIN_URL =
  process.env.PIXEL_SHOOTER_ADMIN_URL ?? "http://127.0.0.1:8081";

const delay = (milliseconds) =>
  new Promise((resolve) => setTimeout(resolve, milliseconds));

const requestJson = async (url, options = {}) => {
  const response = await fetch(url, options);
  const body = await response.json().catch(() => ({}));
  if (!response.ok) {
    throw new Error(`${url}: HTTP ${response.status} ${JSON.stringify(body)}`);
  }
  return body;
};

const matchmake = (playerName) =>
  requestJson(`${MATCHMAKER_URL}/v1/matchmake`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ player_name: playerName }),
  });

const join = (allocation, includeTicket = true) =>
  new Promise((resolve, reject) => {
    const socket = new WebSocket(allocation.game_url);
    const timeout = setTimeout(() => {
      socket.close();
      reject(new Error("WebSocket join timed out"));
    }, 3000);
    socket.addEventListener("open", () => {
      socket.send(
        JSON.stringify({
          type: "join",
          name: "IgnoredByTicket",
          join_ticket: includeTicket ? allocation.join_ticket : null,
        }),
      );
    });
    socket.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      if (message.type === "welcome" || message.type === "rejected") {
        clearTimeout(timeout);
        resolve({ socket, message });
      }
    });
    socket.addEventListener("error", () => {
      clearTimeout(timeout);
      reject(new Error("WebSocket connection failed"));
    });
  });

const postControl = (serverId, action, body) =>
  requestJson(`${ADMIN_URL}/api/servers/${serverId}/${action}`, {
    method: "POST",
    headers: body ? { "content-type": "application/json" } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  });

const state = (serverId) =>
  requestJson(`${ADMIN_URL}/api/servers/${serverId}/state`);

const first = await matchmake("Ticket Player 1");
const second = await matchmake("Ticket Player 2");
if (first.server_id !== second.server_id || first.room_id !== second.room_id) {
  throw new Error("Players were not assigned to the same available room");
}

const firstJoin = await join(first);
const secondJoin = await join(second);
if (firstJoin.message.type !== "welcome" || secondJoin.message.type !== "welcome") {
  throw new Error("Valid Join Ticket was rejected");
}

const invalidJoin = await join(first, false);
if (invalidJoin.message.type !== "rejected") {
  throw new Error("Ticket-less join was unexpectedly accepted");
}
invalidJoin.socket.close();

const paused = await postControl(first.server_id, "pause");
await delay(250);
const stillPaused = await state(first.server_id);
if (
  stillPaused.simulation_mode !== "paused" ||
  stillPaused.tick !== paused.tick
) {
  throw new Error("GameTick advanced while paused");
}

await postControl(first.server_id, "step", { ticks: 1 });
await delay(100);
const stepped = await state(first.server_id);
if (stepped.tick !== paused.tick + 1) {
  throw new Error(`Step advanced ${stepped.tick - paused.tick} ticks instead of one`);
}

await postControl(first.server_id, "resume");
await delay(150);
const resumed = await state(first.server_id);
if (
  resumed.simulation_mode !== "realtime" ||
  resumed.tick <= stepped.tick
) {
  throw new Error("GameTick did not resume");
}

firstJoin.socket.close();
secondJoin.socket.close();
console.log(
  JSON.stringify({
    allocation: {
      server_id: first.server_id,
      room_id: first.room_id,
      game_url: first.game_url,
    },
    tickets: "accepted",
    missing_ticket: "rejected",
    pause_tick: paused.tick,
    step_tick: stepped.tick,
    resume_tick: resumed.tick,
  }),
);
