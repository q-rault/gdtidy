//! Broken input must never make the parser panic, hang or lose a byte.
//!
//! Each of Godot's test scripts is edited at random many times (bytes
//! deleted, lines duplicated or swapped, tokens inserted) and every result
//! must parse quickly into a tree that reproduces it exactly. The edits are
//! seeded, so a failure always comes back the same way.
//!
//! `GDSYNTAX_FUZZ_ITERATIONS` sets the number of edits per script (default 20).

use std::path::Path;
use std::time::{Duration, Instant};

/// A small deterministic random number generator (xorshift64*).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
}

const PIECES: &[&str] = &[
    "(", ")", "[", "]", "{", "}", ":", ",", ".", "..", "\n", "\t", "    ", "\\\n", "func",
    "func():", "var", "if", "elif", "else:", "match", "when", "for", "in", "return", "await",
    "lambda", "\"", "'''", "\"\"\"", "r\"", "&\"", "^\"", "$", "%", "@", "@export", "#", "##", "=",
    ":=", "->", "not", "is", "as", "1", "0x", "1e", "_", "class", "extends", "static", "\r", "é",
    "<<<<<<<",
];

fn mutate(source: &str, rng: &mut Rng) -> String {
    let mut text: Vec<char> = source.chars().collect();
    for _ in 0..1 + rng.below(4) {
        match rng.below(5) {
            0 if !text.is_empty() => {
                // Delete a range.
                let start = rng.below(text.len());
                let end = (start + 1 + rng.below(8)).min(text.len());
                text.drain(start..end);
            }
            1 => {
                // Insert a piece of syntax.
                let at = rng.below(text.len() + 1);
                let piece = PIECES[rng.below(PIECES.len())];
                text.splice(at..at, piece.chars());
            }
            2 => {
                // Duplicate a line.
                let lines: Vec<String> = text
                    .iter()
                    .collect::<String>()
                    .lines()
                    .map(String::from)
                    .collect();
                if !lines.is_empty() {
                    let i = rng.below(lines.len());
                    let mut out = lines.clone();
                    out.insert(i, lines[i].clone());
                    text = (out.join("\n") + "\n").chars().collect();
                }
            }
            3 => {
                // Swap two lines.
                let mut lines: Vec<String> = text
                    .iter()
                    .collect::<String>()
                    .lines()
                    .map(String::from)
                    .collect();
                if lines.len() > 1 {
                    let (a, b) = (rng.below(lines.len()), rng.below(lines.len()));
                    lines.swap(a, b);
                    text = (lines.join("\n") + "\n").chars().collect();
                }
            }
            _ => {
                // Remove indentation from a line.
                let s: String = text.iter().collect();
                let mut lines: Vec<String> = s.lines().map(String::from).collect();
                if !lines.is_empty() {
                    let i = rng.below(lines.len());
                    lines[i] = lines[i].trim_start().to_string();
                    text = (lines.join("\n") + "\n").chars().collect();
                }
            }
        }
    }
    text.into_iter().collect()
}

#[test]
fn random_edits_never_break_the_parser() {
    let iterations: usize = std::env::var("GDSYNTAX_FUZZ_ITERATIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/godot/4.7");
    let mut scripts = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "gd") {
                scripts.push(std::fs::read_to_string(&path).unwrap());
            }
        }
    }
    scripts.sort();

    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut slowest = Duration::ZERO;
    let mut count = 0;
    for script in &scripts {
        for _ in 0..iterations {
            let input = mutate(script, &mut rng);
            let start = Instant::now();
            let parse = gdsyntax::parser::parse(&input);
            let elapsed = start.elapsed();
            slowest = slowest.max(elapsed);
            let text = gdsyntax::SyntaxNode::new_root(parse.green).to_string();
            assert_eq!(text, input, "the tree lost bytes of:\n{input}");
            assert!(
                elapsed < Duration::from_secs(2),
                "parsing took {elapsed:?} on:\n{input}"
            );
            count += 1;
        }
    }
    eprintln!("{count} edited scripts parsed, slowest {slowest:?}");
}
