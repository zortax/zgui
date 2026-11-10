# Running zgui in a browser

This experimental port runs zgui's Rust views, Stylo CSS, Cephal layout, Parley text and retained
paint scenes as `wasm32-unknown-unknown`. Winit connects the application to an HTML canvas.
The default renderer uses WebGPU and Vello. A Canvas 2D renderer supports browsers without a
WebGPU adapter. The [browser architecture guide](browser.md) describes a separate use case:
building a document browser on zgui.

## Build and serve the demo

The build needs stable Rust with the `wasm32-unknown-unknown` target and **wasm-bindgen-cli
0.2.126**. The script uses the stable toolchain explicitly; the repository's pinned nightly is not
needed for this build.
`Cargo.lock` fixes dependency versions. The bindings CLI must match the locked `wasm-bindgen`.

```sh
rustup toolchain install stable
rustup target add --toolchain stable wasm32-unknown-unknown
cargo +stable install wasm-bindgen-cli --version 0.2.126 --locked
./scripts/build-web-demo.sh
python3 -m http.server 8000 --directory target/web-demo
```

Open <http://localhost:8000>. Use HTTPS when hosting the demo remotely. WebGPU and browser
clipboard access require a secure context. The build uses Bash, Cargo and wasm-bindgen; Python is
only the example HTTP server.

Pass an output directory to package the demo elsewhere:

```sh
./scripts/build-web-demo.sh
```

Relative output paths are relative to the caller's directory. Cargo artifacts remain under the
repository's `target` directory. The script builds `zgui-web-demo` with `--locked --profile web`,
then runs `wasm-bindgen --target web`. It packages:

| Path in output | Content |
|---|---|
| `index.html` | Host page copied from `web/index.html` |
| `pkg/zgui_web_demo.js` | JavaScript module and WASM loader |
| `pkg/zgui_web_demo_bg.wasm` | Compiled application, including its font |
| `pkg/*.d.ts` | Generated TypeScript declarations |
| `LICENSE`, `NOTICE`, `OFL-NotoSans.txt` | Project and bundled-font notices |

The host selects WebGPU when `requestAdapter` returns an adapter and otherwise starts Canvas 2D.
Use `?renderer=canvas` or `?renderer=webgpu` to select a backend. Forced WebGPU reports an error if
no adapter is available. If automatic selection encounters a rejected WebGPU startup, the host
reloads with Canvas 2D selected so the new page has a fresh canvas and runtime. Runtime failures
after startup still require a reload; the host offers a Canvas 2D link when it receives an error.

## Application startup

Create a canvas with an explicit CSS size. Give its parent a height when the canvas height is a
percentage:

```html
<div style="height: 600px">
  <canvas id="zgui-canvas" style="display: block; width: 100%; height: 100%"></canvas>
</div>
```

Keep padding, borders and CSS transforms on a wrapper. Winit observes canvas size and device pixel
ratio. The host's CSS controls the existing canvas size; `App::with_size` does not override it.

Register embedded font bytes with `Fonts::shipped_only()` and `Fonts::register`, then start the
application once per page:

```rust,ignore
app()
    .with_fonts(fonts)
    .with_stylesheet(SHEET)
    .start_web("zgui-canvas", || view! { Demo() })
    .await?;
```

`start_web` awaits WebGPU initialization when the application uses its default renderer, then
installs Winit's `spawn_app` event loop. It returns while browser callbacks continue to drive the
application. A successful return confirms startup; it does not wait for a rendered frame.
The runtime stays alive until the event loop exits. Later runtime errors go to the browser console.

To use Canvas 2D, pass a renderer factory before calling `start_web`:

```rust,ignore
use zgui::render::Renderer;
use zgui_render_canvas::CanvasRenderer;

let application = application.with_renderer(Box::new(move |_, target| {
    let mut renderer = CanvasRenderer::new(canvas.clone()).map_err(|error| {
        zgui::runtime::AppError::Platform(zgui::platform::PlatformError::Backend(
            format!("Canvas 2D initialization failed: {error:?}"),
        ))
    })?;
    renderer.configure(target);
    Ok(Box::new(renderer))
}));
```

Here `canvas` is the `web_sys::HtmlCanvasElement` with the same ID passed to `start_web`.
A supplied factory bypasses WebGPU initialization. Use a fresh canvas when changing context types:
the browser fixes the context type when the canvas first opens it.
The [demo source](../../crates/zgui-web-demo/src/lib.rs) shows the complete setup.

## Browser behavior and limits

- **Execution:** style and layout run serially. Browser tasks use local JavaScript microtasks.
  There are no web workers or shared-memory requirements. Work submitted through the blocking
  task API still runs on the browser's main thread and can delay input and painting.
- **Time:** `web-time` supplies browser clocks across platform, runtime, animation and rendering.
- **Input:** Winit supplies pointer and basic keyboard events. The port has no hidden input
  element for IME composition or mobile keyboards. IME capability flags remain disabled.
- **Clipboard:** the standard clipboard supports plain text through the asynchronous browser
  API, subject to browser permissions and user activation. Reads return through the platform
  waker, and paste keeps the original target. Synchronous reads are unavailable. Writes report
  request submission; later permission failures are logged. An immediate write/read pair is not
  serialized by this backend.
- **Accessibility:** the browser adapter does not expose zgui's semantic tree to browser screen
  readers. An ARIA label on the host canvas describes the canvas only.
- **Platform services:** desktop window placement, native menus, primary selection, drag and drop,
  and OS file dialogs are outside this port. The demo uses one canvas. Application assets should
  arrive as embedded bytes or through host-provided browser APIs.
- **Fonts:** browser system-font enumeration is unavailable. The demo embeds the unchanged Noto
  Sans Regular font from the text test fixtures and registers it under an application-local family
  alias. Keep its copyright and [SIL Open Font License](../../crates/zgui-text-parley/tests/fonts/OFL-NotoSans.txt)
  with distributed builds.

Canvas 2D consumes zgui's finished scene and glyph-atlas uploads. CSS, layout, text shaping,
glyph rasterization, hit testing and reactive updates remain in Rust. Its rendering subset and
approximations are listed in [zgui-render-canvas](../../crates/zgui-render-canvas/README.md).
Custom shaders, external GPU textures and several advanced effects remain unsupported there.
It is a compatibility backend with different antialiasing and performance characteristics.
