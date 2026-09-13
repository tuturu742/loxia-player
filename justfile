fmt:
    cargo fmt --all

lint:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace --lib --bins --tests --examples

run:
    cargo run -p loxia-player --

check-all:
    cargo fmt --all -- --check && just lint && just test && cargo deny check

snap:
    cargo insta review
