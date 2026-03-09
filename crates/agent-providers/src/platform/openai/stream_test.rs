use agent_core::{CanonicalStreamEvent, FinishReason, ProviderId, ProviderRawStreamEvent};

use crate::platform::openai::stream::OpenAiStreamProjector;
use crate::streaming::ProviderStreamProjector;

#[test]
fn openai_stream_projector_emits_started_and_completed_events() {
    let mut projector = OpenAiStreamProjector::default();

    let started = projector
        .project(ProviderRawStreamEvent::from_sse(
            ProviderId::OpenAi,
            1,
            Some("response.created".to_string()),
            None,
            None,
            r#"{"type":"response.created","response":{"id":"resp_1","model":"gpt-5-mini"}}"#,
        ))
        .expect("projection should succeed");
    let completed = projector
        .project(ProviderRawStreamEvent::from_sse(
            ProviderId::OpenAi,
            2,
            Some("response.completed".to_string()),
            None,
            None,
            r#"{"type":"response.completed","response":{"output":[],"usage":{"input_tokens":1,"output_tokens":2,"total_tokens":3}}}"#,
        ))
        .expect("projection should succeed");

    assert_eq!(
        started,
        vec![CanonicalStreamEvent::ResponseStarted {
            model: Some("gpt-5-mini".to_string()),
            response_id: Some("resp_1".to_string()),
        }]
    );
    assert!(completed.contains(&CanonicalStreamEvent::Completed {
        finish_reason: FinishReason::Stop,
    }));
}
