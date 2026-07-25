import type { Snapshot } from "./types";

const PLAYER_COLORS = ["#27e5ff", "#ff38c7", "#ffe66d", "#7cff6b"];
const OBSTACLES = [
  { x: 250, y: 85, width: 140, height: 28 },
  { x: 250, y: 247, width: 140, height: 28 },
];

const playerColor = (playerId: number, snapshot: Snapshot): string => {
  const index = snapshot.players.findIndex((player) => player.id === playerId);
  return PLAYER_COLORS[Math.max(index, 0) % PLAYER_COLORS.length];
};

export const drawArena = (
  canvas: HTMLCanvasElement,
  snapshot: Snapshot,
): void => {
  const context = canvas.getContext("2d");
  if (!context) return;

  context.clearRect(0, 0, 640, 360);
  context.fillStyle = "#080d13";
  context.fillRect(0, 0, 640, 360);

  context.strokeStyle = "#dce7ef";
  context.lineWidth = 2;
  context.strokeRect(1, 1, 638, 358);

  context.fillStyle = "#151d27";
  context.strokeStyle = "#dce7ef";
  for (const obstacle of OBSTACLES) {
    context.fillRect(obstacle.x, obstacle.y, obstacle.width, obstacle.height);
    context.strokeRect(obstacle.x, obstacle.y, obstacle.width, obstacle.height);
  }

  for (const item of snapshot.items) {
    context.save();
    context.translate(item.position.x, item.position.y);
    context.rotate(Math.PI / 4);
    context.fillStyle = "#ffe66d";
    context.fillRect(-7, -7, 14, 14);
    context.restore();
  }

  for (const bullet of snapshot.bullets) {
    context.fillStyle = playerColor(bullet.owner_id, snapshot);
    context.fillRect(
      Math.round(bullet.position.x) - 2,
      Math.round(bullet.position.y) - 2,
      5,
      5,
    );
  }

  for (const player of snapshot.players) {
    const color = playerColor(player.id, snapshot);
    context.globalAlpha = player.connected && player.alive ? 1 : 0.35;

    context.strokeStyle = color;
    context.lineWidth = 2;
    context.beginPath();
    context.moveTo(
      player.position.x + player.aim.x * 7,
      player.position.y + player.aim.y * 7,
    );
    context.lineTo(
      player.position.x + player.aim.x * 25,
      player.position.y + player.aim.y * 25,
    );
    context.stroke();

    context.fillStyle = "#080d13";
    context.beginPath();
    context.arc(player.position.x, player.position.y, 11, 0, Math.PI * 2);
    context.fill();
    context.strokeStyle = color;
    context.lineWidth = player.id === snapshot.room.host_player_id ? 4 : 2;
    context.stroke();

    context.globalAlpha = 1;
    context.fillStyle = "#dce7ef";
    context.font = "10px ui-monospace, SFMono-Regular, Menlo, monospace";
    context.textAlign = "center";
    context.fillText(player.name, player.position.x, player.position.y - 18);

    const hpWidth = 28;
    context.fillStyle = "#222d39";
    context.fillRect(player.position.x - hpWidth / 2, player.position.y + 17, hpWidth, 3);
    context.fillStyle = color;
    context.fillRect(
      player.position.x - hpWidth / 2,
      player.position.y + 17,
      hpWidth * Math.max(player.hp, 0) / player.max_hp,
      3,
    );
  }
};
