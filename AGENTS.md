# srcmgr

## Functional Programming Principles

Strictly adhere to these functional programming principles throughout development:

1. Parse, Don’t Validate
2. Make Illegal States Unrepresentable
3. Errors as values
4. Functional Core, Imperative Shell
5. Smart Constructor

## Idiomatic `use` Paths

Follow Rust's conventions for paths brought into scope with `use`:

- For functions, import the parent module and call the function through that
  module. For example, prefer `use crate::front_of_house::hosting;` followed by
  `hosting::add_to_waitlist()` over importing `add_to_waitlist` directly. This
  makes it clear that the function is not defined locally.
- For structs, enums, and other items, import the item with its full path. For
  example, use `use std::collections::HashMap;` and refer to it as `HashMap`.
- When multiple items have the same name, either import their parent modules and
  qualify each item (for example, `fmt::Result` and `io::Result`) or rename an
  import with `as` (for example, `use std::io::Result as IoResult;`).

## Build, Test, and Development Commands

- `cargo build` — compile the library and `sm` binary in debug mode.
- `cargo run --bin sm -- --help` — run the CLI locally without installing it.
- `cargo test --workspace --locked --all-features --all-targets --no-fail-fast` — run the complete unit-test suite.
- `cargo fmt --all -- --check` — verify default `rustfmt` formatting.
- `cargo clippy --config 'build.warnings="deny"' --workspace --locked --all-targets --all-features` — lint every target and reject warnings.

Run all three quality checks before submitting a change. Commit
`Cargo.lock` whenever dependency resolution changes.
