# Brewster

A web browser written in Rust. A page is a Rust file (or a compiled `.wasm` module) that builds its elements, styles and handlers with the macros of the `cre` SDK crate.

Ideas came from these videos:\
Facedev:
- https://www.youtube.com/watch?v=iJsH7AdLmUA
- https://www.youtube.com/watch?v=37ISfJ2NSXQ
- https://www.youtube.com/watch?v=qiOtinFFfk8

Tsoding:
- https://www.youtube.com/watch?v=5YSj7mAAQQ0

## Run

```
cargo run -p brewster -- examples/demo.rs                  # GUI window
cargo run -p brewster-cli -- examples/demo.rs              # interactive terminal viewer
cargo run -p brewster-cli -- a.rs b.wasm                   # two tabs
cargo run -p brewster-cli -- --once examples/demo.rs       # print once, with colors
cargo run -p brewster-cli -- --width 60 --plain examples/demo.rs
cargo test --workspace
```

Requires Rust 1.85+ (edition 2024, resolver 3). The first build downloads `crossterm`, `eframe` and `wasmtime` (the wasm engine, in `crates/host-wasmtime`); the other `crates/*` libraries use only `std`. Both apps have a `wasm` feature, on by default; `--no-default-features` builds without wasmtime, and pages then report that there is no wasm engine. To test everything except the engine without network access: `cargo test --workspace --exclude host-wasmtime`.

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

## Attributes

|                 | Name                 | Value          | Description |
| --------------- | -------------------- | -------------- | ----------- |
| **Positional**  | | | |
|                 | Align                | `Sizing2` [^2] | (Width,Height) of the aligning. [^8] |
|                 | Origin               | `Sizing2` [^2] | (Width,Height) of the origin. [^8] |
| **Box Model**   | | | |
|                 | Background Color     | `Rgb` [^4]     | Background color includes size and padding. |
|                 | Padding Layer        | `Sizing4` [^3] | Top,Right,Bottom,Left padding outside of previous layer. |
|                 | Size                 | `Sizing2` [^2] | Width,Height total size. Example: Width_total = Summation {Pad_n(left + right)} + Width_content |
| **Lines**       | | | |
|                 | Line                 | `Sizing2` [^2] | Width,Height line |
|                 | Line Style           | `LineStyle`    | `LineStyle`: Solid, Dotted, Dashed, and Wavy. String allowed but must map into `FontTypecase`. |
|                 | Line Color           | `Rgb` [^4]     | |
|                 | Line Radius          | `Sizing4` [^3] | Corner curvature. `Curvature`: Circle, Bowed, or Square. String must map into `Curvature`. `Curvature` maps into `Unit`. |
|                 | Line Endcap          | `Sizing4` [^3] | Corner endcap style. `Endcap`: Circle, Bowed, Outward, or Square. String must map into `Endcap`. `Endcap` maps into `Unit`. |
|                 | Line-Shadow          | `Sizing4` [^3] | Top,Right,Bottom,Left line-shadow |
|                 | Line-Shadow Color    | `Rgb` [^4]     | |
|                 | Line-Shadow Radius   | `Sizing4` [^3] | Corner curvature. `Curvature`: Circle, Bowed, or Square. String must map into `Curvature`. `Curvature` maps into `Unit`. |
| **Font**        | | | |
|                 | Font Color           | `Rgb` [^4]     | |
|                 | Font Family          | `FontFamily`   | String must map into `FontFamily`. |
|                 | Font Size            | `Unit` [^1]    | String must map into `FontSize`. `FontSize` maps into `Unit`. |
|                 | Font Weight          | `Unit` [^1]    | String must map into `FontWeight`. `FontWeight` maps into `Unit`. |
|                 | Font Typecase        | `FontTypecase` | `FontTypecase`: Uppercase, Lowercase, Titlecase, Camelcase, or Snakecase. String must map into `FontTypecase`. |
| **Elements**    | | | |
|                 | Before               | `Element`      | Prependable element |
|                 | After                | `Element`      | Appendable element |
|                 | Extends              | `Element`      | Pre/append elements to current element |
| **Interactive** | | | |
|                 | Onhover              | Function [^4]  | Called when cursor hovers over element |
|                 | Onclick              | Function [^4]  | Called when cursor clicks on element |
|                 | Onload               | Function [^4]  | Called when element loads [^5] |
|                 | Ondestroy            | Function [^4]  | Called when element is removed [^6] |

