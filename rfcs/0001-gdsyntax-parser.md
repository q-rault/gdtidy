# RFC 0001: A GDScript parser written from scratch for gdsyntax

Status: accepted, 2026-11-07. M1 to M4 done (M2 to M4 landed together: class level, statements and expressions depend on each other). Target: Godot 4.7, compatible with the 4.8 beta.

## Summary

Replace tree-sitter-gdscript inside `gdsyntax` with a handwritten, lossless parser translated from Godot's own tokenizer and parser (`modules/gdscript/gdscript_tokenizer.cpp` and `gdscript_parser.cpp`). gdsyntax becomes the parser for gdtidy first, then gdhealth and gdtest.

## Why

Measured on about 5,500 files, both existing Rust options reject about 1% of valid code, on different constructs:

| Corpus | Files | tree-sitter-gdscript 6.1 | gdck-syntax 0.8 |
| --- | --- | --- | --- |
| 14 public Godot projects | 3,430 | 32 rejected | 48 rejected |
| Godot 4.7's own valid test scripts | 342 | 14 rejected | 11 rejected |
| gdtoolkit's valid corpus and formatter fixtures | 320 | 14 rejected | 0 rejected |

tree-sitter also has open bugs that produce a wrong tree without any error (precedence of `as` and `**`, upstream issues 101 and 102), which is worse than a rejection for a formatter.

The constructs both get wrong are the ones where Godot's tokenizer and parser cooperate: indentation inside brackets, lambdas inside calls, inline `match` branches. Godot's tokenizer is not a separate pass. The parser switches it into "multiline mode" inside brackets and pushes an indented block when a lambda starts inside an expression (`set_multiline_mode`, `push_expression_indented_block`). A parser that mirrors that structure gets those cases right by construction.

## Goals

1. **Accept everything Godot 4.7's parser accepts, and reject what it rejects as a syntax error.** Acceptance: zero disagreements on Godot's test suite, gdtoolkit's corpus, a private project and the 14 public projects, each disagreement settled by running Godot (gdtypecheck).
2. **Lossless.** Every byte, comment and blank line is in the tree, and `tree.text() == source` for every input, valid or not.
3. **Never fail, never hang, never panic.** Broken input gives error nodes and errors, and parsing continues.
4. **Godot's error messages.** Same wording as the editor, so an error reads the same in gdtidy and in Godot.
5. **A typed tree** for gdtidy's lint rules, gdhealth's metrics and gdtest's discovery.
6. **Fast.** At least as fast as gdck-syntax: under 0.3 s for 429,000 lines on one thread.
7. **Follows Godot.** One Rust function per Godot `parse_*` function, so a change in Godot maps to one change here.

## Non-goals

- **Type checking, name resolution and scope checks.** Godot's parser also reports some semantic errors, such as a variable declared twice or a misused annotation. Those stay with gdtypecheck, which runs Godot itself. gdsyntax only reports syntax.
- **Godot's warnings.** All of Godot's GDScript warnings, such as `STANDALONE_EXPRESSION`, belong to gdtypecheck. gdtidy must not report them again.
- **Annotation argument validation**, such as the type an `@export_range` needs.
- **Godot 3 syntax**, except to reject it with Godot 4's message (for example "The `export` keyword was removed in Godot 4").
- **Incremental reparsing.** The tree design allows it later, for a language server, but it is not built now.

## Design

### Crate layout

```text
crates/gdsyntax/
  src/
    lib.rs            public API: parse, SyntaxTree, errors, positions
    kind.rs           SyntaxKind: Godot's 100 token types, trivia kinds, node kinds
    lexer/            port of GDScriptTokenizerText
      mod.rs          cursor, scan(), trivia
      indent.rs       indentation stack, tabs vs spaces, multiline mode, lambda blocks
      literal.rs      numbers and every string form
    parser/           port of GDScriptParser, one function per parse_* function
      mod.rs          parser state, tree building, progress and depth guards
      class.rs        parse_program, parse_class_body, members, annotations
      stmt.rs         parse_suite, parse_statement, if/for/while/match, ...
      expr.rs         Pratt parser built from Godot's get_rule() table
      pattern.rs      parse_match_pattern
      recovery.rs     Godot's recovery points: end of statement, end of block
    ast/              typed wrappers over the tree (ClassBody, FuncDecl, Expr, ...)
    error.rs          SyntaxError with range, message and code
  tests/
    godot_suite.rs    runs Godot's vendored test scripts
    corpus.rs         round-trip and zero-error checks on local corpora
    snapshots/        tree dumps per construct
  fuzz/               cargo-fuzz targets: no panic, no hang, round-trip
```

