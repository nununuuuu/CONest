use std::{
    io::{self, Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde_json::{json, Value};

const PORT_START: u16 = 17345;
const PORT_END: u16 = 17349;
const MAX_MESSAGE: usize = 1024 * 1024;

fn read_native_message() -> io::Result<Option<Value>> {
    let mut len = [0u8; 4];
    match io::stdin().read_exact(&mut len) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }

    let len = u32::from_le_bytes(len) as usize;
    if len == 0 || len > MAX_MESSAGE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid native message length: {len}"),
        ));
    }

    let mut body = vec![0u8; len];
    io::stdin().read_exact(&mut body)?;
    let value = serde_json::from_slice(&body)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some(value))
}

fn write_native_message(value: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(value)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let len = u32::try_from(body.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "response too large"))?;

    let mut out = io::stdout().lock();
    out.write_all(&len.to_le_bytes())?;
    out.write_all(&body)?;
    out.flush()
}

fn parse_http_response(bytes: &[u8]) -> Result<(u16, Value), String> {
    let marker = b"\r\n\r\n";
    let pos = bytes
        .windows(marker.len())
        .position(|w| w == marker)
        .ok_or_else(|| "invalid HTTP response".to_string())?;

    let head = String::from_utf8_lossy(&bytes[..pos]);
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|v| v.parse::<u16>().ok())
        .ok_or_else(|| "invalid HTTP status".to_string())?;

    let body = &bytes[pos + marker.len()..];
    let data = if body.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(body).unwrap_or_else(|_| {
            json!({ "error": String::from_utf8_lossy(body).to_string() })
        })
    };
    Ok((status, data))
}

fn http_request(port: u16, method: &str, path: &str, body: Option<&Value>) -> Result<(u16, Value), String> {
    let addr = format!("127.0.0.1:{port}");
    let mut stream = TcpStream::connect_timeout(
        &addr.parse().map_err(|e| format!("bad address: {e}"))?,
        Duration::from_millis(500),
    )
    .map_err(|e| e.to_string())?;

    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| e.to_string())?;

    let body_bytes = body
        .map(serde_json::to_vec)
        .transpose()
        .map_err(|e| e.to_string())?
        .unwrap_or_default();

    let request = format!(
        "{method} {path} HTTP/1.1\r\n\
Host: 127.0.0.1:{port}\r\n\
X-CONest-Extension: 1\r\n\
Content-Type: application/json\r\n\
Content-Length: {}\r\n\
Connection: close\r\n\r\n",
        body_bytes.len()
    );

    stream
        .write_all(request.as_bytes())
        .and_then(|_| stream.write_all(&body_bytes))
        .map_err(|e| e.to_string())?;

    let mut response = Vec::new();
    if let Err(error) = stream.read_to_end(&mut response) {
        // On Windows a local peer can close with WSAECONNRESET (10054) after
        // the complete response was already delivered. If bytes were received,
        // parse them before treating the close as a transport failure.
        if response.is_empty()
            || !matches!(
                error.kind(),
                io::ErrorKind::ConnectionReset
                    | io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::UnexpectedEof
            )
        {
            return Err(error.to_string());
        }
    }
    parse_http_response(&response)
}

fn find_bridge() -> Option<u16> {
    for port in PORT_START..=PORT_END {
        if let Ok((status, data)) = http_request(port, "GET", "/health?client=conest-extension", None) {
            if status == 200 && data.get("ok").and_then(Value::as_bool) == Some(true) {
                return Some(port);
            }
        }
    }
    None
}

fn bridge_ready(port: u16) -> bool {
    matches!(
        http_request(port, "GET", "/health?client=conest-extension", None),
        Ok((200, data))
            if data.get("ok").and_then(Value::as_bool) == Some(true)
                && data.get("ready").and_then(Value::as_bool) == Some(true)
    )
}

fn sibling_conest() -> Result<PathBuf, String> {
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = me
        .parent()
        .ok_or_else(|| "native host has no parent directory".to_string())?;

    for name in ["CONest.exe", "conest.exe"] {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    Err(format!("CONest.exe not found beside {}", me.display()))
}

fn launch_conest(exe: &Path) -> Result<(), String> {
    Command::new(exe)
        .arg("--background")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("failed to launch {}: {e}", exe.display()))
}

#[cfg(target_os = "windows")]
fn conest_is_running() -> bool {
    Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq CONest.exe", "/NH"])
        .output()
        .ok()
        .map(|out| {
            String::from_utf8_lossy(&out.stdout)
                .to_ascii_lowercase()
                .contains("conest.exe")
        })
        .unwrap_or(false)
}

#[cfg(not(target_os = "windows"))]
fn conest_is_running() -> bool {
    false
}

