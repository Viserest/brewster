# brewster

A web browser written in Rust, driven by a single Rust/Lua-flavoured language (name TBD) for structure, style and scripts.

## Layout

```
crates/
  lexer     source text -> tokens
  parser    tokens -> AST (let bindings, element trees)
  resolver  let / extends / before / after -> fully merged styles
  style     colors, spacing, alignment, entities
  layout    resolved tree -> rows of styled spans (renderer-agnostic)
  engine    lex -> parse -> resolve -> layout in one call
apps/
  brewster-cli   terminal viewer (ANSI)
  brewster       GUI viewer (placeholder)
examples/
  demo.pseudo
```

Logic lives in `crates/*`; UI lives in `apps/*`. Only `std` is used (no external dependencies).

## Run

```
cargo run -p brewster-cli -- examples/demo.pseudo
cargo run -p brewster-cli -- --width 60 --plain examples/demo.pseudo
cargo test --workspace
```

Requires Rust 1.85+ (edition 2024, resolver 3).
