<script lang="ts">
  import { onMount } from "svelte";
  import { drawArena } from "./arena";
  import MapEditor from "./MapEditor.svelte";
  import type {
    ControlState,
    GameServer,
    InputScenario,
    Player,
    SnapshotEnvelope,
  } from "./types";

  const POLL_INTERVAL_MS = 200;
  const phaseLabels: Record<string, string> = {
    waiting: "WAITING",
    countdown: "COUNTDOWN",
    running: "RUNNING",
    paused: "PAUSED",
    match_finished: "FINISHED",
  };
  const exampleScenario: InputScenario = {
    schema_version: 1,
    name: "trained-policy-sample",
    frames: [
      {
        note: "model output at observation 0",
        inputs: [
          {
            player_id: 2,
            move_x: -1,
            aim_x: 1,
            shooting: true,
            reason: "enemy is inside the preferred engagement range",
            metadata: { confidence: 0.87, policy: "distance_keeper_v1" },
          },
        ],
      },
      {
        note: "model output at observation 1",
        inputs: [
          {
            player_id: 2,
            move_y: 1,
            aim_x: 1,
            shooting: true,
            reason: "strafe while line of sight is clear",
            metadata: { confidence: 0.74, policy: "distance_keeper_v1" },
          },
        ],
      },
    ],
  };

  let snapshot = $state<SnapshotEnvelope | null>(null);
  let connected = $state(false);
  let errorMessage = $state("");
  let updatedAt = $state<Date | null>(null);
  let canvas = $state<HTMLCanvasElement>();
  let servers = $state<GameServer[]>([]);
  let selectedServerId = $state("");
  let controlBusy = $state(false);
  let controlState = $state<ControlState | null>(null);
  let scenarioJson = $state(JSON.stringify(exampleScenario, null, 2));
  let activeView = $state<"observer" | "editor">("observer");
  let controlStateInFlight = false;

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

  const loadControlState = async () => {
    if (!selectedServerId || controlStateInFlight) return;
    controlStateInFlight = true;
    try {
      const response = await fetch(
        `/api/servers/${encodeURIComponent(selectedServerId)}/state`,
        { cache: "no-store" },
      );
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      controlState = (await response.json()) as ControlState;
    } catch {
      controlState = null;
    } finally {
      controlStateInFlight = false;
    }
  };

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
            controlState = null;
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
    loadControlState();
    const serverTimer = window.setInterval(loadServers, 1000);
    const timer = window.setInterval(loadSnapshot, POLL_INTERVAL_MS);
    const controlTimer = window.setInterval(loadControlState, POLL_INTERVAL_MS);
    return () => {
      disposed = true;
      window.clearInterval(serverTimer);
      window.clearInterval(timer);
      window.clearInterval(controlTimer);
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
      await new Promise((resolve) => window.setTimeout(resolve, 40));
      await loadControlState();
      errorMessage = "";
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : "Control failed";
    } finally {
      controlBusy = false;
    }
  };

  const loadInputScenario = async () => {
    if (!selectedServerId || controlBusy) return;
    controlBusy = true;
    try {
      const scenario = JSON.parse(scenarioJson) as InputScenario;
      const response = await fetch(
        `/api/servers/${encodeURIComponent(selectedServerId)}/input-scenario`,
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify(scenario),
        },
      );
      if (!response.ok) {
        const body = (await response.json().catch(() => ({}))) as { error?: string };
        throw new Error(body.error ?? `HTTP ${response.status}`);
      }
      controlState = (await response.json()) as ControlState;
      errorMessage = "";
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : "Invalid scenario";
    } finally {
      controlBusy = false;
    }
  };

  const clearInputScenario = async () => {
    if (!selectedServerId || controlBusy) return;
    controlBusy = true;
    try {
      const response = await fetch(
        `/api/servers/${encodeURIComponent(selectedServerId)}/input-scenario/clear`,
        { method: "POST" },
      );
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      controlState = (await response.json()) as ControlState;
      errorMessage = "";
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : "Clear failed";
    } finally {
      controlBusy = false;
    }
  };

  const selectServer = (serverId: string) => {
    if (serverId === selectedServerId) return;
    selectedServerId = serverId;
    snapshot = null;
    controlState = null;
    connected = false;
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
  <title>Pixel Shooter // Dev Console</title>
</svelte:head>

<main>
  <header class="topbar">
    <div class="identity">
      <span class="eyebrow">PIXEL SHOOTER PVP</span>
      <h1>DEV CONSOLE</h1>
    </div>
    <div class="connection" class:offline={activeView === "observer" && !connected}>
      <span class="status-dot"></span>
      <div>
        <strong>{activeView === "editor" ? "LOCAL" : connected ? "LIVE" : "OFFLINE"}</strong>
        <small>
          {activeView === "editor"
            ? "BROWSER WORKSPACE"
            : connected && updatedAt
            ? `SYNC ${updatedAt.toLocaleTimeString()}`
            : errorMessage || "WAITING FOR SERVER"}
        </small>
      </div>
    </div>
  </header>

  <nav class="mode-tabs" aria-label="Debug tools">
    <button
      class:active={activeView === "observer"}
      aria-pressed={activeView === "observer"}
      onclick={() => (activeView = "observer")}
    >
      SERVER OBSERVER
      <small>LIVE STATE</small>
    </button>
    <button
      class:active={activeView === "editor"}
      aria-pressed={activeView === "editor"}
      onclick={() => (activeView = "editor")}
    >
      MAP EDITOR
      <small>LOCAL DRAFT</small>
    </button>
  </nav>

  {#if activeView === "editor"}
    <MapEditor />
  {:else}
    <section class="panel selected-room-panel" aria-label="Selected room status">
      <div class="selected-room-heading">
        <div>
          <span class="eyebrow">SELECTED ROOM</span>
          <h2>{selectedServer?.room_id ?? "NO ACTIVE ROOM"}</h2>
          <p>
            {selectedServer?.server_id ?? "NO GAME SERVER"}
            <span class:healthy={selectedServer?.healthy}>
              {selectedServer?.healthy ? "HEALTHY" : "UNAVAILABLE"}
            </span>
          </p>
        </div>
        <div class="control-buttons">
          <button disabled={!selectedServer || controlBusy} onclick={() => controlSimulation("pause")}>PAUSE</button>
          <button disabled={!selectedServer || controlBusy || controlState?.simulation_mode !== "paused"} onclick={() => controlSimulation("step")}>STEP +1</button>
          <button disabled={!selectedServer || controlBusy} onclick={() => controlSimulation("resume")}>RESUME</button>
        </div>
      </div>
      <div class="metrics selected-room-metrics" aria-label="Selected room metrics">
        <article>
          <span>PHASE</span>
          <strong class:cyan={snapshot?.phase === "running"}>
            {snapshot ? phaseLabels[snapshot.phase] ?? snapshot.phase.toUpperCase() : "—"}
          </strong>
        </article>
        <article>
          <span>TIME LEFT</span>
          <strong>{snapshot ? formatTime(snapshot.time_left) : "—"}</strong>
        </article>
        <article>
          <span>SERVER TICK</span>
          <strong>{snapshot?.tick.toLocaleString() ?? controlState?.tick.toLocaleString() ?? "—"}</strong>
        </article>
        <article>
          <span>PLAYERS</span>
          <strong>
            {snapshot ? activePlayers : selectedServer?.player_count ?? 0}
            <em>/{snapshot?.room.max_players ?? 4}</em>
          </strong>
        </article>
        <article>
          <span>SIMULATION</span>
          <strong class="simulation-mode">
            {controlState?.simulation_mode.toUpperCase() ?? selectedServer?.simulation_mode.toUpperCase() ?? "—"}
          </strong>
        </article>
      </div>
    </section>

    <section class="panel rooms-overview" aria-label="All rooms overview">
      <div class="panel-heading">
        <div>
          <span class="eyebrow">FLEET OVERVIEW</span>
          <h2>ALL ROOMS</h2>
        </div>
        <span class="overview-count">{servers.length} GAME SERVERS</span>
      </div>
      <div class="room-card-grid">
        {#each servers as server}
          <button
            class="room-card"
            class:selected={server.server_id === selectedServerId}
            class:unhealthy={!server.healthy}
            aria-pressed={server.server_id === selectedServerId}
            onclick={() => selectServer(server.server_id)}
          >
            <span class="room-card-status">
              <i></i>
              {server.healthy ? server.status.toUpperCase() : "OFFLINE"}
            </span>
            <strong>{server.room_id ?? "AVAILABLE SLOT"}</strong>
            <small>{server.server_id}</small>
            <dl>
              <div><dt>PLAYERS</dt><dd>{server.player_count}/4</dd></div>
              <div><dt>MODE</dt><dd>{server.simulation_mode.toUpperCase()}</dd></div>
              <div><dt>TICK</dt><dd>{server.tick.toLocaleString()}</dd></div>
            </dl>
          </button>
        {:else}
          <p class="rooms-empty">NO GAME SERVERS REGISTERED</p>
        {/each}
      </div>
    </section>

  {#if snapshot}
    <div class="detail-section-heading">
      <div>
        <span class="eyebrow">SELECTED ROOM DETAIL</span>
        <h2>{selectedServer?.room_id ?? selectedServer?.server_id}</h2>
      </div>
      <small>AUTHORITATIVE SNAPSHOT</small>
    </div>

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

    <section class="panel scenario-panel">
      <div class="panel-heading">
        <div>
          <span class="eyebrow">MODEL INPUT INJECTION</span>
          <h2>INPUT SCENARIO</h2>
        </div>
        <span class="tag" class:ready={controlState?.input_scenario}>
          {controlState?.input_scenario ? "LOADED" : "IDLE"}
        </span>
      </div>
      <div class="scenario-grid">
        <div class="scenario-editor">
          <label for="scenario-json">ONE FRAME = ONE GAME TICK</label>
          <textarea id="scenario-json" bind:value={scenarioJson} spellcheck="false"></textarea>
          <div class="scenario-actions">
            <button disabled={!selectedServer || controlBusy} onclick={loadInputScenario}>
              LOAD + PAUSE
            </button>
            <button disabled={!controlState?.input_scenario || controlBusy} onclick={clearInputScenario}>
              CLEAR
            </button>
          </div>
          <small>
            POST /api/servers/{selectedServerId || "{server_id}"}/input-scenario
          </small>
        </div>
        <div class="scenario-inspector">
          {#if controlState?.input_scenario}
            <div class="scenario-progress">
              <span>{controlState.input_scenario.name}</span>
              <strong>
                {controlState.input_scenario.next_frame}/{controlState.input_scenario.total_frames}
              </strong>
              <progress
                max={controlState.input_scenario.total_frames}
                value={controlState.input_scenario.next_frame}
              ></progress>
            </div>
            {#if controlState.input_scenario.last_applied}
              <div class="applied-frame">
                <span class="eyebrow">
                  APPLIED FRAME #{controlState.input_scenario.last_applied.index}
                  · SERVER TICK {controlState.tick}
                </span>
                {#if controlState.input_scenario.last_applied.frame.note}
                  <p>{controlState.input_scenario.last_applied.frame.note}</p>
                {/if}
                {#each controlState.input_scenario.last_applied.frame.inputs as input}
                  <article>
                    <strong>PLAYER #{input.player_id}</strong>
                    <code>
                      MOVE {input.move_x ?? 0}, {input.move_y ?? 0}
                      · AIM {input.aim_x ?? 0}, {input.aim_y ?? 0}
                      · FIRE {input.shooting ? "ON" : "OFF"}
                    </code>
                    <p>{input.reason ?? "No model explanation supplied."}</p>
                    {#if input.metadata && Object.keys(input.metadata).length > 0}
                      <pre>{JSON.stringify(input.metadata, null, 2)}</pre>
                    {/if}
                  </article>
                {/each}
              </div>
            {:else}
              <p class="scenario-empty">Press STEP +1 to apply frame 0 and inspect its decision.</p>
            {/if}
          {:else}
            <p class="scenario-empty">
              Paste a trained policy's action sequence, load it, then advance one tick at a time.
            </p>
          {/if}
        </div>
      </div>
    </section>
  {:else}
    <section class="empty-state">
      <span class="loader" aria-hidden="true"></span>
      <p>CONNECTING TO DEBUG ENDPOINT</p>
      <small>Start the Rust server and keep this page open.</small>
    </section>
    {/if}
  {/if}

  <footer>
    <span>ADMIN CONTROL SURFACE</span>
    <span>SNAPSHOT POLL · {POLL_INTERVAL_MS}ms</span>
  </footer>
</main>
