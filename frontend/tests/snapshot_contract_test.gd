extends SceneTree

## クライアントが読むキーが、サーバーの実際の通信フォーマットに存在することを検証する。
##
## fixtureは backend/protocols/game/tests/wire_golden.rs が生成する。
## サーバー側でフィールド名を変えると、Rustのゴールデンテストとこのテストの両方が落ちる。
##
##     godot --headless --path frontend --script res://tests/snapshot_contract_test.gd

const GOLDEN_PATH := "res://tests/fixtures/wire_messages_golden.json"
const CONTRACT := preload("res://src/networking/snapshot_contract.gd")

var _failures: PackedStringArray = PackedStringArray()


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	var file := FileAccess.open(GOLDEN_PATH, FileAccess.READ)
	if file == null:
		push_error(
			"snapshot contract: %s を読めない。" % GOLDEN_PATH
			+ " UPDATE_WIRE_GOLDEN=1 cargo test -p pixel-shooter-protocol"
			+ " --test wire_golden で生成する"
		)
		quit(1)
		return
	var golden = JSON.parse_string(file.get_as_text())
	if typeof(golden) != TYPE_DICTIONARY or typeof(golden.get("messages")) != TYPE_DICTIONARY:
		push_error("snapshot contract: fixtureの形式が不正")
		quit(1)
		return
	var messages: Dictionary = golden["messages"]

	_check("welcome", CONTRACT.validate_welcome(messages.get("welcome", {})))
	_check("rejected", CONTRACT.validate_rejected(messages.get("rejected", {})))
	_check(
		"map_definition",
		CONTRACT.validate_map_definition(messages.get("map_definition", {}).get("map", {}))
	)
	_check("snapshot", CONTRACT.validate_snapshot(messages.get("snapshot", {})))

	# マップ定義は実際にクライアントのパーサーを通るところまで確認する。
	var map := ArenaMapData.from_dictionary(
		messages.get("map_definition", {}).get("map", {}), "wire golden map"
	)
	if map == null:
		_failures.append("map_definition: ArenaMapDataが読み込めない")

	# ルーム画面が使うマップ一覧の形も固定する。
	var catalog = messages.get("map_catalog", {}).get("maps", [])
	if typeof(catalog) != TYPE_ARRAY or catalog.is_empty():
		_failures.append("map_catalog.maps: 配列でないか空")
	elif not (catalog[0].has("id") and catalog[0].has("name")):
		_failures.append("map_catalog.maps[0]: id / name が欠けている")

	if not _failures.is_empty():
		push_error(
			"snapshot contract: クライアントが読むフィールドがサーバーの出力に無い:\n  "
			+ "\n  ".join(_failures)
			+ "\nbackend/protocols/game/src/lib.rs と"
			+ " frontend/src/networking/snapshot_contract.gd を突き合わせること"
		)
		quit(1)
		return

	print("snapshot contract: クライアントが読む全フィールドがサーバー出力に存在した")
	quit(0)


func _check(label: String, missing: PackedStringArray) -> void:
	for path in missing:
		_failures.append("%s: %s" % [label, path])
