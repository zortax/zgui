//! A gallery of what the style engine actually draws.
//!
//! Run it with `cargo run -p zgui-examples --example styled`.
//!
//! There is barely any Rust in it, which is the point: the layout is grid and flexbox, the
//! appearance is gradients, shadows, rounded corners, borders, transforms, filters and opacity,
//! and every one of those is an ordinary declaration in an ordinary style sheet. The one piece of
//! state is which swatch was last clicked, so that the sheet has something to react to through
//! `:hover` and a toggled class.

use zgui::prelude::*;

/// A mark drawn with the colour of whatever it is put inside.
///
/// Every paint in it is `currentColor`, so the same source is a different colour in each of the
/// two swatches below with nothing but a `color` declaration between them.
const MARK: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
     <path d="M12 2 L22 21 L2 21 Z" fill="currentColor"/>
     <circle cx="12" cy="16" r="2.4" fill="#0b0d12"/>
   </svg>"##;

/// A drawing that brings its own palette and keeps it wherever it is put.
const SCENE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 32">
     <defs>
       <linearGradient id="sky" x1="0" y1="0" x2="0" y2="1">
         <stop offset="0" stop-color="#2b3a67"/>
         <stop offset="1" stop-color="#8a5a8f"/>
       </linearGradient>
     </defs>
     <rect x="0" y="0" width="48" height="32" rx="3" fill="url(#sky)"/>
     <circle cx="36" cy="9" r="4.5" fill="#ffd76a"/>
     <path d="M0 32 L14 16 L24 26 L34 14 L48 32 Z" fill="#1f7a5a"/>
     <path d="M0 32 L11 22 L20 32 Z" fill="#145f45" opacity="0.9"/>
   </svg>"##;

/// One panel of the gallery, with a heading above whatever it is showing.
#[component]
fn Panel(
    /// What the panel is called.
    #[prop(into)]
    title: String,
    /// One line about what it shows.
    #[prop(into)]
    note: String,
    /// What it shows.
    children: Children,
) -> impl IntoView {
    view! {
        column(class = "panel") {
            label(class = "panel__title") {{title}}
            label(class = "panel__note") {{note}}
            box(class = "panel__body") {{children.into_view_once()}}
        }
    }
}

