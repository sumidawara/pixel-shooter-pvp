extends Control

signal join_requested(server_url: String, player_name: String)
## ロビーの接続先が変わった。一覧を取り直す。
signal lobby_url_changed(lobby_url: String)
## 一覧から部屋を選んだ。ロビーに入場券を出させてから繋ぐ。
signal room_chosen(game_url: String, player_name: String)
signal cancel_connection_requested
signal create_requested(player_name: String, port: int)
signal add_cpu_requested(level: int)
signal remove_cpu_requested(player_id: int)
## 既に居るCPUの強さを変える。
signal cpu_level_changed(player_id: int, level: int)
## 自分の色を選ぶ。
signal color_chosen(color: int)
signal start_match_requested
signal room_settings_changed(settings: Dictionary)
signal crt_preset_changed(preset_id: String)
signal leave_room_requested
signal quit_requested

const CURSOR_TEXTURE: Texture2D = preload("res://assets/aseprite/ui/menu/cursor.aseprite")
## プレイヤーを見分ける色。並びはサーバーが配る color の番号に対応する。
##
## GameScreen.PLAYER_COLORS と同じ並びでなければならない。数がサーバーと
## 揃っているかは shared_limits_test が見る。
const PLAYER_COLORS := [
	Color("#27e5ff"),
	Color("#ff38c7"),
	Color("#ffe66d"),
	Color("#7cff6b"),
]
const PLAYER_COLOR_NAMES := ["CYAN", "MAGENTA", "YELLOW", "GREEN"]
## 新しく足すCPUの強さ。前回選んだものを覚えておく。
const DEFAULT_CPU_LEVEL := 3
## CPUの強さの選択肢。数値の中身はサーバーが持ち、ここは番号だけを選ぶ。
##
## 名前を付けているのは、番号だけだと何が変わるのか分からないため。
const CPU_LEVEL_LABELS := [
	"1  ROOKIE",
	"2  REGULAR",
	"3  VETERAN",
	"4  ACE",
]

## この操作を押したら、Play画面のフォーカスを最初の行から始める。
const FOCUS_START_ACTIONS := [
	"ui_up", "ui_down", "ui_focus_next", "ui_focus_prev", "ui_accept",
]

const CRT_PRESET_IDS := ["weak", "standard", "strong"]
const CRT_PRESET_LABELS := ["WEAK", "STANDARD", "STRONG"]


@onready var title_page: Control = %TitlePage
@onready var room_list_page: Control = %RoomListPage
## 接続先を変えるモーダル。ルーム一覧の上に重ねて出す。
@onready var join_page: Control = %JoinPage
@onready var lobby_label: Label = %LobbyLabel
@onready var change_server_button: Button = %ChangeServerButton
@onready var room_list_status: Label = %RoomListStatus
@onready var room_list_box: VBoxContainer = %RoomListBox
@onready var refresh_button: Button = %RefreshButton
@onready var create_page: Control = %CreatePage
@onready var settings_page: Control = %SettingsPage
@onready var play_button: Button = %PlayButton
@onready var title_join_button: Button = %TitleJoinButton
@onready var play_hint: Label = %PlayHint
@onready var join_button: Button = %JoinButton
@onready var server_input: LineEdit = %ServerUrlInput
@onready var port_input: SpinBox = %PortInput
@onready var public_host_input: LineEdit = %PublicHostInput
@onready var public_host_hint: Label = %PublicHostHint
@onready var player_name_input: LineEdit = %PlayerNameInput
@onready var crt_preset_option: OptionButton = %CrtPresetOption
@onready var volume_slider: HSlider = %VolumeSlider
@onready var status_label: Label = %StatusLabel
@onready var room_address_label: Label = %RoomAddressLabel
@onready var room_players: VBoxContainer = %RoomPlayers
@onready var room_waiting_label: Label = %RoomWaitingLabel
@onready var add_cpu_button: Button = %AddCpuButton
@onready var remove_cpu_button: Button = %RemoveCpuButton
@onready var start_button: Button = %StartButton
@onready var map_option: OptionButton = %MapOption
@onready var match_seconds_input: SpinBox = %MatchSecondsInput
@onready var kill_points_input: SpinBox = %KillPointsInput
@onready var death_penalty_input: SpinBox = %DeathPenaltyInput
@onready var item_points_input: SpinBox = %ItemPointsInput
@onready var max_items_input: SpinBox = %MaxItemsInput
@onready var sandbox_check: CheckBox = %SandboxCheck
@onready var advanced_toggle: Button = %AdvancedToggle
@onready var advanced_box: VBoxContainer = %AdvancedBox

