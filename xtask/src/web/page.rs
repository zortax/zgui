//! The page that hosts an example.

/// The host page of `example`, which preloads each font of `fonts` from `fonts/`.
///
/// The page is the smallest one that works: the application makes its own canvas and fills the
/// viewport with it. A preload link is how the page gives the application a font.
pub(super) fn render(example: &str, fonts: &[String]) -> String {
    let links: String = fonts
        .iter()
        .map(|font| {
            let kind = if font.ends_with(".otf") { "otf" } else { "ttf" };
            format!(
                "  <link rel=\"preload\" as=\"fetch\" type=\"font/{kind}\" href=\"fonts/{font}\" \
                 crossorigin>\n"
            )
        })
        .collect();
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{example}</title>
{links}  <style>html, body {{ height: 100%; margin: 0; background: #fff; }}</style>
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
    fn the_page_loads_the_module_and_preloads_each_font() {
        let page = render("hello", &["Face.ttf".to_owned(), "Bold.otf".to_owned()]);
        assert!(page.contains(r#"import init from "./pkg/hello.js";"#));
        assert!(page.contains(r#"as="fetch" type="font/ttf" href="fonts/Face.ttf""#));
        assert!(page.contains(r#"as="fetch" type="font/otf" href="fonts/Bold.otf""#));
    }
}
