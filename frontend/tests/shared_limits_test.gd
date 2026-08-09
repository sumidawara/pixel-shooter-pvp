extends SceneTree

## サーバーと画面が同じ範囲を守っているかの検証。
##
## ルーム設定の許容範囲、マップの大きさの上限、CPUの段階数は、両側に同じ数字が
## 書かれている。片方だけ動かしても、動かした側では何も起きないので気付けない。
##
## 正は backend/protocols/game/src/lib.rs。Rustの試験がその値を
## frontend/tests/fixtures/shared_limits_golden.json へ書き出し、ここで突き合わせる。
##
##     godot --headless --path frontend --script res://tests/shared_limits_test.gd

const GOLDEN_PATH := "res://tests/fixtures/shared_limits_golden.json"

## ルーム設定の項目名 → その値を入力するSpinBoxのユニーク名。
##
## 画面に入力欄がある項目だけを並べる。無い項目（item_spawn_interval）は
## 画面側に範囲が存在しないので、突き合わせる相手が無い。
const SETTING_INPUTS := {
	"match_seconds": "MatchSecondsInput",
	"kill_points": "KillPointsInput",
	"death_penalty": "DeathPenaltyInput",
	"item_points": "ItemPointsInput",
	"max_items": "MaxItemsInput",
}

var _failures: PackedStringArray = PackedStringArray()


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	var limits := _load_limits()
	if limits.is_empty():
		push_error("shared limits:\n  " + "\n  ".join(_failures))
		quit(1)
		return

	await _check_input_ranges_match_what_the_server_accepts(limits)
	await _check_the_lobby_offers_every_cpu_level(limits)
	await _check_the_palette_matches(limits)
	_check_map_limits_match(limits)

	if not _failures.is_empty():
		push_error("shared limits:\n  " + "\n  ".join(_failures))
		quit(1)
		return
	print("shared limits: サーバーと画面の範囲が一致していた")
	quit(0)


## 入力欄の範囲が、サーバーが受け付ける範囲と一致すること。
##
## 画面が広く取っていると、入れた値が黙って丸められる。狭く取っていると、
## サーバーが許している値を入れられない。
func _check_input_ranges_match_what_the_server_accepts(limits: Dictionary) -> void:
	var menu = await _open_menu()
	var bounds: Dictionary = limits.get("room_settings", {})

	for key in SETTING_INPUTS:
		if not bounds.has(key):
			_failures.append("%s の範囲がサーバー側に無い" % key)
			continue
		var input: SpinBox = menu.get_node("%" + str(SETTING_INPUTS[key]))
		var expected: Dictionary = bounds[key]
		if not is_equal_approx(input.min_value, float(expected["min"])):
			_failures.append(
				"%s の下限が食い違う: 画面 %s / サーバー %s"
				% [key, input.min_value, expected["min"]]
			)
		if not is_equal_approx(input.max_value, float(expected["max"])):
			_failures.append(
				"%s の上限が食い違う: 画面 %s / サーバー %s"
				% [key, input.max_value, expected["max"]]
			)

	await _close(menu)


## ロビーが、サーバーの持つ段階を全部選べること。
##
## 段階の選択はCPU1体ごとの行にある。行はSnapshotが届いてから作られるので、
## 選択肢の元になる一覧そのものを見る。
func _check_the_lobby_offers_every_cpu_level(limits: Dictionary) -> void:
	var menu = await _open_menu()
	var cpu: Dictionary = limits.get("cpu", {})
	var expected := int(cpu.get("max_level", 0)) - int(cpu.get("min_level", 0)) + 1

	if menu.CPU_LEVEL_LABELS.size() != expected:
		_failures.append(
			"段階の選択肢の数が食い違う: 画面 %d / サーバー %d"
			% [menu.CPU_LEVEL_LABELS.size(), expected]
		)

	await _close(menu)


## 色の数がサーバーと一致すること。
##
## サーバーは番号だけを配る。画面側が少ないと、割り当てられた番号を
## 表現できずに別の色へ化ける。
func _check_the_palette_matches(limits: Dictionary) -> void:
	var menu = await _open_menu()
	var expected := int(limits.get("player_colors", {}).get("count", 0))
	if menu.PLAYER_COLORS.size() != expected:
		_failures.append(
			"ロビーの色数が食い違う: 画面 %d / サーバー %d"
			% [menu.PLAYER_COLORS.size(), expected]
		)
	if menu.PLAYER_COLOR_NAMES.size() != expected:
		_failures.append("色の名前の数が食い違う: %d" % menu.PLAYER_COLOR_NAMES.size())
	await _close(menu)

	var game_screen := load("res://src/game_modes/match/game_screen.gd")
	if game_screen.PLAYER_COLORS.size() != expected:
		_failures.append(
			"対戦画面の色数が食い違う: %d / サーバー %d"
			% [game_screen.PLAYER_COLORS.size(), expected]
		)


## マップの大きさの上限が一致すること。
##
## 食い違うと、サーバーが正しいと判断して送ったマップを画面が拒む。
func _check_map_limits_match(limits: Dictionary) -> void:
	var map: Dictionary = limits.get("map", {})
	var pairs := {
		"max_width": ArenaMapData.MAX_MAP_WIDTH,
		"max_height": ArenaMapData.MAX_MAP_HEIGHT,
		"min_tile_size": ArenaMapData.MIN_TILE_SIZE,
		"max_tile_size": ArenaMapData.MAX_TILE_SIZE,
	}
	for key in pairs:
		if not map.has(key):
			_failures.append("%s がサーバー側に無い" % key)
			continue
		if int(map[key]) != int(pairs[key]):
			_failures.append(
				"%s が食い違う: 画面 %s / サーバー %s" % [key, pairs[key], map[key]]
			)


func _load_limits() -> Dictionary:
	if not FileAccess.file_exists(GOLDEN_PATH):
		_failures.append("%s が無い。make update-goldens で生成する" % GOLDEN_PATH)
		return {}
	var parsed = JSON.parse_string(FileAccess.get_file_as_string(GOLDEN_PATH))
	if typeof(parsed) != TYPE_DICTIONARY:
		_failures.append("%s が読めない" % GOLDEN_PATH)
		return {}
	return parsed


func _open_menu():
	var main_scene: PackedScene = load("res://src/app/main.tscn")
	var main = main_scene.instantiate()
	root.add_child(main)
	await process_frame
	return main.get_node("MenuScreen")


func _close(menu) -> void:
	menu.get_parent().queue_free()
	await process_frame
