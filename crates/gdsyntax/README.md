# gdsyntax

GDScript syntax trees and syntax errors for the [gdsuite](https://github.com/q-rault/gdsuite) tools. It wraps [tree-sitter-gdscript](https://github.com/PrestonKnopp/tree-sitter-gdscript), so trees are lossless: every byte of the source, comments included, belongs to a node.

```rust
let tree = gdsyntax::parse("func f(:\n\tpass\n");
for error in tree.errors() {
    println!("{}:{}: {}", error.start.line, error.start.column, error.message);
}
```

Lines and columns are 1-based, and columns count characters, not bytes.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.
