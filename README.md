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
  cursor    cursor modes, copy text, input editing
  viewer    interactive viewer state and key handling (no terminal or window code)
  engine    lex -> parse -> resolve -> layout in one call
apps/
  brewster-cli   terminal viewer (crossterm)
  brewster       GUI viewer (egui)
examples/
  demo.cre
```

Logic lives in `crates/*`; UI lives in `apps/*`. The crates use only `std`; `brewster-cli` uses `crossterm` and `brewster` uses `eframe` (egui).

## Run

```
cargo run -p brewster -- examples/demo.cre                    # GUI window
cargo run -p brewster-cli -- examples/demo.cre               # interactive terminal viewer
cargo run -p brewster-cli -- --once examples/demo.cre        # print once, with colors
cargo run -p brewster-cli -- --width 60 --plain examples/demo.cre
cargo test --workspace
```

Requires Rust 1.85+ (edition 2024, resolver 3).

## Interactive viewer keys

Cursor: `j`/`k`/arrows/Tab move, `g`/`G` first/last stop, `m` toggles the mode (`focused` = links, inputs and buttons; `all` = every line). `y`/`c` copies the line under the cursor (link text, button label, input value; sent to the system clipboard via OSC 52). `enter`/`i` edits an input; `p` or Ctrl+V pastes into it (terminal paste also works), Ctrl+C copies its value, Esc/Enter finishes.

View: `space`/`b`, PageDown/PageUp page, `J`/`K` one line, Home/End jump, `r` reload, `q`/Esc/Ctrl-C quit. The layout reflows when the terminal is resized.

## Box model

Outside in: `margin`, `border`, `pad`, content. `margin`, `border` and `pad` share one shorthand:

```
.pad:1            all sides
.pad:2,1          x,y
.pad:1,2,3,4      top,right,bottom,left
.pad-x:2  .pad-y:1
```

`border` is 0 or 1 cell per side, drawn with box characters (`.border:1`, `.border-x:1`, `.border:1,0,1,0`). Corners appear only where two drawn sides meet. `.border-color:cyan` sets its color (defaults to the foreground).

## Size

`size:W,H` sets an element's box (margin excluded) in cells. Each value is a number, `min` (smallest the content allows) or `max` (largest the parent allows). The default is `min,min`, so backgrounds, underlines and borders end right after the content. One value applies to both axes (`size:12` is 12x12; use `size:12,min` for width only); `size-x` / `size-y` set one axis. Text and inputs wrap at the width; an input with a fixed height scrolls to follow its caret.

## Headers

All headers are wrapped in `=` marks: `h1` uppercase with `===`; `h2` uppercase; `h3` bold, uppercase; `h4` bold; `h5` bold, underlined; `h6` underlined.

## GUI backends

`apps/brewster` picks its windowing backend with a Cargo feature. `egui` (via `eframe` 0.29) is the default and the only one implemented. `iced` and `slint` are reserved names and fail to compile with a clear message until someone implements them. The GUI paints the same cell grid as the terminal viewer (monospace font, one cell per character) and uses the same keys; the mouse wheel scrolls, Ctrl+C / Ctrl+V (Cmd on macOS) use the system clipboard, and the status line is a bar under the page.

New backends only need to turn their input events into `viewer::KeyPress`es, call `Viewer::on_key` / `on_paste` / `on_resize`, and paint `Viewer::page` with `viewer::paint::row_pieces`; see `apps/brewster/src/egui_backend.rs`.
