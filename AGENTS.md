# srcmgr

## Functional Programming Principles

Strictly adhere to these functional programming principles throughout development:

1. Parse, Don’t Validate
2. Make Illegal States Unrepresentable
3. Errors as values
4. Functional Core, Imperative Shell
5. Smart Constructor

## Build, Test, and Development Commands

- `cargo build` — compile the library and `sm` binary in debug mode.
- `cargo run --bin sm -- --help` — run the CLI locally without installing it.
- `cargo test --workspace --locked --all-features --all-targets --no-fail-fast` — run the complete unit-test suite.
- `cargo fmt --all -- --check` — verify default `rustfmt` formatting.
- `cargo clippy --config 'build.warnings="deny"' --workspace --locked --all-targets --all-features` — lint every target and reject warnings.

Run all three quality checks before submitting a change. Commit
`Cargo.lock` whenever dependency resolution changes.