[^1]: `Unit` is a relative value thats percentage-based on its parent element. For font size its precentage-based on screen height.
[^2]: `Sizing2` is 2 `Unit`s
[^3]: `Sizing4` is 4 `Unit`s
[^4]: `Rgb` is 3 `u8`s for red, green and blue. String allowed but must map into `Color`. `Color` allowed and maps into `Rgb`.
[^5]: Handler function callable by wasm. Accepts &[`Element`] as element calling handler function.
[^6]: Loading event happens on page loading, and element is added to the page.
[^7]: Destory event happens on removing the element. Closing the page doesn't call this.
[^8]: `Align`: Left, right or center. String must map into `Align`. `Align` maps into `Unit`.

### Examples

#### How to center a box element:
```rust
let parent = bx!({}, [
  bx!({ origin: "center", align: "center" }, []),
]);
```

1. Sets child box's origin to center of child box.
2. Sets child box's align to center of parent box.

[*] Origin's center is calculated using child box's total size.
[*] Align's center is calculated using parent box's content size.

#### How to add a border:
```rust
bx!({
    pad: vec!["5 10", "1 black", "5 10"],
}, [])
```

1. Padding layers are from left (inner-most) to right (outer-most)
2. 1st-layer: top/bottom: 5, left-right: 10 *Acts like a css padding*
3. 2st-layer: all-sides: 1 AND color: black *Acts like a css border*
4. 3st-layer: top/bottom: 5, left-right: 10 *Acts like a css margin*

```rust
struct Unit(u8);
struct Rgba(u8, u8, u8, u8);
enum LineCap { Circle, Square }
enum LineStyle { Solid, Pattern { length: Unit, gap: Unit, cap: LineCap }, Wavy(Unit) }
struct Sizing {
    top: Unit,
    right: Unit,
    bottom: Unit, 
    left: Unit
}
enum Layer {
    Padding(Sizing),
    Border { size: Sizing, color: Rgba, style: LineStyle },
}
struct Attribute {
    layers: Vec<Layer>,
}
```

## Elements

| Name   | Description               | Calling [^1]                 | Default Style |
| ------ | ------------------------- | --------------------------- | ------------- |
| Box    | Wrapper for many children | ```bx!({}, [Elements])```   | |
| H1     | Heading Level 1           | ```h1!({}, text)```         | Align: Center; Font Style: [Uppercase, Underline, Bold] |
| H2     | Heading Level 2           | ```h2!({}, text)```         | Font Style: [Uppercase, Underline, Bold] |
| H3     | Heading Level 3           | ```h3!({}, text)```         | Font Style: [Uppercase, Underline] |
| H4     | Heading Level 4           | ```h4!({}, text)```         | Font Style: [Underline, Bold] |
| H5     | Heading Level 5           | ```h5!({}, text)```         | Font Style: [Underline] |
| H6     | Heading Level 6           | ```h6!({}, text)```         | Font Style: [Bold] |
| P      | Paragraph text            | ```p!({}, text)```          | |
| Pre    | Pre-formated text         | ```pre!({}, text)```        | Font Family: monospace; Backcolor: Off-Black; Forecolor: Off-White; Border Radius: 5 |
| Link   | Link to a href with text  | ```link!({}, href, text)``` | Forecolor: Off-Blue; Font Decor: [Underline] |
| Input  | Collect input for text, radio buttons, checkboxes, sliders, etc | ```input!({}, type, value)``` | Pad: 2,1; Border: 1; Bordercolor: Off-Black; Border Radius: 5; Margin: 2,1 |
| Button | Paragraph text formated like a input. All elements can have an onclick function but this is specifically for button clicking | ```button!({}, text)``` | [*] Same as input element |

[^1]: All ```{}``` Would include attibute tags. ```bx!({ pad: "1", border: "1" })```

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

### The page's address, `root()` and the network

`root().url()` is the address of the tab (`Url { method, href, scheme, authority, path, query, fragment }`, with `query_param(name)`). The browser keeps it in step with the tab: following a link to another `#fragment` of the same page does not reload it, it just tells the page. `cre::root()` returns the page's `Root` anywhere (a handler also gets it as its second argument). Do not name a closure parameter `root`: it hides the function.

