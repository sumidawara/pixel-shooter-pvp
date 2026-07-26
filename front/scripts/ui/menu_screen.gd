extends Control

signal join_requested(server_url: String, player_name: String)
signal cancel_connection_requested
signal create_requested(player_name: String, port: int, settings: Dictionary)
signal add_cpu_requested
signal remove_cpu_requested(player_id: int)
signal start_match_requested
signal room_settings_changed(settings: Dictionary)
signal leave_room_requested
signal quit_requested

const DEFAULT_SERVER_URL := "ws://127.0.0.1:9001"
const CURSOR_TEXTURE: Texture2D = preload("res://assets/art/cursor.png")
const PLAYER_COLORS := [
	Color("#27e5ff"),
	Color("#ff38c7"),
	Color("#ffe66d"),
	Color("#7cff6b"),
]

@onready var title_page: Control = %TitlePage
@onready var play_page: Control = %PlayPage
@onready var join_page: Control = %JoinPage
@onready var create_page: Control = %CreatePage
@onready var settings_page: Control = %SettingsPage
@onready var create_room_button: Button = %CreateRoomButton
@onready var join_button: Button = %JoinButton
@onready var server_input: LineEdit = %ServerUrlInput
@onready var port_input: SpinBox = %PortInput
@onready var player_name_input: LineEdit = %PlayerNameInput
@onready var volume_slider: HSlider = %VolumeSlider
@onready var status_label: Label = %StatusLabel
@onready var room_address_label: Label = %RoomAddressLabel
@onready var room_players: VBoxContainer = %RoomPlayers
@onready var room_waiting_label: Label = %RoomWaitingLabel
@onready var add_cpu_button: Button = %AddCpuButton
@onready var remove_cpu_button: Button = %RemoveCpuButton
@onready var start_button: Button = %StartButton
@onready var match_seconds_input: SpinBox = %MatchSecondsInput
@onready var kill_points_input: SpinBox = %KillPointsInput
@onready var death_penalty_input: SpinBox = %DeathPenaltyInput
@onready var item_points_input: SpinBox = %ItemPointsInput
@onready var max_items_input: SpinBox = %MaxItemsInput

var is_web := false
var is_room_host := false
var applying_room_snapshot := false
var last_cpu_id := 0
var is_connecting := false


func _ready() -> void:
	Input.set_custom_mouse_cursor(CURSOR_TEXTURE, Input.CURSOR_ARROW, Vector2(12, 12))
	is_web = OS.has_feature("web")
	_load_local_settings()
	var environment_url := OS.get_environment("PIXEL_SHOOTER_SERVER_URL")
	server_input.text = environment_url if not environment_url.is_empty() else DEFAULT_SERVER_URL
	create_room_button.disabled = is_web
	create_room_button.tooltip_text = "Desktop app only" if is_web else ""
	%CreateRoomHint.text = "DESKTOP APP ONLY" if is_web else "START A LOCAL SERVER"
	_bind_buttons()
	_bind_room_settings()
	show_title()


func _bind_buttons() -> void:
	%PlayButton.pressed.connect(func(): _show_page(play_page))
	%SettingsButton.pressed.connect(func(): _show_page(settings_page))
	%QuitButton.pressed.connect(func(): quit_requested.emit())
	%TitleBackButton.pressed.connect(show_title)
	%JoinBackButton.pressed.connect(_leave_join_page)
	%CreateBackButton.pressed.connect(_leave_room_to_play)
	%SettingsBackButton.pressed.connect(_save_settings_and_return)
	create_room_button.pressed.connect(_request_create_room)
	join_button.pressed.connect(_on_join_button_pressed)
	%OpenJoinButton.pressed.connect(func(): _show_page(join_page))
	server_input.text_submitted.connect(func(_value: String): request_connection())
	add_cpu_button.pressed.connect(func(): add_cpu_requested.emit())
	remove_cpu_button.pressed.connect(func(): remove_cpu_requested.emit(last_cpu_id))
	start_button.pressed.connect(_request_start_match)


func _bind_room_settings() -> void:
	for input in [
		match_seconds_input,
		kill_points_input,
		death_penalty_input,
		item_points_input,
		max_items_input,
	]:
		input.value_changed.connect(func(_value: float): _emit_room_settings())


func show_title() -> void:
	_show_page(title_page)
	status_label.text = "READY"


func show_join() -> void:
	_show_page(join_page)


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
		request_connection()


func _leave_join_page() -> void:
	if is_connecting:
		cancel_connection_requested.emit()
		set_connecting(false)
	_show_page(play_page)
	set_status("READY")


