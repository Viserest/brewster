# brewster

A web browser written in Rust, driven by a Rust/Lua-like language in ```.cre``` files which create the structures, styles and scripts.

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
  brewster       GUI viewer
examples/
  demo.cre
```

Logic lives in `crates/*`; UI lives in `apps/*`. The crates use only `std`; `brewster-cli` uses `crossterm`.

## Run

```
cargo run -p brewster-cli -- examples/demo.pseudo               # interactive viewer
cargo run -p brewster-cli -- --once examples/demo.pseudo        # print once, with colors
cargo run -p brewster-cli -- --width 60 --plain examples/demo.pseudo
cargo test --workspace
```

Requires Rust 1.85+ (edition 2024, resolver 3).

## Interactive viewer keys

`j`/`k`/arrows scroll, `space`/`b` or PageDown/PageUp page, `g`/`G` or Home/End jump, `r` reload the file, `q`/Esc/Ctrl-C quit. The layout reflows when the terminal is resized.

## Box model

Outside in: `margin`, `border`, `pad`, content. `margin`, `border` and `pad` share one shorthand:

```
.pad:1            all sides
.pad:2,1          x,y
.pad:1,2,3,4      top,right,bottom,left
.pad-x:2  .pad-y:1
```

`border` is 0 or 1 cell per side, drawn with box characters (`.border:1`, `.border-x:1`, `.border:1,0,1,0`). Corners appear only where two drawn sides meet. `.border-color:cyan` sets its color (defaults to the foreground).
