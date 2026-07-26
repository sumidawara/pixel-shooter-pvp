# Arena maps

`classic_arena.json` is shared by the authoritative Rust server and the Godot
client. Rust validates it and converts every tile to `TileKind` before a match
starts. Godot uses the same file for rendering and client-side prediction.

## Visual editing

1. Open `res://src/maps/arena/authoring/classic_arena_authoring.tscn`.
2. Select `ClassicArenaAuthoring`.
3. Use **Import JSON** in the Inspector.
4. Paint walls on `TerrainLayer` and spawn markers on `MarkerLayer`.
5. Increment `revision`.
6. Select the root again and run **Validate and Export JSON**.

The TileSet custom data named `tile_kind` controls export semantics:

- empty terrain cell: floor (`.`)
- `solid_wall`: indestructible wall (`#`)
- `destructible_wall`: destructible wall (`X`)
- `player_spawn`: player spawn marker
- `item_spawn`: item candidate marker

Spawn markers are exported to their own arrays and never become terrain tiles.
The exporter rejects missing markers, unknown tile kinds, and markers placed on
walls.

Set `PIXEL_SHOOTER_MAP=/absolute/path/to/map.json` to make a game server load an
external map. Without that variable, the server uses the checked-in
`classic_arena.json` embedded at compile time.
