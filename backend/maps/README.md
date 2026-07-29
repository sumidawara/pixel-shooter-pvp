# Arena maps

This directory is the authoritative source for arena map data.
`classic_arena.json` is the built-in map for the Rust server. Rust validates it
and converts every tile to `TileKind` before a match starts. On each connection,
the server sends the validated definition to Godot once; Godot validates it
again and uses it for rendering and client prediction.

## Web editing

1. Run `make web-dev`.
2. Open the printed local URL and select **MAP EDITOR**.
3. Paint terrain and place player/item spawn markers.
4. Resolve any validation errors shown beside the canvas.
5. Increment `revision` when the map should have a new cache identity.
6. Download the JSON and replace `backend/maps/classic_arena.json`.
7. Run `make reload-maps` to restart only the development Game Servers.

The editor imports and exports this repository's map schema directly. Spawn
markers stay in their own arrays and never become terrain tiles. It rejects
invalid dimensions, missing markers, unknown tiles, and markers placed on
walls.

Set `PIXEL_SHOOTER_MAP=/absolute/path/to/map.json` to make a game server load an
external map. Without that variable, the server uses the checked-in
`classic_arena.json` embedded at compile time.

The development Compose configuration mounts this directory at `/app/maps` and
sets `PIXEL_SHOOTER_MAP` for both Game Servers, so editing a map does not require
an image rebuild.
