@tool
extends Node2D

const SOURCE_ID := 0
const SOLID_ATLAS := Vector2i(0, 0)
const DESTRUCTIBLE_ATLAS := Vector2i(4, 0)
const PLAYER_SPAWN_ATLAS := Vector2i(0, 1)
const ITEM_SPAWN_ATLAS := Vector2i(2, 1)

@export_file("*.json") var map_file := "res://maps/classic_arena.json"
@export var schema_version := 1
@export var map_id := "classic_arena"
@export var revision := "1"
@export var map_name := "Classic Arena"
@export_range(1, 256, 1) var map_width := 20
@export_range(1, 256, 1) var map_height := 11
@export_range(8, 128, 1) var tile_size := 32
@export_tool_button("Import JSON", "Callable") var import_button := import_map
@export_tool_button("Validate and Export JSON", "Callable") var export_button := export_map

@onready var terrain_layer: TileMapLayer = %TerrainLayer
@onready var marker_layer: TileMapLayer = %MarkerLayer


func _ready() -> void:
	if Engine.is_editor_hint() and terrain_layer.get_used_cells().is_empty():
		import_map.call_deferred()


func import_map() -> void:
	var map := ArenaMapData.load_from_file(map_file)
	if map == null:
		return
	schema_version = map.schema_version
	map_id = map.id
	revision = map.revision
	map_name = map.display_name
	map_width = map.width
	map_height = map.height
	tile_size = map.tile_size
	terrain_layer.clear()
	marker_layer.clear()
	for y in range(map_height):
		for x in range(map_width):
			var cell := Vector2i(x, y)
			match map.tile_at(cell):
				ArenaMapData.SOLID_WALL:
					terrain_layer.set_cell(cell, SOURCE_ID, SOLID_ATLAS)
				ArenaMapData.DESTRUCTIBLE_WALL:
					terrain_layer.set_cell(cell, SOURCE_ID, DESTRUCTIBLE_ATLAS)
	for cell in map.spawn_points:
		marker_layer.set_cell(cell, SOURCE_ID, PLAYER_SPAWN_ATLAS)
	for cell in map.item_spawn_points:
		marker_layer.set_cell(cell, SOURCE_ID, ITEM_SPAWN_ATLAS)
	print("Imported %s into TileMapLayer authoring scene" % map_file)


func export_map() -> void:
	var rows: Array[String] = []
	var spawn_points: Array[Array] = []
	var item_spawn_points: Array[Array] = []
	var errors: Array[String] = []

	for y in range(map_height):
		var row := ""
		for x in range(map_width):
			var cell := Vector2i(x, y)
			var terrain_kind := _tile_kind(terrain_layer, cell)
			match terrain_kind:
				"":
					row += "."
				"solid_wall":
					row += "#"
				"destructible_wall":
					row += "X"
				_:
					errors.append("Unknown terrain tile '%s' at %s" % [terrain_kind, cell])
					row += "."

			match _tile_kind(marker_layer, cell):
				"":
					pass
				"player_spawn":
					spawn_points.append([x, y])
				"item_spawn":
					item_spawn_points.append([x, y])
				var marker_kind:
					errors.append("Unknown marker tile '%s' at %s" % [marker_kind, cell])
		rows.append(row)

	if spawn_points.size() < 4:
		errors.append("At least 4 player spawn markers are required")
	if item_spawn_points.is_empty():
		errors.append("At least 1 item spawn marker is required")
	for point in spawn_points + item_spawn_points:
		var cell := Vector2i(point[0], point[1])
		if _tile_kind(terrain_layer, cell) in ["solid_wall", "destructible_wall"]:
			errors.append("Marker at %s overlaps a wall" % cell)
	if not errors.is_empty():
		for error in errors:
			push_error(error)
		return

	var data := {
		"schema_version": schema_version,
		"id": map_id,
		"revision": revision,
		"name": map_name,
		"width": map_width,
		"height": map_height,
		"tile_size": tile_size,
		"tiles": rows,
		"spawn_points": spawn_points,
		"item_spawn_points": item_spawn_points,
	}
	var file := FileAccess.open(map_file, FileAccess.WRITE)
	if file == null:
		push_error("Could not write %s: %s" % [map_file, FileAccess.get_open_error()])
		return
	file.store_string(JSON.stringify(data, "\t") + "\n")
	print("Validated and exported %s" % map_file)


func _tile_kind(layer: TileMapLayer, cell: Vector2i) -> String:
	var tile_data := layer.get_cell_tile_data(cell)
	if tile_data == null:
		return ""
	return str(tile_data.get_custom_data("tile_kind"))
