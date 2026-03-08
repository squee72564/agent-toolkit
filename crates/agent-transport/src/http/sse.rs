use bytes::BytesMut;

use crate::http::transport::TransportError;

#[derive(Debug, Clone)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
    pub id: Option<String>,
    pub retry: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct SseLimits {
    pub max_line_bytes: usize,
    pub max_event_bytes: usize,
    pub max_buffer_bytes: usize,
}

impl Default for SseLimits {
    fn default() -> Self {
        Self {
            max_line_bytes: 64 * 1024,
            max_event_bytes: 1024 * 1024,
            max_buffer_bytes: 1024 * 1024,
        }
    }
}

#[derive(Debug)]
pub struct HttpSseResponse {
    pub head: crate::http::HttpResponseHead,
    pub stream: HttpSseStream,
}

#[derive(Debug)]
pub struct HttpSseStream {
    pub(crate) head: crate::http::HttpResponseHead,
    pub(crate) response: reqwest::Response,
    pub(crate) buffer: BytesMut,
    pub(crate) buffer_offset: usize,
    pub(crate) state: PendingSseEvent,
    pub(crate) limits: SseLimits,
}

#[derive(Debug, Default)]
pub(crate) struct PendingSseEvent {
    event: Option<String>,
    data: Vec<String>,
    id: Option<String>,
    retry: Option<u64>,
    data_bytes: usize,
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
        self.data_bytes = 0;
        Some(event)
    }
}

impl HttpSseStream {
    pub async fn next_event(&mut self) -> Result<Option<SseEvent>, TransportError> {
        loop {
            if let Some(event) = self.try_parse_event()? {
                return Ok(Some(event));
            }

            match self.response.chunk().await {
                Ok(Some(chunk)) => {
                    self.buffer.extend_from_slice(&chunk);
                    self.enforce_buffer_limit()?;
                }
                Ok(None) => {
                    if self.remaining_buffer().is_empty() {
                        return Ok(self.state.finish());
                    }

                    if !self.remaining_buffer().ends_with(b"\n") {
                        return Err(TransportError::StreamTerminated {
                            message: "stream ended with a partial SSE frame".to_string(),
                            head: Box::new(self.head.clone()),
                        });
                    }

                    if let Some(event) = self.try_parse_event()? {
                        return Ok(Some(event));
                    }

                    return Ok(self.state.finish());
                }
                Err(error) => {
                    return Err(TransportError::StreamTerminated {
                        message: error.to_string(),
                        head: Box::new(self.head.clone()),
                    });
                }
            }
        }
    }

    fn try_parse_event(&mut self) -> Result<Option<SseEvent>, TransportError> {
        while let Some(line_end) = find_line_end(self.remaining_buffer()) {
            let line_end = self.buffer_offset + line_end;
            let mut line = self.buffer[self.buffer_offset..line_end].to_vec();
            self.buffer_offset = line_end + 1;

            if line.len() > self.limits.max_line_bytes {
                return Err(TransportError::SseLimit {
                    kind: "SSE line",
                    size: line.len(),
                    max: self.limits.max_line_bytes,
                });
            }

            if line.last() == Some(&b'\r') {
                line.pop();
            }

            let line = std::str::from_utf8(&line).map_err(|error| {
                TransportError::SseParse(format!("invalid UTF-8 in SSE line: {error}"))
            })?;

            if line.is_empty() {
                self.compact_buffer();
                if let Some(event) = self.state.finish() {
                    return Ok(Some(event));
                }
                continue;
            }

            if line.starts_with(':') {
                self.compact_buffer();
                continue;
            }

            let (field, value) = match line.split_once(':') {
                Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
                None => (line, ""),
            };

            match field {
                "event" => self.state.event = Some(value.to_string()),
                "data" => {
                    let separator = usize::from(!self.state.data.is_empty());
                    let next_size = self.state.data_bytes + separator + value.len();
                    if next_size > self.limits.max_event_bytes {
                        return Err(TransportError::SseLimit {
                            kind: "SSE event",
                            size: next_size,
                            max: self.limits.max_event_bytes,
                        });
                    }

                    self.state.data.push(value.to_string());
                    self.state.data_bytes = next_size;
                }
                "id" => self.state.id = Some(value.to_string()),
                "retry" => {
                    let retry = value.parse::<u64>().map_err(|_| {
                        TransportError::SseParse(format!("invalid retry field value: {value}"))
                    })?;
                    self.state.retry = Some(retry);
                }
                _ => {}
            }

            self.compact_buffer();
        }

        if self.remaining_buffer().len() > self.limits.max_line_bytes {
            return Err(TransportError::SseLimit {
                kind: "SSE line",
                size: self.remaining_buffer().len(),
                max: self.limits.max_line_bytes,
            });
        }

        Ok(None)
    }

    fn enforce_buffer_limit(&self) -> Result<(), TransportError> {
        let buffered = self.remaining_buffer().len();
        if buffered > self.limits.max_buffer_bytes {
            return Err(TransportError::SseLimit {
                kind: "SSE buffer",
                size: buffered,
                max: self.limits.max_buffer_bytes,
            });
        }

        Ok(())
    }

    fn compact_buffer(&mut self) {
        if self.buffer_offset == 0 {
            return;
        }

        if self.buffer_offset >= self.buffer.len() {
            self.buffer.clear();
            self.buffer_offset = 0;
            return;
        }

        if self.buffer_offset >= 4096 || self.buffer_offset * 2 >= self.buffer.len() {
            let _ = self.buffer.split_to(self.buffer_offset);
            self.buffer_offset = 0;
        }
    }

    fn remaining_buffer(&self) -> &[u8] {
        &self.buffer[self.buffer_offset..]
    }
}

fn find_line_end(buffer: &[u8]) -> Option<usize> {
    buffer.iter().position(|byte| *byte == b'\n')
}
