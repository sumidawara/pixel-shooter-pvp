extends SceneTree

## Play画面の見え方の検証。
##
## 全部を同じ強さで見せると、どれも強調されていないのと同じになる。この画面は
## 「常時は枠を出さず、選んでいる行だけ明るくする」「戻るは一段弱くする」で
## 順位を付けている。どれも数値で決まるので、崩れたら気付けるようにする。
##
##     godot --headless --path frontend --script res://tests/play_page_layout_test.gd

## 選んでいる行の枠が、休んでいる行の枠より何倍強ければ「はっきり違う」とするか。
##
## わずかでも強ければ合格にすると、全部の枠を最大の強さで出しておいて選択時だけ
## 1px太らせる、という元の状態に戻せてしまう。倍率そのものは目分量だが、
## 「差がある」ではなく「差が分かる」を要求するために置いている。
const CHOSEN_FRAME_RATIO := 1.5

var _failures: PackedStringArray = PackedStringArray()


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	await _check_the_chosen_action_outshines_the_resting_ones()
	await _check_the_hint_only_appears_when_it_matters()
	await _check_only_the_chosen_row_is_marked()
	await _check_back_is_weaker_than_the_actions()
	await _check_the_panel_hides_the_backdrop()
	await _check_opening_the_page_puts_focus_somewhere()

	if not _failures.is_empty():
		push_error("play page:\n  " + "\n  ".join(_failures))
		quit(1)
		return
	print("play page: 枠・印・順位が期待どおりだった")
	quit(0)


## 主操作は枠で囲うが、選んでいる行の枠のほうが強いこと。
##
## 枠があること自体は、押せる場所を示すのに役立つ。問題になるのは全部が
## 同じ強さで主張することなので、休んでいる枠は落とし、選んだ行だけを立てる。
func _check_the_chosen_action_outshines_the_resting_ones() -> void:
	var menu = await _open_menu()
	for name in ["CreateRoomButton", "OpenJoinButton"]:
		var button: Button = _action(menu, name)
		var resting := _frame_strength(button.get_theme_stylebox("normal"))
		if resting <= 0.0:
			_failures.append("%s に枠が無い。押せる場所が分からない" % name)
		for state in ["hover", "focus"]:
			var chosen := _frame_strength(button.get_theme_stylebox(state))
			if chosen < resting * CHOSEN_FRAME_RATIO:
				_failures.append(
					"%s の %s が休んでいるときと大差ない: %.1f / %.1f"
					% [name, state, chosen, resting]
				)
		# 高さは文字に対して間延びしない範囲に収める。
		if button.custom_minimum_size.y > 34.0:
			_failures.append(
				"%s が高すぎる: %s" % [name, button.custom_minimum_size.y]
			)
	await _close(menu)


## 補足の1行は、押せないときだけ出ること。
##
## 押せるボタンの下に常時1行あると、選択肢そのものより先に目へ入るうえ、
## 読んでも何もすることがない。押せない理由なら読む意味がある。
func _check_the_hint_only_appears_when_it_matters() -> void:
	var menu = await _open_menu()
	var hint: Label = menu.play_page.get_node("Panel/Margin/Content/CreateRoomHint")
	var create: Button = _action(menu, "CreateRoomButton")

	if create.disabled != hint.visible:
		_failures.append(
			"押せる状態と補足の表示が噛み合っていない: disabled=%s, hint=%s"
			% [create.disabled, hint.visible]
		)
	if hint.visible and not hint.text.strip_edges().contains("DESKTOP"):
		_failures.append("押せない理由が書かれていない: %s" % hint.text)

	await _close(menu)


## 印が出るのは、いま選んでいる1行だけであること。
func _check_only_the_chosen_row_is_marked() -> void:
	var menu = await _open_menu()
	menu._show_page(menu.play_page)
	await process_frame
	await process_frame

	var rows := _rows(menu)
	if rows.size() < 3:
		_failures.append("検査の前提が崩れている: 行が%d個しかない" % rows.size())
		await _close(menu)
		return

	_action(menu, "CreateRoomButton").grab_focus()
	await process_frame
	var marked := _marked_rows(rows)
	if marked != ["CreateRow"]:
		_failures.append("CREATE ROOM を選んでいるのに印が %s" % [marked])

	_action(menu, "OpenJoinButton").grab_focus()
	await process_frame
	marked = _marked_rows(rows)
	if marked != ["JoinRow"]:
		_failures.append("JOIN ROOM を選んでいるのに印が %s" % [marked])

	# 戻る行の印は向きが違う。同じ ">" だと、進む操作に見える。
	var back_row = _row(menu, "BackRow")
	_action(menu, "TitleBackButton").grab_focus()
	await process_frame
	if back_row.get_node("Marker").text != "<":
		_failures.append("BACK の印が戻る向きでない: %s" % back_row.get_node("Marker").text)

	await _close(menu)


