//! Integration test for the native `--mcp-bridge` subcommand.
//!
//! Spawns the real built binary as `fenceymd --mcp-bridge --endpoint <url>`,
//! pointed at an in-process mock HTTP server, and drives it over stdio exactly
//! as a real agent would. Guards the contract ported from `mcp-bridge.mjs`:
//!   1. round-trip + request/response ORDER preserved under a burst
//!   2. stdout carries ONLY JSON-RPC frames (no log noise) — agents break
//!      silently otherwise
//!   3. a connection error surfaces as a -32000 JSON-RPC error frame, not a hang
//!   4. graceful exit (status 0) on stdin EOF

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

const BIN: &str = env!("CARGO_BIN_EXE_fenceymd");

/// Read one HTTP request off the stream and return its body bytes.
fn read_request_body(stream: &mut std::net::TcpStream) -> Vec<u8> {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut buf = Vec::new();
    let mut tmp = [0u8; 1024];
    // Read until headers complete, then until we've consumed Content-Length.
    loop {
        let n = stream.read(&mut tmp).unwrap_or(0);
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&buf[..pos]).to_lowercase();
            let len: usize = headers
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(0);
            if buf.len() >= pos + 4 + len {
                return buf[pos + 4..pos + 4 + len].to_vec();
            }
        }
    }
    Vec::new()
}

/// A mock MCP HTTP server that echoes each request's `id` back in a 1-element
/// JSON array (the array shape the real server uses). Handles `count`
/// connections then stops. Returns the bound port.
fn spawn_echo_server(count: usize) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        let mut handled = 0;
        for stream in listener.incoming() {
            let mut stream = match stream {
                Ok(s) => s,
                Err(_) => break,
            };
            let body = read_request_body(&mut stream);
            let id = serde_json::from_slice::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v.get("id").cloned())
                .unwrap_or(serde_json::Value::Null);
            let payload = format!(
                "[{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{{\"echo\":{id}}}}}]"
            );
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                payload.len(),
                payload
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
            handled += 1;
            if handled >= count {
                break;
            }
        }
    });
    port
}

#[test]
fn bridge_round_trips_and_preserves_order() {
    let port = spawn_echo_server(3);
    let endpoint = format!("http://127.0.0.1:{port}/mcp");

    let mut child = Command::new(BIN)
        .args(["--mcp-bridge", "--endpoint", &endpoint])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null()) // logs go to stderr; we only assert on stdout
        .spawn()
        .expect("spawn bridge");

    {
        let mut stdin = child.stdin.take().unwrap();
        for id in 1..=3 {
            writeln!(stdin, "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"method\":\"ping\"}}").unwrap();
        }
        // drop stdin → EOF
    }

    let stdout = child.stdout.take().unwrap();
    let lines: Vec<String> = BufReader::new(stdout)
        .lines()
        .map(|l| l.unwrap())
        .filter(|l| !l.trim().is_empty())
        .collect();

    let status = child.wait().unwrap();
    assert!(status.success(), "bridge should exit 0 on stdin EOF");

    // stdout hygiene: exactly one frame per request, nothing else.
    assert_eq!(lines.len(), 3, "expected 3 frames, got: {lines:?}");
    for (i, line) in lines.iter().enumerate() {
        let v: serde_json::Value =
            serde_json::from_str(line).unwrap_or_else(|_| panic!("stdout line not JSON: {line}"));
        let want = (i + 1) as i64;
        assert_eq!(v["id"], want, "order not preserved at line {i}");
        assert_eq!(v["result"]["echo"], want);
        assert!(v.get("error").is_none());
    }
}

#[test]
fn bridge_emits_error_frame_on_connection_refused() {
    // Grab a port, then drop the listener so nothing is listening there.
    let dead_port = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let endpoint = format!("http://127.0.0.1:{dead_port}/mcp");

    let mut child = Command::new(BIN)
        .args(["--mcp-bridge", "--endpoint", &endpoint])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn bridge");

    {
        let mut stdin = child.stdin.take().unwrap();
        writeln!(stdin, "{{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}}").unwrap();
    }

    let mut out = String::new();
    child.stdout.take().unwrap().read_to_string(&mut out).unwrap();
    let status = child.wait().unwrap();
    assert!(status.success(), "bridge should still exit 0");

    let line = out.lines().find(|l| !l.trim().is_empty()).expect("a frame");
    let v: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(v["id"], 7, "error frame must carry the request id");
    assert_eq!(v["error"]["code"], -32000, "expected -32000 transport error");
}
