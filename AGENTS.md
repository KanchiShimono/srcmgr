# srcmgr

## Functional Programming Principles

Strictly adhere to these functional programming principles throughout development:

1. Parse, Don’t Validate
2. Make Illegal States Unrepresentable
3. Errors as values
4. Functional Core, Imperative Shell
5. Smart Constructor

## Idiomatic Paths

Apply these rules to all Rust code, including tests and `cfg`-gated code. Audit
inline paths containing `::` as well as `use` declarations.

- Reference crate roots directly by name, such as `std`; import their child
  modules and items as needed.
- For free functions, import the parent module and call the function through
  it; for example, use `use std::fs;` and `fs::read_to_string(path)` instead of
  importing `read_to_string`. A function already qualified by its crate root
  or `super`, such as `super::run()`, needs no additional import.
- For structs, enums, traits, type aliases, and other non-function items,
  import the full item and use its short name everywhere, including fields,
  signatures, associated types, error variants, patterns, and expressions,
  unless the same-name rule applies. For example, use
  `use std::collections::HashMap;` and `HashMap<String, usize>`, not
  `std::collections::HashMap<String, usize>` inline.
- Qualify associated functions, constants, and variants through their imported
  owning type; for example, use `PathBuf::from(...)`, `usize::MAX`, and
  `Ordering::Less` rather than importing associated items separately.
- If a module provides both free functions and types, import the module for
  function calls and import each type directly; for example, use
  `use std::fs;` and `use std::fs::Metadata;`, then call `fs::metadata(path)`
  and refer to the return type as `Metadata`.
- For same-named items, prefer parent-module qualification; for example, use
  `use std::fmt;` and `use std::io;` with `fmt::Result` and `io::Result`.
  Use an `as` alias only when it is a widely recognized Rust convention, such as
  `std::sync::atomic::Ordering as AtomicOrdering`, not merely to avoid
  qualification.

## Build, Test, and Development Commands

- `cargo build` — compile the library and `sm` binary in debug mode.
- `cargo run --bin sm -- --help` — run the CLI locally without installing it.
- `cargo test --workspace --locked --all-features --all-targets --no-fail-fast` — run the complete unit-test suite.
- `cargo +nightly fmt --all -- --check` — verify project `rustfmt` formatting.
- `cargo clippy --workspace --locked --all-targets --all-features` — lint every target and reject warnings.

Run all three quality checks before submitting a change. Commit
`Cargo.lock` whenever dependency resolution changes.
