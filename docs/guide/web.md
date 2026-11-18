# Running in a browser

A zgui application runs in a browser as WebAssembly. It draws into one canvas with the same
renderer as on a desktop: WebGPU where the browser has it, WebGL 2 where it does not. Style, layout,
text and hit testing are the same code as in a native build.

[Building a browser on zgui](browser.md) is a different subject: it describes how to build a
document browser with zgui.

## Build and serve

The build needs the `wasm32-unknown-unknown` target and the `wasm-bindgen` command at the version
`Cargo.lock` pins. `cargo xtask web` checks the version and prints the install command when it does
not match.

```console
cargo xtask web serve hello
cargo xtask web serve gallery --package zgui-ui
```

`serve` builds the example and serves `target/web/` on `http://127.0.0.1:8080/<example>/`.
`build` only builds. `--dev` builds without optimisation, which compiles faster and runs much
slower: measure and judge with the default build. `--font <file>` names a font file the page
preloads. With none named, the page preloads the faces this machine resolves for sans-serif at
four weights and for monospace, so that the page shows the faces a native build shows.

The output directory holds everything a static host needs:

| Path | Content |
|---|---|
| `index.html` | the host page |
| `pkg/<example>.js` | the bindings and the loader |
| `pkg/<example>_bg.wasm` | the application |
| `fonts/` | the font the page preloads |

Serve the `.wasm` file as `application/wasm`, so that the browser compiles it while it downloads.
Serve it compressed: the component gallery is 15 MB, and 4 MB with Brotli.

## The application

An application needs no change. `App::run` starts it on a new canvas that fills the page and
returns at once, because a page has no thread to block. An example with a `fn main` builds for the
browser as it is.

```rust,ignore
fn main() -> Result<(), zgui::Error> {
    app().with_stylesheet(SHEET).run(|| view! { Counter() })
}
```

`App::mount` starts the application on a canvas of the page and reports a failure to the caller.
The page sets the size of its canvas with CSS.

```rust,ignore
use zgui::app::Mount;

let mounted = app()
    .with_stylesheet(SHEET)
    .mount(Mount::selector("#app"), || view! { Counter() })
    .await?;
// Later:
mounted.unmount();
```

A page of a browser is no window, and `capabilities().window_controls` is `false` there. An
application that draws its own title bar reads it and leaves out the buttons that minimise,
maximise and close. An application that closes its window ends, and a canvas it made for itself
goes with it.

A page runs one application, and an application has one window. A second window has no canvas to
open on, and `use_windows().open` reports that the platform does not support it. Menus, dialogs and
tooltips are layers of the window, so they work.

## Fonts

A browser gives a page no list of installed fonts. The page names each font file with a preload
link, and the application registers the files before its first frame:

```html
<link rel="preload" as="fetch" type="font/ttf" href="fonts/Inter.ttf" crossorigin>
```

The link says `as="fetch"` because that is how the application reads the file. Name every font the
interface needs, including the fonts for other scripts and for emoji. A registered family is also a
fallback for every other family. An application can register fonts from its own bytes with
`Fonts::register` and `App::with_fonts`.

Name a file for each weight the interface uses. A weight with no file is drawn by thickening the
outlines of another weight, which is visibly softer than the real face.

A page that names no font draws no text, and the console says so.

## Renderers

| Renderer | When | Vectors | Text |
|---|---|---|---|
| WebGPU | the browser has an adapter | compute rasteriser (vello) | per-channel coverage on a desktop display of ordinary density |
| WebGL 2 | no WebGPU adapter | coverage rasteriser | one coverage for each pixel |

`?renderer=webgl2` or `?renderer=webgpu` in the page's address selects one. The two renderers are
the same renderer on two graphics interfaces, and they show the same pixels except for the text
antialiasing.

When the browser takes the graphics device away, the application gives up its surfaces, opens a new
device and builds its window again from its view. State in signals that the view creates starts
again. State the application keeps above the view stays.

## Frames and scrolling

The browser calls the application once for each display refresh in which a surface asked for a
frame, and never when nothing changed. An animation's next step is answered by that same call, so
an animation runs at the rate of the display.

A notched wheel arrives as whole detents, and the framework carries each one over several frames.
A detent travels the distance the browser gives it, as in any other page.
A touch surface arrives as a stream the system already animates, and passes through unchanged. A
touch on a phone pans and flings with the framework's own momentum. The canvas takes every wheel
event over it. With the control key held, the wheel is the browser's zoom.

## Text input and the clipboard

While a field is typed into, a hidden text area beside the canvas has the keyboard. An input
method, a dead key and the keyboard of a phone write into it, and the application receives what
they write as input-method events. The text area sits at the caret, so the input method opens its
candidates there.

Paste uses the browser's paste event, which carries the text with no permission prompt. Copy uses
the asynchronous clipboard, which a browser allows in a secure context and during a user gesture.
The clipboard carries plain text.

On an Apple system, the command key is the shortcut modifier: the command key copies and pastes.

## Images

The `src` of an image is a URL. The browser loads and decodes the image off the thread the
interface runs on, and scales it to the size it is shown at. An image from another origin needs
CORS headers. Bytes registered in the application still work.

## Accessibility

Each surface can mirror its accessibility tree as elements of the page: one element for each node,
with its role, name and states, placed where the node is drawn. A screen reader reads those
elements, and a click or a move of focus on one is an action on the node.

The mirror costs work on every update, so it is built on request. Each surface holds a button named
"Enable accessibility", which is the first thing a screen reader finds.
`zgui::app::enable_accessibility` turns the mirror on from code.

A field that is typed into is announced with its name. A screen reader cannot read its content
character by character.

## Limits

- One thread. Style and layout run on the thread of the page. Work given to `blocking` runs in a
  task of its own between frames, on the same thread. `std::thread::spawn` fails.
- No file system. A style sheet comes from the binary, an image from a URL.
- No dropped files and no drag out of the canvas.
- A canvas inside a scrolling page keeps every wheel event over it.
- A canvas under a CSS transform is measured wrong. Put transforms on an element around it.
- A paste that an application starts from its own menu reads the text of the last paste or copy
  in the page.

## Console

At its start the application says which renderer it draws with, which vector rasteriser it has and
how it draws text. Warnings, errors and panics go to the browser console. An application that installs its own `log`
logger before it starts keeps it.
