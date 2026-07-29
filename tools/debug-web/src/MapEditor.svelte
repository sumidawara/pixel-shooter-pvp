<script lang="ts">
  import { onMount } from "svelte";
  import classicArenaSource from "../../../backend/maps/classic_arena.json";

  type Tile = "." | "#" | "X";
  type Brush =
    | "floor"
    | "solid"
    | "destructible"
    | "player_spawn"
    | "item_spawn"
    | "erase";
  type Point = [number, number];
  type MapDefinition = {
    schema_version: number;
    id: string;
    revision: string;
    name: string;
    width: number;
    height: number;
    tile_size: number;
    tiles: string[];
    spawn_points: Point[];
    item_spawn_points: Point[];
  };
  type ValidationIssue = {
    level: "error" | "warning";
    message: string;
  };

  const MAX_HISTORY = 60;
  const DRAFT_STORAGE_KEY = "pixel-shooter.arena-map-draft.v1";
  const brushes: Array<{ id: Brush; key: string; label: string; detail: string }> = [
    { id: "floor", key: "1", label: "FLOOR", detail: "." },
    { id: "solid", key: "2", label: "SOLID", detail: "#" },
    { id: "destructible", key: "3", label: "BREAKABLE", detail: "X" },
    { id: "player_spawn", key: "4", label: "PLAYER", detail: "S" },
    { id: "item_spawn", key: "5", label: "ITEM", detail: "I" },
    { id: "erase", key: "6", label: "ERASER", detail: "⌫" },
  ];

  const cloneMap = (source: MapDefinition): MapDefinition =>
    JSON.parse(JSON.stringify(source)) as MapDefinition;

  const normalizeMap = (value: unknown): MapDefinition => {
    if (!value || typeof value !== "object") throw new Error("Root must be a JSON object");
    const source = value as Partial<MapDefinition>;
    if (!Array.isArray(source.tiles)) throw new Error("tiles must be an array");
    if (!Array.isArray(source.spawn_points)) throw new Error("spawn_points must be an array");
    if (!Array.isArray(source.item_spawn_points)) {
      throw new Error("item_spawn_points must be an array");
    }
    return {
      schema_version: Number(source.schema_version),
      id: String(source.id ?? ""),
      revision: String(source.revision ?? ""),
      name: String(source.name ?? ""),
      width: Number(source.width),
      height: Number(source.height),
      tile_size: Number(source.tile_size),
      tiles: source.tiles.map(String),
      spawn_points: source.spawn_points.map((point) => [Number(point[0]), Number(point[1])]),
      item_spawn_points: source.item_spawn_points.map((point) => [
        Number(point[0]),
        Number(point[1]),
      ]),
    };
  };

  const pointKey = (x: number, y: number): string => `${x}:${y}`;
  const includesPoint = (points: Point[], x: number, y: number): boolean =>
    points.some(([pointX, pointY]) => pointX === x && pointY === y);

  const validateMap = (candidate: MapDefinition): ValidationIssue[] => {
    const issues: ValidationIssue[] = [];
    if (candidate.schema_version !== 1) {
      issues.push({ level: "error", message: "schema_version must be 1" });
    }
    if (!candidate.id.trim()) issues.push({ level: "error", message: "Map ID is required" });
    if (!candidate.name.trim()) issues.push({ level: "warning", message: "Display name is empty" });
    if (!candidate.revision.trim()) {
      issues.push({ level: "error", message: "Revision is required" });
    }
    if (
      !Number.isInteger(candidate.width) ||
      candidate.width < 1 ||
      candidate.width > 256 ||
      !Number.isInteger(candidate.height) ||
      candidate.height < 1 ||
      candidate.height > 256
    ) {
      issues.push({ level: "error", message: "Width and height must be integers from 1 to 256" });
    }
    if (
      !Number.isInteger(candidate.tile_size) ||
      candidate.tile_size < 8 ||
      candidate.tile_size > 128
    ) {
      issues.push({ level: "error", message: "Tile size must be an integer from 8 to 128" });
    }
    if (candidate.tiles.length !== candidate.height) {
      issues.push({
        level: "error",
        message: `Expected ${candidate.height} terrain rows, found ${candidate.tiles.length}`,
      });
    }
    candidate.tiles.forEach((row, y) => {
      if (row.length !== candidate.width) {
        issues.push({
          level: "error",
          message: `Row ${y} has ${row.length} cells; expected ${candidate.width}`,
        });
      }
      if (/[^.#X]/.test(row)) {
        issues.push({ level: "error", message: `Row ${y} contains an unknown tile` });
      }
    });

    const validatePoints = (points: Point[], label: string) => {
      const seen = new Set<string>();
      for (const [x, y] of points) {
        const key = pointKey(x, y);
        if (!Number.isInteger(x) || !Number.isInteger(y)) {
          issues.push({ level: "error", message: `${label} ${key} is not on the grid` });
        } else if (x < 0 || x >= candidate.width || y < 0 || y >= candidate.height) {
          issues.push({ level: "error", message: `${label} ${key} is outside the map` });
        } else if (candidate.tiles[y]?.[x] !== ".") {
          issues.push({ level: "error", message: `${label} ${key} overlaps a wall` });
        }
        if (seen.has(key)) {
          issues.push({ level: "error", message: `${label} ${key} is duplicated` });
        }
        seen.add(key);
      }
    };
    validatePoints(candidate.spawn_points, "Player spawn");
    validatePoints(candidate.item_spawn_points, "Item spawn");
    if (candidate.spawn_points.length < 4) {
      issues.push({
        level: "error",
        message: `Add ${4 - candidate.spawn_points.length} more player spawn(s)`,
      });
    }
    if (candidate.item_spawn_points.length < 1) {
      issues.push({ level: "error", message: "Add at least one item spawn" });
    }
    const borderOpen =
      candidate.tiles[0]?.includes(".") ||
      candidate.tiles[candidate.height - 1]?.includes(".") ||
      candidate.tiles.some((row) => row[0] === "." || row[candidate.width - 1] === ".");
    if (borderOpen) {
      issues.push({ level: "warning", message: "The outer border contains floor tiles" });
    }
    return issues;
  };

  const initialMap = normalizeMap(classicArenaSource);
  let map = $state<MapDefinition>(cloneMap(initialMap));
  let brush = $state<Brush>("solid");
  let undoStack = $state<string[]>([]);
  let redoStack = $state<string[]>([]);
  let painting = $state(false);
  let widthInput = $state(initialMap.width);
  let heightInput = $state(initialMap.height);
  let hoverCell = $state<Point | null>(null);
  let status = $state("BUILT-IN MAP LOADED");
  let fileInput = $state<HTMLInputElement>();
  let persistenceReady = $state(false);

  let issues = $derived(validateMap(map));
  let errorCount = $derived(issues.filter((issue) => issue.level === "error").length);
  let floorCount = $derived(map.tiles.reduce((total, row) => total + row.split(".").length - 1, 0));
  let wallCount = $derived(
    map.tiles.reduce((total, row) => total + [...row].filter((tile) => tile !== ".").length, 0),
  );

  $effect(() => {
    if (persistenceReady) localStorage.setItem(DRAFT_STORAGE_KEY, JSON.stringify(map));
  });

  const snapshot = (): string => JSON.stringify(map);
  const checkpoint = () => {
    undoStack.push(snapshot());
    if (undoStack.length > MAX_HISTORY) undoStack.shift();
    redoStack = [];
  };
  const restore = (serialized: string) => {
    map = normalizeMap(JSON.parse(serialized));
    widthInput = map.width;
    heightInput = map.height;
  };
  const undo = () => {
    const previous = undoStack.pop();
    if (!previous) return;
    redoStack.push(snapshot());
    restore(previous);
    status = "UNDO";
  };
  const redo = () => {
    const next = redoStack.pop();
    if (!next) return;
    undoStack.push(snapshot());
    restore(next);
    status = "REDO";
  };

  const setTerrain = (x: number, y: number, tile: Tile) => {
    const row = map.tiles[y] ?? ".".repeat(map.width);
    if (row[x] === tile) return;
    map.tiles[y] = `${row.slice(0, x)}${tile}${row.slice(x + 1)}`;
    if (tile !== ".") {
      map.spawn_points = map.spawn_points.filter(([pointX, pointY]) => pointX !== x || pointY !== y);
      map.item_spawn_points = map.item_spawn_points.filter(
        ([pointX, pointY]) => pointX !== x || pointY !== y,
      );
    }
  };

  const setMarker = (x: number, y: number, marker: "player_spawn" | "item_spawn") => {
    setTerrain(x, y, ".");
    map.spawn_points = map.spawn_points.filter(([pointX, pointY]) => pointX !== x || pointY !== y);
    map.item_spawn_points = map.item_spawn_points.filter(
      ([pointX, pointY]) => pointX !== x || pointY !== y,
    );
    if (marker === "player_spawn") map.spawn_points.push([x, y]);
    else map.item_spawn_points.push([x, y]);
  };

  const eraseCell = (x: number, y: number) => {
    setTerrain(x, y, ".");
    map.spawn_points = map.spawn_points.filter(([pointX, pointY]) => pointX !== x || pointY !== y);
    map.item_spawn_points = map.item_spawn_points.filter(
      ([pointX, pointY]) => pointX !== x || pointY !== y,
    );
  };

  const paintCell = (x: number, y: number, activeBrush = brush) => {
    if (activeBrush === "erase") eraseCell(x, y);
    else if (activeBrush === "floor") setTerrain(x, y, ".");
    else if (activeBrush === "solid") setTerrain(x, y, "#");
    else if (activeBrush === "destructible") setTerrain(x, y, "X");
    else setMarker(x, y, activeBrush);
  };

  const beginPaint = (event: PointerEvent, x: number, y: number) => {
    event.preventDefault();
    checkpoint();
    painting = true;
    paintCell(x, y, event.button === 2 ? "erase" : brush);
  };
  const continuePaint = (event: PointerEvent, x: number, y: number) => {
    hoverCell = [x, y];
    if (!painting || event.buttons === 0) return;
    paintCell(x, y, event.buttons === 2 ? "erase" : brush);
  };

  const resizeMap = () => {
    const width = Math.max(1, Math.min(256, Math.round(widthInput)));
    const height = Math.max(1, Math.min(256, Math.round(heightInput)));
    checkpoint();
    const rows = Array.from({ length: height }, (_, y) => {
      const existing = map.tiles[y] ?? "";
      return `${existing.slice(0, width)}${".".repeat(Math.max(0, width - existing.length))}`;
    });
    map.width = width;
    map.height = height;
    map.tiles = rows;
    map.spawn_points = map.spawn_points.filter(([x, y]) => x < width && y < height);
    map.item_spawn_points = map.item_spawn_points.filter(([x, y]) => x < width && y < height);
    widthInput = width;
    heightInput = height;
    status = `RESIZED TO ${width} × ${height}`;
  };

  const frameWalls = () => {
    checkpoint();
    for (let y = 0; y < map.height; y += 1) {
      for (let x = 0; x < map.width; x += 1) {
        if (x === 0 || y === 0 || x === map.width - 1 || y === map.height - 1) {
          setTerrain(x, y, "#");
        }
      }
    }
    status = "SOLID BORDER APPLIED";
  };

  const clearMap = () => {
    if (!window.confirm("Clear all terrain and spawn markers?")) return;
    checkpoint();
    map.tiles = Array.from({ length: map.height }, () => ".".repeat(map.width));
    map.spawn_points = [];
    map.item_spawn_points = [];
    status = "MAP CLEARED";
  };

  const resetDefault = () => {
    if (!window.confirm("Replace this draft with the checked-in Classic Arena?")) return;
    checkpoint();
    map = normalizeMap(classicArenaSource);
    widthInput = map.width;
    heightInput = map.height;
    status = "BUILT-IN MAP RESTORED";
  };

  const bumpRevision = () => {
    checkpoint();
    const numeric = Number.parseInt(map.revision, 10);
    map.revision = Number.isFinite(numeric) ? String(numeric + 1) : `${map.revision}-next`;
    status = `REVISION ${map.revision}`;
  };

  const importMap = async (event: Event) => {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    try {
      const imported = normalizeMap(JSON.parse(await file.text()));
      const importedIssues = validateMap(imported);
      if (importedIssues.some((issue) => issue.level === "error")) {
        throw new Error(importedIssues.find((issue) => issue.level === "error")?.message);
      }
      checkpoint();
      map = imported;
      widthInput = map.width;
      heightInput = map.height;
      status = `IMPORTED ${file.name.toUpperCase()}`;
    } catch (error) {
      status = `IMPORT FAILED · ${error instanceof Error ? error.message : "INVALID JSON"}`;
    } finally {
      input.value = "";
    }
  };

  const exportMap = () => {
    if (errorCount > 0) return;
    const data = `${JSON.stringify(map, null, 2)}\n`;
    const blob = new Blob([data], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = `${map.id.replace(/[^a-z0-9_-]/gi, "_") || "arena"}.json`;
    link.click();
    URL.revokeObjectURL(url);
    status = `EXPORTED ${link.download.toUpperCase()}`;
  };

  const copyMap = async () => {
    if (errorCount > 0) return;
    await navigator.clipboard.writeText(`${JSON.stringify(map, null, 2)}\n`);
    status = "JSON COPIED";
  };

  onMount(() => {
    const savedDraft = localStorage.getItem(DRAFT_STORAGE_KEY);
    if (savedDraft) {
      try {
        map = normalizeMap(JSON.parse(savedDraft));
        widthInput = map.width;
        heightInput = map.height;
        status = "LOCAL DRAFT RESTORED";
      } catch {
        localStorage.removeItem(DRAFT_STORAGE_KEY);
      }
    }
    persistenceReady = true;
    const endPaint = () => {
      painting = false;
    };
    const handleKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.matches("input, textarea, select")) return;
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "z") {
        event.preventDefault();
        if (event.shiftKey) redo();
        else undo();
        return;
      }
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "y") {
        event.preventDefault();
        redo();
        return;
      }
      const selected = brushes.find((candidate) => candidate.key === event.key);
      if (selected) brush = selected.id;
    };
    window.addEventListener("pointerup", endPaint);
    window.addEventListener("keydown", handleKey);
    return () => {
      window.removeEventListener("pointerup", endPaint);
      window.removeEventListener("keydown", handleKey);
    };
  });
