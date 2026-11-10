//! A browser demo of zgui's retained renderer, text and reactive views.

#![forbid(unsafe_code)]
#![cfg(target_arch = "wasm32")]

use std::sync::Arc;

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use zgui::prelude::*;
use zgui::render::Renderer;
use zgui_render_canvas::CanvasRenderer;

const RING: &str = "M12 3 C16.971 3 21 7.029 21 12 C21 16.971 16.971 21 12 21 \
    C7.029 21 3 16.971 3 12 C3 7.029 7.029 3 12 3 Z \
    M12 6 C8.686 6 6 8.686 6 12 C6 15.314 8.686 18 12 18 \
    C15.314 18 18 15.314 18 12 C18 8.686 15.314 6 12 6 Z";

const TICK: &str = "M20.5 7.1 L18.9 5.5 L9.6 14.8 L5.1 10.3 L3.5 11.9 L9.6 18 Z";

#[component]
fn Demo(webgpu: bool) -> impl IntoView {
    let (count, set_count) = signal(0_i32);
    let (light, set_light) = signal(false);
    let pipeline = if webgpu {
        "Stylo / Cephal / Parley / wgpu + Vello"
    } else {
        "Stylo / Cephal / Parley / Canvas 2D"
    };

    view! {
        column(class = "demo", class:light = move || light.get()) {
            row(class = "heading") {
                column(class = "heading__text") {
                    label(class = "eyebrow") {"ZGUI / WEBASSEMBLY"}
                    text(class = "title") {"A native engine. In your browser."}
                    label(class = "subtitle") {"Rust views, CSS, text and vectors running as WebAssembly."}
                }
                control(
                    class = "theme",
                    tabindex = Focus::Sequential,
                    a11y:label = "Toggle color theme",
                    on:click = move |_| set_light.update(|value| *value = !*value)
                ) {{move || if light.get() { "Dark theme" } else { "Light theme" }}}
            }
            row(class = "panels") {
                column(class = "panel counter") {
                    label(class = "eyebrow") {"REACTIVE COUNTER"}
                    text(class = "number") {{move || count.get().to_string()}}
                    row(class = "buttons") {
                        control(class = "button", tabindex = Focus::Sequential,
                            a11y:label = "Decrease count",
                            on:click = move |_| set_count.update(|value| *value -= 1)) {"−"}
                        control(class = "button button--primary", tabindex = Focus::Sequential,
                            a11y:label = "Increase count",
                            on:click = move |_| set_count.update(|value| *value += 1)) {"+1"}
                        control(class = "button reset", tabindex = Focus::Sequential,
                            a11y:label = "Reset count",
                            on:click = move |_| set_count.set(0)) {"Reset"}
                    }
                    label(class = "caption") {
                        {move || if count.get() % 2 == 0 { "Even · one signal, two readers" } else { "Odd · one signal, two readers" }}
                    }
                }
                column(class = "panel vectors") {
                    label(class = "eyebrow") {"VECTOR PATHS"}
                    row(class = "marks") {
                        vector(class = "ring", prop:d = RING, prop:viewBox = "0 0 24 24")
                        vector(class = "tick", prop:d = TICK, prop:viewBox = "0 0 24 24")
                    }
                    text(class = "vector-title") {"The full drawing pipeline"}
                    label(class = "caption") {{pipeline}}
                    label(class = "caption") {"Hover the shapes. Resize the window."}
                }
            }
            row(class = "footer") {
                box(class = "status-dot")
                label(class = "caption") {"Everything inside this canvas is drawn by zgui."}
            }
        }
    }
}

