# gdtidy lint rules

Every rule gdtidy can report. `gdtidy rules` lists them with whether they are on by default, and `gdtidy rules --format json` adds each rule's concern and options. [RFC 0002](../rfcs/0002-lint-rules.md) lists the rules still to come, and where gdlint's other rules went.

Options are set in `gdtidy.toml`:

```toml
[lint.rules.max-line-length]
max = 130
```

## Naming rules

The 14 rules below are gdlint's, with its names, default patterns and messages. Each has one option, `pattern`: a regular expression the whole name must match, as with gdlint's `re.fullmatch`. Patterns use the [`regex` crate's syntax](https://docs.rs/regex/latest/regex/#syntax), which is Python's without look-around or backreferences; gdlint's own patterns work unchanged.

```toml
[lint.rules.function-name]
pattern = '_?[a-z][a-z0-9]*(_[a-z0-9]+)*'
```

"Set directly to `load()` or `preload()`" means the call is the whole value: `var Enemy = preload("res://enemy.gd")` counts, `var enemy = preload("res://enemy.tscn").instantiate()` does not.

## function-name

On by default. A function's name. Lambda names are not checked. The default pattern allows `_?snake_case`, or `_on_PascalCase_snake_case` for signal callbacks:

```
(_on_([A-Z][a-z0-9]*)+(_[a-z0-9]+)*|_?[a-z][a-z0-9]*(_[a-z0-9]+)*)
```

## class-name

On by default. A script's `class_name`, including `class_name Name extends Base` on one line (which gdlint skips). The default pattern allows `PascalCase`:

```
([A-Z][a-z0-9]*)+
```

## sub-class-name

On by default. An inner class's name. The default pattern allows `_?PascalCase`:

```
_?([A-Z][a-z0-9]*)+
```

## signal-name

On by default. A signal's name. Signal parameters are not checked. The default pattern allows `snake_case`:

```
[a-z][a-z0-9]*(_[a-z0-9]+)*
```

## class-variable-name

On by default. A member variable's name, static, exported and `@onready` ones included, unless it is set directly to `load()` or `preload()`. The default pattern allows `_?snake_case`:

```
_?[a-z][a-z0-9]*(_[a-z0-9]+)*
```

## class-load-variable-name

On by default. A member variable set directly to `load(...)` or `preload(...)`, as in `var Scene = preload("res://scene.tscn")`. The default pattern allows `PascalCase` or `_?snake_case`:

```
(([A-Z][a-z0-9]*)+|_?[a-z][a-z0-9]*(_[a-z0-9]+)*)
```

## function-variable-name

On by default. A local variable's name, in a function, a lambda or a property's `get`/`set`, unless it is set directly to `load()` or `preload()`. The default pattern allows `snake_case`:

```
[a-z][a-z0-9]*(_[a-z0-9]+)*
```

## function-preload-variable-name

On by default. A local variable set directly to `preload(...)`. As in gdlint, a local variable set to `load(...)` is checked by neither this rule nor `function-variable-name`. The default pattern allows `PascalCase`:

```
([A-Z][a-z0-9]*)+
```

## function-argument-name

On by default. A function's or lambda's parameter. Setter parameters (`set(value):`) are not checked. The default pattern allows `_?snake_case`:

```
_?[a-z][a-z0-9]*(_[a-z0-9]+)*
```

## loop-variable-name

On by default. A `for` loop's variable. The default pattern allows `_?snake_case`:

```
_?[a-z][a-z0-9]*(_[a-z0-9]+)*
```

## enum-name

On by default. A named enum's name. The default pattern allows `PascalCase`:

```
([A-Z][a-z0-9]*)+
```

## enum-element-name

On by default. An enum value's name, in named and unnamed enums. The default pattern allows `UPPER_SNAKE_CASE`:

```
[A-Z][A-Z0-9]*(_[A-Z0-9]+)*
```

## constant-name

On by default. A constant's name, at class level or in a block, unless it is set directly to `load()` or `preload()`. The default pattern allows `_?UPPER_SNAKE_CASE`:

```
_?[A-Z][A-Z0-9]*(_[A-Z0-9]+)*
```

## load-constant-name

On by default. A constant set directly to `load(...)` or `preload(...)`. The default pattern allows `PascalCase` or `_?UPPER_SNAKE_CASE`:

```
(([A-Z][a-z0-9]*)+|_?[A-Z][A-Z0-9]*(_[A-Z0-9]+)*)
```


## file-name

Off by default. A script's file name, without `.gd`, does not match `pattern`. The style guide asks for `snake_case` file names, but many projects name scripts after their class, so this rule is opt-in. The default pattern allows `snake_case`, digits included:

```
[a-z0-9]+(_[a-z0-9]+)*
```

## Member order

## class-definitions-order

On by default. In each class (the script, and each inner class apart), declarations must come in the order of the `sections` option. Each declaration belongs to one section:

| Section | Declarations |
|---|---|
| `tools` | `@tool` |
| `classnames` | `class_name` |
| `extends` | `extends`, and `class_name Name extends Base` on one line |
| `docstrings` | A string used as a comment |
| `signals`, `enums`, `consts` | Signals, enums, constants |
| `staticvars` | `static var` |
| `exports` | Variables with an `@export...` annotation |
| `pubvars`, `prvvars` | Other variables; private ones start with `_` |
| `onreadypubvars`, `onreadyprvvars` | `@onready` variables |
| `others` | Functions |

Inner classes, `pass` and annotations other than `@tool` are not checked. A declaration whose section the option leaves out is reported once, so a shortened list does not silently pass.

| Option | Default | Meaning |
|---|---|---|
| `sections` | the 14 sections above, in that order | The order sections must come in. gdlint's `class-definitions-order` list works unchanged |

## Limits

Each has one option, `max`. gdlint's limits (`max-returns`, `function-arguments-number`, `max-public-methods`, `max-file-lines`) keep its defaults, except that gdtidy also counts static functions, which gdlint leaves out. The three limits from gdstyle are off by default.

## max-returns

On by default. A function has more than `max` (6) `return` statements. Returns inside lambdas are the lambda's, not the function's. Reported at the last one.

## function-arguments-number

On by default. A function has more than `max` (10) parameters.

## max-public-methods

On by default. A class has more than `max` (20) functions whose name does not start with `_`. The script and each inner class are counted apart.

## max-file-lines

On by default. A file has more than `max` (1000) lines.

## max-function-lines

Off by default. A function body has more than `max` (50) lines of code. Blank lines and lines with only a comment are not counted; a lambda's lines are, since they make the function longer.

## max-branches

Off by default. A function has more than `max` (8) branches: each `if`, `elif` and `else`, and each branch of a `match`. Lambdas in the function are not counted.

## max-nesting-depth

Off by default. A block in a function is nested more than `max` (4) levels deep. The function's own body is level 0, and each `if`, `for`, `while` or `match` adds one (a `match` and its branches count as one level). Reported once, at the statement whose block goes too deep.

## Lines

## max-line-length

On by default. A line is longer than `max` columns.

| Option | Default | Meaning |
|---|---|---|
| `max` | 100 | The longest line allowed, in columns |
| `tab-width` | 4 | How many columns a tab counts for, as in Godot's editor. gdlint counts 1, and `gdtidy migrate` keeps that for converted projects |

## Code

## duplicated-load

On by default. The same path is loaded twice in one file, with `load("...")` or `preload("...")`. Load it once into a constant instead. Unlike gdlint, `load("a.gd")` and `load('a.gd')` count as the same path.

## comparison-with-itself

On by default. Something is compared with itself with `==`, `!=`, `<`, `<=`, `>` or `>=`, as in `x == x`, which is always true or always false. Spacing does not matter: `(x + 1) < (x+1)` counts. To test for NaN, use `is_nan(x)`.

## self-assignment

On by default. Something is assigned to itself with `=`, as in `x = x` or `self.position = self.position`, which does nothing. Godot does not warn about it.

## loop-variable-assignment

On by default. A `for` loop's variable is assigned inside the loop, as in `for item in items: item = 1`. The next iteration sets it again, so the assignment changes neither the list nor the iteration. Lambdas and inner loops with their own variable of the same name are not counted.

## unnecessary-pass

On by default, fixable. `pass` in a block, or a class, that has other statements or members. Unlike gdlint, a class with `pass` and only functions counts too.

`--fix` deletes the `pass` line when it is alone on its line; a `pass` sharing its line with a comment or another statement is reported and left alone.

## no-elif-return

On by default. An `elif` follows a branch that always returns, so it could be a plain `if`. A block always returns when one of its own statements is a `return`, an `if` with an `else` whose branches all always return, or a `match` with a `_` branch (without `when`) whose branches all always return. As in gdlint, the `return` does not have to be the block's last statement.

## no-else-return

On by default. An `else` follows branches that all always return (as above), so its body could follow the `if` unindented. Not reported when the `else` declares a variable with the same name as one in the enclosing block, since moving it out would declare it twice.

## private-call

Off by default. A method whose name starts with `_` is called on something other than `self` or `super`, as in `enemy._take_hit()`. Calling a private static method through the script's `class_name` or an inner class's name is allowed. Off by default because tests often call private methods on purpose.

## Comments

## doc-tag

On by default. A `##` doc comment line starts with a tag Godot 4.7 does not read as one, so the line is dropped or shown as plain text in the built-in docs:

- `@deprecated` and `@experimental` must be alone, or followed directly by `:` and a reason. `@deprecated : reason` and `@deprecated reason` are shown as text.
- `@tutorial` must be `@tutorial: url` or `@tutorial(Title): url`. Godot silently drops any other `@tutorial` line.
- Any other `@tag`, such as `@param` or `@return`, is shown as text.

Lines inside a `[codeblock]` are not checked.

## comment-space

Off by default, fixable. A comment starts with a letter or digit right after `#`, as in `#comment`. Left alone: commented-out code (text after `#` that reads as GDScript, which the style guide writes without a space), `#region` and `#endregion`, and comments starting with a symbol, such as `#---` or `#!`. A lone word counts as a comment, not code.

`--fix` inserts the space.

## Silencing comments

## unknown-rule-in-comment

On by default. A silencing comment names a rule gdtidy does not know, so it silences nothing. Names of gdlint rules that moved to another tool, and GDQuest's names for gdtidy rules (`private-access`, `enum-member-name`, `variable-name`), are accepted.
