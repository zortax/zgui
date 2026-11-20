//! A static file server for one directory, on this machine only.
//!
//! A browser loads a module and a WebAssembly file only over HTTP, and compiles the second while
//! it downloads only when it is served as `application/wasm`. The server sends the two headers
//! that make a page cross-origin isolated, which a build with threads needs.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};

use crate::error::{Error, Result};

/// Serves `directory` on `port` until the process is stopped, and names the page of `open`.
pub(super) fn run(directory: &Path, open: &str, port: u16) -> Result<()> {
    let address = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&address).map_err(|source| Error::io(&address, source))?;
    println!(
        "serving {} on http://{address}/{open}/",
        directory.display()
    );
    for stream in listener.incoming().flatten() {
        let directory = directory.to_path_buf();
        std::thread::spawn(move || {
            // A connection the browser dropped is not a failure of the server.
            let _ = answer(&directory, stream);
        });
    }
    Ok(())
}

/// Answers one request.
fn answer(directory: &Path, mut stream: TcpStream) -> std::io::Result<()> {
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line)?;
    let target = line.split_whitespace().nth(1).unwrap_or("/");
    match resolve(directory, target).and_then(|path| Some((fs::read(&path).ok()?, path))) {
        Some((body, path)) => {
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n\
                 Cache-Control: no-store\r\nCross-Origin-Opener-Policy: same-origin\r\n\
                 Cross-Origin-Embedder-Policy: require-corp\r\nConnection: close\r\n\r\n",
                content_type(&path),
                body.len()
            )?;
            stream.write_all(&body)
        }
        None => stream
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"),
    }
}

/// The file a request target names, when it is inside `directory`.
fn resolve(directory: &Path, target: &str) -> Option<PathBuf> {
    let path = target.split(['?', '#']).next().unwrap_or("");
    let relative = Path::new(path.trim_start_matches('/'));
    if relative
        .components()
        .any(|part| !matches!(part, Component::Normal(_)))
    {
        return None;
    }
    let resolved = directory.join(relative);
    if resolved.is_dir() {
        Some(resolved.join("index.html"))
    } else {
        Some(resolved)
    }
}

/// The media type of a file, from its extension.
fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("woff2") => "font/woff2",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{content_type, resolve};

    #[test]
    fn a_target_outside_the_directory_names_no_file() {
        let directory = Path::new("/srv/page");
        assert_eq!(resolve(directory, "/../secret"), None);
        assert_eq!(
            resolve(directory, "/pkg/app.js?v=1"),
            Some(directory.join("pkg/app.js"))
        );
    }

    #[test]
    fn a_module_is_served_as_the_type_the_browser_compiles_while_it_downloads() {
        assert_eq!(content_type(Path::new("app_bg.wasm")), "application/wasm");
    }
}