## BACKが主操作より弱く見えること。
##
## 同じ強さだと、戻るのが主操作と並んで見えて、どこから読めばいいか決まらない。
func _check_back_is_weaker_than_the_actions() -> void:
	var menu = await _open_menu()
	var create: Button = _action(menu, "CreateRoomButton")
	var back: Button = _action(menu, "TitleBackButton")

	for state in ["normal", "hover", "focus", "pressed"]:
		if _border_width(back.get_theme_stylebox(state)) > 0:
			_failures.append("BACK が %s で枠を出している" % state)
	if back.get_theme_font_size("font_size") >= create.get_theme_font_size("font_size"):
		_failures.append("BACK の字が主操作と同じか大きい")
	var back_color := back.get_theme_color("font_color")
	var create_color := create.get_theme_color("font_color")
	if _brightness(back_color) >= _brightness(create_color):
		_failures.append("BACK が主操作より暗くない")

	await _close(menu)


## 前面のパネルが背景を透かさないこと。
##
## 背景には罫線もノイズも大きな文字もある。透けると、その上の文字と重なって読めない。
func _check_the_panel_hides_the_backdrop() -> void:
	var menu = await _open_menu()
	var panel: PanelContainer = menu.play_page.get_node("Panel")
	var style := panel.get_theme_stylebox("panel")
	if not style is StyleBoxFlat:
		_failures.append("パネルの背景が確かめられない")
	elif style.bg_color.a < 0.98:
		_failures.append("パネルが透けている: a=%s" % style.bg_color.a)

	# パネルの後ろも一段落とす。背景そのものが騒がしいままだと、
	# パネルの外へ視線が逃げる。
	if not menu.play_page.has_node("PageScrim"):
		_failures.append("背景を落とす覆いが無い")

	await _close(menu)


## Playを開いたら、どこかにフォーカスがあること。
##
## 選んでいる行だけを明るくする画面なので、どこにも無いと全部暗いままになる。
func _check_opening_the_page_puts_focus_somewhere() -> void:
	var menu = await _open_menu()
	menu._show_page(menu.play_page)
	await process_frame
	await process_frame

	var focused: Control = menu.get_viewport().gui_get_focus_owner()
	if focused != _action(menu, "CreateRoomButton"):
		_failures.append("開いた直後に最初の操作へフォーカスが無い: %s" % focused)

	await _close(menu)


func _rows(menu) -> Array:
	var content = menu.play_page.get_node("Panel/Margin/Content")
	return content.get_children().filter(func(child): return child.has_method("is_active"))


func _row(menu, row_name: String):
	return menu.play_page.get_node("Panel/Margin/Content/%s" % row_name)


func _marked_rows(rows: Array) -> Array:
	var marked: Array = []
	for row in rows:
		if not str(row.get_node("Marker").text).is_empty():
			marked.append(str(row.name))
	return marked


func _action(menu, button_name: String) -> Button:
	for row in _rows(menu):
		for child in row.get_children():
			if child is Button and str(child.name) == button_name:
				return child
	_failures.append("ボタンが見つからない: %s" % button_name)
	return null


## 枠の強さ。4辺の太さの合計と、色の明るさ・濃さを掛ける。
##
## 一番太い辺だけで見ると、選択時に1辺を1px太らせただけで「強くなった」ことに
## なってしまう。実際に目に入るのは枠全体の量なので、合計で見る。
func _frame_strength(style: StyleBox) -> float:
	if not style is StyleBoxFlat:
		return 0.0
	var total: int = (
		style.border_width_left + style.border_width_right
		+ style.border_width_top + style.border_width_bottom
	)
	var color: Color = style.border_color
	return float(total) * color.a * _brightness(color)


func _border_width(style: StyleBox) -> int:
	if style is StyleBoxFlat:
		return maxi(
			maxi(style.border_width_left, style.border_width_right),
			maxi(style.border_width_top, style.border_width_bottom)
		)
	return 0


func _brightness(color: Color) -> float:
	return color.r + color.g + color.b


func _open_menu():
	var main_scene: PackedScene = load("res://src/app/main.tscn")
	var main = main_scene.instantiate()
	root.add_child(main)
	await process_frame
	return main.get_node("MenuScreen")


func _close(menu) -> void:
	menu.get_parent().queue_free()
	await process_frame
