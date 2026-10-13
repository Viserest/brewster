# Brewster

A web browser written in Rust. A page is a Rust file (or a compiled `.wasm` module) that builds its elements, styles and handlers with the macros of the `cre` SDK crate.

Ideas came from these videos:\
Facedev:
- https://www.youtube.com/watch?v=iJsH7AdLmUA
- https://www.youtube.com/watch?v=37ISfJ2NSXQ
- https://www.youtube.com/watch?v=qiOtinFFfk8

Tsoding:\
- https://www.youtube.com/watch?v=5YSj7mAAQQ0

## Run

```
cargo run -p brewster -- examples/demo.rs                    # GUI window
cargo run -p brewster -- examples/demo.wasm                  # Using pre-compiled wasm
cargo run -p brewster-cli -- examples/demo.rs                # interactive terminal viewer
cargo run -p brewster-cli -- a.rs b.rs                       # two tabs
cargo run -p brewster-cli -- --once examples/demo.rs         # print once, with colors
cargo run -p brewster-cli -- --once --plain examples/demo.rs # print once, without colors
cargo test --workspace
```

Requires Rust 1.85+ (edition 2024, resolver 3). The first build downloads `crossterm` and `eframe`; nothing else is fetched from the network, and the `crates/*` libraries use only `std`.

A `.rs` page is compiled to wasm by calling `rustc` directly (no cargo, no other crates): the browser needs `rustc`, the `wasm32-unknown-unknown` target and the prebuilt `cre` rlib (`BREWSTER_SDK_DIR`, see `plugin::Compiler::detect`). The result is cached by a hash of the source, the compiler and the SDK. When a page cannot be compiled here, the `.wasm` next to it (same URL, other extension) is requested instead.

Both apps take the same options:

```
[FILE|URL]...        open these as tabs (none: the last session's tabs, or the home page)
--profile NAME       use the profile NAME (default: default)
--profile-file PATH  use a profile file (.toml, .json, .yaml, .yml)
--private            a throwaway profile that is never saved
```

The page is as big as the terminal or window by default: the layout is made for the whole viewport, top-level `size:max` fills it, and it is laid out again when the viewport is resized (`view.fill_window`).

## Profiles

A profile is a sandbox: its settings, open tabs, application list, history (not recorded yet) and plugin store are separate from every other profile. The profile file stores all of it (toml, json or yaml by extension) at `~/.config/brewster/profiles/NAME.toml`. Plugin code is kept one file per plugin: `~/.brewster/plugins/NAME/` by default, or `/tmp/brewster/plugins/NAME/` with `storage = "tmp"` (gone after a reboot, and always used by `--private`). A window opened from another window is a *follower*: it does not overwrite the first window's remembered tabs.

### Settings

`ctrl-p` (or `f1`) opens a box in the middle of the screen: a search input on top and the list of matching options below. Type to filter (matches the key and the description), Up/Down choose, Enter changes the option (flips a boolean, cycles a choice, edits a number, text or list), Esc closes. Changes apply at once and last for the session; `ctrl-s` (`keybinds.save_settings`) writes them to the profile file, `ctrl-r` resets the option to its default. Settings changed but not saved are session-only: quitting keeps them out of the file.

Settings are key/value pairs with nesting (`tabs.position` is `position` inside `tabs`). In the profile file only what differs from the defaults is written:

```toml
version = 1
name = "default"
storage = "home"

[settings.tabs]
position = "left"
sidebar_width = 20

[settings.keybinds]
quit = ["ctrl-q"]
```

Options: `general.homepage`, `general.restore_tabs`, `view.cursor_mode`, `view.fill_window`, `view.scale_headers`, `tabs.position` (`top`, `bottom`, `left`, `right`), `tabs.show` (`always`, `multiple`, `never`), `tabs.sidebar_width`, `tabs.title_width`, and the `keybinds.*` lists above. The box shows every option with its description.

### Tabs

Tabs work like a browser's: a bar with titles and a `×` to close, and `+` to add one. `tabs.position` puts the bar at the top or bottom, or a sidebar left or right; `tabs.show` hides it when there is only one tab (or always). The last row is the status line. The open tabs are remembered in the profile and come back next time (`general.restore_tabs`) unless addresses are given on the command line. `about:newtab` lists the applications; `about:blank` is empty; `about:settings` opens the settings box.

## User Interfaces

New backends only need to turn their input events into `viewer::KeyPress`es, call `Viewer::on_key` / `on_paste` / `on_resize`, and paint `Viewer::page` with `viewer::paint::row_pieces`; see `apps/brewster/src/egui_backend.rs`.

### Brewster GUI

`apps/brewster` picks its windowing backend with a Cargo feature. `egui` (via `eframe` 0.29) is the default and the only one implemented; `iced` and `slint` are reserved names and fail to compile with a clear message until someone implements them. The GUI paints the browser's cell screen: every cell edge is on a whole physical pixel and every glyph is drawn in its own cell, so rows of a colored background meet without lines and the last character of a line is never clipped. Ctrl+C / Ctrl+V (Cmd on macOS) use the system clipboard.

