class_name SnapshotContract
extends RefCounted

## Godotクライアントが依存する、サーバーメッセージのキー一覧。
##
## 正式な型定義は backend/protocols/game/src/lib.rs にあるが、GDScriptは
## `dictionary.get(key, default)` で読むため、サーバー側でフィールド名を変えても
## 例外は出ず、無言でデフォルト値へ落ちる。たとえば `move_speed` が届かないと
## クライアント予測だけが別の速度で走り、原因の分かりにくいズレになる。
##
## そこで「クライアントが実際に読むキー」をここに明示し、
##
## - 実行時: NetworkClient が接続ごとに最初の1通を検査し、欠けていれば警告する
## - CI: frontend/tests/snapshot_contract_test.gd が、Rustが生成した
##   frontend/tests/fixtures/wire_messages_golden.json と突き合わせる
##
## の両方で守る。読むキーを増やしたら、必ずここにも追加すること。

const SNAPSHOT_KEYS: Array[String] = [
	"phase",
	"time_left",
	"winner_id",
	"reconnect_grace_left",
	"move_speed",
	"dash_speed",
	"dash_duration",
	"dash_cooldown",
	"players",
	"bullets",
	"items",
	"larokin_poppos",
	"room",
]

const PLAYER_KEYS: Array[String] = [
	"id",
	"name",
	"position",
	"aim",
	"hp",
	"max_hp",
	"score",
	"is_cpu",
	"connected",
	"alive",
	"respawn_left",
	"invulnerable_left",
	"ammo",
	"max_ammo",
	"reloading",
	"reload_left",
	"dash_cooldown_left",
	"dashing",
	"dash_time_left",
	"berserk_left",
	"shield_hp",
	"last_input_sequence",
]

const BULLET_KEYS: Array[String] = ["id", "owner_id", "position", "velocity"]
const ITEM_KEYS: Array[String] = ["id", "position", "points", "kind"]
const LAROKIN_KEYS: Array[String] = ["id", "position", "telegraph_left"]
const ROOM_KEYS: Array[String] = ["host_player_id", "can_start", "max_players", "settings"]
const ROOM_SETTINGS_KEYS: Array[String] = [
	"map_id",
	"match_seconds",
	"kill_points",
	"death_penalty",
	"item_points",
	"item_spawn_interval",
	"max_items",
]
## `held_item` はnull許容だが、値が入っている場合はこの形でなければならない。
const HELD_ITEM_KEYS: Array[String] = ["kind", "charges"]
const VECTOR_KEYS: Array[String] = ["x", "y"]

const WELCOME_KEYS: Array[String] = ["player_id", "reconnect_token", "reconnected"]
const REJECTED_KEYS: Array[String] = ["reason", "retryable"]
const MAP_DEFINITION_KEYS: Array[String] = [
	"schema_version",
	"id",
	"revision",
	"width",
	"height",
	"tile_size",
	"tiles",
	"spawn_points",
	"item_spawn_points",
]


## 欠けているキーのパス一覧を返す。空なら契約を満たしている。
static func validate_snapshot(snapshot: Dictionary) -> PackedStringArray:
	var missing := PackedStringArray()
	_require(snapshot, SNAPSHOT_KEYS, "snapshot", missing)

	var players = snapshot.get("players", [])
	if typeof(players) == TYPE_ARRAY and not players.is_empty():
		var player = players[0]
		if typeof(player) == TYPE_DICTIONARY:
			_require(player, PLAYER_KEYS, "snapshot.players[0]", missing)
			_require_vector(player.get("position"), "snapshot.players[0].position", missing)
			_require_vector(player.get("aim"), "snapshot.players[0].aim", missing)
		for entry in players:
			# held_item はnullでもよいが、辞書ならキーが揃っていなければならない。
			if typeof(entry) == TYPE_DICTIONARY and typeof(entry.get("held_item")) == TYPE_DICTIONARY:
				_require(
					entry["held_item"], HELD_ITEM_KEYS, "snapshot.players[].held_item", missing
				)
				break

	_require_first(snapshot.get("bullets"), BULLET_KEYS, "snapshot.bullets[0]", missing)
	_require_first(snapshot.get("items"), ITEM_KEYS, "snapshot.items[0]", missing)
	_require_first(
		snapshot.get("larokin_poppos"), LAROKIN_KEYS, "snapshot.larokin_poppos[0]", missing
	)

	var room = snapshot.get("room")
	if typeof(room) == TYPE_DICTIONARY:
		_require(room, ROOM_KEYS, "snapshot.room", missing)
		if typeof(room.get("settings")) == TYPE_DICTIONARY:
			_require(room["settings"], ROOM_SETTINGS_KEYS, "snapshot.room.settings", missing)
	return missing


static func validate_map_definition(map_definition: Dictionary) -> PackedStringArray:
	var missing := PackedStringArray()
	_require(map_definition, MAP_DEFINITION_KEYS, "map_definition.map", missing)
	return missing


static func validate_welcome(message: Dictionary) -> PackedStringArray:
	var missing := PackedStringArray()
	_require(message, WELCOME_KEYS, "welcome", missing)
	return missing


static func validate_rejected(message: Dictionary) -> PackedStringArray:
	var missing := PackedStringArray()
	_require(message, REJECTED_KEYS, "rejected", missing)
	return missing


static func _require(
	value: Dictionary, keys: Array[String], path: String, missing: PackedStringArray
) -> void:
	for key in keys:
		if not value.has(key):
			missing.append("%s.%s" % [path, key])


static func _require_first(
	list, keys: Array[String], path: String, missing: PackedStringArray
) -> void:
	if typeof(list) != TYPE_ARRAY or list.is_empty():
		return
	if typeof(list[0]) == TYPE_DICTIONARY:
		_require(list[0], keys, path, missing)
		if keys.has("position"):
			_require_vector(list[0].get("position"), "%s.position" % path, missing)
		if keys.has("velocity"):
			_require_vector(list[0].get("velocity"), "%s.velocity" % path, missing)


static func _require_vector(value, path: String, missing: PackedStringArray) -> void:
	if typeof(value) != TYPE_DICTIONARY:
		missing.append(path)
		return
	_require(value, VECTOR_KEYS, path, missing)