fn wait_for_bridge(duration: Duration) -> Option<u16> {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        if let Some(port) = find_bridge() {
            return Some(port);
        }
        thread::sleep(Duration::from_millis(250));
    }
    None
}

fn ensure_bridge() -> Result<u16, String> {
    if let Some(port) = find_bridge() {
        return Ok(port);
    }

    if conest_is_running() {
        return wait_for_bridge(Duration::from_secs(15))
            .ok_or_else(|| "CONest is running but its bridge is not ready".into());
    }

    let exe = sibling_conest()?;
    launch_conest(&exe)?;

    wait_for_bridge(Duration::from_secs(15))
        .ok_or_else(|| "CONest was launched but its bridge did not become ready".into())
}

fn ensure_ready_bridge() -> Result<u16, String> {
    let port = ensure_bridge()?;
    if bridge_ready(port) {
        return Ok(port);
    }

    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if bridge_ready(port) {
            return Ok(port);
        }
        thread::sleep(Duration::from_millis(250));
    }

    Err("CONest started but its frontend did not become ready in time".into())
}

fn url_encode(value: &str) -> String {
    let mut out = String::new();
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn process_message(message: &Value) -> Value {
    let request_id = message.get("requestId").cloned().unwrap_or(Value::Null);
    let action = message
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or_default();

    // Health/task polling must only report current state. They must never launch
    // CONest by themselves, otherwise merely opening the extension starts the app.
    if action == "health" || action == "tasks" {
        let Some(port) = find_bridge() else {
            return json!({
                "requestId": request_id,
                "ok": false,
                "status": 0,
                "offline": true,
                "transport": "native"
            });
        };

        let result = if action == "health" {
            http_request(port, "GET", "/health?client=conest-extension", None)
        } else {
            http_request(port, "GET", "/api/tasks?client=conest-extension", None)
        };

        return match result {
            Ok((status, data)) => json!({
                "requestId": request_id,
                "ok": (200..300).contains(&status),
                "status": status,
                "data": data,
                "transport": "native"
            }),
            Err(error) => json!({
                "requestId": request_id,
                "ok": false,
                "status": 0,
                "error": error,
                "transport": "native"
            }),
        };
    }

    let port = match ensure_ready_bridge() {
        Ok(port) => port,
        Err(error) => {
            return json!({
                "requestId": request_id,
                "ok": false,
                "status": 0,
                "error": error,
                "transport": "native"
            })
        }
    };

    let result = match action {
        "download" => {
            let payload = message.get("payload").cloned().unwrap_or_else(|| json!({}));
            http_request(
                port,
                "POST",
                "/api/download?client=conest-extension",
                Some(&payload),
            )
        }
        "start-task" => {
            let id = message
                .get("payload")
                .and_then(|v| v.get("id"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            if id.is_empty() {
                Err("missing task id".into())
            } else {
                let path = format!(
                    "/api/start?id={}&client=conest-extension",
                    url_encode(id)
                );
                http_request(port, "POST", &path, Some(&json!({})))
            }
        }
        "clear-completed" => http_request(
            port,
            "POST",
            "/api/clear-completed?client=conest-extension",
            Some(&json!({})),
        ),
        "remove-task" => {
            let id = message
                .get("payload")
                .and_then(|v| v.get("id"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            if id.is_empty() {
                Err("missing task id".into())
            } else {
                let path = format!(
                    "/api/remove?id={}&client=conest-extension",
                    url_encode(id)
                );
                http_request(port, "POST", &path, Some(&json!({})))
            }
        }
        "configure-task" => {
            let payload = message.get("payload").cloned().unwrap_or_else(|| json!({}));
            http_request(
                port,
                "POST",
                "/api/configure?client=conest-extension",
                Some(&payload),
            )
        }
        "task-settings" => {
            let payload = message.get("payload").cloned().unwrap_or_else(|| json!({}));
            http_request(
                port,
                "POST",
                "/api/task-settings?client=conest-extension",
                Some(&payload),
            )
        }
        _ => Err(format!("unsupported action: {action}")),
    };

    match result {
        Ok((status, data)) => json!({
            "requestId": request_id,
            "ok": (200..300).contains(&status),
            "status": status,
            "data": data,
            "transport": "native"
        }),
        Err(error) => json!({
            "requestId": request_id,
            "ok": false,
            "status": 0,
            "error": error,
            "transport": "native"
        }),
    }
}

fn main() {
    loop {
        let message = match read_native_message() {
            Ok(Some(message)) => message,
            Ok(None) => break,
            Err(_) => break,
        };
        let response = process_message(&message);
        if write_native_message(&response).is_err() {
            break;
        }
    }
}