New backends only need to turn their input events into `browser::KeyPress` / `MouseEvent`, call `Browser::on_key` / `on_mouse` / `on_paste` / `on_resize`, paint `Browser::screen()`, and carry out `Browser::take_effects()`; see `apps/brewster/src/egui_backend.rs`.

### Brewster CLI/TUI

NOTHING

### Keybinds

All keys can be changed in the settings (`keybinds.*`). Defaults:

| | |
|---|---|
| `q`, `ctrl-q` | quit (`ctrl-c` always quits outside text editing in the terminal) |
| `ctrl-p`, `f1` | settings box |
| `ctrl-t` / `ctrl-w` | new tab / close tab |
| `alt-right`, `alt-l` / `alt-left`, `alt-h` | next / previous tab |
| `ctrl-l`, `o` | open an address or file in this tab |
| `r`, `f5` | reload |
| `click`, `enter` | follow the link under the cursor (left mouse button or Enter on the selected link) |
| `ctrl-click`, `middle-click`, `ctrl-enter`, `t` | open the link in a new tab |
| `shift-click`, `shift-enter`, `w` | open the link in a new window (the terminal has one window: a tab opens and the status line says so) |

Terminals often cannot send `ctrl-enter` or `shift-enter`, so `t` and `w` work too. A binding is `[ctrl-][alt-][shift-]KEY`; KEY is a character, `enter`, `esc`, `tab`, `space`, `up`, `down`, `left`, `right`, `home`, `end`, `pageup`, `pagedown`, `backspace`, `delete`, `f1`..`f12`, or for mouse bindings `click`, `middle-click`, `right-click`.

Page: `j`/`k`/arrows/Tab move the cursor, `g`/`G` first/last stop, `m` toggles the cursor mode (`focused` = links, inputs and buttons; `all` = every line), `y`/`c` copies the line under the cursor (link text, button label, input value; the terminal sends it to the system clipboard with OSC 52), `enter`/`i` edits an input (`p` or Ctrl+V pastes, Esc/Enter finishes), `space`/`b`/PageDown/PageUp page, `J`/`K` one line, Home/End jump. The mouse wheel scrolls, clicking a tab selects it (middle click or its `×` closes it), clicking a link, button or input uses it. In the terminal, mouse capture is on, so use your terminal's usual modifier (often Shift) to select text with the mouse.

## Structure

NOTHING

### Headers

All headers are wrapped in `=` marks: `h1` uppercase with `===`; `h2` uppercase; `h3` bold, uppercase; `h4` bold; `h5` bold, underlined; `h6` underlined.

### Links

`link!("target", "text")` opens when clicked or when Enter is pressed on it. Relative targets are resolved against the page's address (`file:` paths and `http://`; `https://` is not supported because `std` has no TLS, and the page says so). Opening a link in a new tab or window keeps the current page.

## Styles

NOTHING

### Box Model

Outside in: `margin`, `border`, `pad`, content, each with one shorthand:

```
pad: "1"            all sides
pad: "2,1"          x,y
pad: "1,2,3,4"      top,right,bottom,left
pad_x: "2", pad_y: "1"
```

`border` is 0 or 1 cell per side, drawn with box characters (`border: "1"`, `"1,0"`, `"1,0,0,1"`); `border_color: "cyan"` sets its color. `size: "W,H"` sets the box in cells; each value is a number, `min` (the content) or `max` (what the parent allows). The default is `min,min`, so backgrounds, underlines and borders end right after the content. One value applies to both axes (`size: "12"` is 12x12; use `size: "12,min"` for width only). Text and inputs wrap at the width.

Headers differ in weight, underline and size; there are no `=` marks. `h1` size 3, bold, uppercase; `h2` size 2, bold, uppercase; `h3` size 2, uppercase; `h4` bold; `h5` bold, underlined; `h6` underlined. Font sizes need a graphical viewer (`view.scale_headers`); a terminal cannot draw them and shows the weight, underline and case only.

## Pages

A page is a Rust file that uses the `cre` crate: tag macros build [`Element`]s and `cre::page!(page);` exports the function that returns the root. See `examples/demo.rs`.

```rust
use cre::*;
cre::page!(page);

fn page() -> Element {
    let card = bx!({ pad: "1", border: "1" }, [p!("template children are skipped when extended")]);
    bx!({ extends: &card, id: "main" }, [h1!("Hello")])
}
```

Tags (`box` is a Rust keyword, so it is `bx!`):

```
bx!( {attributes}? , [children]? )              h1!..h6!, p!, span!, pre!, button!:
p!( {attributes}? , "text" )                      text only, no children
link!( {attributes}? , "href", "label" )
input!( {attributes}? , Text|Password, "name", "value" )
```