### Lexer

A port of `GDScriptTokenizerText` (1,644 lines of C++ in 4.7), with two additions Godot does not need: trivia and positions in bytes.

- **Driven by the parser.** The parser tells the lexer when it enters or leaves brackets and when a lambda body starts inside an expression, as Godot does. This is the single most important design point.
- **Token kinds** are Godot's `Token::Type` list (100 kinds in 4.7, including the reserved `trait`, `namespace` and `yield`) plus trivia: whitespace, comment, doc comment (`##`), line continuation (`\`), newline. `Indent` and `Dedent` are zero-width markers next to the whitespace, not instead of it.
- **Literals** follow `number()` and `string()`: underscores and their rules, hex and binary, exponents, `"`, `'`, `"""`, `'''`, raw `r"..."`, `&"StringName"`, `^"NodePath"`, plain strings that span lines, and escape validation.
- **Node paths**: `$Path`, `$"Quoted path"`, `%Unique`, `%A/%B`, `$%X`, `% "spaced"`.
- **Names**: Unicode identifiers as Godot defines them, and the keywords Godot still accepts as names (`match`, `when`).
- **Godot's lexer errors**: mixed tabs and spaces, version-control conflict markers, invisible direction characters, characters that look like keywords.
- **Encoding**: a UTF-8 BOM is trivia; CRLF is one newline token.

### Tree

