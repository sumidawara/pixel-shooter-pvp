extends SceneTree

## Ghost奪取演出の軌道を検証する。
##
## 見た目そのものはヘッドレスでは確かめられないが、「使用者から対象へ飛び、
## 掴んで戻ってくる」という演出の中身は位置の計算で表せる。そこだけを固定する。
##
##     godot --headless --path frontend --script res://tests/ghost_thief_view_test.gd

const GhostThiefView := preload("res://src/combat/items/ghost_thief_view.gd")

var _failures: PackedStringArray = PackedStringArray()


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	var user := Vector2(100.0, 200.0)
	var target := Vector2(300.0, 200.0)

	# 出発は使用者、折り返しで対象、最後は使用者へ戻る。
	_close("開始位置", GhostThiefView.flight_position(user, target, 0.0), user)
	_close("折り返し", GhostThiefView.flight_position(user, target, GhostThiefView.TURNING_POINT), target)
	_close("帰着位置", GhostThiefView.flight_position(user, target, 1.0), user)

	# 往路と復路で反対側へ膨らむ。直線移動だと掠め取った感じが出ない。
	var outward := GhostThiefView.flight_position(user, target, 0.25)
	var homeward := GhostThiefView.flight_position(user, target, 0.75)
	if not (outward.y < user.y):
		_failures.append("往路が上へ膨らんでいない: %s" % outward)
	if not (homeward.y > user.y):
		_failures.append("復路が下へ膨らんでいない: %s" % homeward)

	# 往路と復路は同じx帯を通る（行って戻る）。
	if absf(outward.x - homeward.x) > 1.0:
		_failures.append("往路と復路が同じ道を通っていない: %s / %s" % [outward.x, homeward.x])

	# 範囲外のprogressでも飛び出さない。
	_close("範囲外(負)", GhostThiefView.flight_position(user, target, -5.0), user)
	_close("範囲外(超過)", GhostThiefView.flight_position(user, target, 5.0), user)

	# 実際にノードを生成し、全区間を描いても落ちないこと。
	# GDScriptの実行時エラーはquit()へ到達せず、プロセスが止まらなくなる。
	var view = GhostThiefView.new()
	root.add_child(view)
	for step in range(0, 21):
		view.apply_state({
			"from": {"x": user.x, "y": user.y},
			"to": {"x": target.x, "y": target.y},
			"progress": step / 20.0,
			"stolen_kind": "shield",
		}, Color.CYAN)
		await process_frame
	view.queue_free()
	await process_frame

	if not _failures.is_empty():
		push_error("ghost thief view:\n  " + "\n  ".join(_failures))
		quit(1)
		return
	print("ghost thief view: 使用者→対象→使用者の往復軌道が期待どおりだった")
	quit(0)


func _close(label: String, actual: Vector2, expected: Vector2) -> void:
	if actual.distance_to(expected) > 0.01:
		_failures.append("%s: %s（期待 %s）" % [label, actual, expected])
