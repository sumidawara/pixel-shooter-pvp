extends SceneTree

## クライアント予測がサーバーの権威計算と一致していることを検証する。
##
## fixtureは backend/game-core/tests/movement_prediction_golden.rs が生成する。
## 移動・ダッシュ・壁判定の規則を片側だけ変えると、このテストが落ちる。
##
##     godot --headless --path frontend --script res://tests/movement_prediction_golden_test.gd

const GOLDEN_PATH := "res://tests/fixtures/movement_prediction_golden.json"
const PREDICTOR := preload("res://src/game_modes/match/movement_predictor.gd")

## サーバーはf32、GDScriptはf64で計算するため、丸め差だけは許容する。
## 規則が食い違うと1tickでpx単位のずれになるので、この幅でも十分検出できる。
const POSITION_TOLERANCE := 0.05
const TIMER_TOLERANCE := 0.002


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	var file := FileAccess.open(GOLDEN_PATH, FileAccess.READ)
	if file == null:
		push_error(
			"movement prediction: %s を読めない。" % GOLDEN_PATH
			+ " UPDATE_MOVEMENT_GOLDEN=1 cargo test -p pixel-shooter-game-core"
			+ " --test movement_prediction_golden で生成する"
		)
		quit(1)
		return
	var golden = JSON.parse_string(file.get_as_text())
	if typeof(golden) != TYPE_DICTIONARY:
		push_error("movement prediction: fixtureがJSONオブジェクトではない")
		quit(1)
		return

	var map := ArenaMapData.from_dictionary(golden.get("map", {}), "golden fixture map")
	if map == null:
		push_error("movement prediction: fixtureのマップ定義を読み込めない")
		quit(1)
		return

	var gameplay: Dictionary = golden.get("gameplay", {})
	var tick_rate := float(golden.get("tick_rate", 60.0))
	if tick_rate <= 0.0:
		push_error("movement prediction: tick_rateが不正")
		quit(1)
		return
	var delta := 1.0 / tick_rate

	var predictor = PREDICTOR.new()
	predictor.set_map(map)
	predictor.set_gameplay(
		float(gameplay.get("move_speed", 0.0)),
		float(gameplay.get("dash_speed", 0.0)),
		float(gameplay.get("dash_duration", 0.0)),
		float(gameplay.get("dash_cooldown", 0.0))
	)
	predictor.reset_to(_to_vector(golden.get("start_position", {})), 0.0, 0.0, true, 0.0)

	var frames: Array = golden.get("frames", [])
	if frames.is_empty():
		push_error("movement prediction: fixtureにフレームが入っていない")
		quit(1)
		return

	for index in range(frames.size()):
		var frame: Dictionary = frames[index]
		# サーバーが決める状態は、生成側がtickの直前に設定した値をそのまま使う。
		predictor.alive = bool(frame.get("alive", true))
		predictor.berserk_left = float(frame.get("berserk_left", 0.0))
		predictor.simulate({
			"delta": delta,
			"movement": Vector2(float(frame.get("move_x", 0.0)), float(frame.get("move_y", 0.0))),
			"dash_pressed": bool(frame.get("dash_pressed", false)),
			"use_item_pressed": bool(frame.get("use_item_pressed", false)),
			"held_kind": "dash" if int(frame.get("dash_charges", 0)) > 0 else "",
		})

		var expected: Dictionary = frame.get("expected", {})
		var expected_position := _to_vector(expected.get("position", {}))
		var predicted_position: Vector2 = predictor.position
		var error := predicted_position.distance_to(expected_position)
		if error > POSITION_TOLERANCE:
			push_error(
				"movement prediction: frame %d (%s) で位置が一致しない。" % [
					index, str(frame.get("note", ""))
				]
				+ " server=%s client=%s error=%.4fpx" % [
					expected_position, predicted_position, error
				]
			)
			quit(1)
			return
		if not _close(
			predictor.dash_time_left, float(expected.get("dash_time_left", 0.0)), TIMER_TOLERANCE
		):
			push_error(
				"movement prediction: frame %d (%s) でdash_time_leftが一致しない。" % [
					index, str(frame.get("note", ""))
				]
				+ " server=%f client=%f" % [
					float(expected.get("dash_time_left", 0.0)), predictor.dash_time_left
				]
			)
			quit(1)
			return
		if not _close(
			predictor.dash_cooldown_left,
			float(expected.get("dash_cooldown_left", 0.0)),
			TIMER_TOLERANCE
		):
			push_error(
				"movement prediction: frame %d (%s) でdash_cooldown_leftが一致しない。" % [
					index, str(frame.get("note", ""))
				]
				+ " server=%f client=%f" % [
					float(expected.get("dash_cooldown_left", 0.0)), predictor.dash_cooldown_left
				]
			)
			quit(1)
			return

	print(
		"movement prediction: %d フレームすべてでサーバーの確定位置と一致した" % frames.size()
	)
	quit(0)


func _close(left: float, right: float, tolerance: float) -> bool:
	return absf(left - right) <= tolerance


func _to_vector(value: Dictionary) -> Vector2:
	return Vector2(float(value.get("x", 0.0)), float(value.get("y", 0.0)))
