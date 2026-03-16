use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::{
    AnthropicClient, MessageCreateInput, OpenAiClient, OpenRouterClient, RuntimeErrorKind,
};

const OPENAI_SUCCESS_BODY: &str = include_str!(
    "../../../../agent-providers/data/openai/responses/decoded/basic_chat/gpt-5-mini.json"
);

static CHILD_PROCESS_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[test]
fn openai_from_env_loads_dotenv_and_applies_trimmed_overrides() {
    if is_child_process() {
        child_openai_from_env_loads_dotenv_and_applies_trimmed_overrides();
        return;
    }

    let _guard = child_process_lock()
        .lock()
        .expect("child process mutex poisoned");
    let temp_dir = temp_test_dir("openai-from-env-success");

    run_in_child(
        "openai_from_env_loads_dotenv_and_applies_trimmed_overrides",
        &temp_dir,
        &[(
            ".env",
            "OPENAI_API_KEY=  test-openai-key  \nOPENAI_BASE_URL=  CHILD_SERVER_BASE_URL  \nOPENAI_MODEL=  gpt-5-mini  \n",
        )],
    );
}

#[test]
fn openai_from_env_rejects_missing_api_key() {
    if is_child_process() {
        let error = OpenAiClient::from_env().expect_err("missing OpenAI API key should fail");
        assert_eq!(error.kind, RuntimeErrorKind::Configuration);
        assert_eq!(error.message, "missing required env var OPENAI_API_KEY");
        return;
    }

    let _guard = child_process_lock()
        .lock()
        .expect("child process mutex poisoned");
    let temp_dir = temp_test_dir("openai-from-env-missing");
    run_in_child("openai_from_env_rejects_missing_api_key", &temp_dir, &[]);
}

#[test]
fn anthropic_from_env_rejects_missing_api_key() {
    if is_child_process() {
        let error = AnthropicClient::from_env().expect_err("missing Anthropic API key should fail");
        assert_eq!(error.kind, RuntimeErrorKind::Configuration);
        assert_eq!(error.message, "missing required env var ANTHROPIC_API_KEY");
        return;
    }

    let _guard = child_process_lock()
        .lock()
        .expect("child process mutex poisoned");
    let temp_dir = temp_test_dir("anthropic-from-env-missing");
    run_in_child("anthropic_from_env_rejects_missing_api_key", &temp_dir, &[]);
}

#[test]
fn openrouter_from_env_rejects_missing_api_key() {
    if is_child_process() {
        let error =
            OpenRouterClient::from_env().expect_err("missing OpenRouter API key should fail");
        assert_eq!(error.kind, RuntimeErrorKind::Configuration);
        assert_eq!(error.message, "missing required env var OPENROUTER_API_KEY");
        return;
    }

    let _guard = child_process_lock()
        .lock()
        .expect("child process mutex poisoned");
    let temp_dir = temp_test_dir("openrouter-from-env-missing");
    run_in_child(
        "openrouter_from_env_rejects_missing_api_key",
        &temp_dir,
        &[],
    );
}

fn child_openai_from_env_loads_dotenv_and_applies_trimmed_overrides() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind capture listener");
    listener
        .set_nonblocking(false)
        .expect("configure listener blocking mode");
    let base_url = format!(
        "http://{}",
        listener.local_addr().expect("listener local addr")
    );

    replace_file_text(Path::new(".env"), "CHILD_SERVER_BASE_URL", &base_url);

    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept request");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("set read timeout");

        let request_bytes = read_http_request(&mut stream);
        let request_text = String::from_utf8(request_bytes).expect("request should be utf8");
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nx-request-id: req_from_env\r\nconnection: close\r\n\r\n{}",
            OPENAI_SUCCESS_BODY.len(),
            OPENAI_SUCCESS_BODY
        );
        stream
            .write_all(response.as_bytes())
            .expect("write response");
        request_text
    });

    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime should build");
    runtime.block_on(async {
        let client = OpenAiClient::from_env().expect("client should load from .env");
        let (_response, meta) = client
            .messages()
            .create_with_meta(MessageCreateInput::user("hello from env"))
            .await
            .expect("request should succeed");

        assert_eq!(meta.selected_model, "gpt-5-mini");
    });

    let request = server.join().expect("server thread should join");
    assert!(request.starts_with("POST /v1/responses HTTP/1.1\r\n"));
    assert!(request.contains("\r\nauthorization: Bearer test-openai-key\r\n"));

    let body = request_body_json(&request);
    assert_eq!(
        body.pointer("/model").and_then(|value| value.as_str()),
        Some("gpt-5-mini")
    );
}

fn read_http_request(stream: &mut std::net::TcpStream) -> Vec<u8> {
    let mut header_bytes = Vec::new();
    let mut scratch = [0_u8; 1024];

    loop {
        let read = stream.read(&mut scratch).expect("read request");
        assert!(read > 0, "request ended before headers");
        header_bytes.extend_from_slice(&scratch[..read]);

        if let Some(header_end) = find_header_end(&header_bytes) {
            let content_length = parse_content_length(&header_bytes[..header_end]);
            let body_end = header_end + 4 + content_length;

            while header_bytes.len() < body_end {
                let read = stream.read(&mut scratch).expect("read request body");
                assert!(read > 0, "request ended before body");
                header_bytes.extend_from_slice(&scratch[..read]);
            }

            return header_bytes[..body_end].to_vec();
        }
    }
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_content_length(headers: &[u8]) -> usize {
    let header_text = String::from_utf8(headers.to_vec()).expect("headers should be utf8");
    header_text
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case("content-length") {
                Some(
                    value
                        .trim()
                        .parse::<usize>()
                        .expect("content-length should parse"),
                )
            } else {
                None
            }
        })
        .unwrap_or(0)
}

fn request_body_json(request: &str) -> serde_json::Value {
    let (_, body) = request
        .split_once("\r\n\r\n")
        .expect("request should include body");
    serde_json::from_str(body).expect("request body should be valid json")
}

fn replace_file_text(path: &Path, needle: &str, replacement: &str) {
    let content = fs::read_to_string(path).expect("read file");
    let updated = content.replace(needle, replacement);
    fs::write(path, updated).expect("write file");
}

fn run_in_child(test_name: &str, temp_dir: &Path, files: &[(&str, &str)]) {
    for (path, content) in files {
        let file_path = temp_dir.join(path);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).expect("create parent directory");
        }
        fs::write(file_path, content).expect("write test file");
    }

    let status = Command::new(std::env::current_exe().expect("current exe path"))
        .current_dir(temp_dir)
        .env("AGENT_RUNTIME_CLIENTS_TEST_CHILD", "1")
        .arg("--exact")
        .arg(format!("test::clients_test::{test_name}"))
        .arg("--nocapture")
        .status()
        .expect("spawn child test");

    assert!(status.success(), "child test {test_name} failed");
}

fn child_process_lock() -> &'static Mutex<()> {
    CHILD_PROCESS_LOCK.get_or_init(|| Mutex::new(()))
}

fn is_child_process() -> bool {
    std::env::var("AGENT_RUNTIME_CLIENTS_TEST_CHILD").as_deref() == Ok("1")
}

fn temp_test_dir(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("agent-runtime-{label}-{unique}"));
    fs::create_dir_all(&path).expect("create temp dir");
    path
}