</script>

<section class="editor-shell">
  <header class="editor-heading">
    <div>
      <span class="eyebrow">DATA-DRIVEN ARENA</span>
      <h2>MAP EDITOR</h2>
      <p>Paint server-authoritative terrain and spawn markers, then export the project JSON.</p>
    </div>
    <div class="editor-actions">
      <input
        bind:this={fileInput}
        class="file-input"
        type="file"
        accept="application/json,.json"
        onchange={importMap}
      />
      <button class="secondary" onclick={() => fileInput?.click()}>IMPORT JSON</button>
      <button class="secondary" disabled={errorCount > 0} onclick={copyMap}>COPY JSON</button>
      <button class="primary" disabled={errorCount > 0} onclick={exportMap}>
        EXPORT JSON
      </button>
    </div>
  </header>

  <div class="editor-layout">
    <aside class="editor-sidebar">
      <section class="editor-panel">
        <div class="section-label">MAP IDENTITY</div>
        <label>
          <span>ID</span>
          <input bind:value={map.id} spellcheck="false" />
        </label>
        <label>
          <span>DISPLAY NAME</span>
          <input bind:value={map.name} />
        </label>
        <div class="field-pair">
          <label>
            <span>REVISION</span>
            <input bind:value={map.revision} />
          </label>
          <button class="icon-action" title="Increment revision" onclick={bumpRevision}>+1</button>
        </div>
      </section>

      <section class="editor-panel">
        <div class="section-label">GRID SETTINGS</div>
        <div class="field-grid">
          <label>
            <span>WIDTH</span>
            <input type="number" min="1" max="256" bind:value={widthInput} />
          </label>
          <label>
            <span>HEIGHT</span>
            <input type="number" min="1" max="256" bind:value={heightInput} />
          </label>
          <label>
            <span>TILE PX</span>
            <input type="number" min="8" max="128" bind:value={map.tile_size} />
          </label>
          <button class="apply-size" onclick={resizeMap}>APPLY SIZE</button>
        </div>
      </section>

      <section class="editor-panel">
        <div class="section-label">BRUSH PALETTE</div>
        <div class="brushes">
          {#each brushes as candidate}
            <button
              class:active={brush === candidate.id}
              onclick={() => (brush = candidate.id)}
              aria-pressed={brush === candidate.id}
            >
              <kbd>{candidate.key}</kbd>
              <span>{candidate.label}</span>
              <strong>{candidate.detail}</strong>
            </button>
          {/each}
        </div>
        <small class="hint">Drag to paint · Right-click to erase · Keys 1–6 switch tools</small>
      </section>

      <section class="editor-panel utility-actions">
        <button onclick={frameWalls}>FRAME SOLID WALLS</button>
        <button onclick={undo} disabled={undoStack.length === 0}>UNDO</button>
        <button onclick={redo} disabled={redoStack.length === 0}>REDO</button>
        <button onclick={resetDefault}>RESET BUILT-IN</button>
        <button class="danger" onclick={clearMap}>CLEAR MAP</button>
      </section>
    </aside>

    <section class="map-workspace">
      <div class="map-toolbar">
        <div>
          <strong>{map.name || "UNTITLED MAP"}</strong>
          <span>{map.width} × {map.height} · {map.tile_size}px · {map.width * map.tile_size} × {map.height * map.tile_size}px</span>
        </div>
        <div class="coordinate">
          {hoverCell ? `CELL ${hoverCell[0]}, ${hoverCell[1]}` : "CELL —, —"}
        </div>
      </div>

      <div class="map-scroll">
        <div
          class="map-grid"
          style={`grid-template-columns: repeat(${map.width}, minmax(24px, 1fr));`}
          onmouseleave={() => (hoverCell = null)}
          oncontextmenu={(event) => event.preventDefault()}
          role="grid"
          tabindex="0"
          aria-label="Arena tile grid"
        >
          {#each Array.from({ length: map.height }) as _, y}
            {#each Array.from({ length: map.width }) as _, x}
              {@const tile = map.tiles[y]?.[x] ?? "."}
              {@const playerSpawn = includesPoint(map.spawn_points, x, y)}
              {@const itemSpawn = includesPoint(map.item_spawn_points, x, y)}
              <button
                class="map-cell"
                class:floor={tile === "."}
                class:solid={tile === "#"}
                class:destructible={tile === "X"}
                class:player-spawn={playerSpawn}
                class:item-spawn={itemSpawn}
                aria-label={`Cell ${x}, ${y}: ${playerSpawn ? "player spawn" : itemSpawn ? "item spawn" : tile}`}
                onpointerdown={(event) => beginPaint(event, x, y)}
                onpointerenter={(event) => continuePaint(event, x, y)}
                onfocus={() => (hoverCell = [x, y])}
                role="gridcell"
              >
                {#if playerSpawn}
                  <span class="marker player">S</span>
                {:else if itemSpawn}
                  <span class="marker item">I</span>
                {:else if tile === "X"}
                  <span class="wall-x">×</span>
                {/if}
              </button>
            {/each}
          {/each}
        </div>
      </div>

      <div class="map-status">
        <span>{status}</span>
        <dl>
          <div><dt>FLOOR</dt><dd>{floorCount}</dd></div>
          <div><dt>WALLS</dt><dd>{wallCount}</dd></div>
          <div><dt>PLAYERS</dt><dd>{map.spawn_points.length}/4+</dd></div>
          <div><dt>ITEMS</dt><dd>{map.item_spawn_points.length}/1+</dd></div>
        </dl>
      </div>
    </section>

    <aside class="validation-panel">
      <div class="validation-heading">
        <div>
          <span class="section-label">VALIDATION</span>
          <strong class:valid={errorCount === 0}>{errorCount === 0 ? "READY TO EXPORT" : `${errorCount} ERROR${errorCount === 1 ? "" : "S"}`}</strong>
        </div>
        <span class="validation-light" class:valid={errorCount === 0}></span>
      </div>
      {#if issues.length === 0}
        <p class="validation-empty">All server and client map constraints pass.</p>
      {:else}
        <ul class="issues">
          {#each issues as issue}
            <li class:warning={issue.level === "warning"}>
              <span>{issue.level === "error" ? "×" : "!"}</span>
              {issue.message}
            </li>
          {/each}
        </ul>
      {/if}
      <div class="schema-note">
        <span>OUTPUT SCHEMA</span>
        <code>schema_version: 1</code>
        <small>Terrain is exported as <b>.</b>, <b>#</b>, and <b>X</b>. Markers remain separate coordinate arrays.</small>
      </div>
    </aside>
  </div>
</section>

<style>
  .editor-shell {
    margin-top: 18px;
  }

  .editor-heading {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 24px;
    padding: 20px;
    border: 1px solid #263340;
    background: rgba(9, 14, 20, 0.96);
  }

  .editor-heading h2 {
    margin-top: 5px;
    font: 650 clamp(1.25rem, 2vw, 1.8rem) ui-monospace, SFMono-Regular, Menlo, monospace;
    letter-spacing: 0.06em;
  }

  .editor-heading p {
    margin-top: 7px;
    color: var(--ink-label);
    font-size: var(--fs-body);
  }

  .editor-actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 8px;
  }

  button {
    min-height: 36px;
    padding: 8px 11px;
    color: #a9bac7;
    border: 1px solid #344555;
    background: #0b1219;
    font: 700 0.64rem ui-monospace, SFMono-Regular, Menlo, monospace;
    letter-spacing: 0.07em;
    cursor: pointer;
  }

  button:hover:not(:disabled) {
    color: #e7f5fb;
    border-color: #27e5ff;
  }

  button:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }

  button.primary {
    color: #061016;
    border-color: #27e5ff;
    background: #27e5ff;
  }

  .file-input {
    display: none;
  }

  .editor-layout {
    display: grid;
    grid-template-columns: 238px minmax(480px, 1fr) 260px;
    min-height: 680px;
    border: 1px solid #263340;
    border-top: 0;
    background: #070b10;
  }

  .editor-sidebar,
  .validation-panel {
    background: #090e14;
  }

  .editor-sidebar {
    border-right: 1px solid #263340;
  }

  .editor-panel {
    padding: 16px;
    border-bottom: 1px solid #263340;
  }

  .section-label {
    display: block;
    margin-bottom: 12px;
    color: var(--ink-label);
    font: 700 0.58rem ui-monospace, SFMono-Regular, Menlo, monospace;
    letter-spacing: var(--track-label);
  }

  label {
    display: block;
    margin-top: 10px;
  }

  label > span {
    display: block;
    margin-bottom: 5px;
    color: var(--ink-label);
    font: 700 0.56rem ui-monospace, SFMono-Regular, Menlo, monospace;
    letter-spacing: 0.1em;
  }

  input {
    width: 100%;
    min-width: 0;
    padding: 8px 9px;
    color: #dce7ef;
    border: 1px solid #2b3b49;
    border-radius: 0;
    outline: none;
    background: #070c11;
    font: 0.72rem ui-monospace, SFMono-Regular, Menlo, monospace;
  }

  input:focus {
    border-color: #27e5ff;
  }

  .field-pair {
    display: grid;
    grid-template-columns: 1fr 44px;
    gap: 7px;
    align-items: end;
  }

  .icon-action {
    height: 33px;
    min-height: 33px;
    padding: 0;
  }

  .field-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }

  .field-grid label {
    margin: 0;
  }

  .apply-size {
    align-self: end;
  }

  .brushes {
    display: grid;
    gap: 6px;
  }

  .brushes button {
    display: grid;
    grid-template-columns: 24px 1fr 20px;
    align-items: center;
    gap: 8px;
    width: 100%;
    text-align: left;
  }

  .brushes button.active {
    color: #27e5ff;
    border-color: #27e5ff;
    background: rgba(39, 229, 255, 0.08);
  }

  .brushes kbd {
    padding: 2px 0;
    color: var(--ink-label);
    border: 1px solid #2c3d4b;
    text-align: center;
    font: inherit;
  }

  .brushes strong {
    text-align: right;
  }

  .hint {
    display: block;
    margin-top: 10px;
    color: var(--ink-dim);
    font-size: var(--fs-label);
    line-height: 1.5;
  }

  .utility-actions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }

  .utility-actions button:first-child,
  .utility-actions button:nth-child(4),
  .utility-actions button:last-child {
    grid-column: 1 / -1;
  }

  .utility-actions .danger {
    color: #ff7a84;
    border-color: rgba(255, 91, 104, 0.4);
  }

  .map-workspace {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    min-width: 0;
    background:
      linear-gradient(rgba(39, 229, 255, 0.025) 1px, transparent 1px),
      linear-gradient(90deg, rgba(39, 229, 255, 0.025) 1px, transparent 1px),
      #060a0e;
    background-size: 20px 20px;
  }

  .map-toolbar,
  .map-status {
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-height: 58px;
    padding: 11px 14px;
    border-bottom: 1px solid #263340;
    background: rgba(8, 13, 19, 0.95);
  }

  .map-toolbar strong,
  .map-toolbar span,
  .coordinate,
  .map-status {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  }

  .map-toolbar strong {
    display: block;
    font-size: var(--fs-body);
    letter-spacing: 0.06em;
  }

  .map-toolbar span,
  .coordinate {
    color: var(--ink-label);
    font-size: var(--fs-label);
  }

  .map-toolbar span {
    display: block;
    margin-top: 4px;
  }

  .map-scroll {
    min-height: 480px;
    overflow: auto;
    display: grid;
    place-items: center;
    padding: 28px;
  }

  .map-grid {
    display: grid;
    width: min(100%, 880px);
    min-width: max-content;
    border-top: 1px solid #334654;
    border-left: 1px solid #334654;
    box-shadow: 0 18px 70px rgba(0, 0, 0, 0.4);
    user-select: none;
    touch-action: none;
  }

  .map-cell {
    position: relative;
    width: 100%;
    min-width: 24px;
    aspect-ratio: 1;
    min-height: 0;
    padding: 0;
    overflow: hidden;
    border: 0;
    border-right: 1px solid #21313d;
    border-bottom: 1px solid #21313d;
    background: #101923;
  }

  .map-cell.floor:nth-child(even) {
    background: #0e1720;
  }

  .map-cell.solid {
    background: #253340;
    box-shadow: inset 0 0 0 3px #17222d;
  }

  .map-cell.destructible {
    background: #71462f;
    box-shadow: inset 0 0 0 3px #4c3022;
  }

  .map-cell:hover,
  .map-cell:focus-visible {
    z-index: 2;
    outline: 2px solid #27e5ff;
    outline-offset: -2px;
  }

  .wall-x {
    color: #d89b64;
    font-size: clamp(0.65rem, 1.6vw, 1.15rem);
  }

  .marker {
    position: absolute;
    inset: 18%;
    display: grid;
    place-items: center;
    color: #061016;
    font: 900 clamp(0.55rem, 1.3vw, 0.9rem) ui-monospace, SFMono-Regular, Menlo, monospace;
  }

  .marker.player {
    border-radius: 50%;
    background: #27e5ff;
    box-shadow: 0 0 12px rgba(39, 229, 255, 0.55);
  }

  .marker.item {
    inset: 23%;
    background: #ffe66d;
    transform: rotate(45deg);
  }

  .marker.item::first-letter {
    transform: rotate(-45deg);
  }

  .map-status {
    border-top: 1px solid #263340;
    border-bottom: 0;
    color: var(--ink-label);
    font-size: var(--fs-label);
    letter-spacing: 0.05em;
  }

  .map-status dl {
    display: flex;
    gap: 18px;
    margin: 0;
  }

  .map-status dl div {
    display: flex;
    gap: 6px;
  }

  .map-status dt {
    color: var(--ink-dim);
  }

  .map-status dd {
    margin: 0;
    color: #c7d7e2;
  }

  .validation-panel {
    border-left: 1px solid #263340;
  }

  .validation-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 18px 16px;
    border-bottom: 1px solid #263340;
  }

  .validation-heading .section-label {
    margin-bottom: 7px;
  }

  .validation-heading strong {
    color: #ff6d78;
    font: 700 0.68rem ui-monospace, SFMono-Regular, Menlo, monospace;
    letter-spacing: 0.07em;
  }

  .validation-heading strong.valid {
    color: #7cff6b;
  }

  .validation-light {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: #ff5b68;
    box-shadow: 0 0 12px rgba(255, 91, 104, 0.65);
  }

  .validation-light.valid {
    background: #7cff6b;
    box-shadow: 0 0 12px rgba(124, 255, 107, 0.65);
  }

  .validation-empty {
    margin: 16px;
    padding: 14px;
    color: #8dcf86;
    border: 1px solid rgba(124, 255, 107, 0.24);
    font-size: var(--fs-body);
    line-height: 1.5;
  }

  .issues {
    display: grid;
    gap: 7px;
    margin: 0;
    padding: 14px;
    list-style: none;
  }

  .issues li {
    display: grid;
    grid-template-columns: 18px 1fr;
    gap: 6px;
    padding: 9px;
    color: #d89399;
    border-left: 2px solid #ff5b68;
    background: rgba(255, 91, 104, 0.06);
    font-size: var(--fs-label);
    line-height: 1.4;
  }

  .issues li.warning {
    color: #c9b773;
    border-color: #ffe66d;
    background: rgba(255, 230, 109, 0.05);
  }

  .schema-note {
    display: grid;
    gap: 8px;
    margin: 14px;
    padding: 14px;
    border: 1px solid #263340;
  }

  .schema-note > span {
    color: var(--ink-label);
    font: 700 0.56rem ui-monospace, SFMono-Regular, Menlo, monospace;
    letter-spacing: var(--track-label);
  }

  .schema-note code {
    color: #27e5ff;
    font-size: var(--fs-body);
  }

  .schema-note small {
    color: var(--ink-label);
    font-size: var(--fs-label);
    line-height: 1.5;
  }

  .schema-note b {
    color: #dce7ef;
  }

  @media (max-width: 1100px) {
    .editor-layout {
      grid-template-columns: 220px minmax(440px, 1fr);
    }

    .validation-panel {
      grid-column: 1 / -1;
      border-top: 1px solid #263340;
      border-left: 0;
    }
  }

  @media (max-width: 760px) {
    .editor-heading {
      align-items: stretch;
      flex-direction: column;
    }

    .editor-actions {
      justify-content: flex-start;
    }

    .editor-layout {
      grid-template-columns: 1fr;
    }

    .editor-sidebar {
      border-right: 0;
    }

    .map-workspace {
      min-height: 620px;
    }

    .map-scroll {
      place-items: start;
      min-height: 430px;
      padding: 16px;
    }

    .map-status {
      align-items: flex-start;
      flex-direction: column;
      gap: 10px;
    }
  }
</style>