var is_web := false
var is_room_host := false
var applying_room_snapshot := false
## サーバーから最後に届いたルーム設定。
##
## 画面に無い項目は、こちらで値を作らずここからそのまま返す。作ってしまうと
## server.json に書いた値を、クライアントの思い込みで上書きすることになる
## （`item_spawn_interval` が実際にそうなっていた）。
var _room_settings: Dictionary = {}
## サーバーから一度でも設定が届いたか。届く前は送らない。
var _received_room_settings := false
## 設定名 → 編集するSpinBoxと、整数かどうか。
##
## 取り出しと書き戻しを1つの表から作る。別々に書くと、片方だけ足して
## もう片方を忘れる。
var _setting_inputs: Dictionary = {}
## いま一覧を引いているロビーのURL。
var lobby_url := ""
var last_cpu_id := 0
var next_cpu_level := DEFAULT_CPU_LEVEL
var is_connecting := false
var selected_map_id := "classic_arena"


func _ready() -> void:
	Input.set_custom_mouse_cursor(CURSOR_TEXTURE, Input.CURSOR_ARROW, Vector2(12, 12))
	_configure_crt_preset_option()
	is_web = OS.has_feature("web")
	_load_local_settings()
	lobby_url = NetworkConfig.initial_connection_url()
	server_input.text = lobby_url
	# web版はサーバーを起動できない。部屋に入ることはできるので、
	# 押せない選択肢は残したまま理由を添える。黙って消すと、
	# 別の端末では見えているものが無いことに気付けない。
	play_button.disabled = is_web
	# 説明は押せないときだけ出す。押せるボタンの下に常時1行あると、
	# 選択肢そのものより先に目へ入るうえ、読んでも何もすることがない。
	play_hint.visible = is_web
	_bind_buttons()
	set_available_maps([{"id": "classic_arena", "name": "Classic Arena"}])
	_bind_room_settings()
	show_title()


func _bind_buttons() -> void:
	# PLAY は部屋を作る操作そのものにする。作る／入るの二択を先に迫らない。
	play_button.pressed.connect(_request_create_room)
	title_join_button.pressed.connect(show_room_list)
	change_server_button.pressed.connect(_open_server_modal)
	refresh_button.pressed.connect(func(): lobby_url_changed.emit(lobby_url))
	%RoomListBackButton.pressed.connect(show_title)
	%SettingsButton.pressed.connect(func(): _show_page(settings_page))
	%QuitButton.pressed.connect(func(): quit_requested.emit())
	%JoinBackButton.pressed.connect(_close_server_modal)
	%CreateBackButton.pressed.connect(_leave_room_to_title)
	crt_preset_option.item_selected.connect(_on_crt_preset_selected)
	%SettingsBackButton.pressed.connect(_save_settings_and_return)
	join_button.pressed.connect(_on_join_button_pressed)
	server_input.text_submitted.connect(func(_value: String): _apply_server_url())
	advanced_toggle.pressed.connect(_toggle_advanced)
	add_cpu_button.pressed.connect(func(): add_cpu_requested.emit(next_cpu_level))
	remove_cpu_button.pressed.connect(func(): remove_cpu_requested.emit(last_cpu_id))
	start_button.pressed.connect(_request_start_match)


func _bind_room_settings() -> void:
	_setting_inputs = {
		"match_seconds": {"input": match_seconds_input, "integer": false},
		"kill_points": {"input": kill_points_input, "integer": true},
		"death_penalty": {"input": death_penalty_input, "integer": true},
		"item_points": {"input": item_points_input, "integer": true},
		"max_items": {"input": max_items_input, "integer": true},
	}
	map_option.item_selected.connect(_on_map_selected)
	sandbox_check.toggled.connect(func(_pressed: bool): _emit_room_settings())
	for spec in _setting_inputs.values():
		spec["input"].value_changed.connect(func(_value: float): _emit_room_settings())


func show_title() -> void:
	_show_page(title_page)
	status_label.text = "READY"
	play_button.call_deferred("grab_focus")


func show_join() -> void:
	show_room_list()


## ルーム一覧を開き、取り直す。
func show_room_list() -> void:
	_show_page(room_list_page)
	lobby_label.text = lobby_url
	set_room_list_status("SEARCHING...")
	lobby_url_changed.emit(lobby_url)


