use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub(super) async fn spawn_json_success_stub(request_id: &str) -> String {
    const OPENAI_SUCCESS_BODY: &str = include_str!(
        "../../../../agent-providers/data/openai/responses/decoded/basic_chat/gpt-5-mini.json"
    );

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test listener");
    let addr = listener.local_addr().expect("local addr");
    let request_id = request_id.to_string();

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept test stream");
        let mut scratch = [0_u8; 8192];
        let _ = stream.read(&mut scratch).await;

        let http = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nx-request-id: {}\r\nconnection: close\r\n\r\n{}",
            OPENAI_SUCCESS_BODY.len(),
            request_id,
            OPENAI_SUCCESS_BODY
        );
        let _ = stream.write_all(http.as_bytes()).await;
        let _ = stream.shutdown().await;
    });

    format!("http://{addr}")
}
