use agent_core::{ExecutionPlan, ProviderKind, Response, ResponseFormat, ResponseMode};
use agent_providers::request_plan::ProviderRequestPlan;
use agent_transport::{
    HttpJsonResponse, HttpRequestBody, HttpResponse, TransportExecutionInput, TransportResponseFraming,
};

use crate::provider_runtime::{
    OpenedProviderStream, ProviderRuntime, extract_provider_code, join_url,
    prepend_encode_warnings, response_mode_mismatch_error,
};
use crate::runtime_error::RuntimeError;

pub(super) struct PlannedExecution {
    pub(super) plan: ProviderRequestPlan,
    pub(super) response_format: ResponseFormat,
    pub(super) execution_plan: ExecutionPlan,
    pub(super) url: String,
}

pub(super) fn plan_execution(
    runtime: &ProviderRuntime,
    execution_plan: &ExecutionPlan,
) -> Result<PlannedExecution, RuntimeError> {
    let response_format = execution_plan.task.response_format.clone();
    let plan = runtime
        .adapter
        .plan_request(execution_plan)
        .map_err(RuntimeError::from_adapter)?;
    validate_response_framing(execution_plan, &plan)?;
    let endpoint_path = plan
        .endpoint_path_override
        .as_deref()
        .unwrap_or(runtime.adapter.descriptor().endpoint_path);
    let url = join_url(&execution_plan.platform.base_url, endpoint_path);

    Ok(PlannedExecution {
        plan,
        response_format,
        execution_plan: execution_plan.clone(),
        url,
    })
}

pub(super) async fn execute_planned_non_streaming(
    runtime: &ProviderRuntime,
    planned: PlannedExecution,
) -> Result<(Response, HttpJsonResponse), RuntimeError> {
    match planned.plan.response_framing {
        TransportResponseFraming::Json => execute_json_attempt(runtime, planned).await,
        TransportResponseFraming::Sse => execute_sse_attempt(runtime, planned).await,
        TransportResponseFraming::Bytes => Err(RuntimeError::configuration(format!(
            "unsupported provider execution plan for {:?}: response_framing=Bytes",
            runtime.kind
        ))),
    }
}

pub(super) fn validate_streaming_plan(
    provider: ProviderKind,
    plan: &ProviderRequestPlan,
) -> Result<(), RuntimeError> {
    match plan.response_framing {
        TransportResponseFraming::Sse => Ok(()),
        framing => Err(RuntimeError::configuration(format!(
            "streaming API requires an SSE stream plan for {:?}: response_framing={framing:?}",
            provider
        ))),
    }
}

pub(super) async fn open_planned_stream(
    runtime: &ProviderRuntime,
    planned: PlannedExecution,
) -> Result<OpenedProviderStream, RuntimeError> {
    open_sse_stream(
        runtime,
        planned.plan,
        planned.response_format,
        &planned.execution_plan,
        &planned.url,
    )
    .await
}

async fn execute_json_attempt(
    runtime: &ProviderRuntime,
    planned: PlannedExecution,
) -> Result<(Response, HttpJsonResponse), RuntimeError> {
    let PlannedExecution {
        plan,
        response_format,
        execution_plan,
        url,
    } = planned;
    let body = serialize_request_body(&plan)?;

    let mut provider_response = match runtime
        .transport
        .send(transport_request(&execution_plan, &plan, &url, body))
        .await
        .map_err(|error| RuntimeError::from_transport(runtime.kind, error))?
    {
        HttpResponse::Json(response) => response,
        HttpResponse::Sse(response) => {
            return Err(response_mode_mismatch_error(
                runtime.kind,
                TransportResponseFraming::Json,
                "SSE",
                &response.head,
            ));
        }
        HttpResponse::Bytes(response) => {
            return Err(response_mode_mismatch_error(
                runtime.kind,
                TransportResponseFraming::Json,
                "bytes",
                &response.head,
            ));
        }
    };
    let provider_code = extract_provider_code(&provider_response.body);
    let response_body = std::mem::replace(&mut provider_response.body, serde_json::Value::Null);
    let mut response = runtime
        .adapter
        .decode_response_json(response_body, &response_format)
        .map_err(|mut error| {
            if error.provider_code.is_none() {
                error.provider_code = provider_code;
            }
            runtime.runtime_error_from_adapter(error, Some(&provider_response))
        })?;
    prepend_encode_warnings(&mut response, plan.warnings);
    Ok((response, provider_response))
}

