# RFC 0003: The gdtidy formatter

Status: M1 to M3 done, 2026-11-08; M4 is on gdsuite's `gdtidy-fmt` branch, waiting for the gdtidy 0.2.0 release. Target: replace `gdformat` (gdtoolkit 4.x) in gdsuite, so gdsuite no longer needs Python.

## Summary

`gdtidy fmt` rewrites GDScript files to the [official style guide](https://docs.godotengine.org/en/stable/tutorials/scripting/gdscript/gdscript_styleguide.html), changing only layout and spelling, never what the code does. It keeps the line breaks the author wrote: it does not join lines and does not wrap long ones, so its output is predictable and a reviewer can read its diff. `max-line-length` stays a lint rule.

Once this RFC is done, gdsuite runs `gdtidy fmt` instead of `gdformat`, and drops gdtoolkit, pip and its Python virtual environment.

## Goals

1. **Never change what the code does.** Every formatted file must parse without errors and give the same code as the original, apart from the spelling changes listed below. If it does not, the file is left alone and reported. This is checked on every file, every run, not only in tests.
2. **Stable.** Formatting a formatted file changes nothing. A test checks this on the whole corpus.
3. **No judgement calls.** Each change follows a rule in the style guide that has one right answer. Where the guide says nothing, the author's choice is kept.
4. **Fast.** The 14-project corpus (about 430,000 lines) formats in under 1 second on 8 threads.

## Non-goals

- **Wrapping or joining lines.** gdformat's main source of surprise is that it joins lines the author split and splits lines by width. gdtidy keeps the author's breaks inside brackets; `max-line-length` in `gdtidy lint` reports lines that are too long.
- **Reordering anything,** such as members or imports: that is `class-definitions-order`, a lint rule.
- **Renaming:** naming is lint's.
- **Choices the guide leaves open,** such as the spaces before an inline comment, or whether a one-line `if x: return` should be split. (The guide asks to avoid it, but splitting it touches nearly every project; it is a later candidate, below.)
- **gdformat's exact output.** gdformat departs from the guide in several places (listed below), so matching it is not a goal.

## What `gdtidy fmt` changes

All of these come from the style guide.

### Whitespace and lines

- **Line endings and encoding:** LF line endings, no byte order mark, no trailing whitespace (except inside multi-line strings), exactly one newline at the end of the file.
- **Indentation:** one tab per block level. A file indented with spaces is converted.
- **Continuation lines,** the lines of a statement after its first:
  - inside brackets whose opener ends its line (`foo(` then a line break), one more level than the opener's line, and the closer on its own line goes back to the opener's level. The guide asks for this in arrays, dictionaries and enums, and gdformat writes every bracket this way;
  - inside brackets whose content starts on the opener's line, and after a `\`, two more levels (the guide's rule for continuation lines; the `continuation-indent` setting);
  - comment lines between statements are indented like the code around them: like the next line of code, but a comment written inside a block that just ended stays in it.