## 接続先を変えるモーダルを、一覧の上に重ねて出す。
##
## 一覧を消してしまうと「今どこを見ているのか」が分からなくなる。
func _open_server_modal() -> void:
	server_input.text = lobby_url
	join_page.visible = true
	server_input.call_deferred("grab_focus")


func _close_server_modal() -> void:
	if is_connecting:
		cancel_connection_requested.emit()
		set_connecting(false)
	join_page.visible = false


## モーダルで入れたURLを採用し、一覧を取り直す。
func _apply_server_url() -> void:
	var next := server_input.text.strip_edges()
	if next.is_empty():
		set_status("ENTER A LOBBY ADDRESS")
		return
	lobby_url = next
	join_page.visible = false
	show_room_list()


func set_room_list_status(text: String) -> void:
	room_list_status.text = text


## 一覧を並べ直す。
##
## 満室と試合中は押せなくする。押してから断られるより、押せないほうが早く分かる。
func show_rooms(rooms: Array) -> void:
	for child in room_list_box.get_children():
		child.queue_free()
	if rooms.is_empty():
		set_room_list_status("NO OPEN ROOMS")
		return
	set_room_list_status("%d ROOM(S)" % rooms.size())
	for room in rooms:
		if typeof(room) != TYPE_DICTIONARY:
			continue
		room_list_box.add_child(_build_room_button(room))


func _build_room_button(room: Dictionary) -> Button:
	var button := Button.new()
	var host := str(room.get("host_name", "")).strip_edges()
	var open := bool(room.get("accepting_players", false))
	button.text = "%-16s %d/%d  %s" % [
		host if not host.is_empty() else "NO HOST",
		int(room.get("player_count", 0)),
		int(room.get("max_players", 4)),
		"OPEN" if open else "IN MATCH",
	]
	button.alignment = HORIZONTAL_ALIGNMENT_LEFT
	button.action_mode = BaseButton.ACTION_MODE_BUTTON_PRESS
	button.disabled = not open
	var game_url := str(room.get("game_url", ""))
	button.pressed.connect(func(): _join_room_at(game_url))
	return button


## 一覧から選んだ部屋へ入る。
##
## 一覧は必ず古い。押した瞬間に埋まっていることがあるので、断られたら
## 一覧へ戻して取り直す（`show_room_failed`）。
func _join_room_at(game_url: String) -> void:
	if game_url.is_empty():
		set_status("THIS ROOM HAS NO ADDRESS")
		return
	set_connecting(true)
	room_chosen.emit(game_url, player_name_input.text)


## ルームを開けなかったので、選択画面へ戻す。
##
## ルーム画面に残すと、ADD CPU も START GAME も効かない画面で詰む。
## 原因は status に出るが、そこから抜ける手段が LEAVE ROOM しかない状態になる。
## ルームへ入れなかったので、一覧へ戻して取り直す。
##
## 一覧はGameServerの報告間隔ぶん遅れるため、押した瞬間に満室は普通に起こる。
## タイトルまで戻すと、選び直すのに最初からやり直すことになる。
func show_room_failed(reason: String) -> void:
	is_room_host = false
	show_room_list()
	set_status(reason)


## 実際に使っている接続先を表示し直す。
##
## ルーム画面はサーバーの起動を待たずに開くので、希望のポートが埋まって
## 別の番号になった場合、最初に出した表示が嘘になる。
## 他の人はこの表示を見て JOIN ROOM に入力するため、必ず合わせる。
func set_room_address(address: String) -> void:
	room_address_label.text = address


func show_room(hosting: bool, address: String) -> void:
	is_room_host = hosting
	_show_page(create_page)
	room_address_label.text = address
	room_waiting_label.text = "CONNECTING TO ROOM..."
	_update_host_controls(0, false)


func request_connection() -> void:
	set_connecting(true)
	join_requested.emit(server_input.text, player_name_input.text)


func _on_join_button_pressed() -> void:
	if is_connecting:
		cancel_connection_requested.emit()
		set_connecting(false)
		set_status("CONNECTION CANCELLED")
	else:
		_apply_server_url()


func _request_create_room() -> void:
	if is_web:
		set_status("CREATE ROOM IS AVAILABLE IN THE DESKTOP APP")
		return
	set_connecting(true)
	var port := int(port_input.value)
	show_room(true, NetworkConfig.local_game_server_url(port))
	# 設定は送らない。この時点の画面の値はシーンの初期値であり、サーバーが
	# server.json から決めた設定を上書きしてしまう。設定はサーバーから届く。
	create_requested.emit(player_name_input.text, port)


