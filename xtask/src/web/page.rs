//! The page that hosts an example.

/// The host page of `example`, which preloads the font `font` from `fonts/`.
///
/// The page is the smallest one that works: the application makes its own canvas and fills the
/// viewport with it. The preload link is how the page gives the application a font.
pub(super) fn render(example: &str, font: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{example}</title>
  <link rel="preload" as="fetch" type="font/ttf" href="fonts/{font}" crossorigin>
  <style>html, body {{ height: 100%; margin: 0; background: #fff; }}</style>
</head>
<body>
  <script type="module">
    import init from "./pkg/{example}.js";
    await init();
  </script>
</body>
</html>
"#
    )
}

#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn the_page_loads_the_module_and_preloads_the_font() {
        let page = render("hello", "Face.ttf");
        assert!(page.contains(r#"import init from "./pkg/hello.js";"#));
        assert!(page.contains(r#"as="fetch" type="font/ttf" href="fonts/Face.ttf""#));
    }
}
