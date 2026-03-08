use std::collections::{HashMap, VecDeque};
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::StatusCode;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

#[derive(Debug, Clone)]
pub struct CapturedRequest {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

#[derive(Clone)]
pub struct ScriptedResponse {
    pub status: StatusCode,
    pub headers: Vec<(String, String)>,
    pub delay_before_headers: Option<Duration>,
    pub body: ScriptedBody,
}

#[derive(Debug, Clone)]
pub enum ScriptedBody {
    Fixed(String),
    Chunks(Vec<String>),
    ChunksThenDisconnect(Vec<String>),
    TimedChunks(Vec<(Duration, String)>),
    TimedChunksThenDisconnect(Vec<(Duration, String)>),
    RawChunks(Vec<Vec<u8>>),
    RawChunksThenDisconnect(Vec<Vec<u8>>),
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn invalid_data_error(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn parse_request_head(head: &str) -> io::Result<(String, String, HashMap<String, String>)> {
    let mut lines = head.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| invalid_data_error("missing request line"))?;

    let mut request_parts = request_line.split_whitespace();
    let method = request_parts
        .next()
        .ok_or_else(|| invalid_data_error("missing request method"))?
        .to_string();
    let path = request_parts
        .next()
        .ok_or_else(|| invalid_data_error("missing request path"))?
        .to_string();
    let _http_version = request_parts
        .next()
        .ok_or_else(|| invalid_data_error("missing request HTTP version"))?;

    let mut headers = HashMap::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }

        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| invalid_data_error(format!("invalid header line: {line}")))?;
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
    }

    Ok((method, path, headers))
}

async fn read_request(stream: &mut TcpStream) -> io::Result<CapturedRequest> {
    let mut buffer = Vec::new();
    let headers_end = loop {
        let mut chunk = [0_u8; 1024];
        let bytes_read = stream.read(&mut chunk).await?;
        if bytes_read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "connection closed before full request",
            ));
        }

        buffer.extend_from_slice(&chunk[..bytes_read]);
        if let Some(index) = find_subsequence(&buffer, b"\r\n\r\n") {
            break index;
        }
    };

    let head = std::str::from_utf8(&buffer[..headers_end])
        .map_err(|error| invalid_data_error(format!("invalid utf8 in header block: {error}")))?;
    let (method, path, headers) = parse_request_head(head)?;

    let content_length = headers
        .get("content-length")
        .map(String::as_str)
        .unwrap_or("0")
        .parse::<usize>()
        .map_err(|error| invalid_data_error(format!("invalid content-length: {error}")))?;

    let body_start = headers_end + 4;
    let mut body = if buffer.len() > body_start {
        buffer[body_start..].to_vec()
    } else {
        Vec::new()
    };

    while body.len() < content_length {
        let mut chunk = [0_u8; 1024];
        let bytes_read = stream.read(&mut chunk).await?;
        if bytes_read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "connection closed before reading full body",
            ));
        }
        let remaining = content_length - body.len();
        let take = bytes_read.min(remaining);
        body.extend_from_slice(&chunk[..take]);
    }

    Ok(CapturedRequest {
        method,
        path,
        headers,
        body,
    })
}

