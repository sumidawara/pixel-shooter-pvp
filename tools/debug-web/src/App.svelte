<script lang="ts">
  import { onMount } from "svelte";
  import { drawArena } from "./arena";
  import type { GameServer, Player, SnapshotEnvelope } from "./types";

  const POLL_INTERVAL_MS = 200;
  const phaseLabels: Record<string, string> = {
    waiting: "WAITING",
    countdown: "COUNTDOWN",
    running: "RUNNING",
    paused: "PAUSED",
    match_finished: "FINISHED",
  };

  let snapshot = $state<SnapshotEnvelope | null>(null);
  let connected = $state(false);
  let errorMessage = $state("");
  let updatedAt = $state<Date | null>(null);
  let canvas = $state<HTMLCanvasElement>();
  let servers = $state<GameServer[]>([]);
  let selectedServerId = $state("");
  let controlBusy = $state(false);

  let sortedPlayers = $derived(
    snapshot ? [...snapshot.players].sort((left, right) => left.id - right.id) : [],
  );
  let activePlayers = $derived(
    snapshot?.players.filter((player) => player.is_cpu || player.connected).length ?? 0,
  );
  let selectedServer = $derived(
    servers.find((server) => server.server_id === selectedServerId) ?? null,
  );

  $effect(() => {
    if (canvas && snapshot) drawArena(canvas, snapshot);
  });

  onMount(() => {
    let disposed = false;
    let requestInFlight = false;

    const loadServers = async () => {
      try {
        const response = await fetch("/api/servers", { cache: "no-store" });
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        const nextServers = (await response.json()) as GameServer[];
        if (!disposed) {
          servers = nextServers;
          if (!nextServers.some((server) => server.server_id === selectedServerId)) {
            selectedServerId = nextServers[0]?.server_id ?? "";
            snapshot = null;
          }
        }
      } catch {
        if (!disposed) servers = [];
      }
    };

    const loadSnapshot = async () => {
      if (!selectedServerId) return;
      if (requestInFlight) return;
      requestInFlight = true;
      try {
        const response = await fetch(
          `/debug/api/state?server_id=${encodeURIComponent(selectedServerId)}`,
          { cache: "no-store" },
        );
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        const nextSnapshot = (await response.json()) as SnapshotEnvelope;
        if (!disposed) {
          snapshot = nextSnapshot;
          connected = true;
          errorMessage = "";
          updatedAt = new Date();
        }
      } catch (error) {
        if (!disposed) {
          connected = false;
          errorMessage = error instanceof Error ? error.message : "Unknown error";
        }
      } finally {
        requestInFlight = false;
      }
    };

    loadServers();
    const serverTimer = window.setInterval(loadServers, 1000);
    const timer = window.setInterval(loadSnapshot, POLL_INTERVAL_MS);
    return () => {
      disposed = true;
      window.clearInterval(serverTimer);
      window.clearInterval(timer);
    };
  });

  const controlSimulation = async (action: "pause" | "step" | "resume") => {
    if (!selectedServerId || controlBusy) return;
    controlBusy = true;
    try {
      const response = await fetch(`/api/servers/${encodeURIComponent(selectedServerId)}/${action}`, {
        method: "POST",
        headers: action === "step" ? { "content-type": "application/json" } : undefined,
        body: action === "step" ? JSON.stringify({ ticks: 1 }) : undefined,
      });
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      errorMessage = "";
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : "Control failed";
    } finally {
      controlBusy = false;
    }
  };

  const playerStatus = (player: Player): string => {
    if (!player.connected && !player.is_cpu) return "DISCONNECTED";
    if (!player.alive) return `RESPAWN ${player.respawn_left.toFixed(1)}s`;
    if (player.reloading) return `RELOAD ${player.reload_left.toFixed(1)}s`;
    if (player.dashing) return "DASHING";
    return "ACTIVE";
  };

  const formatTime = (seconds: number): string => {
    const safeSeconds = Math.max(seconds, 0);
    const minutes = Math.floor(safeSeconds / 60);
    const remainder = Math.floor(safeSeconds % 60);
    return `${minutes}:${remainder.toString().padStart(2, "0")}`;
  };
