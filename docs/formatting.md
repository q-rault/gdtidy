# What `gdtidy fmt` changes

Everything here comes from the [official style guide](https://docs.godotengine.org/en/stable/tutorials/scripting/gdscript/gdscript_styleguide.html). `gdtidy fmt` changes layout and spelling only. Each result is parsed again and must have the same code and comments as the input; if it does not, the file is left alone and reported as `unsafe`, which is a gdtidy bug.

It never wraps or joins lines, reorders anything, or renames anything. Where the guide says nothing, such as the spaces before an inline comment, the author's choice is kept.

## Lines and indentation

| Rule | Example |
|---|---|
| LF line endings, no byte order mark, no trailing whitespace, one final newline | |
| One tab per block level; a file indented with spaces is converted | |
| Inside a bracket that ends its line, one more level; the closing bracket goes back | `foo(`<br>`→1,`<br>`)` |
| A line continued after content on the opener's line, or after `\`, gets `continuation-indent` levels (2 by default) | `foo(a,`<br>`→→b)` |
| Comment lines are indented like the code around them | |
| Two blank lines around functions and classes, comments and annotations directly above included | |
| At most two blank lines between other class members, one inside a function or brackets, none at the start of a block | |
| One statement per line: `a(); b()` is split, a final `;` removed (one-line lambdas are left alone) | |

## Spacing

| Rule | Before | After |
|---|---|---|
| One space around binary and assignment operators, and `->` | `a+b`, `x=1`, `f()->int` | `a + b`, `x = 1`, `f() -> int` |
| One space after `,` and `:`, none before | `f(a ,b)`, `var a : int` | `f(a, b)`, `var a: int` |
| `:=` kept together | `var a : = 1` | `var a := 1` |
| No space after a unary operator, except `not` | `- x` | `-x` |
| No space before a call's or subscript's bracket, nor inside brackets | `f (x)`, `a [0]`, `[ 1 ]` | `f(x)`, `a[0]`, `[1]` |
| One space inside a one-line dictionary's braces (`dict-brace-spaces`) | `{"a": 1}` | `{ "a": 1 }` |
| At least one space before an inline comment; more are kept | `x#c` | `x #c` |

Node paths (`$Node/Path`, `%Unique`) are never touched.

## Spelling

| Rule | Before | After |
|---|---|---|
| Boolean operators in words | `a && !b \|\| c` | `a and not b or c` |
| Double quotes, unless single quotes need fewer escapes | `'a'`, `"say \"hi\""` | `"a"`, `'say "hi"'` |
| Lowercase hexadecimal digits | `0xFF` | `0xff` |
| Floats with both zeros | `.5`, `1.` | `0.5`, `1.0` |
| No parentheses around a whole one-line `if`, `elif` or `while` condition | `if (a and b):` | `if a and b:` |

## Layout

| Rule | Before | After |
|---|---|---|
| Enums one member per line | `enum E {A, B}` | `enum E {`<br>`→A,`<br>`→B,`<br>`}` |
| A trailing comma in a multi-line array, dictionary or enum (`trailing-commas`) | `[`<br>`→1`<br>`]` | `[`<br>`→1,`<br>`]` |
| No trailing comma in a one-line one | `[1, 2,]` | `[1, 2]` |

## Differences with gdformat

| gdformat 4.5 | gdtidy |
|---|---|
| Joins lines that fit, splits lines that do not | Keeps the author's line breaks |
| `enum E { A, B }` on one line | One member per line |
| Keeps `&&`, `\|\|`, `!` | `and`, `or`, `not` |
| Keeps `0xFF`, `.5`, `1.` | `0xff`, `0.5`, `1.0` |
| `{"a": 1}` | `{ "a": 1 }`; `gdtidy migrate` sets `dict-brace-spaces = false` to keep gdformat's |
| Two spaces before an inline comment | The author's spacing |
| Writes CRLF on Windows | Always LF |