async fn write_response(stream: &mut TcpStream, response: &ScriptedResponse) -> io::Result<()> {
    if let Some(delay) = response.delay_before_headers {
        tokio::time::sleep(delay).await;
    }

    let reason = response.status.canonical_reason().unwrap_or("Unknown");
    let mut response_text = format!("HTTP/1.1 {} {}\r\n", response.status.as_u16(), reason);
    let is_streaming = matches!(
        response.body,
        ScriptedBody::Chunks(_)
            | ScriptedBody::ChunksThenDisconnect(_)
            | ScriptedBody::TimedChunks(_)
            | ScriptedBody::TimedChunksThenDisconnect(_)
            | ScriptedBody::RawChunks(_)
            | ScriptedBody::RawChunksThenDisconnect(_)
    );

    let mut has_content_type = false;
    for (name, value) in &response.headers {
        if name.eq_ignore_ascii_case("content-type") {
            has_content_type = true;
        }
        response_text.push_str(name);
        response_text.push_str(": ");
        response_text.push_str(value);
        response_text.push_str("\r\n");
    }

    if !has_content_type {
        let default_content_type = if is_streaming {
            "text/event-stream"
        } else {
            "application/json"
        };
        response_text.push_str("Content-Type: ");
        response_text.push_str(default_content_type);
        response_text.push_str("\r\n");
    }

    match &response.body {
        ScriptedBody::Fixed(body) => {
            response_text.push_str("Connection: close\r\n");
            response_text.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
            response_text.push_str(body);
            stream.write_all(response_text.as_bytes()).await?;
            stream.shutdown().await
        }
        ScriptedBody::Chunks(chunks) | ScriptedBody::ChunksThenDisconnect(chunks) => {
            response_text.push_str("Transfer-Encoding: chunked\r\n");
            response_text.push_str("Connection: close\r\n\r\n");
            stream.write_all(response_text.as_bytes()).await?;

            for chunk in chunks {
                let header = format!("{:X}\r\n", chunk.len());
                stream.write_all(header.as_bytes()).await?;
                stream.write_all(chunk.as_bytes()).await?;
                stream.write_all(b"\r\n").await?;
            }

            if matches!(response.body, ScriptedBody::Chunks(_)) {
                stream.write_all(b"0\r\n\r\n").await?;
            }

            stream.shutdown().await
        }
        ScriptedBody::TimedChunks(chunks) | ScriptedBody::TimedChunksThenDisconnect(chunks) => {
            response_text.push_str("Transfer-Encoding: chunked\r\n");
            response_text.push_str("Connection: close\r\n\r\n");
            stream.write_all(response_text.as_bytes()).await?;

            for (delay, chunk) in chunks {
                tokio::time::sleep(*delay).await;
                let header = format!("{:X}\r\n", chunk.len());
                stream.write_all(header.as_bytes()).await?;
                stream.write_all(chunk.as_bytes()).await?;
                stream.write_all(b"\r\n").await?;
            }

            if matches!(response.body, ScriptedBody::TimedChunks(_)) {
                stream.write_all(b"0\r\n\r\n").await?;
            }

            stream.shutdown().await
        }
        ScriptedBody::RawChunks(chunks) | ScriptedBody::RawChunksThenDisconnect(chunks) => {
            response_text.push_str("Transfer-Encoding: chunked\r\n");
            response_text.push_str("Connection: close\r\n\r\n");
            stream.write_all(response_text.as_bytes()).await?;

            for chunk in chunks {
                let header = format!("{:X}\r\n", chunk.len());
                stream.write_all(header.as_bytes()).await?;
                stream.write_all(chunk).await?;
                stream.write_all(b"\r\n").await?;
            }

            if matches!(response.body, ScriptedBody::RawChunks(_)) {
                stream.write_all(b"0\r\n\r\n").await?;
            }

            stream.shutdown().await
        }
    }
}

pub async fn spawn_scripted_server(
    responses: Vec<ScriptedResponse>,
) -> io::Result<(
    String,
    Arc<Mutex<Vec<CapturedRequest>>>,
    JoinHandle<io::Result<()>>,
)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let recorded_clone = Arc::clone(&recorded);

    let mut queue: VecDeque<ScriptedResponse> = responses.into();
    let handle = tokio::spawn(async move {
        while let Some(response) = queue.pop_front() {
            let (mut stream, _) = listener.accept().await?;
            let request = read_request(&mut stream).await?;

            {
                let mut guard = recorded_clone
                    .lock()
                    .map_err(|_| io::Error::other("failed to acquire request capture lock"))?;
                guard.push(request);
            }

            write_response(&mut stream, &response).await?;
        }

        Ok(())
    });

    Ok((format!("http://{address}"), recorded, handle))
}

pub fn captured_requests(
    recorded: &Arc<Mutex<Vec<CapturedRequest>>>,
) -> io::Result<Vec<CapturedRequest>> {
    let guard = recorded
        .lock()
        .map_err(|_| io::Error::other("failed to read captured requests"))?;
    Ok(guard.clone())
}

pub async fn await_server(handle: JoinHandle<io::Result<()>>) -> io::Result<()> {
    handle
        .await
        .map_err(|error| io::Error::other(format!("server join failed: {error}")))?
}