</script>

<svelte:head>
  <title>Pixel Shooter // Server Observer</title>
</svelte:head>

<main>
  <header class="topbar">
    <div class="identity">
      <span class="eyebrow">PIXEL SHOOTER PVP</span>
      <h1>SERVER OBSERVER</h1>
    </div>
    <div class="connection" class:offline={!connected}>
      <span class="status-dot"></span>
      <div>
        <strong>{connected ? "LIVE" : "OFFLINE"}</strong>
        <small>
          {connected && updatedAt
            ? `SYNC ${updatedAt.toLocaleTimeString()}`
            : errorMessage || "WAITING FOR SERVER"}
        </small>
      </div>
    </div>
  </header>

  <section class="server-controls" aria-label="Game server controls">
    <label>
      <span>GAME SERVER</span>
      <select bind:value={selectedServerId}>
        {#each servers as server}
          <option value={server.server_id}>
            {server.server_id} · {server.status.toUpperCase()} · {server.player_count}/4
          </option>
        {/each}
      </select>
    </label>
    <div class="server-meta">
      <span class:healthy={selectedServer?.healthy}>
        {selectedServer?.healthy ? "HEALTHY" : "UNAVAILABLE"}
      </span>
      <strong>{selectedServer?.room_id ?? "NO ROOM"}</strong>
      <small>{selectedServer?.simulation_mode.toUpperCase() ?? "—"}</small>
    </div>
    <div class="control-buttons">
      <button disabled={!selectedServer || controlBusy} onclick={() => controlSimulation("pause")}>PAUSE</button>
      <button disabled={!selectedServer || controlBusy || selectedServer?.simulation_mode !== "paused"} onclick={() => controlSimulation("step")}>STEP +1</button>
      <button disabled={!selectedServer || controlBusy} onclick={() => controlSimulation("resume")}>RESUME</button>
    </div>
  </section>

  {#if snapshot}
    <section class="metrics" aria-label="Server metrics">
      <article>
        <span>PHASE</span>
        <strong class:cyan={snapshot.phase === "running"}>
          {phaseLabels[snapshot.phase] ?? snapshot.phase.toUpperCase()}
        </strong>
      </article>
      <article>
        <span>TIME LEFT</span>
        <strong>{formatTime(snapshot.time_left)}</strong>
      </article>
      <article>
        <span>SERVER TICK</span>
        <strong>{snapshot.tick.toLocaleString()}</strong>
      </article>
      <article>
        <span>PLAYERS</span>
        <strong>{activePlayers}<em>/{snapshot.room.max_players}</em></strong>
      </article>
      <article>
        <span>ENTITIES</span>
        <strong>{snapshot.bullets.length + snapshot.items.length}<em> LIVE</em></strong>
      </article>
    </section>

    <section class="workspace">
      <div class="arena-panel panel">
        <div class="panel-heading">
          <div>
            <span class="eyebrow">AUTHORITATIVE WORLD</span>
            <h2>ARENA 640 × 360</h2>
          </div>
          <div class="legend" aria-label="Map legend">
            <span><i class="player-mark"></i>PLAYER</span>
            <span><i class="bullet-mark"></i>BULLET</span>
            <span><i class="item-mark"></i>ITEM</span>
          </div>
        </div>
        <div class="canvas-frame">
          <canvas bind:this={canvas} width="640" height="360" aria-label="Live arena map"></canvas>
          <span class="corner top-left"></span>
          <span class="corner top-right"></span>
          <span class="corner bottom-left"></span>
          <span class="corner bottom-right"></span>
        </div>
      </div>

      <aside class="side-column">
        <section class="panel room-panel">
          <div class="panel-heading compact">
            <div>
              <span class="eyebrow">ROOM STATE</span>
              <h2>SESSION</h2>
            </div>
            <span class:ready={snapshot.room.can_start} class="tag">
              {snapshot.room.can_start ? "READY" : "LOCKED"}
            </span>
          </div>
          <dl class="key-values">
            <div><dt>HOST ID</dt><dd>{snapshot.room.host_player_id ?? "—"}</dd></div>
            <div><dt>WINNER ID</dt><dd>{snapshot.winner_id ?? "—"}</dd></div>
            <div><dt>MATCH</dt><dd>{snapshot.room.settings.match_seconds}s</dd></div>
            <div><dt>KILL / DEATH</dt><dd>+{snapshot.room.settings.kill_points} / −{snapshot.room.settings.death_penalty}</dd></div>
            <div><dt>ITEM</dt><dd>+{snapshot.room.settings.item_points}</dd></div>
            <div><dt>MOVE / DASH</dt><dd>{snapshot.move_speed} / {snapshot.dash_speed}</dd></div>
          </dl>
        </section>

        <section class="panel entity-panel">
          <div class="panel-heading compact">
            <div>
              <span class="eyebrow">ENTITY COUNTS</span>
              <h2>WORLD LOAD</h2>
            </div>
          </div>
          <div class="entity-counts">
            <div><span>PLAYERS</span><strong>{snapshot.players.length}</strong></div>
            <div><span>BULLETS</span><strong>{snapshot.bullets.length}</strong></div>
            <div><span>ITEMS</span><strong>{snapshot.items.length}</strong></div>
          </div>
        </section>
      </aside>
    </section>

    <section class="panel players-panel">
      <div class="panel-heading">
        <div>
          <span class="eyebrow">CONNECTED ENTITIES</span>
          <h2>PLAYERS</h2>
        </div>
      </div>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>ID</th>
              <th>NAME</th>
              <th>TYPE</th>
              <th>STATE</th>
              <th>HP</th>
              <th>SCORE</th>
              <th>AMMO</th>
              <th>POSITION</th>
              <th>INPUT ACK</th>
            </tr>
          </thead>
          <tbody>
            {#each sortedPlayers as player}
              <tr class:disconnected={!player.connected && !player.is_cpu}>
                <td class="mono">#{player.id}</td>
                <td>
                  <strong>{player.name}</strong>
                  {#if player.id === snapshot.room.host_player_id}<span class="host-tag">HOST</span>{/if}
                </td>
                <td>{player.is_cpu ? "CPU" : "HUMAN"}</td>
                <td><span class="state-label">{playerStatus(player)}</span></td>
                <td>
                  <div class="hp-cell">
                    <span>{Math.max(player.hp, 0)}/{player.max_hp}</span>
                    <i><b style={`width:${Math.max(player.hp, 0) / player.max_hp * 100}%`}></b></i>
                  </div>
                </td>
                <td class="score">{player.score}</td>
                <td>{player.ammo}/{player.max_ammo}</td>
                <td class="mono">{player.position.x.toFixed(1)}, {player.position.y.toFixed(1)}</td>
                <td class="mono">{player.is_cpu ? "SERVER" : player.last_input_sequence}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </section>

    <details class="panel raw-panel">
      <summary>
        <span>RAW SNAPSHOT</span>
        <small>JSON · READ ONLY</small>
      </summary>
      <pre>{JSON.stringify(snapshot, null, 2)}</pre>
    </details>
  {:else}
    <section class="empty-state">
      <span class="loader" aria-hidden="true"></span>
      <p>CONNECTING TO DEBUG ENDPOINT</p>
      <small>Start the Rust server and keep this page open.</small>
    </section>
  {/if}

  <footer>
    <span>ADMIN CONTROL SURFACE</span>
    <span>SNAPSHOT POLL · {POLL_INTERVAL_MS}ms</span>
  </footer>
</main>
