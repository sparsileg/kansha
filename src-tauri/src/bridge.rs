//! Dev-only IPC bridge for screenshot automation (#63).
//!
//! Lets a page in an ordinary browser (Playwright's Chromium) call the
//! running app's commands, so the real UI can be shot with the real
//! engine behind it. Debug builds only, and off unless `KANSHA_BRIDGE`
//! names a port. Refuses to start without `KANSHA_DB`, so it never opens
//! the user's own book.
//!
//! `POST /invoke` with `{"cmd": "...", "args": {...}}` and the token
//! printed at start in the `x-kansha-token` header. The request goes to
//! the main webview's own command handler, exactly as if the window had
//! sent it, so every registered command works and nothing here lists
//! them. Reply: `{"ok": <value>}` or `{"err": <value>}`.
//!
//! Loopback only; no logic, and no SQL: it only carries messages.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use serde_json::Value;
use tauri::ipc::{CallbackFn, InvokeBody, InvokeError, InvokeResponse, InvokeResponseBody};
use tauri::webview::InvokeRequest;
use tauri::{AppHandle, Manager};

/// A command that takes longer than this is reported as failed.
const REPLY_WAIT: Duration = Duration::from_secs(300);
/// Largest request body taken.
const MAX_BODY: usize = 16 * 1024 * 1024;

/// Starts the bridge when `KANSHA_BRIDGE` is set. Prints one line,
/// `KANSHA_BRIDGE http://127.0.0.1:<port> token=<token>`, for the
/// script that started the app.
pub fn start(app: &AppHandle) {
    let Some(port) = std::env::var("KANSHA_BRIDGE").ok() else {
        return;
    };
    if std::env::var_os("KANSHA_DB").is_none() {
        eprintln!("KANSHA_BRIDGE needs KANSHA_DB (a throwaway book); bridge not started");
        return;
    }
    let Ok(port) = port.parse::<u16>() else {
        eprintln!("KANSHA_BRIDGE must be a port number; bridge not started");
        return;
    };
    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("bridge could not listen on 127.0.0.1:{port}: {e}");
            return;
        }
    };
    let token = new_token();
    println!("KANSHA_BRIDGE http://127.0.0.1:{port} token={token}");
    hide_window(app.clone());
    let app = app.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let app = app.clone();
            let token = token.clone();
            std::thread::spawn(move || {
                if let Err(e) = serve(stream, &app, &token) {
                    eprintln!("bridge: {e}");
                }
            });
        }
    });
}

/// With the bridge on, the per-computer config sits beside the
/// throwaway book (`KANSHA_DB`), so a run never changes this computer's
/// theme, window size, or recent books.
pub fn config_path() -> Option<PathBuf> {
    std::env::var_os("KANSHA_BRIDGE")?;
    let db = PathBuf::from(std::env::var_os("KANSHA_DB")?);
    Some(db.with_file_name("bridge-config.json"))
}

/// The browser page does the drawing; the app's own window stays
/// hidden. It still has to exist: commands run through its webview.
fn hide_window(app: AppHandle) {
    std::thread::spawn(move || {
        for _ in 0..100 {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    });
}

/// Not secret against this computer's user; it keeps other local pages
/// from driving the app through the bridge.
fn new_token() -> String {
    let a = RandomState::new().build_hasher().finish();
    let mut h = RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default(),
    );
    format!("{a:016x}{:016x}", h.finish())
}

fn serve(stream: TcpStream, app: &AppHandle, token: &str) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();

    let mut length = 0usize;
    let mut given = String::new();
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 {
            break;
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            let value = value.trim();
            match name.trim().to_ascii_lowercase().as_str() {
                "content-length" => length = value.parse().unwrap_or(0),
                "x-kansha-token" => given = value.to_string(),
                _ => {}
            }
        }
    }

    let mut out = stream;
    if method == "OPTIONS" {
        return reply(&mut out, 204, "");
    }
    if method != "POST" || path != "/invoke" {
        return reply(&mut out, 404, r#"{"err":"not found"}"#);
    }
    if given != token {
        return reply(&mut out, 403, r#"{"err":"bad token"}"#);
    }
    if length > MAX_BODY {
        return reply(&mut out, 413, r#"{"err":"too large"}"#);
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    let request: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => return reply(&mut out, 400, &err_json(&e.to_string())),
    };
    let cmd = request
        .get("cmd")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let args = request
        .get("args")
        .cloned()
        .unwrap_or(Value::Object(Default::default()));
    let answer = invoke(app, cmd, args);
    reply(&mut out, 200, &answer)
}

/// Runs one command through the main webview's handler; returns the
/// reply as JSON text.
fn invoke(app: &AppHandle, cmd: &str, args: Value) -> String {
    let Some(window) = app.get_webview_window("main") else {
        return err_json("the main window is not open");
    };
    let webview: tauri::Webview = window.as_ref().clone();
    let url = match webview.url() {
        Ok(u) => u,
        Err(e) => return err_json(&e.to_string()),
    };
    let (tx, rx) = mpsc::sync_channel(1);
    let request = InvokeRequest {
        cmd: cmd.to_string(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url,
        body: InvokeBody::Json(args),
        headers: Default::default(),
        invoke_key: app.invoke_key().to_string(),
    };
    webview.on_message(
        request,
        Box::new(move |_webview, _cmd, response, _callback, _error| {
            let _ = tx.send(response);
        }),
    );
    match rx.recv_timeout(REPLY_WAIT) {
        Ok(InvokeResponse::Ok(InvokeResponseBody::Json(json))) => format!(r#"{{"ok":{json}}}"#),
        Ok(InvokeResponse::Ok(InvokeResponseBody::Raw(bytes))) => {
            format!(r#"{{"ok":{}}}"#, Value::from(bytes))
        }
        Ok(InvokeResponse::Err(InvokeError(value))) => format!(r#"{{"err":{value}}}"#),
        Err(_) => err_json(&format!("{cmd}: no reply")),
    }
}

fn err_json(message: &str) -> String {
    format!(r#"{{"err":{}}}"#, Value::from(message))
}

fn reply(out: &mut TcpStream, status: u16, body: &str) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        413 => "Payload Too Large",
        _ => "Error",
    };
    write!(
        out,
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: POST, OPTIONS\r\n\
         Access-Control-Allow-Headers: content-type, x-kansha-token\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    )?;
    out.flush()
}
