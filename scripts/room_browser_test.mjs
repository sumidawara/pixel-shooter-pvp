// 一覧から選んだ部屋へ、実際に入れることを確認する。
//
// 一覧に出る行には2種類ある。既に部屋が立っているものと、まだ空のプールのサーバー。
// 後者へ直接繋ぐと「この GameServer には割り当てられたルームがありません」と
// 断られる。一覧には出るのに入れない、という状態が実際に起きた。
//
// ロビーに「この部屋の入場券」を出させてから繋ぐ経路を確かめる。選んだ部屋と
// 違う所へ案内されないことも見る。案内先が変わるなら、一覧を見た意味が無い。

const LOBBY_URL = process.env.PIXEL_SHOOTER_MATCHMAKER_URL ?? "http://127.0.0.1:8080";

const problems = [];

function check(condition, message) {
  if (!condition) {
    problems.push(message);
  }
}

async function rooms() {
  const response = await fetch(`${LOBBY_URL}/v1/rooms`);
  if (!response.ok) {
    throw new Error(`ルーム一覧が取れない: ${response.status}`);
  }
  return (await response.json()).rooms;
}

/// 指定した部屋の入場券をロビーへ求める。
async function ticketFor(gameUrl, playerName) {
  const response = await fetch(`${LOBBY_URL}/v1/matchmake`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ player_name: playerName, game_url: gameUrl }),
  });
  return { status: response.status, body: response.ok ? await response.json() : null };
}

/// 入場券を持って部屋へ入る。返るのは最初に届いたメッセージ。
function join(gameUrl, playerName, joinTicket) {
  return new Promise((resolve) => {
    const socket = new WebSocket(gameUrl);
    const timer = setTimeout(() => {
      socket.close();
      resolve({ type: "timeout" });
    }, 5000);
    socket.addEventListener("open", () => {
      socket.send(JSON.stringify({ type: "join", name: playerName, join_ticket: joinTicket }));
    });
    socket.addEventListener("message", (event) => {
      clearTimeout(timer);
      const message = JSON.parse(event.data);
      socket.close();
      resolve(message);
    });
    socket.addEventListener("error", () => {
      clearTimeout(timer);
      resolve({ type: "error" });
    });
  });
}

const listed = await rooms();
check(listed.length >= 2, `一覧に2部屋以上必要（今 ${listed.length}）`);
check(
  listed.every((room) => !("control_url" in room) && !("server_id" in room)),
  "一覧に制御面の情報が混ざっている",
);

if (listed.length >= 2) {
  // 1台目ではなく2台目をわざと選ぶ。自動で選び直されると1台目になり、
  // 「たまたま合っていた」で通ってしまう。
  const target = listed[1].game_url;
  const { status, body } = await ticketFor(target, "RoomBrowser");
  if (status !== 200 || body === null) {
    problems.push(`選んだ部屋の入場券が出ない: ${status}`);
  } else {
    check(
      body.game_url === target,
      `違う部屋へ案内された: ${body.game_url}（選んだのは ${target}）`,
    );
    const welcome = await join(body.game_url, "RoomBrowser", body.join_ticket);
    check(
      welcome.type === "welcome",
      `選んだ部屋へ入れない: ${welcome.type} ${welcome.reason ?? ""}`,
    );
  }
}

// 存在しない部屋を指したら、黙って別の部屋へ案内せずに断ること。
const missing = await ticketFor("ws://127.0.0.1:65535", "RoomBrowser");
check(missing.status === 404, `無い部屋を指しても断らない: ${missing.status}`);

if (problems.length > 0) {
  console.error(JSON.stringify({ error: problems }));
  process.exit(1);
}
console.log(JSON.stringify({ rooms: listed.length, chosenRoomJoined: true }));
