set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

test:
    cargo test --quiet

check:
    cargo fmt --all --check
    cargo clippy --all-targets --locked -- -D warnings
    cargo test --locked --quiet

build:
    cargo build --release --quiet
