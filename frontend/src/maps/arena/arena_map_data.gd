class_name ArenaMapData
extends RefCounted

const FLOOR := 0
const SOLID_WALL := 1
const DESTRUCTIBLE_WALL := 2
const MAX_MAP_WIDTH := 256
const MAX_MAP_HEIGHT := 256
const MIN_TILE_SIZE := 8
const MAX_TILE_SIZE := 128

var width := 0
var height := 0
var tile_size := 0
var tiles: Array[int] = []


static func from_dictionary(data: Dictionary, source := "server map") -> ArenaMapData:
	var map := ArenaMapData.new()
	if not map._load_dictionary(data, source):
		return null
	return map


func _load_dictionary(data: Dictionary, source: String) -> bool:
	var schema_version := int(data.get("schema_version", 0))
	var map_id := str(data.get("id", ""))
	var map_revision := str(data.get("revision", ""))
	width = int(data.get("width", 0))
	height = int(data.get("height", 0))
	tile_size = int(data.get("tile_size", 0))
	if schema_version != 1 or map_id.is_empty() or map_revision.is_empty():
		push_error("%s has invalid schema_version, id, or revision" % source)
		return false
	if (
		width <= 0
		or width > MAX_MAP_WIDTH
		or height <= 0
		or height > MAX_MAP_HEIGHT
		or tile_size < MIN_TILE_SIZE
		or tile_size > MAX_TILE_SIZE
	):
		push_error("%s has invalid map dimensions" % source)
		return false

	var rows = data.get("tiles", [])
	if typeof(rows) != TYPE_ARRAY:
		push_error("%s tiles must be an array" % source)
		return false
	if rows.size() != height:
		push_error("%s has %d tile rows; expected %d" % [source, rows.size(), height])
		return false
	tiles.clear()
	for y in range(height):
		var row := str(rows[y])
		if row.length() != width:
			push_error("%s row %d has %d tiles; expected %d" % [source, y, row.length(), width])
			return false
		for x in range(width):
			match row[x]:
				".":
					tiles.append(FLOOR)
				"#":
					tiles.append(SOLID_WALL)
				"X":
					tiles.append(DESTRUCTIBLE_WALL)
				var unknown:
					push_error("%s has unknown tile '%s' at (%d, %d)" % [source, unknown, x, y])
					return false

	var spawn_points = _read_points(data.get("spawn_points", []), "spawn point", source)
	var item_spawn_points = _read_points(
		data.get("item_spawn_points", []), "item spawn point", source
	)
	if spawn_points == null or item_spawn_points == null:
		return false
	if spawn_points.size() < 4 or item_spawn_points.is_empty():
		push_error("%s needs at least 4 player spawns and 1 item spawn" % source)
		return false
	for point in spawn_points + item_spawn_points:
		if not contains_cell(point) or is_wall(point):
			push_error("%s has an invalid marker at (%d, %d)" % [source, point.x, point.y])
			return false
	return true


func _read_points(value, label: String, source: String):
	var result: Array[Vector2i] = []
	if typeof(value) != TYPE_ARRAY:
		push_error("%s has invalid %s list" % [source, label])
		return null
	for entry in value:
		if typeof(entry) != TYPE_ARRAY or entry.size() != 2:
			push_error("%s contains an invalid %s" % [source, label])
			return null
		result.append(Vector2i(int(entry[0]), int(entry[1])))
	return result


func size_pixels() -> Vector2:
	return Vector2(width * tile_size, height * tile_size)


func contains_cell(cell: Vector2i) -> bool:
	return cell.x >= 0 and cell.x < width and cell.y >= 0 and cell.y < height


func tile_at(cell: Vector2i) -> int:
	if not contains_cell(cell):
		return SOLID_WALL
	return tiles[cell.y * width + cell.x]


func is_wall(cell: Vector2i) -> bool:
	return tile_at(cell) in [SOLID_WALL, DESTRUCTIBLE_WALL]


func obstacle_at(position: Vector2, margin: float) -> bool:
	var min_cell := Vector2i(
		floori(maxf(position.x - margin, 0.0) / tile_size),
		floori(maxf(position.y - margin, 0.0) / tile_size)
	)
	var max_cell := Vector2i(
		floori(maxf(position.x + margin, 0.0) / tile_size),
		floori(maxf(position.y + margin, 0.0) / tile_size)
	)
	for y in range(min_cell.y, mini(max_cell.y, height - 1) + 1):
		for x in range(min_cell.x, mini(max_cell.x, width - 1) + 1):
			if is_wall(Vector2i(x, y)):
				return true
	return false