- **Blank lines:** two around functions and classes, at most two between other class members, at most one inside a function (the guide's "one blank line inside functions to separate logical sections") and inside brackets. None at the start of a block or of the file. Comments and annotations directly above a function count as part of it, so the blank lines go above them.
- **One statement per line:** statements separated with `;` are split onto their own lines; a `;` at the end of a line is removed. A one-line lambda (`func(): a(); b()`) is left as it is.

### Spacing within a line

- One space around binary and assignment operators (`+`, `==`, `and`, `=`, `:=`, `+=`, `->`, ...), and after `,` and `:`; none before them.
- No space after a unary operator (`-x`, `~x`), but one after `not`.
- No space before `(` in a call or a function declaration, nor before `[` in a subscript or a type (`Array[int]`); none inside brackets, nor around `.`.
- **One-line dictionaries:** one space inside the braces, `{ "a": 1 }`, as the guide asks. The `dict-brace-spaces` setting turns this off (`{"a": 1}`, as gdformat writes).
- At least one space before an inline comment; the author's spacing is kept beyond that, so aligned comments stay aligned.
- Node paths (`$Node/Path`, `%Unique`) and annotations' names are never touched.
- Between tokens no rule covers, the author's spacing is kept, with runs of spaces reduced to one.

### Spelling

- `and`, `or`, `not` instead of `&&`, `||`, `!`.
- Double quotes, unless single quotes need fewer escapes (`'say "hi"'` stays). Raw strings and triple-quoted strings change only when no quote or backslash in them makes it matter.
- Lowercase hexadecimal digits: `0xff`.
- Floats with both zeros: `0.5`, `1.0`.
- No parentheses around a whole `if`, `elif` or `while` condition, when they are on one line (parentheses that let a condition span lines are kept).

### Layout

- **Enums:** one member per line.
- **Trailing commas:** added after the last item of an array, dictionary or enum whose closing bracket is on its own line, and removed from one written on one line (the `trailing-commas` setting turns the adding off).

### Not touched

Comments' text (except trailing whitespace), string contents, the order of anything, and line breaks inside brackets.

## How it differs from gdformat

| gdformat 4.5 | gdtidy |
|---|---|
| Joins lines that fit, splits lines that do not | Keeps the author's line breaks |
| `enum E { A, B }` on one line | One member per line, as the guide asks |
| Keeps `&&`, `\|\|`, `!` | `and`, `or`, `not`, as the guide asks |
| Keeps `0xFF`, `.5`, `1.` | `0xff`, `0.5`, `1.0`, as the guide asks |
| `{"a": 1}` | `{ "a": 1 }` by default, as the guide asks; `dict-brace-spaces = false` gives gdformat's |
| Two spaces before an inline comment | The author's spacing |
| Writes CRLF on Windows | Always LF |

## Configuration

In `gdtidy.toml`, next to `[lint]`:

```toml
[fmt]
exclude = ["generated"]        # as in [lint]
continuation-indent = 2        # levels for a continuation line; 1 is common too
dict-brace-spaces = true       # { "a": 1 }; false gives {"a": 1}
trailing-commas = true         # add them to multi-line arrays, dictionaries and enums
```

An unknown key is an error, as in `[lint]`.

**Turning formatting off** for a part of a file, such as a hand-aligned table:

```gdscript
# gdtidy: fmt-off
const TABLE = [
	[1,   0, 0],
	[0,  10, 0],
]
# gdtidy: fmt-on
```

The lines in between are kept byte for byte.

### Migrating from gdformat

`gdtidy migrate` also reads `gdformatrc` (or `.gdformatrc`) and writes its `[fmt]` section:
- `excluded_directories` becomes `exclude` (hidden folders and `addons` are always skipped, as in lint);
- `dict-brace-spaces = false`, so a project formatted with gdformat keeps its dictionaries;
- `line_length` is dropped, with a note: gdtidy does not wrap lines, and `max-line-length` in `[lint]` reports them.

`migrate` now fills in only the sections `gdtidy.toml` does not have yet, so a project converted from gdlint can convert its gdformatrc later. `--force` rewrites both.

## Command line

```sh
gdtidy fmt                    # format the whole project, in place
gdtidy fmt player/ hud.gd     # only these files and folders
gdtidy fmt --check            # change nothing; list the files that would change
gdtidy fmt --diff             # change nothing; print the changes as a unified diff
gdtidy fmt --check --format json
gdtidy fmt -                  # read standard input, write standard output (for editors)
```

Files are found as for lint: hidden folders, `addons`, folders with `.gdignore` and nested projects are skipped when walking; a file named directly is always formatted.

| Exit code | Meaning |
| --- | --- |
| `0` | Formatted, or with `--check`/`--diff`, nothing to change |
| `1` | With `--check`/`--diff`: some files would change |
| `2` | Some files were skipped (a syntax error, or the safety check failed), or the configuration is invalid |

The JSON report is `{"version": 1, "files": N, "changed": ["path", ...], "skipped": [...]}`, with `skipped` as in lint's report, and a `reason` of `syntax-error` or `unsafe` (the safety check failed, which is a gdtidy bug to report).

## Design

- **A line printer over the lossless tree.** gdsyntax's tree keeps every byte. The printer walks its tokens, splits them into physical lines, and for each line computes the indentation, the spacing between tokens and the blank lines before it. It decides from the tree, not from the text: whether a line starts a statement in a block, which bracket a line continues, whether a `-` is unary or binary.
- **Rewrites before printing.** Spelling changes, `;` splits, enum and trailing-comma changes are token edits applied as the tokens are walked, so the printer sees one stream.
- **The safety check.** The output is parsed. It must have no errors, and the same code as the input after normalizing what `fmt` is allowed to change: `&&`/`and` and the like; literals compared by value; trailing commas, `;` and the parentheses around a condition ignored; each token paired with how many blocks it is in, so a statement can never move into or out of a block. Comments must have the same text, ignoring trailing whitespace. A file that fails keeps its text and is reported as `unsafe`.
- **Parallelism and order** as in lint: rayon, results sorted by path.

## Testing

1. **Fixtures:** pairs of `tests/fmt/<case>.in.gd` and `<case>.out.gd`. The test checks that `fmt(in) == out`, and that `fmt(out) == out`.
2. **Corpus** (`GDTIDY_FMT_CORPUS=<folders>`, off by default): on every file, the safety check passes and formatting twice changes nothing. It also prints how many lines change per project, to see the cost of switching.
3. **Godot's test scripts:** the same corpus checks run on gdsyntax's copy of Godot's valid test scripts, which use every piece of syntax.
4. **Speed:** the corpus formats in under 1 second on 8 threads.

## Milestones

Each milestone ends with a commit.

| | Content | Done when |
|---|---|---|
| M1 | The printer: line endings, indentation, blank lines, spacing; the safety check; `fmt`, `--check`, `--diff`, `-`, JSON | Fixtures pass; the corpus is safe and stable |
| M2 | Spelling and layout: operators, quotes, numbers, condition parentheses, `;`, enums, trailing commas; the three settings; `fmt-off` | Fixtures pass; the corpus is safe and stable; under 1 s |
| M3 | `gdtidy migrate` for gdformatrc, README and docs; release gdtidy 0.2.0 (needs approval) | Docs list every change and every difference with gdformat |
| M4 | gdsuite runs `gdtidy fmt` and drops gdtoolkit, pip and the Python environment; converts gdformatrc during setup | gdsuite's tests pass without Python; gdsuite release (needs approval) |

## Later candidates

Not part of this RFC:
- splitting one-line blocks (`if x: return`), which the guide asks to avoid;
- `_` separators in long numbers, which the guide recommends but with no agreed threshold;
- putting `and`/`or` at the start of a continuation line, as the guide asks.
