//! Minimal HTTP server that exposes POST /propose → cli_bridge::propose().
//!
//! This is a local-only development server (localhost:5198). It uses only
//! std::net — zero external HTTP dependencies.
//!
//! The server is compiled as a separate binary (see `core/src/bin/server.rs`)
//! and is NOT linked into the WASM core. It respects INV-4 by living in a
//! separate binary target, not the core library's default compilation path.
//!
//! # Protocol
//!
//! POST /propose  Content-Type: application/json
//! Body: {"intent": "描述关卡布局的自然语言"}
//! Response: {"ok": true, "commands": [...]} or {"ok": false, "error": "..."}
//!
//! OPTIONS /propose returns CORS preflight headers so browsers can call it.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

use crate::cli_bridge;

const PORT: u16 = 5198;
const BIND_ADDR: &str = "127.0.0.1";

const CORS_HEADERS: &str = "\
Access-Control-Allow-Origin: *\r\n\
Access-Control-Allow-Methods: POST, OPTIONS\r\n\
Access-Control-Allow-Headers: Content-Type\r\n";

// ── Public API ───────────────────────────────────────────────────────

/// Start the blocking HTTP server on localhost:5198.
///
/// Handles one request at a time. Press Ctrl+C to stop.
/// Panics if the port is already in use.
pub fn run_server() {
    let listener = TcpListener::bind((BIND_ADDR, PORT))
        .unwrap_or_else(|e| panic!("Failed to bind {BIND_ADDR}:{PORT}: {e}"));
    eprintln!("[workbench-server] Listening on http://{BIND_ADDR}:{PORT}");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => handle_connection(stream),
            Err(e) => eprintln!("[workbench-server] Connection error: {e}"),
        }
    }
}

// ── Connection handling ──────────────────────────────────────────────

fn handle_connection(mut stream: TcpStream) {
    let peer = stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_default();
    let mut reader = BufReader::new(stream.try_clone().unwrap_or_else(|_| {
        // If clone fails, we can still read but not write.
        // This shouldn't happen on localhost.
        panic!("Failed to clone stream for {peer}")
    }));

    // Read request line
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        eprintln!("[{peer}] Failed to read request line");
        return;
    }
    let parts: Vec<&str> = request_line.trim().split_whitespace().collect();
    if parts.len() < 2 {
        write_400(&mut stream, "Bad request line");
        return;
    }
    let method = parts[0];
    let path = parts[1];

    // Read headers
    let mut content_length: usize = 0;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).is_err() {
            break;
        }
        let header = header.trim();
        if header.is_empty() {
            break;
        }
        if let Some(value) = header
            .to_lowercase()
            .strip_prefix("content-length:")
            .map(|v| v.trim().to_string())
        {
            content_length = value.parse().unwrap_or(0);
        }
    }

    match (method, path) {
        ("OPTIONS", "/propose") => {
            write_cors_preflight(&mut stream);
        }
        ("POST", "/propose") => {
            handle_propose(&mut reader, &mut stream, content_length, &peer);
        }
        _ => {
            write_404(&mut stream);
        }
    }
}

