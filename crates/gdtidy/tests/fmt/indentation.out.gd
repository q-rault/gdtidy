extends Node


func f(x):
	if x:
		return 1
	match x:
		1:
			pass
		_:
			pass


var p := 1:
	get:
		return p
	set(value):
		p = value