/// The gallery.
#[component]
fn Gallery() -> impl IntoView {
    let picked = RwSignal::new(2_usize);

    view! {
        column(class = "page") {
            row(class = "masthead") {
                column(class = "masthead__text") {
                    label(class = "masthead__title") {"zgui"}
                    label(class = "masthead__subtitle") {
                        "components, signals and CSS, in a real window"
                    }
                }
                spacer()
                box(class = "badge") {"gallery"}
            }

            box(class = "grid") {
                Panel(title = "Grid", note = "three tracks, one gap") {
                    box(class = "tiles") {
                        box(class = "tile tile--a")
                        box(class = "tile tile--b")
                        box(class = "tile tile--c")
                        box(class = "tile tile--c")
                        box(class = "tile tile--a")
                        box(class = "tile tile--b")
                    }
                }

                Panel(title = "Gradients", note = "linear and radial, with stops") {
                    column(class = "ramps") {
                        box(class = "ramp ramp--linear")
                        box(class = "ramp ramp--radial")
                        box(class = "ramp ramp--angled")
                    }
                }

                Panel(title = "Shadows", note = "cast, inset and a lifted card") {
                    row(class = "shadows") {
                        box(class = "chip chip--cast")
                        box(class = "chip chip--inset")
                        box(class = "chip chip--lifted")
                    }
                }

                Panel(title = "Corners and borders", note = "per-corner radii and dashed edges") {
                    row(class = "corners") {
                        box(class = "corner corner--pill")
                        box(class = "corner corner--leaf")
                        box(class = "corner corner--dashed")
                    }
                }

                Panel(title = "Transforms", note = "rotate, scale and skew about a chosen origin") {
                    row(class = "transforms") {
                        box(class = "turn turn--rotate")
                        box(class = "turn turn--scale")
                        box(class = "turn turn--skew")
                    }
                }

                Panel(title = "Filters", note = "blur, saturate and opacity") {
                    row(class = "filters") {
                        box(class = "lens lens--blur")
                        box(class = "lens lens--saturate")
                        box(class = "lens lens--faded")
                    }
                }

                Panel(title = "Type", note = "size, weight, spacing and decoration") {
                    column(class = "type") {
                        text(class = "type__display") {"Aa"}
                        text(class = "type__body") {"The quick brown fox jumps over the lazy dog."}
                        row(class = "type__row") {
                            text(class = "type__thin") {"thin"}
                            text(class = "type__bold") {"bold"}
                            text(class = "type__wide") {"w i d e"}
                            text(class = "type__struck") {"struck"}
                        }
                    }
                }

                Panel(title = "Vector", note = "one asset taking the colour around it, one keeping its own") {
                    row(class = "art") {
                        box(class = "art__ink art__ink--cyan") {
                            vector(class = "art__mark", prop:svg = MARK)
                        }
                        box(class = "art__ink art__ink--amber") {
                            vector(class = "art__mark", prop:svg = MARK)
                        }
                        vector(class = "art__scene", prop:svg = SCENE)
                    }
                }

                Panel(title = "Text transforms", note = "rotated, skewed and scaled, about a chosen origin") {
                    row(class = "turns") {
                        text(class = "turns__word turns__word--tilt") {"tilted"}
                        text(class = "turns__word turns__word--skew") {"skewed"}
                        text(class = "turns__word turns__word--tall") {"tall"}
                        text(class = "turns__word turns__word--upright") {"90°"}
                    }
                }

                Panel(title = "State", note = "a class toggled by a signal, and :hover in the sheet") {
                    row(class = "swatches") {
                        for index in || [0_usize, 1, 2, 3], key = |index: &usize| *index {
                            control(
                                class = "swatch",
                                class:swatch-picked = move || picked.get() == index,
                                a11y:label = "Swatch",
                                on:click = move |_| picked.set(index)
                            )
                        }
                    }
                }
            }
        }
    }
}

