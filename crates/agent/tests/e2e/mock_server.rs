use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

#[derive(Debug, Clone)]
pub struct MockResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Value,
    pub delay: Duration,
}

impl MockResponse {
    pub fn json(status: u16, body: Value) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body,
            delay: Duration::ZERO,
        }
    }

    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }
}

#[derive(Debug, Clone)]
pub struct CapturedRequest {
    pub method: String,
    pub path: String,
    pub headers: BTreeMap<String, String>,
    pub body_json: Value,
}

#[derive(Debug)]
pub struct MockServer {
    base_url: String,
    captured: Arc<Mutex<Vec<CapturedRequest>>>,
    task: JoinHandle<()>,
}

impl MockServer {
    pub async fn spawn(responses: Vec<MockResponse>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock server listener");
        let addr = listener.local_addr().expect("mock server local addr");

        let queue = Arc::new(Mutex::new(VecDeque::from(responses)));
        let captured = Arc::new(Mutex::new(Vec::new()));
        let captured_for_task = Arc::clone(&captured);

        let task = tokio::spawn(async move {
            loop {
                let (mut stream, _) = match listener.accept().await {
                    Ok(pair) => pair,
                    Err(_) => break,
                };

                let queue = Arc::clone(&queue);
                let captured = Arc::clone(&captured_for_task);

                tokio::spawn(async move {
                    let maybe_request = read_http_request(&mut stream).await;

                    if let Some(request) = maybe_request {
                        captured.lock().await.push(request);
                    }

                    let response = {
                        let mut guard = queue.lock().await;
                        guard.pop_front().unwrap_or_else(|| {
                            MockResponse::json(
                                500,
                                json!({ "error": { "message": "mock response queue empty" } }),
                            )
                        })
                    };

                    if response.delay > Duration::ZERO {
                        tokio::time::sleep(response.delay).await;
                    }

                    let _ = write_http_response(&mut stream, response).await;
                });
            }
        });

        Self {
            base_url: format!("http://{addr}"),
            captured,
            task,
        }
    }

    pub fn base_url(&self) -> String {
        self.base_url.clone()
    }

    pub async fn captured_requests(&self) -> Vec<CapturedRequest> {
        self.captured.lock().await.clone()
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub fn unused_local_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral local port");
    let addr = listener.local_addr().expect("ephemeral local addr");
    drop(listener);
    format!("http://{addr}")
}

async fn read_http_request(stream: &mut tokio::net::TcpStream) -> Option<CapturedRequest> {
    let mut buffer = Vec::new();
    let mut scratch = [0_u8; 4096];

    let mut header_end = None;
    let mut content_length = 0_usize;

    loop {
        let bytes_read = stream.read(&mut scratch).await.ok()?;
        if bytes_read == 0 {
            break;
        }

        buffer.extend_from_slice(&scratch[..bytes_read]);

        if header_end.is_none() {
            header_end = find_header_end(&buffer);
            if let Some(end) = header_end {
                content_length = parse_content_length(&buffer[..end]);
            }
        }

        if let Some(end) = header_end {
            let current_body_len = buffer.len().saturating_sub(end);
            if current_body_len >= content_length {
                break;
            }
        }
    }

    let header_end = header_end?;
    let header_text = std::str::from_utf8(&buffer[..header_end]).ok()?;
    let mut header_lines = header_text.split("\r\n");
    let request_line = header_lines.next()?;

    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next()?.to_string();
    let path = request_parts.next()?.to_string();

    let mut headers = BTreeMap::new();
    for line in header_lines {
        if line.is_empty() {
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    let body_bytes = &buffer[header_end..];
    let body_json = if body_bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(body_bytes).unwrap_or(Value::Null)
    };

    Some(CapturedRequest {
        method,
        path,
        headers,
        body_json,
    })
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
}

fn parse_content_length(header_bytes: &[u8]) -> usize {
    let header_text = match std::str::from_utf8(header_bytes) {
        Ok(text) => text,
        Err(_) => return 0,
    };

    for line in header_text.split("\r\n") {
        if let Some((name, value)) = line.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
            && let Ok(length) = value.trim().parse::<usize>()
        {
            return length;
        }
    }

    0
}

async fn write_http_response(
    stream: &mut tokio::net::TcpStream,
    response: MockResponse,
) -> std::io::Result<()> {
    let body_text = response.body.to_string();

    let reason = if (200..300).contains(&response.status) {
        "OK"
    } else {
        "ERROR"
    };

    let mut response_bytes = format!(
        "HTTP/1.1 {} {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n",
        response.status,
        reason,
        body_text.len()
    )
    .into_bytes();

    for (name, value) in response.headers {
        response_bytes.extend_from_slice(name.as_bytes());
        response_bytes.extend_from_slice(b": ");
        response_bytes.extend_from_slice(value.as_bytes());
        response_bytes.extend_from_slice(b"\r\n");
    }

    response_bytes.extend_from_slice(b"\r\n");
    response_bytes.extend_from_slice(body_text.as_bytes());

    stream.write_all(&response_bytes).await?;
    stream.shutdown().await
}