func _request_create_room() -> void:
	if is_web:
		set_status("CREATE ROOM IS AVAILABLE IN THE DESKTOP APP")
		return
	set_connecting(true)
	var port := int(port_input.value)
	show_room(true, "ws://127.0.0.1:%d" % port)
	create_requested.emit(player_name_input.text, port, get_room_settings())


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
	for index in range(sorted.size()):
		var player: Dictionary = sorted[index]
		var label := Label.new()
		var suffix := " [CPU]" if bool(player.get("is_cpu", false)) else ""
		if int(player.get("id", 0)) == host_id:
			suffix += " [HOST]"
		label.text = "%d  %s%s" % [index + 1, str(player.get("name", "PLAYER")), suffix]
		label.modulate = PLAYER_COLORS[index % PLAYER_COLORS.size()]
		room_players.add_child(label)
		if bool(player.get("is_cpu", false)):
			last_cpu_id = int(player.get("id", 0))
	for index in range(sorted.size(), max_players):
		var empty_label := Label.new()
		empty_label.text = "%d  --- WAITING ---" % (index + 1)
		empty_label.modulate = Color("#788592")
		room_players.add_child(empty_label)
	room_waiting_label.text = "WAITING FOR PLAYERS  %d/%d" % [sorted.size(), max_players]
	_update_host_controls(sorted.size(), can_start)


func get_room_settings() -> Dictionary:
	return {
		"match_seconds": match_seconds_input.value,
		"kill_points": int(kill_points_input.value),
		"death_penalty": int(death_penalty_input.value),
		"item_points": int(item_points_input.value),
		"item_spawn_interval": 5.0,
		"max_items": int(max_items_input.value),
	}


func set_connecting(connecting: bool) -> void:
	is_connecting = connecting
	# 接続中もボタンを無効化せず、同じ場所から即座に中止できるようにする。
	join_button.disabled = false
	join_button.text = "CANCEL" if connecting else "JOIN ROOM"


func set_status(text: String) -> void:
	status_label.text = text


func _show_page(page: Control) -> void:
	for candidate in [title_page, play_page, join_page, create_page, settings_page]:
		candidate.visible = candidate == page


func _leave_room_to_play() -> void:
	leave_room_requested.emit()
	_show_page(play_page)


func _update_host_controls(player_count: int, can_start: bool) -> void:
	add_cpu_button.visible = is_room_host
	remove_cpu_button.visible = is_room_host
	start_button.visible = is_room_host
	add_cpu_button.disabled = player_count >= 4
	remove_cpu_button.disabled = last_cpu_id == 0
	start_button.disabled = not can_start
	start_button.text = "START GAME (+1 CPU)" if player_count == 1 else "START GAME"
	for input in [
		match_seconds_input,
		kill_points_input,
		death_penalty_input,
		item_points_input,
		max_items_input,
	]:
		input.editable = is_room_host


func _emit_room_settings() -> void:
	if is_room_host and not applying_room_snapshot:
		room_settings_changed.emit(get_room_settings())


func _apply_room_settings(settings: Dictionary) -> void:
	if settings.is_empty():
		return
	applying_room_snapshot = true
	match_seconds_input.value = float(settings.get("match_seconds", 120.0))
	kill_points_input.value = float(settings.get("kill_points", 100))
	death_penalty_input.value = float(settings.get("death_penalty", 25))
	item_points_input.value = float(settings.get("item_points", 20))
	max_items_input.value = float(settings.get("max_items", 3))
	applying_room_snapshot = false


func _load_local_settings() -> void:
	var config := ConfigFile.new()
	var default_name := "Player-%03d" % (OS.get_process_id() % 1000)
	if config.load("user://client.cfg") == OK:
		player_name_input.text = str(config.get_value("player", "name", default_name))
		volume_slider.value = float(config.get_value("audio", "volume", 80.0))
	else:
		player_name_input.text = default_name
		volume_slider.value = 80.0
	_apply_volume()
	volume_slider.value_changed.connect(func(_value: float): _apply_volume())


func _save_settings_and_return() -> void:
	var config := ConfigFile.new()
	config.set_value("player", "name", player_name_input.text)
	config.set_value("audio", "volume", volume_slider.value)
	config.save("user://client.cfg")
	show_title()


func _apply_volume() -> void:
	var linear := maxf(volume_slider.value / 100.0, 0.0001)
	AudioServer.set_bus_volume_db(AudioServer.get_bus_index("Master"), linear_to_db(linear))
