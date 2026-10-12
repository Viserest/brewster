# Brewster

A web browser written in Rust, driven by a Rust/Lua-like language in `.cre` files which create the structures, styles and scripts.

Ideas came from these videos:\
Facedev:
- https://www.youtube.com/watch?v=iJsH7AdLmUA
- https://www.youtube.com/watch?v=37ISfJ2NSXQ
- https://www.youtube.com/watch?v=qiOtinFFfk8

Tsoding:
- https://www.youtube.com/watch?v=5YSj7mAAQQ0

## Run

```
cargo run -p brewster -- examples/demo.cre                  # GUI window
cargo run -p brewster-cli -- examples/demo.cre              # interactive terminal viewer
cargo run -p brewster-cli -- a.cre b.cre                    # two tabs
cargo run -p brewster-cli -- --once examples/demo.cre       # print once, with colors
cargo run -p brewster-cli -- --width 60 --plain examples/demo.cre
cargo test --workspace
```

Requires Rust 1.85+ (edition 2024, resolver 3). `Cargo.lock` lists the workspace crates and `crossterm`; the first build adds `eframe` and its dependencies.

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

`link TARGET "text"` opens when clicked or when Enter is pressed on it. Relative targets are resolved against the page's address (`file:` paths and `http://`; `https://` is not supported because `std` has no TLS, and the page says so). Opening a link in a new tab or window keeps the current page.

## Styles

NOTHING

### Box Model

Outside in: `margin`, `border`, `pad`, content, each with one shorthand:

```
.pad:1            all sides
.pad:2,1          x,y
.pad:1,2,3,4      top,right,bottom,left
.pad-x:2  .pad-y:1
```

`border` is 0 or 1 cell per side, drawn with box characters; `.border-color:cyan` sets its color. `size:W,H` sets the box in cells; each value is a number, `min` (the content) or `max` (what the parent allows). The default is `min,min`, so backgrounds, underlines and borders end right after the content. One value applies to both axes (`size:12` is 12x12; use `size:12,min` for width only). Text and inputs wrap at the width.

Headers differ in weight, underline and size; there are no `=` marks. `h1` size 3, bold, uppercase; `h2` size 2, bold, uppercase; `h3` size 2, uppercase; `h4` bold; `h5` bold, underlined; `h6` underlined. Font sizes need a graphical viewer (`view.scale_headers`); a terminal cannot draw them and shows the weight, underline and case only.

## The language

Elements, styles and scripts share one grammar. Elements: `box`, `h1`..`h6`, `p`, `span`, `link`, `input`, `button`, `pre`; styles are `.key:value` modifiers; a `{ ... }` body holds children.

```
let card = box.pad:1.border:1 { p "template children are skipped when extended" }
box.extends:card.id:main { h1 "Hello" }
```

- `let name = <element>` is a template; `.extends:name` copies its style, skips its children, and carries over its `.before:` / `.after:` content.
- Everything else is a script: `let [mut]`, `fn`, `if` / `else if` / `else`, `match` (patterns `_`, names, numbers, strings, `true`/`false`, `Some(..)`, `None`, `Ok(..)`, `Err(..)`), `while`, `return`, closures `|a, b| a + b`, `+ - * / %`, comparisons, `&& || !`, lists, `{expr}` inside quoted strings (`{{` and `}}` are literal braces). Types (`x: String`, `-> Result<(), PluginError>`, `type X = ...`) are accepted but not checked.
- An element written in an element body (or at the top level) is appended to it; in a function body the last expression is the result.
- `.id:name` names an element for `root.tree.query("#name")`.

### Tree

`root.tree` is the root node of the current document. Node methods: `query(selector) -> Result<Node, QueryError>` (selectors `*`, `tag`, `#id`, `tag#id`, descendants only), `query_all(selector)`, `create(Tag::p) -> Result<Node, NodeError>`, `append(node) -> Result<Node, NodeError>` (only `root` and `box` hold children; moving a node into its own descendant is an error), `remove(node)`, `children()`, `parent()`, `tag()`, `text()`, `set_text(text)`, `get_style(key)`, `set_style(key, value)`, `get_arg(i)`, `set_arg(i, value)`. Errors are values (`Result`), never crashes.

