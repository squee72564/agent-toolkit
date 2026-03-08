use reqwest::{StatusCode, header::HeaderMap};

use crate::http::transport::TransportError;

#[derive(Debug, Clone)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
    pub id: Option<String>,
    pub retry: Option<u64>,
}

#[derive(Debug)]
pub struct HttpSseResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub request_id: Option<String>,
    pub stream: HttpSseStream,
}

#[derive(Debug)]
pub struct HttpSseStream {
    pub(crate) response: reqwest::Response,
    pub(crate) buffer: Vec<u8>,
    pub(crate) state: PendingSseEvent,
}

#[derive(Debug, Default)]
pub(crate) struct PendingSseEvent {
    event: Option<String>,
    data: Vec<String>,
    id: Option<String>,
    retry: Option<u64>,
}

impl PendingSseEvent {
    fn has_content(&self) -> bool {
        self.event.is_some() || !self.data.is_empty() || self.id.is_some() || self.retry.is_some()
    }

    fn finish(&mut self) -> Option<SseEvent> {
        if !self.has_content() {
            return None;
        }

        let event = SseEvent {
            event: self.event.take(),
            data: self.data.join("\n"),
            id: self.id.take(),
            retry: self.retry.take(),
        };
        self.data.clear();
        Some(event)
    }
}

impl HttpSseStream {
    pub async fn next_event(&mut self) -> Result<Option<SseEvent>, TransportError> {
        loop {
            if let Some(event) = self.try_parse_event()? {
                return Ok(Some(event));
            }

            match self.response.chunk().await? {
                Some(chunk) => self.buffer.extend_from_slice(&chunk),
                None => {
                    if self.buffer.is_empty() {
                        return Ok(self.state.finish());
                    }

                    if !self.buffer.ends_with(b"\n") {
                        return Err(TransportError::SseParse(
                            "stream ended with a partial SSE frame".to_string(),
                        ));
                    }

                    if let Some(event) = self.try_parse_event()? {
                        return Ok(Some(event));
                    }

                    return Ok(self.state.finish());
                }
            }
        }
    }

    fn try_parse_event(&mut self) -> Result<Option<SseEvent>, TransportError> {
        while let Some(line_end) = find_line_end(&self.buffer) {
            let mut line = self.buffer.drain(..line_end).collect::<Vec<u8>>();
            drain_newline_prefix(&mut self.buffer);

            if line.last() == Some(&b'\r') {
                line.pop();
            }

            let line = std::str::from_utf8(&line).map_err(|error| {
                TransportError::SseParse(format!("invalid UTF-8 in SSE line: {error}"))
            })?;

            if line.is_empty() {
                if let Some(event) = self.state.finish() {
                    return Ok(Some(event));
                }
                continue;
            }

            if line.starts_with(':') {
                continue;
            }

            let (field, value) = match line.split_once(':') {
                Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
                None => (line, ""),
            };

            match field {
                "event" => self.state.event = Some(value.to_string()),
                "data" => self.state.data.push(value.to_string()),
                "id" => self.state.id = Some(value.to_string()),
                "retry" => {
                    let retry = value.parse::<u64>().map_err(|_| {
                        TransportError::SseParse(format!("invalid retry field value: {value}"))
                    })?;
                    self.state.retry = Some(retry);
                }
                _ => {}
            }
        }

        Ok(None)
    }
}

fn find_line_end(buffer: &[u8]) -> Option<usize> {
    buffer.iter().position(|byte| *byte == b'\n')
}

fn drain_newline_prefix(buffer: &mut Vec<u8>) {
    if buffer.first() == Some(&b'\n') {
        buffer.remove(0);
    }
}
