export type Vec2 = {
  x: number;
  y: number;
};

export type Player = {
  id: number;
  name: string;
  position: Vec2;
  aim: Vec2;
  hp: number;
  max_hp: number;
  score: number;
  is_cpu: boolean;
  connected: boolean;
  reconnect_grace_left: number;
  alive: boolean;
  respawn_left: number;
  invulnerable_left: number;
  ammo: number;
  max_ammo: number;
  reloading: boolean;
  reload_left: number;
  dash_cooldown_left: number;
  dashing: boolean;
  dash_time_left: number;
  last_input_sequence: number;
};

export type Bullet = {
  id: number;
  owner_id: number;
  position: Vec2;
  velocity: Vec2;
};

export type ScoreItem = {
  id: number;
  position: Vec2;
  points: number;
};

export type RoomSettings = {
  match_seconds: number;
  kill_points: number;
  death_penalty: number;
  item_points: number;
  item_spawn_interval: number;
  max_items: number;
};

export type Snapshot = {
  tick: number;
  phase: "waiting" | "countdown" | "running" | "paused" | "match_finished";
  time_left: number;
  winner_id: number | null;
  reconnect_grace_left: number;
  move_speed: number;
  dash_speed: number;
  dash_duration: number;
  dash_cooldown: number;
  players: Player[];
  bullets: Bullet[];
  items: ScoreItem[];
  room: {
    host_player_id: number | null;
    can_start: boolean;
    max_players: number;
    settings: RoomSettings;
  };
};

export type SnapshotEnvelope = Snapshot & {
  type: "snapshot";
};

export type GameServer = {
  server_id: string;
  public_url: string;
  control_url: string;
  status: "available" | "allocated";
  room_id: string | null;
  player_count: number;
  tick: number;
  simulation_mode: "realtime" | "paused";
  healthy: boolean;
};
