extends SceneTree

## プレイヤーの絵が進む向きを向くかの検証。
##
## 原本は左向きに描かれているので、右へ進むときだけ左右反転する。ここを
## 取り違えると両方向とも進行方向と逆を向き、ずっと後ずさりして見える。
## 実際に一度そうなった。
##
## どちら向きに描かれているかは PlayerView.ART_FACING が持つ。目が頭の中心より
## どちらへ寄っているかで決まる造作の話で、自動では判定していない（1〜2画素の
## 違いを数値で拾おうとすると、絵を少し直しただけで誤判定する）。絵を描き直して
## 向きを変えたときは、人が ART_FACING を変える必要がある。
##
## そのぶん「反転しても絵が変わらない」状態だけは機械で塞いでおく。左右対称な
## 絵に差し替わると、向きを切り替えても何も起きなくなるため。
##
##     godot --headless --path frontend --script res://tests/player_facing_test.gd

const PLAYER_VIEW_SCENE := preload("res://src/actors/player/player_view.tscn")
const PLAYER_VIEW_SCRIPT := preload("res://src/actors/player/player_view.gd")
const PLAYER_STAND: Texture2D = preload("res://assets/aseprite/actors/player/player_stand.aseprite")

## 原本と反転した絵が、最低このくらいは違っていること。
##
## 実測で20%前後ある。半分にしても余裕があり、少し描き直したくらいでは割らない。
const MIN_MIRROR_DIFFERENCE := 0.10

## 人が向きを見て ART_FACING を決めたときの絵。
##
## どちらを向いているかは機械で判定できないので、代わりに「絵が変わったこと」を
## 検知して人へ差し戻す。絵を描き直したら、向きを目で確かめたうえでここを更新する。
const VERIFIED_ART := {
	"player_stand": "dd983a8812fa2190",
	"player_run": "2ab9525a12d3dbdd",
}

var _failures: PackedStringArray = PackedStringArray()
var _art_facing := 0.0
var _against_art := 0.0


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	_art_facing = PLAYER_VIEW_SCRIPT.ART_FACING
	_against_art = -_art_facing

	_check_flipping_actually_changes_the_picture()
	_check_the_artwork_is_the_one_someone_looked_at()
	await _check_the_sprite_turns_the_way_it_moves()
	await _check_standing_still_keeps_the_last_direction()
	await _check_both_sprites_turn_together()
	await _check_other_players_turn_too()

	if not _failures.is_empty():
		push_error("player facing:\n  " + "\n  ".join(_failures))
		quit(1)
		return
	print("player facing: 進む向きを向くことを確認した")
	quit(0)


## 左右反転すると絵が実際に変わること。
##
## 対称な絵だと、向きを切り替えても見た目が動かない。コードは正しいのに
## 直っていない、という一番気付きにくい状態になる。
func _check_flipping_actually_changes_the_picture() -> void:
	var img := PLAYER_STAND.get_image()
	var width := img.get_width()
	var height := img.get_height()
	var diff := 0
	for y in range(height):
		for x in range(width):
			if img.get_pixel(x, y) != img.get_pixel(width - 1 - x, y):
				diff += 1
	var ratio := float(diff) / float(width * height)
	if ratio < MIN_MIRROR_DIFFERENCE:
		_failures.append(
			"原本が左右対称に近い（違い %.1f%%）。反転しても向きが変わらない" % (ratio * 100.0)
		)


## 絵が、ART_FACING を決めたときのものから変わっていないこと。
##
## この検査だけは「間違いを見つける」ためではなく「人に見直させる」ためにある。
## ART_FACING が絵と合っているかはテストから判定できず、実際に左向きの絵を
## 右向きだと思い込んで、両方向とも後ずさりして見える状態を作ってしまった。
func _check_the_artwork_is_the_one_someone_looked_at() -> void:
	for name in VERIFIED_ART:
		var tex: Texture2D = load("res://assets/aseprite/actors/player/%s.aseprite" % name)
		var ctx := HashingContext.new()
		ctx.start(HashingContext.HASH_SHA256)
		ctx.update(tex.get_image().get_data())
		var digest := ctx.finish().hex_encode().substr(0, 16)
		if digest != VERIFIED_ART[name]:
			_failures.append(
				(
					"%s の絵が変わった（%s → %s）。"
					+ "どちらを向いているか目で確かめ、PlayerView.ART_FACING が"
					+ "今も正しいことを確認したうえで VERIFIED_ART を更新すること"
				) % [name, VERIFIED_ART[name], digest]
			)


## 原本の向きへ進めばそのまま、逆へ進めば反転すること。
func _check_the_sprite_turns_the_way_it_moves() -> void:
	var view = await _open_view()

	view.apply_state(_alive_player(), Color.WHITE, true, true, _art_facing)
	if view.character_sprite.flip_h:
		_failures.append("原本と同じ向きへ進んでいるのに反転している")

	view.apply_state(_alive_player(), Color.WHITE, true, true, _against_art)
	if not view.character_sprite.flip_h:
		_failures.append("原本と逆へ進んでいるのに反転していない")

	await _close(view)


## 横へ動いていない間は、直前の向きを保つこと。
##
## 0を「正面」と解釈して原本の向きへ戻すと、逆へ逃げて止まった相手が反転する。
func _check_standing_still_keeps_the_last_direction() -> void:
	var view = await _open_view()

	view.apply_state(_alive_player(), Color.WHITE, true, true, _against_art)
	view.apply_state(_alive_player(), Color.WHITE, false, true, 0.0)
	if not view.character_sprite.flip_h:
		_failures.append("向きを変えて止まると原本の向きへ戻ってしまう")

	# 止まっている間に絵を描き直しても保つこと。
	view._process(0.016)
	if not view.character_sprite.flip_h:
		_failures.append("止まったまま描き直すと原本の向きへ戻ってしまう")

	await _close(view)


