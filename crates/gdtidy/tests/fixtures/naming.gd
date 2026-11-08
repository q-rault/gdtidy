class_name bad_class_name  # expect: class-name
# gdtidy: disable=class-definitions-order, duplicated-load
extends Node

signal some_signal
signal signal_with_args(a, b, c)
signal someSignal  # expect: signal-name
signal Signal(a, b)  # expect: signal-name
signal ok_signal(BadParam)

enum Name {}
enum PascalXCase { XXX, Y_Y_Y }
enum some_name {}  # expect: enum-name
enum PascalCase_ { ZZZ }  # expect: enum-name
enum { X_, Y }  # expect: enum-element-name
enum { _XXX, xx_xx, SomeStuff }  # expect: enum-element-name, enum-element-name, enum-element-name

const X = 1
const _X = 2
const X_Y_Z = 3
const Xx = 1  # expect: constant-name
const x_x = 2  # expect: constant-name
const Scene = preload("res://scene.tscn")
const SCENE = preload("res://scene.tscn")
const _PRIVATE_SCENE = load("res://scene.tscn")
const scene = preload("res://scene.tscn")  # expect: load-constant-name
const not_a_load = load("res://a.gd").new()  # expect: constant-name

var x
var _private_x := 1
static var static_x = 1
@export var exported_x: int
@onready var label = $Label
var X_Y  # expect: class-variable-name
static var StaticX = 1  # expect: class-variable-name
@export var exportedX: int  # expect: class-variable-name
var Script = preload("res://a.gd")
var other_script = load("res://a.gd")
var _private_script = load("res://a.gd")
var BAD_SCRIPT = preload("res://a.gd")  # expect: class-load-variable-name
var property_x: int:
	set(BadValue):
		property_x = BadValue
	get:
		return property_x


func foo(a, _b, xxx: int, c_d := 123) -> void:
	var xyz
	var x_y = 1
	var y: int = 1
	var y2 := 1
	var xxx_  # expect: function-variable-name
	var _x_y = 1  # expect: function-variable-name
	var Y: int = 1  # expect: function-variable-name
	var yY := 1  # expect: function-variable-name
	var XxxYyy = preload("res://a.gd")
	var x_yy = preload("res://a.gd")  # expect: function-preload-variable-name
	var AnyName = load("res://a.gd")
	const LOCAL_CONST = 1
	const localConst = 1  # expect: constant-name
	for _i in xxx:
		pass
	for aaa_bbb: int in xxx:
		pass
	for x_ in xxx:  # expect: loop-variable-name
		pass
	for X_X: int in xxx:  # expect: loop-variable-name
		pass
	var f := func NamedLambda(ok_arg, BadArg):  # expect: function-argument-name
		var inLambda = ok_arg  # expect: function-variable-name
		return inLambda
	print(a, _b, c_d, xyz, x_y, y, y2, xxx_, _x_y, Y, yY, XxxYyy, x_yy, AnyName, f)


func bad_args(
	a_,  # expect: function-argument-name
	xX,  # expect: function-argument-name
	X_X,  # expect: function-argument-name
) -> void:
	pass


static func static_ok() -> void:
	pass


func _private() -> void:
	pass


func _on_Button_pressed() -> void:
	pass


func _on_button_pressed() -> void:
	pass


func some_Button_pressed() -> void:  # expect: function-name
	pass


func SomeName() -> void:  # expect: function-name
	pass


class _SubClassName:
	pass


class SubClassName:
	var InnerVar  # expect: class-variable-name

	func InnerFunc() -> void:  # expect: function-name
		pass


class SubClassName_:  # expect: sub-class-name
	pass


class sub_class_name:  # expect: sub-class-name
	pass
