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

    var shader_code: String = material.shader.code
    if not shader_code.contains("phosphor_tint_strength : hint_range(0.0, 1.0) = 0.18"):
        _fail("CRT shader must preserve semantic UI colors")
        return

    var menu = main.get_node("MenuScreen")
    if menu.get_node_or_null("TerminalBackdrop") == null:
        _fail("terminal wireframe backdrop is missing")
        return
    if menu.get_node_or_null("TitlePage/SystemPanel") == null:
        _fail("system status instrument is missing")
        return

    var crt_option: OptionButton = menu.crt_preset_option
    if crt_option.item_count != 3:
        _fail("settings must expose weak, standard, and strong CRT presets")
        return
    var expected_ids := ["weak", "standard", "strong"]
    for index in range(expected_ids.size()):
        if str(crt_option.get_item_metadata(index)) != expected_ids[index]:
            _fail("CRT preset order or metadata is invalid")
            return

    crt_option.select(0)
    crt_option.item_selected.emit(0)
    await process_frame
    if not is_equal_approx(float(material.get_shader_parameter("scanline_strength")), 0.08):
        _fail("weak CRT preset was not applied")
        return

    crt_option.select(2)
    crt_option.item_selected.emit(2)
    await process_frame
    if not is_equal_approx(float(material.get_shader_parameter("bloom_strength")), 0.5):
        _fail("strong CRT preset was not applied")
        return
    if not is_equal_approx(float(material.get_shader_parameter("curvature")), 0.07):
        _fail("strong CRT curvature was not applied")
        return

    crt_option.select(1)
    crt_option.item_selected.emit(1)
    await process_frame

    var game = main.get_node("GameScreen")
    var hud = game.get_node("HUD")
    var radar = hud.get_node("HUDRoot/RadarDisplay")
    game.visible = true
    hud.visible = true
    var players := [
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
    ]
    hud.apply_snapshot(players, 1, "playing", 90.0, null, 0.0, 1.1, players[0])
    await process_frame
    if radar.players.size() != 2 or radar.local_player_id != 1:
        _fail("tactical radar did not accept snapshot state")
        return

    var first_accent: Color = hud.get_node("HUDRoot/PlayerOneStatus/Accent").color
    var second_accent: Color = hud.get_node("HUDRoot/PlayerTwoStatus/Accent").color
    if not first_accent.is_equal_approx(Color("#27e5ff")):
        _fail("local player must keep the cyan identity color")
        return
    if not second_accent.is_equal_approx(Color("#ff38c7")):
        _fail("remote player must keep the magenta identity color")
        return
    if first_accent.is_equal_approx(second_accent):
        _fail("player identity colors must remain visually distinct")
        return
    var radar_first: Color = radar.player_colors.get(1, Color.TRANSPARENT)
    if not radar_first.is_equal_approx(first_accent):
        _fail("radar must reuse HUD player identity colors")
        return

    print("crt ui: effects, instruments, and semantic colors passed")
    main.queue_free()
    await process_frame
    quit(0)