/// How it looks. This is the example.
const SHEET: &str = css!(
    ":root {
        background-color: #0b0d12;
        color: #e9edf6;
        font-family: sans-serif;
        overflow: auto;
    }

    .page { gap: 20px; padding: 28px 32px; }

    .masthead { align-items: center; gap: 16px; }
    .masthead__text { gap: 4px; }
    .masthead__title {
        font-size: 30px;
        font-weight: 700;
        letter-spacing: -1px;
    }
    .masthead__subtitle { font-size: 14px; color: #79839a; }

    .badge {
        padding: 6px 14px;
        border-radius: 999px;
        font-size: 12px;
        letter-spacing: 1px;
        color: #0b0d12;
        background-image: linear-gradient(100deg, #7ee3ff, #7d8bff 55%, #d18bff);
    }

    .grid {
        display: grid;
        grid-template-columns: repeat(4, 1fr);
        gap: 16px;
    }

    .panel {
        gap: 2px;
        padding: 16px;
        border-radius: 14px;
        border: 1px solid #1e2531;
        background-color: #12161e;
        box-shadow: 0 10px 24px rgba(0, 0, 0, 0.35);
    }

    .panel__title { font-size: 15px; font-weight: 700; }
    .panel__note { font-size: 11px; color: #79839a; }
    .panel__body { padding-top: 12px; }

    .tiles {
        display: grid;
        grid-template-columns: repeat(3, 1fr);
        gap: 6px;
    }
    .tile { height: 34px; border-radius: 6px; }
    .tile--a { background-color: #2f6bff; }
    .tile--b { background-color: #24d3a5; }
    .tile--c { background-color: #ff8a5b; }

    .ramps { gap: 8px; }
    .ramp { height: 24px; border-radius: 8px; }
    .ramp--linear { background-image: linear-gradient(90deg, #2f6bff, #d18bff); }
    .ramp--radial {
        background-image: radial-gradient(circle at 30% 50%, #ffd36e, #ff6b6b 70%);
    }
    .ramp--angled {
        background-image: linear-gradient(135deg, #24d3a5 0%, #2f6bff 50%, #12161e 100%);
    }

    .shadows, .corners, .transforms, .filters { gap: 14px; align-items: center; }

    .chip { width: 46px; height: 46px; border-radius: 12px; background-color: #2f6bff; }
    .chip--cast { box-shadow: 0 10px 18px rgba(47, 107, 255, 0.55); }
    .chip--inset {
        background-color: #1a2130;
        box-shadow: inset 0 6px 12px rgba(0, 0, 0, 0.8);
    }
    .chip--lifted {
        background-color: #24d3a5;
        box-shadow: 0 2px 0 #12161e, 0 16px 26px rgba(36, 211, 165, 0.4);
    }

    .corner { width: 52px; height: 46px; background-color: #232b3a; }
    .corner--pill { border-radius: 999px; }
    .corner--leaf { border-radius: 22px 4px 22px 4px; }
    .corner--dashed {
        border: 2px dashed #4a5670;
        border-radius: 10px;
        background-color: transparent;
    }

    .turn { width: 44px; height: 44px; border-radius: 8px; background-color: #d18bff; }
    .turn--rotate { transform: rotate(18deg); }
    .turn--scale { transform: scale(1.25); transform-origin: center; }
    .turn--skew { transform: skewX(-14deg); background-color: #7ee3ff; }

    .lens {
        width: 46px;
        height: 46px;
        border-radius: 10px;
        background-image: linear-gradient(45deg, #ff6b6b, #ffd36e);
    }
    .lens--blur { filter: blur(3px); }
    .lens--saturate { filter: saturate(0.25); }
    .lens--faded { opacity: 0.45; }

    .type { gap: 6px; }
    .type__display { font-size: 40px; font-weight: 700; line-height: 1; }
    .type__body { font-size: 13px; color: #b9c2d4; }
    .type__row { gap: 10px; }
    .type__thin { font-size: 12px; font-weight: 300; }
    .type__bold { font-size: 12px; font-weight: 800; }
    .type__wide { font-size: 12px; letter-spacing: 2px; }
    .type__struck { font-size: 12px; text-decoration: line-through; }

    .art { gap: 14px; align-items: center; }
    .art__ink {
        width: 46px;
        height: 46px;
        border-radius: 12px;
        background-color: #171c26;
        align-items: center;
        justify-content: center;
    }
    /* The mark below is drawn entirely in `currentColor`, so these two declarations are the
       only difference between the two copies of it. */
    .art__ink--cyan { color: #45d0d8; }
    .art__ink--amber { color: #f0a84a; }
    .art__mark { width: 26px; height: 26px; }
    /* The scene brings its own palette and is not tinted by anything around it. */
    .art__scene { width: 96px; height: 64px; }

    .turns { gap: 22px; align-items: center; height: 64px; }
    .turns__word {
        font-size: 20px;
        font-weight: 600;
        color: #cfd6e4;
        transform-origin: 50% 50%;
    }
    .turns__word--tilt { transform: rotate(-18deg); color: #8fb3ff; }
    .turns__word--skew { transform: skewX(-22deg); color: #c9a6ff; }
    .turns__word--tall { transform: scale(1.1, 2.1); color: #79e0b0; }
    .turns__word--upright { transform: rotate(-90deg); color: #f0a84a; }

    .swatches { gap: 10px; }
    .swatch {
        width: 34px;
        height: 34px;
        border-radius: 10px;
        border: 2px solid transparent;
        background-color: #232b3a;
    }
    .swatch:hover { background-color: #2c3546; }
    .swatch-picked {
        border-color: #7ee3ff;
        background-color: #2f6bff;
    }"
);

fn main() -> Result<(), zgui::Error> {
    app()
        .with_application_id("dev.zgui.Styled")
        .with_title("Styled")
        .with_size(1080.0, 720.0)
        .with_stylesheet(SHEET)
        .run(|| view! { Gallery() })
}
