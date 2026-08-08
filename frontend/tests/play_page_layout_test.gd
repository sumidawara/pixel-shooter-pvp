extends SceneTree

## Play画面の見え方の検証。
##
## 全部を同じ強さで見せると、どれも強調されていないのと同じになる。この画面は
## 「常時は枠を出さず、選んでいる行だけ明るくする」「戻るは一段弱くする」で
## 順位を付けている。どれも数値で決まるので、崩れたら気付けるようにする。
##
##     godot --headless --path frontend --script res://tests/play_page_layout_test.gd

var _failures: PackedStringArray = PackedStringArray()


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	await _check_actions_have_no_permanent_frame()
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


## 選択肢を常時シアンの矩形で囲わないこと。
##
## 文字より枠の面積のほうが大きいと、視線が「何を選ぶか」ではなく箱に引かれる。
## 強いシアンはフォーカスとホバーのときだけ出す。
func _check_actions_have_no_permanent_frame() -> void:
	var menu = await _open_menu()
	for name in ["CreateRoomButton", "OpenJoinButton"]:
		var button: Button = _action(menu, name)
		var normal := button.get_theme_stylebox("normal")
		if _border_width(normal) > 0:
			_failures.append("%s が常時枠を出している" % name)
		# 押せる状態だと分かる変化は要る。枠を消しただけでは、どこが操作対象か
		# 分からなくなる。
		if _border_width(button.get_theme_stylebox("hover")) <= 0:
			_failures.append("%s はホバーしても何も変わらない" % name)
		if _border_width(button.get_theme_stylebox("focus")) <= 0:
			_failures.append("%s はフォーカスしても何も変わらない" % name)
		# 高さは文字に対して間延びしない範囲に収める。
		if button.custom_minimum_size.y > 34.0:
			_failures.append(
				"%s が高すぎる: %s" % [name, button.custom_minimum_size.y]
			)
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
