@tool
class_name OrderFixture
extends Node

signal done
enum State { IDLE }
const LIMIT = 1
static var count = 0
@export var speed = 1.0
var health = 1
var _secret = 2
@onready var label = $Label
@onready var _icon = $Icon
var late_public = 3  # expect: class-definitions-order
signal late_signal  # expect: class-definitions-order
@export
var exported_late = 1  # expect: class-definitions-order


func _ready() -> void:
	pass


const LATE = 2  # expect: class-definitions-order


class Inner:
	var _a = 1
	var b = 2  # expect: class-definitions-order

	func f() -> void:
		pass