[`rowan`](https://crates.io/crates/rowan), the tree library behind rust-analyzer: an immutable green tree with cheap clones and parent pointers, which also leaves incremental reparsing open. The parser emits start, token and finish events into a builder with checkpoints, so a node can be opened after its first child has been read. That is needed for binary operators, calls and subscripts.

### Parser

- **One function per Godot function.** The 57 `parse_*` functions keep their names (`parse_class_member`, `parse_lambda`, `parse_match_pattern`, ...), each with a comment naming the C++ function and the Godot version it matches.
- **Expressions** use Godot's own precedence table: 24 levels from `PREC_ASSIGNMENT` to `PREC_PRIMARY`, with prefix and infix rules copied from `get_rule()`.
- **Statements and members** follow `parse_statement` and `parse_class_body`, including standalone strings at class level. Those become a `StringStatement` node, not a comment, because Godot 4.8 treats them as expressions (see below).
- **Error recovery** uses Godot's own points (end of statement, end of block, closing bracket), plus two guards: every loop must consume a token, and nesting deeper than 256 levels becomes an error instead of a stack overflow.
- **Semantic checks** in Godot's parser are left out and listed in one file, `tests/godot/semantic-errors.txt`, so the test harness knows which of Godot's parser errors are not ours.

### Typed layer

Handwritten wrappers in `ast/` over about 40 node kinds, matching Godot's AST node types: `SourceFile`, `ClassBody`, `ClassName`, `Extends`, `Annotation`, `Signal`, `Enum`, `Const`, `Var`, `Property` (with `get` and `set`), `Func`, `Param`, `Lambda`, `Suite`, statements and an `Expr` enum. Each accessor returns `Option`, because a broken file still produces a tree.

### Public API

What gdtidy already uses stays the same: `parse(source) -> SyntaxTree`, `errors()`, `position()`, `text()`. `root()` returns the typed `SourceFile`, and the untyped `SyntaxNode` stays available. gdtidy needs no change beyond the version bump.

## Testing, and staying in sync with Godot

| Layer | What | Pass condition |
| --- | --- | --- |
| Unit | One snapshot of the tree per construct | Snapshot unchanged |
| Godot suite | 4.7's `modules/gdscript/tests/scripts`, every script with an expected-output file: 424 that Godot parses (337 that run cleanly, 66 with analyzer errors, 21 with runtime errors) and 73 with parser errors, vendored under its MIT license | Every valid file parses with no error. Every syntax-error file gives Godot's first error message at Godot's line. Semantic errors are listed and skipped. |
| gdtoolkit corpus | Valid, invalid and formatter fixtures (MIT) | Valid files parse; invalid files report an error |
| Real projects | The 14 public projects, pinned to commits and fetched by a script, not vendored; private projects through an environment variable, never committed | Zero errors; any disagreement is settled by Godot through gdtypecheck |
| Losslessness | Every file of every corpus, valid or not | `text() == source` |
| Fuzzing | cargo-fuzz on Linux CI, nightly | No panic, no hang, round-trip holds |
| Comparison | tree-sitter-gdscript and gdck-syntax, as a dev-only tool | Report only; never a dependency |
| Speed | A large private project and the public corpus | Within the budget above |

Godot's completion tests are left out: they contain a cursor marker and are not valid GDScript by design.

**Sync procedure for each Godot release** (`just godot-sync <tag>`):

1. Diff `gdscript_tokenizer.*` and `gdscript_parser.*` between the last synced tag and the new one, and list the `parse_*` functions that changed.
2. Vendor the new tag's test scripts next to the old ones.
3. Port the changes function by function, updating the version in each function's comment.
4. CI runs the Godot suite for every supported version, so 4.7 code keeps parsing after 4.8 support lands.

## Godot 4.8: what is coming

Godot's `master` is 4.8-beta on 2026-11-07. 4.7.1 and 4.7.2 did not touch the tokenizer or parser. Between 4.7-stable and the 4.8 beta the parser changed by 387 additions and 303 deletions, almost all internal: the syntax tree's memory layout, code completion, and moving warnings into a separate lint pass. **No new syntax.** What matters here:

| Change | Pull request | Effect on gdsyntax and gdtidy |
| --- | --- | --- |
| Strings used as comments get a `STANDALONE_EXPRESSION` warning | godotengine/godot#121833 | Still valid syntax. Keep standalone strings as `StringStatement` nodes so the formatter preserves them. The warning is Godot's, so gdtidy must not add its own. |
| `..` outside a match pattern gets a clear error | godotengine/godot#121133 | Copy the new message: Expected ":" after "if" condition, found ".." instead. |
| Empty doc comment lines become line breaks | godotengine/godot#123357 | Affects documentation text only; parsing is unchanged. |
| `@export_enum` accepts StringNames | godotengine/godot#121684 | Annotation argument validation, which is out of scope. |
| Warnings moved to a dedicated lint pass | commit 776bf054c3 | Confirms the boundary: Godot's warning list is gdtypecheck's, not gdtidy's. |
| Tokenizer string errors report the right line | godotengine/godot#121560 | Our lexer reports positions from the start anyway. |

**Watch list** (not scheduled for 4.8): structs (godotengine/godot#90356, #108503), nullable types (#76843) and traits. The `trait` and `namespace` keywords are already reserved in 4.7, and gdsyntax will reserve them too, with Godot's error.

## Milestones

| | Work | Done when | Agent time |
| --- | --- | --- | --- |
| M1 | Lexer, kinds, rowan tree, trivia | Every corpus file round-trips; lexer errors match Godot's | 0.5–1 day |
| M2 | Class level: annotations, `class_name`, `extends`, signals, enums, constants, variables, properties, functions, inner classes | Godot's class-level tests pass | 1 day |
| M3 | Statements and expressions: Pratt table, lambdas, `match` patterns, node paths, `await`, `preload` | Godot's valid tests parse with no errors | 1 day |
| M4 | Error recovery and Godot's messages; Godot-suite harness | All 73 parser-error tests either match or are listed as semantic | 0.5–1 day |
| M5 | Typed layer, gdtidy switched over, tree-sitter removed, real-project corpus at zero errors, fuzzing and speed checks | gdtidy's tests pass on the new parser; corpus clean | 1 day |

Total: about 4–5 agent days, and about one day of your review, mostly at M2 and M5. gdhealth and gdtest-analysis move to gdsyntax afterwards, in their own repositories.

## Risks

- **Coupling between lexer and parser** is the hard part. It is also why the other parsers fail, so it gets the most tests: lambdas in calls, inline `match` branches, and brackets spanning lines with comments.
- **Godot mixes syntax and semantic checks** in one parser. The list of skipped semantic errors must be reviewed when syncing each version.
- **Error messages drift between Godot versions.** The harness compares messages per vendored version, so a drift shows up as a test failure, not as a silent difference.
- **Licenses.** Godot's code and tests are MIT, and so is gdtoolkit; both need their notices kept. Public projects are fetched for tests, never committed.
