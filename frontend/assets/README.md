# Game assets

`aseprite/` contains the editable source files. `generated/` contains the PNG
files imported by Godot. Both are committed: the game can be built without an
Aseprite installation, while artists can still edit the original layers and
animation frames.

Asset pairs are declared in `aseprite-assets.json`.

## First-time migration

The repository may initially contain only the generated PNGs. With Aseprite
installed, create the corresponding `.aseprite` sources:

```sh
make assets-bootstrap
```

On macOS the default application path is detected automatically. For another
installation location:

```sh
make assets-bootstrap ASEPRITE_BIN="/path/to/aseprite"
```

The bootstrap keeps an existing `.aseprite` file unchanged. A sprite sheet is
imported as one flat image, so split it into animation frames in Aseprite if
frame-by-frame editing is needed.

## Development

Open Godot together with the asset watcher:

```sh
make godot-assets
```

Or run only the watcher:

```sh
make assets-watch
```

Saving a declared `.aseprite` file regenerates its PNG. Godot then notices the
PNG change and reimports it. Use `make assets-build` for a one-time export.

When adding an asset, add its source/output pair to `aseprite-assets.json` and
reference only the generated PNG from Godot scenes and scripts.
