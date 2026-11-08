func f():
	var a = foo(
		1,
		2,
	)
	var b = foo(1,
			2)
	var c = [
		1,
		[
			2,
		],
	]
	var d = a + \
			b
	foo(func():
		pass
	)
	if a and \
			b:
		pass