func _request_start_match() -> void:
	if not is_room_host or start_button.disabled:
		return
	print("START GAME pressed: sending start_match")
	start_match_requested.emit()


func apply_room_snapshot(players: Array, room: Dictionary, local_player_id: int) -> void:
	var host_id := int(room.get("host_player_id", 0))
	is_room_host = host_id == local_player_id
	var max_players := int(room.get("max_players", 4))
	var can_start := bool(room.get("can_start", false))
	var settings: Dictionary = room.get("settings", {})
	_apply_room_settings(settings)
	for child in room_players.get_children():
		child.queue_free()
	last_cpu_id = 0
	# 切断済みの人を含む古いSnapshotを受け取ってもロビーには表示しない。
	# CPUはネットワーク接続を持たないため、is_cpuなら参加中として扱う。
	var active_players := players.filter(func(player):
		return bool(player.get("is_cpu", false)) or bool(player.get("connected", false))
	)
	var sorted := active_players.duplicate()
	sorted.sort_custom(func(a, b): return int(a.get("id", 0)) < int(b.get("id", 0)))
	var taken_colors: Array[int] = []
	for player in sorted:
		taken_colors.append(int(player.get("color", 0)))
	for player in sorted:
		room_players.add_child(_build_player_row(player, host_id, local_player_id, taken_colors))
		if bool(player.get("is_cpu", false)):
			last_cpu_id = int(player.get("id", 0))
	for index in range(sorted.size(), max_players):
		var empty_label := Label.new()
		empty_label.text = "---  WAITING  ---"
		empty_label.modulate = Color("#315f3b")
		room_players.add_child(empty_label)
	room_waiting_label.text = "WAITING FOR PLAYERS  %d/%d" % [sorted.size(), max_players]
	_update_host_controls(sorted.size(), can_start)


## 参加者1人分の行。
##
## 色とCPUの強さを、それが属する行の中に置く。以前は強さが試合ルールの欄にあり、
## 「この試合の決まり」と「この1体の性質」が同じ並びに混ざっていた。
func _build_player_row(
	player: Dictionary, host_id: int, local_player_id: int, taken_colors: Array[int]
) -> Control:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 8)

	var swatch := ColorRect.new()
	swatch.custom_minimum_size = Vector2(10, 10)
	swatch.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	swatch.color = PLAYER_COLORS[int(player.get("color", 0)) % PLAYER_COLORS.size()]
	row.add_child(swatch)

	var name_label := Label.new()
	name_label.text = str(player.get("name", "PLAYER"))
	name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_child(name_label)

	var player_id := int(player.get("id", 0))
	if player_id == host_id:
		row.add_child(_tag_label("HOST"))

	if bool(player.get("is_dummy", false)):
		row.add_child(_tag_label("DUMMY"))
	elif bool(player.get("is_cpu", false)):
		row.add_child(_cpu_level_picker(player_id, int(player.get("cpu_level", DEFAULT_CPU_LEVEL))))
	elif player_id == local_player_id:
		row.add_child(_color_picker(int(player.get("color", 0)), taken_colors))
	return row


func _tag_label(text: String) -> Label:
	var label := Label.new()
	label.text = text
	label.modulate = Color("#6f8a93")
	return label


## そのCPU1体の強さを選ぶ。ホスト以外は読むだけ。
func _cpu_level_picker(player_id: int, level: int) -> OptionButton:
	var picker := OptionButton.new()
	for label in CPU_LEVEL_LABELS:
		picker.add_item(label)
	picker.select(clampi(level - 1, 0, CPU_LEVEL_LABELS.size() - 1))
	picker.disabled = not is_room_host
	picker.item_selected.connect(func(index: int):
		next_cpu_level = index + 1
		cpu_level_changed.emit(player_id, index + 1)
	)
	return picker


## 自分の色を選ぶ。他の人が持っている色は選べないように見せる。
##
## 押せてしまうとサーバーに無視されるだけで、なぜ変わらないのか分からない。
func _color_picker(color: int, taken_colors: Array[int]) -> OptionButton:
	var picker := OptionButton.new()
	for index in range(PLAYER_COLOR_NAMES.size()):
		picker.add_item(PLAYER_COLOR_NAMES[index])
		if index != color and taken_colors.has(index):
			picker.set_item_disabled(index, true)
	picker.select(clampi(color, 0, PLAYER_COLOR_NAMES.size() - 1))
	picker.item_selected.connect(func(index: int): color_chosen.emit(index))
	return picker


