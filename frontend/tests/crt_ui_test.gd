extends SceneTree


func _initialize() -> void:
    call_deferred("_run")


func _fail(message: String) -> void:
    push_error("crt ui: %s" % message)
    quit(1)


func _run() -> void:
    var main_scene: PackedScene = load("res://src/app/main.tscn")
    var main = main_scene.instantiate()
    root.add_child(main)
    await process_frame

    var post_process: ColorRect = main.get_node("CRTDisplay/PostProcess")
    var material := post_process.material as ShaderMaterial
    if material == null or material.shader == null:
        _fail("full-screen CRT shader is not configured")
        return

    var menu = main.get_node("MenuScreen")
    if menu.get_node_or_null("TerminalBackdrop") == null:
        _fail("terminal wireframe backdrop is missing")
        return
    if menu.get_node_or_null("TitlePage/SystemPanel") == null:
        _fail("system status instrument is missing")
        return

    var game = main.get_node("GameScreen")
    var hud = game.get_node("HUD")
    var radar = hud.get_node("HUDRoot/RadarDisplay")
    game.visible = true
    hud.visible = true
    radar.apply_snapshot([
        {
            "id": 1,
            "alive": true,
            "connected": true,
            "position": {"x": 320.0, "y": 180.0},
        },
        {
            "id": 2,
            "alive": true,
            "connected": true,
            "position": {"x": 460.0, "y": 120.0},
        },
    ], 1)
    await process_frame
    if radar.players.size() != 2 or radar.local_player_id != 1:
        _fail("tactical radar did not accept snapshot state")
        return

    print("crt ui: shader, terminal instruments, and tactical radar passed")
    main.queue_free()
    await process_frame
    quit(0)
