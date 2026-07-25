extends Node

@onready var menu_screen := %MenuScreen
@onready var game_screen := %GameScreen


func _ready() -> void:
	get_window().title = "Pixel Shooter PvP"
	menu_screen.connect_requested.connect(_on_connect_requested)
	game_screen.exit_requested.connect(_on_exit_requested)
	NetworkClient.status_changed.connect(_on_status_changed)
	NetworkClient.welcome_received.connect(_on_welcome_received)
	NetworkClient.rejected.connect(_on_rejected)
	_show_menu()
	if OS.get_environment("PIXEL_SHOOTER_AUTOCONNECT") == "1":
		menu_screen.request_connection()


func _on_connect_requested(server_url: String, player_name: String) -> void:
	menu_screen.set_connecting(true)
	NetworkClient.connect_to_server(server_url, player_name)


func _on_welcome_received(player_id: int, reconnected: bool) -> void:
	if game_screen.visible and reconnected:
		game_screen.resume_session(player_id)
	else:
		game_screen.start_session(player_id)
	menu_screen.visible = false
	game_screen.visible = true


func _on_rejected(reason: String) -> void:
	_show_menu()
	menu_screen.set_status(reason)


func _on_status_changed(text: String) -> void:
	menu_screen.set_status(text)
	game_screen.set_connection_status(text)


func _on_exit_requested() -> void:
	NetworkClient.disconnect_from_server()
	game_screen.end_session()
	_show_menu()


func _show_menu() -> void:
	menu_screen.visible = true
	menu_screen.set_connecting(false)
	game_screen.visible = false
