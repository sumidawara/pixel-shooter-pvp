extends Node2D

const SPARKLE_TEXTURE: Texture2D = preload("res://assets/generated/effects/sparkle.png")

var particles: Array = []
var sparkles: Array = []
var flash_strength := 0.0
var shake_strength := 0.0


func spawn_burst(position: Vector2, color: Color, count: int, speed: float) -> void:
	for index in range(count):
		var angle := TAU * float(index) / float(count) + randf_range(-0.25, 0.25)
		var life := randf_range(0.18, 0.42)
		particles.append({
			"position": position,
			"velocity": Vector2.from_angle(angle) * randf_range(speed * 0.45, speed),
			"life": life,
			"max_life": life,
			"color": color,
			"size": randi_range(2, 4),
		})


func spawn_sparkle(position: Vector2, color: Color) -> void:
	sparkles.append({
		"position": position,
		"life": 0.09,
		"max_life": 0.09,
		"color": color,
	})


func flash(amount: float) -> void:
	flash_strength = maxf(flash_strength, amount)


func shake(amount: float) -> void:
	shake_strength = maxf(shake_strength, amount)


func current_shake_offset() -> Vector2:
	if shake_strength <= 0.0:
		return Vector2.ZERO
	return Vector2(
		randf_range(-shake_strength, shake_strength),
		randf_range(-shake_strength, shake_strength)
	).round()


func clear() -> void:
	particles.clear()
	sparkles.clear()
	flash_strength = 0.0
	shake_strength = 0.0
	queue_redraw()


func _process(delta: float) -> void:
	for index in range(particles.size() - 1, -1, -1):
		var particle: Dictionary = particles[index]
		particle.life = float(particle.life) - delta
		if particle.life <= 0.0:
			particles.remove_at(index)
			continue
		particle.position += Vector2(particle.velocity) * delta
		particle.velocity = Vector2(particle.velocity) * exp(-4.0 * delta)
		particles[index] = particle
	for index in range(sparkles.size() - 1, -1, -1):
		sparkles[index].life = float(sparkles[index].life) - delta
		if sparkles[index].life <= 0.0:
			sparkles.remove_at(index)
	flash_strength = maxf(flash_strength - delta * 2.8, 0.0)
	shake_strength = maxf(shake_strength - delta * 28.0, 0.0)
	queue_redraw()


func _draw() -> void:
	for particle in particles:
		var ratio: float = clampf(float(particle.life) / float(particle.max_life), 0.0, 1.0)
		var color: Color = particle.color
		color.a = ratio
		var size := float(particle.size)
		draw_rect(Rect2(Vector2(particle.position) - Vector2.ONE * size * 0.5, Vector2.ONE * size), color)
	for sparkle in sparkles:
		var ratio: float = clampf(float(sparkle.life) / float(sparkle.max_life), 0.0, 1.0)
		var color: Color = sparkle.color
		color.a = ratio
		draw_texture_rect(
			SPARKLE_TEXTURE,
			Rect2(Vector2(sparkle.position) - Vector2(15, 5), Vector2(30, 10)),
			false,
			color
		)
	if flash_strength > 0.0:
		draw_rect(Rect2(0, 0, 640, 360), Color(0.91, 0.95, 0.97, flash_strength * 0.3))