### Buttons

`button.onclick:name("literal", 5, true) "Submit"` calls `fn name(self, ...)` when clicked (mouse or Enter). `self` is the clicked element; the extra arguments are the literals. A function error shows on the status line. See `examples/demo.cre`.

### Plugins and applications

```
root.plugin.oneshot(plugin: Plugin) -> Result<(), PluginError>
root.plugin.install(url_app: String, plugin: Plugin) -> Result<Uuid, PluginError>
root.plugin.list(url_app: String) -> Result<List<Plugin>, PluginError>
root.plugin.update(id: Uuid, plugin: Plugin) -> Result<(), PluginError>
root.plugin.delete(id: Uuid) -> Result<(), PluginError>
root.plugin.clear(url_app: String) -> Result<(), PluginError>
```

A script in a `.cre` file is not "a script" the browser keeps; a **plugin** is. A plugin is one of:

1. compiled on the server and sent as bytes (`Plugin::compiled("<hex>")`): the client cannot easily read the code (`Plugin::compile(source)` makes one);
2. `.cre` source compiled on the client (`Plugin::source("...")`);
3. a url of either of the above (`Plugin::url("https://...")`), fetched when it is needed.

An **application** (the `url_app`) is scoped like a service worker: the authority (domain or ip) of the page that installs it, plus a path. `url_app` is that path: `"shop"` on a page of `example.com` means `example.com/shop`, which covers that page and everything below it (`""` or `"/"` is the whole domain; a leading `example.com/` is accepted; `..` is refused). Several plugins can be installed per application; they run in the order installed, after the page's own script, on every matching page. They are saved to the plugin store before they first run, so they run again on reload even when the connection is lost. Installing does not run the plugin at once: it runs on the next load. Outside an application, `oneshot` runs a plugin once, right now. See `examples/plugins.cre`.

## Project Tree

```
crates/
  lexer     source text -> tokens (and the compiled byte format of plugins)
  parser    tokens -> AST: elements, styles, let / fn / if / match / while
  tree      the live document: nodes you can query, create and append
  script    the interpreter: runs a page's script into the tree, root.tree, root.plugin, onclick
  plugin    plugins, applications, Uuid and the on-disk plugin store
  location  urls and loaders (file:, http:)
  style     colors, spacing, alignment, entities
  layout    element snapshot -> rows of styled spans (renderer-agnostic)
  cursor    cursor modes, copy text, input editing
  viewer    one page viewer: cursor, scrolling, key handling (no terminal or window code)
  settings  key/value settings with nesting; toml, json and yaml; the search index
  profile   a profile: settings, tabs, applications, history; saved as toml / json / yaml
  browser   tabs, links, key bindings, the settings box, the screen of cells
  engine    parse -> run -> layout in one call
apps/
  brewster-cli   terminal viewer (crossterm)
  brewster       GUI viewer (egui)
examples/
  demo.cre       elements, templates, a button with an onclick function
  plugins.cre    applications and plugins
```

Logic lives in `crates/*`; UI lives in `apps/*`. The crates use only `std`; `brewster-cli` uses `crossterm` and `brewster` uses `eframe` (egui).

The old `resolver` crate is gone: it merged `let` / `extends` / `before` / `after` into a static tree once, which cannot work when scripts change the tree at runtime. The interpreter (`script`) now builds the live tree (`tree`) with the same template rules, and layout reads a snapshot of it.

## Known limits

- https is not supported (std has no TLS); http and files work.
- Types are parsed but not checked; `doc.onload`-style hooks do not exist (a page's own script runs at load, plugins after it).
- History is part of the profile format but nothing records it yet.
- The terminal cannot show different font sizes; "new window" is a tab there.
- The egui app was written against the egui 0.29 API but has not been built against the real crates in the environment this was developed in; the logic it uses (`browser`, `grid`) is unit-tested.
