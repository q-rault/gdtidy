fn main() {
    let src = std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap();
    let tree = gdsyntax::parse(src);
    for event in tree.root().preorder_with_tokens() {
        if let rowan::WalkEvent::Enter(e) = event {
            let depth = match &e {
                rowan::NodeOrToken::Node(n) => n.ancestors().count() - 1,
                rowan::NodeOrToken::Token(t) => t.parent_ancestors().count(),
            };
            match e {
                rowan::NodeOrToken::Node(n) => println!("{}{:?}", "  ".repeat(depth), n.kind()),
                rowan::NodeOrToken::Token(t) => {
                    println!("{}{:?} {:?}", "  ".repeat(depth), t.kind(), t.text())
                }
            }
        }
    }
    for e in tree.errors() {
        println!("error {}:{} {}", e.start.line, e.start.column, e.message);
    }
}