## サーバーへ送るルーム設定。
##
## サーバーから届いた設定を土台にし、画面で編集できる項目だけを上書きする。
## 画面に無い項目をこちらで作ると、server.json の値を潰してしまう。
func get_room_settings() -> Dictionary:
	var settings := _room_settings.duplicate()
	settings["map_id"] = selected_map_id
	settings["sandbox"] = sandbox_check.button_pressed
	for key in _setting_inputs:
		var spec: Dictionary = _setting_inputs[key]
		var value: float = spec["input"].value
		settings[key] = int(value) if spec["integer"] else value
	return settings


func set_connecting(connecting: bool) -> void:
	is_connecting = connecting
	# 接続中もボタンを無効化せず、同じ場所から即座に中止できるようにする。
	join_button.disabled = false
	join_button.text = "CANCEL" if connecting else "SEARCH"


func set_status(text: String) -> void:
	status_label.text = text


func _show_page(page: Control) -> void:
	for candidate in [title_page, room_list_page, create_page, settings_page]:
		candidate.visible = candidate == page
	# モーダルはページの切り替えで残さない。
	join_page.visible = false


## キーボードで操作を始めたときだけ、最初の行へフォーカスを移す。
##
## どこにもフォーカスが無い状態では、方向キーの移動先が決まらず何も起きない。
## マウスの人には最初から光らせず、キーを押した人にだけ起点を与える。
func _unhandled_input(event: InputEvent) -> void:
	if not title_page.visible or get_viewport().gui_get_focus_owner() != null:
		return
	for action in FOCUS_START_ACTIONS:
		if event.is_action_pressed(action):
			_focus_first_action()
			get_viewport().set_input_as_handled()
			return


func _focus_first_action() -> void:
	var first: Button = title_join_button if play_button.disabled else play_button
	first.call_deferred("grab_focus")


## 点数まわりの詳細を開閉する。
##
## 既定は閉じておく。試合を始めるのに要る判断は「誰と」「どこで」であって、
## 撃破点が何点かではない。畳んでおかないと、決めなくてよいものが
## 決めるべきものと同じ大きさで並ぶ。
func _toggle_advanced() -> void:
	advanced_box.visible = not advanced_box.visible
	advanced_toggle.text = "v ADVANCED" if advanced_box.visible else "> ADVANCED"


func _leave_room_to_title() -> void:
	leave_room_requested.emit()
	_show_page(title_page)


func _update_host_controls(player_count: int, can_start: bool) -> void:
	add_cpu_button.visible = is_room_host
	remove_cpu_button.visible = is_room_host
	start_button.visible = is_room_host
	add_cpu_button.disabled = player_count >= 4
	remove_cpu_button.disabled = last_cpu_id == 0
	start_button.disabled = not can_start
	# 練習場では空きスロットが的で埋まるので、相手のCPUは足されない。
	if sandbox_check.button_pressed:
		start_button.text = "START SANDBOX"
	else:
		start_button.text = "START GAME (+1 CPU)" if player_count == 1 else "START GAME"
	map_option.disabled = not is_room_host
	sandbox_check.disabled = not is_room_host
	for input in [
		match_seconds_input,
		kill_points_input,
		death_penalty_input,
		item_points_input,
		max_items_input,
	]:
		input.editable = is_room_host


func _emit_room_settings() -> void:
	# サーバーから設定が届く前は送らない。届く前の画面の値はシーンに書かれた
	# 初期値でしかなく、送ると server.json に書いた値をそれで潰してしまう。
	if not _received_room_settings or applying_room_snapshot or not is_room_host:
		return
	room_settings_changed.emit(get_room_settings())


func _apply_room_settings(settings: Dictionary) -> void:
	if settings.is_empty():
		return
	applying_room_snapshot = true
	# 画面に無い項目も含めて丸ごと覚える。送り返すときの土台になる。
	_room_settings = settings.duplicate()
	_received_room_settings = true
	selected_map_id = str(settings.get("map_id", "classic_arena"))
	_select_map(selected_map_id)
	for key in _setting_inputs:
		if settings.has(key):
			_setting_inputs[key]["input"].value = float(settings[key])
	sandbox_check.button_pressed = bool(settings.get("sandbox", false))
	applying_room_snapshot = false


