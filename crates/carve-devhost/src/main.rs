//! Development host: runs one kernel and answers `kernel_call` over plain
//! HTTP on the loopback interface, so `pnpm -C app dev:kernel` can drive the
//! real kernel from a browser. Not shipped in the app bundle.
//!
//! Request: `POST /kernel/call?name=<command>` with the JSON arguments as the
//! body. Reply: the framed bytes the Tauri shell would return (`JSON` or
//! `BLOB` tag, then the payload). Vite proxies `/kernel` here, so the page
//! and this host share an origin and no CORS headers are needed.
//!
//! Each connection is served on a thread of its own, so while one command
//! runs `POST /kernel/progress` answers `{running, progress}` and
//! `POST /kernel/cancel` asks it to stop (answering `{running}`), as the
//! shell's `kernel_progress` and `kernel_cancel` do. Commands still run one
//! at a time.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use carve_core::Kernel;

/// Port used when `CARVE_DEVHOST_PORT` is not set. Vite proxies to it.
const DEFAULT_PORT: u16 = 1421;
/// Largest request body accepted, which is far above any argument object.
const MAX_BODY: usize = 64 * 1024 * 1024;

fn main() {
    // Started again by itself to decode a DWG file out of process.
    if let Some(code) = carve_core::io::dwg::run_helper(std::env::args()) {
        std::process::exit(code);
    }
    carve_core::io::dwg::set_helper(carve_core::io::dwg::Helper::current_exe());
    let port = std::env::var("CARVE_DEVHOST_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);
    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("carve-devhost: cannot listen on 127.0.0.1:{port}: {error}");
            std::process::exit(1);
        }
    };
    eprintln!("carve-devhost: kernel listening on http://127.0.0.1:{port}/kernel/call");
    let kernel = Arc::new(Mutex::new(Kernel::new()));
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let kernel = Arc::clone(&kernel);
                std::thread::spawn(move || {
                    if let Err(error) = serve(&kernel, stream) {
                        eprintln!("carve-devhost: {error}");
                    }
                });
            }
            Err(error) => eprintln!("carve-devhost: connection failed: {error}"),
        }
    }
}

/// What a request asks for.
#[derive(Debug, PartialEq, Eq)]
enum Route {
    /// Run a kernel command.
    Call(String),
    /// How far the running command has got.
    Progress,
    /// Stop the running command.
    Cancel,
}

/// Answers one request on one connection, then closes it.
fn serve(kernel: &Mutex<Kernel>, stream: TcpStream) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            if key.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();
    let mut out = stream;
    let Some(route) = route(method, target) else {
        return respond(&mut out, "404 Not Found", "text/plain", b"not found");
    };
    if length > MAX_BODY {
        return respond(
            &mut out,
            "413 Payload Too Large",
            "text/plain",
            b"too large",
        );
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    match route {
        Route::Call(name) => {
            let args = String::from_utf8_lossy(&body);
            // A command that panicked is answered inside the kernel, so
            // the lock is only poisoned by a fault here; go on with it.
            let framed = {
                let mut kernel = kernel.lock().unwrap_or_else(|poison| poison.into_inner());
                kernel.call(&name, &args).into_framed()
            };
            respond(&mut out, "200 OK", "application/octet-stream", &framed)
        }
        Route::Progress => {
            let status = carve_core::progress::status();
            let text = format!(
                "{{\"running\":{},\"progress\":{}}}",
                status.running, status.progress
            );
            respond(&mut out, "200 OK", "application/json", text.as_bytes())
        }
        Route::Cancel => {
            let running = carve_core::progress::request_stop();
            let text = format!("{{\"running\":{running}}}");
            respond(&mut out, "200 OK", "application/json", text.as_bytes())
        }
    }
}

/// The route of a request: a POST to `/kernel/call`, `/kernel/progress` or
/// `/kernel/cancel`.
fn route(method: &str, target: &str) -> Option<Route> {
    match (method, target) {
        ("POST", "/kernel/progress") => Some(Route::Progress),
        ("POST", "/kernel/cancel") => Some(Route::Cancel),
        _ => command_name(method, target).map(Route::Call),
    }
}

/// The command a request asks for, when it is a well formed kernel call.
fn command_name(method: &str, target: &str) -> Option<String> {
    if method != "POST" {
        return None;
    }
    let query = target.strip_prefix("/kernel/call?")?;
    let name = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("name="))?;
    let valid = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_');
    valid.then(|| name.to_string())
}

fn respond(out: &mut TcpStream, status: &str, kind: &str, body: &[u8]) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    out.write_all(head.as_bytes())?;
    out.write_all(body)?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_posts_to_the_call_path_name_a_command() {
        assert_eq!(
            command_name("POST", "/kernel/call?name=entities.list"),
            Some("entities.list".to_string())
        );
        assert_eq!(command_name("GET", "/kernel/call?name=ping"), None);
        assert_eq!(command_name("POST", "/other?name=ping"), None);
        assert_eq!(command_name("POST", "/kernel/call?name=a%20b"), None);
        assert_eq!(command_name("POST", "/kernel/call?name="), None);
    }

    #[test]
    fn progress_and_cancel_have_routes_of_their_own() {
        assert_eq!(
            route("POST", "/kernel/call?name=ping"),
            Some(Route::Call("ping".to_string()))
        );
        assert_eq!(route("POST", "/kernel/progress"), Some(Route::Progress));
        assert_eq!(route("POST", "/kernel/cancel"), Some(Route::Cancel));
        assert_eq!(route("GET", "/kernel/cancel"), None);
    }
}
