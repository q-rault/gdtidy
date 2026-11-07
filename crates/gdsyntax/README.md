# gdsyntax

A GDScript parser for the [gdsuite](https://github.com/q-rault/gdsuite) tools, translated from Godot 4.7's own tokenizer and parser. It accepts and rejects what Godot does, with Godot's error messages, and its tree is lossless: every byte of the file, comments and whitespace included, is in the tree.

```rust
use gdsyntax::ast::Member;

let tree = gdsyntax::parse("extends Node\n\n\nfunc _ready() -> void:\n\tprint(\"hi\")\n");
assert!(tree.errors().is_empty());
assert_eq!(tree.root().to_string(), tree.source());

for member in tree.file().members() {
    if let Member::Func(func) = member {
        println!("func {}", func.name().unwrap().text());
    }
}
```

- `parse` returns the tree and the errors, in the order Godot reports them. Lines and columns are 1-based, and columns count characters, not bytes.
- `ast` gives typed access to declarations, statements and expressions. Every accessor returns an `Option`, because a file with errors still has a tree.
- The tree is a [`rowan`](https://crates.io/crates/rowan) tree.

## What it checks

Syntax only. Godot's parser also reports some errors that need scope information, such as a name declared twice. Those are left to Godot itself; [`tests/godot/semantic-errors.txt`](tests/godot/semantic-errors.txt) lists them. Godot's check for identifiers that look like keywords is not ported either. Nesting deeper than 256 levels is reported as an error, so deeply nested input cannot overflow the stack.

## How it is tested

- **Godot's own test scripts** for 4.7 (in `tests/godot`): every script Godot parses parses with no error, and every script Godot rejects gets Godot's first error message, apart from the cases above.
- **Real projects**: `GDSYNTAX_CORPUS=<folders> cargo test -p gdsyntax --test corpus` checks that every file round-trips and parses without errors.
- **Broken input**: random edits to valid files must never make the parser panic, hang or lose a byte.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option. The tokenizer and parser follow Godot's, which is MIT-licensed ([Godot's license](tests/godot/LICENSE.txt)).