`root().net().post("/login", body)` (also `get`, `put`, `delete`, `patch`, `head`) returns a future of `Result<Response, NetworkError>`, like `fetch`. Pages run a tiny single-threaded executor: `cre::spawn(async { .. })` starts one, button handlers may `spawn`. A request is queued, the browser sends it (same origin only, `http`; an answer of any status is a `Response`, a refusal is `NetworkError::Blocked`) and hands the answer back.

`cre::router!` picks a handler by method and path; the method may be written in any case:

```rust
cre::router!([
    get "/": index,
    get "/hello/:name": hello,     // :name and a final *rest are parameters
    post "/login": login,
]);
fn hello(req: &cre::Request) -> Element { p!(format!("hi {}", req.param("name").unwrap_or(""))) }
fn index() -> Element { h1!("home") }   // a handler may take the request, or nothing
```

`cre::form!` builds a login form from a struct:

```rust
cre::form!(struct Login { username: String => Text, password: String => Password }, post "/login");
// Login::element() -> a bx! with an input per field, a Submit button and a status line
// Login::read(&root) -> Login (the current inputs)      Login::call(&self) -> the request future
```

`Submit` sends `Login::call` and writes the status or the failure into the status line. A wrong method or input type does not compile.

### Tree

A handler gets `root: &Root` (passed in, never global). `root.tree` has `query(selector) -> Option<Element>` (selectors `*`, `tag`, `#id`, `tag#id`, descendants only), `query_all`, `find(id)` and `append(&element)`. An `Element` is a cheap shared handle: `set_text`, `text`, `set`, `style`, `push`, `extend`, `children`, `parent`, `remove`, `query`, `query_all`, `deep_clone`. Mistakes (children on a leaf and the like) are recorded and shown as diagnostics, never crashes.

### Buttons

`button!({ onclick: |this: &Element, root: &Root| greet(this, root) }, "Submit")` runs the closure when the button is clicked (mouse or Enter). `this` is the clicked element. After a handler runs, the page is read back and laid out again. A failure shows on the status line.

### Plugins and applications

A page module runs sandboxed in wasmtime: it is instantiated with no imports (no files, network or clock), every call into it has a fuel budget (an endless loop traps instead of hanging the browser), and its memory is capped (`host_wasmtime::Limits`).

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
  host      running page modules: Guest / Runner traits, WasmGuest (the page protocol over any wasm engine), Session (a page and its plugins), Host
  host-wasmtime  the wasm engine: a Runner on wasmtime, with fuel and memory limits and no imports
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

Logic lives in `crates/*`; UI lives in `apps/*`. The crates use only `std` (except `host-wasmtime`); `brewster-cli` uses `crossterm` and `brewster` uses `eframe` (egui).

The text language and its `lexer`, `parser` and `script` crates are gone: pages are Rust, so `rustc` is the parser and the type checker, and the `cre` crate builds the live tree (`tree`) with the template rules (`extends`, `before`, `after`). Layout reads a snapshot of it.

## Known limits

- Requests are sent one after another on the browser's thread, so a slow server freezes the window; the method is always checked, the origin must equal the page's.
- The wasm interface is version 2 (`cre_alloc`, `cre_set_url`, `cre_input`, `cre_net_take`, `cre_net_done` were added); a module built with an older SDK is refused with a clear message.
- `cre::root()` is a thread-local accessor: it works inside the page and its handlers, which is where pages run.
- Shapes, themes and the GUI restyle (`apps/brewster`, `browser::geometry`) are unit-tested up to the egui calls, which were not compiled here.

- https is not supported (std has no TLS); http and files work.
- The wasmtime runner (`crates/host-wasmtime`) was written against the wasmtime 49 documentation and type-checked against a stub of its API, but it has not been compiled or run against the real crate in the environment it was written in (no network). Its own tests (`cargo test -p host-wasmtime`) are the first thing to run. The protocol around it (`host::WasmGuest`) is tested with a fake engine.
- A `.rs` page needs `rustc`, the `wasm32-unknown-unknown` target and the prebuilt SDK on the computer that opens it; otherwise only `.wasm` pages work.
- History is part of the profile format but nothing records it yet.
- The terminal cannot show different font sizes; "new window" is a tab there.
- The egui app was written against the egui 0.29 API but has not been built against the real crates in the environment this was developed in; the logic it uses (`browser`, `grid`) is unit-tested.