async fn execute_sse_attempt(
    runtime: &ProviderRuntime,
    planned: PlannedExecution,
) -> Result<(Response, HttpJsonResponse), RuntimeError> {
    let mut stream = open_planned_stream(runtime, planned).await?;
    while stream.next_envelope().await?.is_some() {}
    stream.finish()
}

async fn open_sse_stream(
    runtime: &ProviderRuntime,
    plan: ProviderRequestPlan,
    response_format: ResponseFormat,
    execution_plan: &ExecutionPlan,
    url: &str,
) -> Result<OpenedProviderStream, RuntimeError> {
    let body = serialize_request_body(&plan)?;

    let response = match runtime
        .transport
        .send(transport_request(execution_plan, &plan, url, body))
        .await
        .map_err(|error| RuntimeError::from_transport(runtime.kind, error))?
    {
        HttpResponse::Sse(response) => *response,
        HttpResponse::Json(response) => {
            return Err(response_mode_mismatch_error(
                runtime.kind,
                TransportResponseFraming::Sse,
                "JSON",
                &response.head,
            ));
        }
        HttpResponse::Bytes(response) => {
            return Err(response_mode_mismatch_error(
                runtime.kind,
                TransportResponseFraming::Sse,
                "bytes",
                &response.head,
            ));
        }
    };

    Ok(OpenedProviderStream {
        provider: runtime.kind,
        response,
        response_format,
        prepended_warnings: plan.warnings,
        projector: runtime.adapter.create_stream_projector(),
        runtime: crate::provider_stream_runtime::ProviderStreamRuntime::new(runtime.kind),
        transcript: Vec::new(),
    })
}

fn serialize_request_body(plan: &ProviderRequestPlan) -> Result<HttpRequestBody, RuntimeError> {
    serde_json::to_vec(&plan.body)
        .map(Into::into)
        .map(HttpRequestBody::Json)
        .map_err(|error| {
            RuntimeError::configuration(format!(
                "failed to serialize provider request body: {error}"
            ))
        })
}

fn transport_request<'a>(
    execution_plan: &'a ExecutionPlan,
    plan: &'a ProviderRequestPlan,
    url: &'a str,
    body: HttpRequestBody,
) -> TransportExecutionInput<'a> {
    TransportExecutionInput {
        platform: &execution_plan.platform,
        auth: execution_plan.auth.credentials.as_ref(),
        method: plan.method.clone(),
        url,
        body,
        response_framing: plan.response_framing,
        options: plan.request_options.clone(),
        transport: execution_plan.transport.clone(),
        provider_headers: plan.provider_headers.clone(),
    }
}

fn validate_response_framing(
    execution_plan: &ExecutionPlan,
    plan: &ProviderRequestPlan,
) -> Result<(), RuntimeError> {
    match (execution_plan.response_mode, plan.response_framing) {
        (ResponseMode::NonStreaming, TransportResponseFraming::Sse) => {
            Err(RuntimeError::configuration(format!(
                "non-streaming execution cannot use SSE response framing for {:?}",
                execution_plan.provider_attempt.provider_kind
            )))
        }
        (ResponseMode::Streaming, TransportResponseFraming::Sse)
        | (ResponseMode::NonStreaming, TransportResponseFraming::Json)
        | (ResponseMode::NonStreaming, TransportResponseFraming::Bytes) => Ok(()),
        (ResponseMode::Streaming, framing) => Err(RuntimeError::configuration(format!(
            "streaming execution requires SSE response framing for {:?}, got {framing:?}",
            execution_plan.provider_attempt.provider_kind
        ))),
    }
}