## 本体と縁取りが同じ向きを向くこと。
##
## 縁取りは一回り大きい同じ絵を重ねている。片方だけ反転すると、
## 反対側にはみ出して輪郭が二重に見える。
func _check_both_sprites_turn_together() -> void:
	var view = await _open_view()

	view.apply_state(_alive_player(), Color.WHITE, true, true, _against_art)
	if view.outline_sprite.flip_h != view.character_sprite.flip_h:
		_failures.append(
			"縁取りと本体の向きが違う: 縁取り %s / 本体 %s"
			% [view.outline_sprite.flip_h, view.character_sprite.flip_h]
		)

	await _close(view)


## 自分以外のプレイヤーも向きを変えること。
##
## 自機は入力から向きを取れるが、他プレイヤーの入力は届かない。位置の変化から
## 導く必要があり、ここを落とすと「自分だけ向く」状態になる。
func _check_other_players_turn_too() -> void:
	var main = await _open_main()
	var game = main.get_node("GameScreen")
	game.start_session(1)
	game._on_map_definition_received(_map_definition())

	# まず2人を置き、表示位置を落ち着かせる。
	await _send_snapshot(game, Vector2(320.0, 176.0), Vector2(330.0, 176.0))
	if not await _settle(game, 2, Vector2(330.0, 176.0)):
		_failures.append("相手の表示位置が落ち着かない")
		await _close_main(main)
		return

	# 右へ飛ばす。補間で追いかける間、右向きになっているはず。
	await _send_snapshot(game, Vector2(320.0, 176.0), Vector2(430.0, 176.0))
	await process_frame
	if not _facing_matches(game.player_views[2], 1.0):
		_failures.append("相手が右へ動いても右を向かない")

	# 左へ戻せば左を向くこと。
	await _send_snapshot(game, Vector2(320.0, 176.0), Vector2(230.0, 176.0))
	await process_frame
	if not _facing_matches(game.player_views[2], -1.0):
		_failures.append("相手が左へ動いても左を向かない")

	await _close_main(main)


## viewが direction（1で右、-1で左）を向いているか。
func _facing_matches(view, direction: float) -> bool:
	return view.character_sprite.flip_h == (direction != _art_facing)


## 表示位置が目標へ届くまで待つ。届いたらtrue。
func _settle(game, id: int, target: Vector2) -> bool:
	for _attempt in range(600):
		await process_frame
		if (
			game.player_views.has(id)
			and game.player_views[id].global_position.distance_to(target) < 0.5
		):
			return true
	return false


func _send_snapshot(game, local: Vector2, other: Vector2) -> void:
	game._on_snapshot_received({
		"tick": 1,
		"phase": "running",
		"time_left": 90.0,
		"winner_id": null,
		"reconnect_grace_left": 0.0,
		"move_speed": 150.0,
		"dash_speed": 520.0,
		"dash_duration": 0.13,
		"dash_cooldown": 1.1,
		"players": [
			_snapshot_player(1, local),
			_snapshot_player(2, other),
		],
		"bullets": [],
		"items": [],
		"larokin_poppos": [],
		"ghost_thieves": [],
		"room": {"host_player_id": 1, "can_start": false, "max_players": 4, "settings": {}},
	})
	await process_frame


func _snapshot_player(id: int, position: Vector2) -> Dictionary:
	return {
		"id": id,
		"name": "P%d" % id,
		"position": {"x": position.x, "y": position.y},
		"aim": {"x": 1.0, "y": 0.0},
		"hp": 5,
		"max_hp": 5,
		"score": 0,
		"is_cpu": false,
		"is_dummy": false,
		"connected": true,
		"alive": true,
		"ammo": 6,
		"max_ammo": 6,
		"last_input_sequence": 0,
	}


## 絵を出すのに最低限必要な状態。倒れていると描き直さない。
func _alive_player() -> Dictionary:
	return {
		"alive": true,
		"connected": true,
		"aim": {"x": 1.0, "y": 0.0},
		"ammo": 6,
		"hp": 5,
	}


## 検査用の20×11マップ。
func _map_definition() -> Dictionary:
	var rows: Array[String] = []
	for y in range(11):
		if y == 0 or y == 10:
			rows.append("#".repeat(20))
		else:
			rows.append("#" + ".".repeat(18) + "#")
	return {
		"schema_version": 1,
		"id": "player_facing_test",
		"revision": "1",
		"name": "Player Facing Test",
		"width": 20,
		"height": 11,
		"tile_size": 32,
		"tiles": rows,
		"spawn_points": [[1, 1], [18, 9], [18, 1], [1, 9]],
		"item_spawn_points": [[5, 5], [14, 5]],
	}


func _open_view():
	var view = PLAYER_VIEW_SCENE.instantiate()
	root.add_child(view)
	await process_frame
	return view


func _close(view) -> void:
	view.queue_free()
	await process_frame


func _open_main():
	var main_scene: PackedScene = load("res://src/app/main.tscn")
	var main = main_scene.instantiate()
	root.add_child(main)
	await process_frame
	return main


func _close_main(main) -> void:
	main.queue_free()
	await process_frame
