enum A {X, Y}
enum B {X, Y = 2,}
enum C {
	X,
	Y
}
enum D {}
var a = [1, 2,]
var b = [
	1,
	2
]
var c = {
	"a": 1
}
var d = [1,
		2]
func f():
	a(); b()
	c();
	if a: b(); c()
	var l = func(): a(); b()