/// Handle POST /propose: parse intent, call cli_bridge, return JSON.
fn handle_propose(
    reader: &mut BufReader<TcpStream>,
    writer: &mut TcpStream,
    content_length: usize,
    peer: &str,
) {
    // Read body
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        if reader.read_exact(&mut body).is_err() {
            write_400(writer, "Failed to read body");
            return;
        }
    }

    // Parse JSON: {"intent": "..."}
    let body_str = String::from_utf8_lossy(&body);
    let intent = match parse_intent(&body_str) {
        Ok(i) => i,
        Err(e) => {
            let msg = format!("Invalid request body: {e}");
            eprintln!("[{peer}] {msg}");
            write_json(writer, 400, &serde_json::json!({"ok": false, "error": msg}));
            return;
        }
    };

    eprintln!("[{peer}] propose: {intent}");

    // Call the real CLI bridge (which shells out to opencode)
    match cli_bridge::propose(&intent) {
        Ok(result) => {
            let commands_json: Vec<serde_json::Value> = result
                .commands
                .iter()
                .map(|cmd| serde_json::to_value(cmd).unwrap_or(serde_json::Value::Null))
                .collect();
            let response = serde_json::json!({
                "ok": true,
                "commands": commands_json,
            });
            eprintln!(
                "[{peer}] → {} commands ({} bytes raw)",
                result.commands.len(),
                result.raw_output.len()
            );
            write_json(writer, 200, &response);
        }
        Err(e) => {
            let msg = format!("{e}");
            eprintln!("[{peer}] propose error: {msg}");
            write_json(writer, 500, &serde_json::json!({"ok": false, "error": msg}));
        }
    }
}

/// Parse `{"intent": "..."}` from a JSON body.
fn parse_intent(body: &str) -> std::result::Result<String, String> {
    let val: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("JSON parse error: {e}"))?;
    let intent = val
        .get("intent")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing 'intent' field (string)".to_string())?;
    if intent.trim().is_empty() {
        return Err("'intent' must not be empty".to_string());
    }
    Ok(intent.to_string())
}

// ── Response writers ─────────────────────────────────────────────────

fn write_json(writer: &mut TcpStream, status: u16, body: &serde_json::Value) {
    let body_str = body.to_string();
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {len}\r\n\
         {cors}\
         Connection: close\r\n\
         \r\n\
         {body}",
        status = status,
        reason = status_reason(status),
        len = body_str.len(),
        cors = CORS_HEADERS,
        body = body_str,
    );
    let _ = writer.write_all(response.as_bytes());
    let _ = writer.flush();
}

fn write_cors_preflight(writer: &mut TcpStream) {
    let response = format!(
        "HTTP/1.1 204 No Content\r\n\
         {cors}\
         Content-Length: 0\r\n\
         Connection: close\r\n\
         \r\n",
        cors = CORS_HEADERS,
    );
    let _ = writer.write_all(response.as_bytes());
    let _ = writer.flush();
}

fn write_400(writer: &mut TcpStream, msg: &str) {
    let response = format!(
        "HTTP/1.1 400 Bad Request\r\n\
         Content-Type: text/plain\r\n\
         Content-Length: {len}\r\n\
         {cors}\
         Connection: close\r\n\
         \r\n\
         {msg}",
        len = msg.len(),
        cors = CORS_HEADERS,
        msg = msg,
    );
    let _ = writer.write_all(response.as_bytes());
    let _ = writer.flush();
}

fn write_404(writer: &mut TcpStream) {
    let body = "404 Not Found";
    let response = format!(
        "HTTP/1.1 404 Not Found\r\n\
         Content-Type: text/plain\r\n\
         Content-Length: {len}\r\n\
         {cors}\
         Connection: close\r\n\
         \r\n\
         {body}",
        len = body.len(),
        cors = CORS_HEADERS,
        body = body,
    );
    let _ = writer.write_all(response.as_bytes());
    let _ = writer.flush();
}

fn status_reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Unknown",
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_intent_valid() {
        let body = r#"{"intent": "生成中央大厅+三支线"}"#;
        let intent = parse_intent(body).unwrap();
        assert_eq!(intent, "生成中央大厅+三支线");
    }

    #[test]
    fn test_parse_intent_missing_field() {
        let body = r#"{"other": "value"}"#;
        assert!(parse_intent(body).is_err());
    }

    #[test]
    fn test_parse_intent_empty_string() {
        let body = r#"{"intent": ""}"#;
        assert!(parse_intent(body).is_err());
    }

    #[test]
    fn test_parse_intent_invalid_json() {
        let body = "not json";
        assert!(parse_intent(body).is_err());
    }
}
