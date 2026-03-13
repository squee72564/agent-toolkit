use std::collections::BTreeMap;

use agent_core::{
    Message, ProviderFamilyId, ProviderInstanceId, ProviderKind, Request, TaskRequest, ToolChoice,
};

#[test]
fn legacy_request_task_request_projection_drops_execution_fields() {
    let mut metadata = BTreeMap::new();
    metadata.insert("trace_id".to_string(), "abc123".to_string());

    let request = Request {
        model_id: "gpt-5".to_string(),
        stream: true,
        messages: vec![Message::user_text("hello")],
        tools: Vec::new(),
        tool_choice: ToolChoice::Auto,
        response_format: Default::default(),
        temperature: Some(0.5),
        top_p: Some(0.9),
        max_output_tokens: Some(256),
        stop: vec!["DONE".to_string()],
        metadata: metadata.clone(),
    };

    let task = request.task_request();

    assert_eq!(
        task,
        TaskRequest {
            messages: vec![Message::user_text("hello")],
            tools: Vec::new(),
            tool_choice: ToolChoice::Auto,
            response_format: Default::default(),
            temperature: Some(0.5),
            top_p: Some(0.9),
            max_output_tokens: Some(256),
            stop: vec!["DONE".to_string()],
            metadata,
        }
    );
}

#[test]
fn provider_kind_and_instance_identity_are_distinct() {
    assert_eq!(ProviderKind::OpenAi, agent_core::ProviderId::OpenAi);
    assert_eq!(
        ProviderInstanceId::from(ProviderKind::GenericOpenAiCompatible).as_str(),
        "generic-openai-compatible-default"
    );
    assert_eq!(
        ProviderFamilyId::OpenAiCompatible,
        ProviderFamilyId::OpenAiCompatible
    );
}