func set_available_maps(maps: Array) -> void:
	applying_room_snapshot = true
	map_option.clear()
	for map in maps:
		if typeof(map) != TYPE_DICTIONARY:
			continue
		var id := str(map.get("id", "")).strip_edges()
		if id.is_empty():
			continue
		map_option.add_item(str(map.get("name", id)))
		map_option.set_item_metadata(map_option.item_count - 1, id)
	if map_option.item_count == 0:
		map_option.add_item("Classic Arena")
		map_option.set_item_metadata(0, "classic_arena")
	_select_map(selected_map_id)
	applying_room_snapshot = false


func _select_map(map_id: String) -> void:
	for index in range(map_option.item_count):
		if str(map_option.get_item_metadata(index)) == map_id:
			map_option.select(index)
			selected_map_id = map_id
			return
	if map_option.item_count > 0:
		map_option.select(0)
		selected_map_id = str(map_option.get_item_metadata(0))


func _on_map_selected(index: int) -> void:
	selected_map_id = str(map_option.get_item_metadata(index))
	_emit_room_settings()

func _configure_crt_preset_option() -> void:
	crt_preset_option.clear()
	for index in range(CRT_PRESET_IDS.size()):
		crt_preset_option.add_item(CRT_PRESET_LABELS[index])
		crt_preset_option.set_item_metadata(index, CRT_PRESET_IDS[index])
	crt_preset_option.action_mode = BaseButton.ACTION_MODE_BUTTON_PRESS


func get_crt_preset() -> String:
	if crt_preset_option.item_count == 0:
		return "standard"
	var preset_id := str(crt_preset_option.get_item_metadata(crt_preset_option.selected))
	return preset_id if preset_id in CRT_PRESET_IDS else "standard"


func _select_crt_preset(preset_id: String) -> void:
	var normalized_id := preset_id if preset_id in CRT_PRESET_IDS else "standard"
	for index in range(crt_preset_option.item_count):
		if str(crt_preset_option.get_item_metadata(index)) == normalized_id:
			crt_preset_option.select(index)
			return
	crt_preset_option.select(1)


func _on_crt_preset_selected(_index: int) -> void:
	crt_preset_changed.emit(get_crt_preset())


func _load_local_settings() -> void:
	var config := ConfigFile.new()
	var default_name := "Player-%03d" % (OS.get_process_id() % 1000)
	var crt_preset_id := "standard"
	if config.load("user://client.cfg") == OK:
		player_name_input.text = str(config.get_value("player", "name", default_name))
		volume_slider.value = float(config.get_value("audio", "volume", 80.0))
		crt_preset_id = str(config.get_value("display", "crt_preset", "standard"))
		public_host_input.text = str(config.get_value("network", "public_host", ""))
	else:
		player_name_input.text = default_name
		volume_slider.value = 80.0
	_select_crt_preset(crt_preset_id)
	_update_public_host_hint()
	public_host_input.text_changed.connect(func(_text: String): _update_public_host_hint())
	_apply_volume()
	volume_slider.value_changed.connect(func(_value: float): _apply_volume())


## 他の人から見える自分のアドレス。
##
## 空なら同じLAN内のアドレスを自動で使う。外から入ってもらう場合だけ、
## ポート開放したうえでここへ書く。ホスト名だけなら実際に開いたポートが付き、
## `host:port` と書けばその番号がそのまま使われる。
func get_public_host() -> String:
	var typed := public_host_input.text.strip_edges()
	return typed if not typed.is_empty() else NetworkConfig.local_network_host()


## 今どのアドレスで名乗るのかを、設定画面に出す。
##
## 「AUTO」とだけ書いてあると、何が使われるのか確かめる手段が無い。
func _update_public_host_hint() -> void:
	var host := get_public_host()
	if host.is_empty():
		public_host_hint.text = "NO NETWORK ADDRESS FOUND. ONLY THIS PC CAN JOIN."
	else:
		public_host_hint.text = "OTHERS WILL CONNECT TO %s" % host


func _save_settings_and_return() -> void:
	var config := ConfigFile.new()
	config.set_value("network", "public_host", public_host_input.text.strip_edges())
	config.set_value("player", "name", player_name_input.text)
	config.set_value("audio", "volume", volume_slider.value)
	config.set_value("display", "crt_preset", get_crt_preset())
	config.save("user://client.cfg")
	show_title()


func _apply_volume() -> void:
	var linear := maxf(volume_slider.value / 100.0, 0.0001)
	AudioServer.set_bus_volume_db(AudioServer.get_bus_index("Master"), linear_to_db(linear))
