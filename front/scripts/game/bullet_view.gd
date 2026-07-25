extends Node2D

@onready var trail: Line2D = %Trail
@onready var body: Polygon2D = %Body

var velocity := Vector2.ZERO


func configure(color: Color, next_velocity: Vector2) -> void:
	velocity = next_velocity
	body.color = color
	trail.default_color = Color(color, 0.32)
	var direction := velocity.normalized()
	trail.points = PackedVector2Array([-direction * 9.0, Vector2.ZERO])
