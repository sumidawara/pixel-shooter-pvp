extends SceneTree

const PODIUM_SCENE := "res://src/ui/hud/result_podium.tscn"
const PODIUM_SCRIPT := "res://src/ui/hud/result_podium.gd"


func _initialize() -> void:
	call_deferred("_run")


func _fail(message: String) -> void:
	push_error("result podium: %s" % message)
	quit(1)


func _run() -> void:
	var podium_script := load(PODIUM_SCRIPT)

	var ranking: Array = podium_script.build_ranking([
		{"id": 3, "name": "C", "score": 40, "is_cpu": false},
		{"id": 1, "name": "A", "score": 120, "is_cpu": false},
		{"id": 4, "name": "D", "score": -25, "is_cpu": true},
		{"id": 2, "name": "B", "score": 75, "is_cpu": false},
	])
	var ids: Array = ranking.map(func(entry): return entry["id"])
	if ids != [1, 2, 3, 4]:
		_fail("ranking must sort by score descending, got %s" % [ids])
		return
	var ranks: Array = ranking.map(func(entry): return entry["rank"])
	if ranks != [1, 2, 3, 4]:
		_fail("ranks must count up when no score is tied, got %s" % [ranks])
		return

	# 同点は同じ順位番号を共有し、並び順はIDの小さい方を先にして固定する。
	var tied: Array = podium_script.build_ranking([
		{"id": 2, "name": "B", "score": 100},
		{"id": 1, "name": "A", "score": 100},
		{"id": 3, "name": "C", "score": 10},
	])
	if tied.map(func(entry): return entry["id"]) != [1, 2, 3]:
		_fail("tied scores must fall back to ascending id")
		return
	var tied_ranks: Array = tied.map(func(entry): return entry["rank"])
	if tied_ranks != [1, 1, 3]:
		_fail("tied scores must share a rank number, got %s" % [tied_ranks])
		return

	var podium = load(PODIUM_SCENE).instantiate()
	root.add_child(podium)
	await process_frame

	podium.apply([
		{"id": 1, "name": "A", "score": 120, "is_cpu": false},
		{"id": 2, "name": "B", "score": 75, "is_cpu": false},
		{"id": 3, "name": "C", "score": 40, "is_cpu": false},
		{"id": 4, "name": "D", "score": -25, "is_cpu": true},
	], {1: Color.RED, 2: Color.BLUE, 3: Color.GREEN, 4: Color.YELLOW})

	var first = podium.slots[0]
	if first.rank_label.text != "1" or first.name_label.text != "A":
		_fail("center slot must show 1st place, got %s / %s" % [
			first.rank_label.text, first.name_label.text
		])
		return
	if first.score_label.text != "120 PTS":
		_fail("score must carry the PTS suffix, got %s" % first.score_label.text)
		return
	var fourth = podium.slots[3]
	if not fourth.visible or fourth.rank_label.text != "4":
		_fail("4th place must be shown beside the podium")
		return
	if fourth.name_label.text != "D*":
		_fail("CPU players must be marked, got %s" % fourth.name_label.text)
		return
	if fourth.score_label.text != "-25 PTS":
		_fail("negative score must keep the minus sign, got %s" % fourth.score_label.text)
		return
	if fourth.box.visible:
		_fail("4th place must not stand on a podium box")
		return
	if podium.podium_group.position.x != 0.0:
		_fail("a 4 player result must not shift the podium")
		return

	podium.apply([
		{"id": 1, "name": "A", "score": 30},
		{"id": 2, "name": "B", "score": 90},
	], {1: Color.RED, 2: Color.BLUE})
	if podium.slots[0].name_label.text != "B":
		_fail("the higher score must take the center box, got %s" % podium.slots[0].name_label.text)
		return
	if podium.slots[2].visible or podium.slots[3].visible:
		_fail("unused slots must be hidden when fewer players finish")
		return
	if podium.podium_group.position.x == 0.0:
		_fail("a 2 player result must recenter the podium")
		return

	print("result podium: ranking order, ties, and slot visibility passed")
	podium.queue_free()
	await process_frame
	quit(0)
