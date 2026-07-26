extends SceneTree


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	var game_scene: PackedScene = load("res://src/game_modes/match/game_screen.tscn")
	var game = game_scene.instantiate()
	root.add_child(game)
	await process_frame

	game.visible = true
	game.session_active = true
	var exit_count := [0]
	game.exit_requested.connect(func(): exit_count[0] += 1)

	var escape := InputEventAction.new()
	escape.action = "ui_cancel"
	escape.pressed = true
	game._unhandled_input(escape)
	if not game.exit_confirm_modal.is_open() or exit_count[0] != 0:
		push_error("exit modal: first Escape must open the modal without leaving")
		quit(1)
		return

	game._unhandled_input(escape)
	if game.exit_confirm_modal.is_open() or exit_count[0] != 0:
		push_error("exit modal: second Escape must cancel without leaving")
		quit(1)
		return

	game.exit_confirm_modal.open_modal()
	game.exit_confirm_modal.cancel_button.pressed.emit()
	if game.exit_confirm_modal.is_open() or exit_count[0] != 0:
		push_error("exit modal: CANCEL must close the modal without leaving")
		quit(1)
		return

	game.exit_confirm_modal.open_modal()
	game.exit_confirm_modal.exit_button.pressed.emit()
	if game.exit_confirm_modal.is_open() or exit_count[0] != 1:
		push_error("exit modal: LEAVE ROOM must close the modal and request exit")
		quit(1)
		return

	print("exit modal: Escape, cancel, and leave behavior passed")
	game.queue_free()
	await process_frame
	quit(0)