- The `{ }` group is the attributes, `name: value` separated by commas; the `[ ]` group is the children. Both are optional.
- `name` must be a variant of `cre::Attr`: a misspelled name does not compile, and rustc suggests the closest one. There are no hyphens in names: `border_color`, `align_x`, `pad_x`.
- Every value is written as a string and converted with `Into`. `Color`, `Pad` and `Border` are enums inside `Attr` (`Attr::bcolor(Color)`, `Attr::pad(Pad)`, `Attr::border(Border)`): `"white"` / `"#181818"` become a `Color`, `"1,0,1,2"` a `Pad`, `"1"` a `Border`. A value that does not parse is kept as `Invalid(text)` and reported by the browser when the page is drawn, not at compile time.
- `extends: &template` copies the style of a template element, skips its children, and carries over its `before` / `after` content. `before: bx!(..)` and `after: bx!(..)` add content around the element's own children.
- `id: "name"` names an element for `root.tree.query("#name")`.

### Tree

A handler gets `root: &Root` (passed in, never global). `root.tree` has `query(selector) -> Option<Element>` (selectors `*`, `tag`, `#id`, `tag#id`, descendants only), `query_all`, `find(id)` and `append(&element)`. An `Element` is a cheap shared handle: `set_text`, `text`, `set`, `style`, `push`, `extend`, `children`, `parent`, `remove`, `query`, `query_all`, `deep_clone`. Mistakes (children on a leaf and the like) are recorded and shown as diagnostics, never crashes.

### Buttons

`button!({ onclick: |this: &Element, root: &Root| greet(this, root) }, "Submit")` runs the closure when the button is clicked (mouse or Enter). `this` is the clicked element. After a handler runs, the page is read back and laid out again. A failure shows on the status line.

### Plugins and applications

A **plugin** is a page module the browser keeps and runs next to a page. It is one of:

1. `Plugin::Wasm(bytes)`: already compiled, nothing to build;
2. `Plugin::Source(text)`: Rust source, compiled to wasm on this computer and cached;
3. `Plugin::Url(url)`: a url of either of the above, fetched when needed. If the source behind it cannot be compiled here, the `.wasm` beside it is requested automatically.

An **application** (`plugin::AppKey`) is scoped like a service worker: the authority (domain or ip) of the page that installs it, plus a path. `"shop"` on a page of `example.com` means `example.com/shop`, which covers that page and everything below it (`""` or `"/"` is the whole domain; a leading `example.com/` is accepted; `..` is refused). Several plugins can be installed per application; they run in the order installed, after the page itself, on every matching page. They are saved to the plugin store (`plugin::Registry`) before they first run, so they run again on reload even when the connection is lost. A plugin that fails is left out and reported; the page and the other plugins still run. The Rust SDK does not yet expose `install` to page code; the registry is driven from the browser side.

## Project Tree

```
crates/
  cre       the page SDK: tag macros, Attr / Color / Pad / Border / InputType, Element, Root, wasm exports
  tree      the live document: nodes you can query, create and append, and the binary format pages are sent in
  plugin    plugins, applications, Uuid, the plugin store, and the rustc -> wasm compiler with its cache
  host      running page modules: Guest / Runner traits, Session (a page and its plugins), Host
  location  urls and loaders (file:, http:)
  style     colors, spacing, alignment, entities
  layout    element snapshot -> rows of styled spans (renderer-agnostic)
  cursor    cursor modes, copy text, input editing
  viewer    one page viewer: cursor, scrolling, key handling (no terminal or window code)
  settings  key/value settings with nesting; toml, json and yaml; the search index
  profile   a profile: settings, tabs, applications, history; saved as toml / json / yaml
  browser   tabs, links, key bindings, the settings box, the screen of cells
  engine    load (fetch, compile if needed) -> run -> layout in one call
apps/
  brewster-cli   terminal viewer (crossterm)
  brewster       GUI viewer (egui)
examples/
  demo.rs        elements, templates, a button with an onclick closure
  banner.rs      a small page that works as a plugin
```

Logic lives in `crates/*`; UI lives in `apps/*`. The crates use only `std`; `brewster-cli` uses `crossterm` and `brewster` uses `eframe` (egui).

The text language and its `lexer`, `parser` and `script` crates are gone: pages are Rust, so `rustc` is the parser and the type checker, and the `cre` crate builds the live tree (`tree`) with the template rules (`extends`, `before`, `after`). Layout reads a snapshot of it.

## Known limits

- https is not supported (std has no TLS); http and files work.
- The wasm engine is not part of this workspace yet: `host::Unavailable` refuses modules with a message, and `host::MapRunner` runs pages compiled into the program (tests and tooling). A `Runner` backed by a wasm interpreter (for example `wasmi`) must be added, and it needs a crate from the network.
- A `.rs` page needs `rustc`, the `wasm32-unknown-unknown` target and the prebuilt SDK on the computer that opens it; otherwise only `.wasm` pages work.
- History is part of the profile format but nothing records it yet.
- The terminal cannot show different font sizes; "new window" is a tab there.
- The egui app was written against the egui 0.29 API but has not been built against the real crates in the environment this was developed in; the logic it uses (`browser`, `grid`) is unit-tested.