const SHEET: &str = css!(
    ":root { display: flex; font-family: 'Demo Sans'; background: #10141b; color: #edf2f7; }
    .demo { width: 100%; height: 100%; padding: 36px; gap: 28px;
        background: #10141b; color: #edf2f7; }
    .heading { justify-content: space-between; align-items: center; gap: 20px; }
    .heading__text { gap: 8px; flex: 1; }
    .eyebrow { font-size: 11px; letter-spacing: 1.5px; color: #83d7c4; }
    .title { font-size: 27px; font-weight: 600; line-height: 1.25; }
    .subtitle { font-size: 13px; color: #94a3b8; }
    .theme { padding: 10px 14px; border: 1px solid #364151; border-radius: 8px;
        font-size: 12px; background: #1a2230; color: #dce6f0; }
    .theme:hover { background: #283549; }
    .panels { gap: 20px; flex: 1; min-height: 290px; }
    .panel { flex: 1; padding: 26px; gap: 18px; border: 1px solid #293545;
        border-radius: 14px; background: #18202c; }
    .counter { align-items: flex-start; }
    .number { font-size: 92px; line-height: 1.1; font-weight: 600; }
    .buttons { gap: 10px; align-items: center; }
    .button { padding: 12px 22px; border-radius: 8px; border: 1px solid #3d4a5c;
        background: #253144; color: #f0f5fa; font-size: 18px; line-height: 1; }
    .button:hover { background: #35465e; }
    .button--primary { background: #83d7c4; border-color: #83d7c4; color: #0e2d27; }
    .button--primary:hover { background: #a5efdf; }
    .reset { font-size: 13px; padding: 15px 16px; }
    .caption { color: #9aaabd; font-size: 12px; line-height: 1.5; }
    .vectors { justify-content: space-between; }
    .marks { align-items: center; gap: 18px; height: 128px; }
    .ring { width: 116px; height: 116px; color: #83d7c4; }
    .tick { width: 90px; height: 90px; color: #ffb77b; }
    .ring:hover, .tick:hover { color: #e9eff9; }
    .vector-title { font-size: 18px; }
    .footer { gap: 8px; align-items: center; }
    .status-dot { width: 7px; height: 7px; border-radius: 4px; background: #83d7c4; }
    .light { background: #eff3f7; color: #152539; }
    .light .panel { background: #ffffff; border-color: #d2dce7; }
    .light .caption, .light .subtitle { color: #53657a; }
    .light .eyebrow { color: #147362; }
    .light .ring { color: #1d9781; }
    .light .tick { color: #db7a30; }
    .light .ring:hover, .light .tick:hover { color: #254676; }
    @media (max-width: 650px) {
        .demo { padding: 20px; gap: 18px; }
        .heading { align-items: flex-start; }
        .title { font-size: 21px; }
        .subtitle { font-size: 12px; }
        .panels { flex-direction: column; }
        .panel { padding: 20px; gap: 12px; }
        .number { font-size: 64px; }
        .marks { height: 65px; }
        .ring { width: 65px; height: 65px; }
        .tick { width: 55px; height: 55px; }
    }"
);

/// Starts the demo in the host page's `zgui-canvas` element.
#[wasm_bindgen]
pub async fn start(use_webgpu: bool) -> Result<String, JsValue> {
    console_error_panic_hook::set_once();
    let fonts = zgui::app::Fonts::shipped_only();
    fonts
        .register(
            Arc::new(
                include_bytes!("../../zgui-text-parley/tests/fonts/NotoSans-Regular.ttf").to_vec(),
            ),
            Some("Demo Sans"),
        )
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    let mut application = app()
        .with_title("zgui WebAssembly")
        .with_size(980.0, 600.0)
        .with_fonts(fonts)
        .with_style_threads(1)
        .with_layout_threads(1)
        .with_stylesheet(SHEET);
    if !use_webgpu {
        let canvas = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id("zgui-canvas"))
            .ok_or_else(|| JsValue::from_str("missing zgui-canvas element"))?
            .dyn_into::<web_sys::HtmlCanvasElement>()?;
        application = application.with_renderer(Box::new(move |_, target| {
            let mut renderer = CanvasRenderer::new(canvas.clone()).map_err(|error| {
                zgui::runtime::AppError::Platform(zgui::platform::PlatformError::Backend(format!(
                    "Canvas 2D initialization failed: {error:?}"
                )))
            })?;
            renderer.configure(target);
            Ok(Box::new(renderer))
        }));
    }
    application
        .start_web("zgui-canvas", move || view! { Demo(webgpu = use_webgpu) })
        .await
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    Ok(if use_webgpu { "WebGPU" } else { "Canvas 2D" }.into())
}
