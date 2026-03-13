use agent_core::{AdapterContext, ProviderId, Request, Response, ResponseFormat};
use agent_providers::request_plan::{
    ProviderRequestPlan, ProviderResponseKind, ProviderTransportKind,
};
use agent_transport::{
    HttpJsonResponse, HttpRequestBody, HttpResponse, HttpResponseMode, HttpSendRequest,
};
use reqwest::Method;

use crate::provider_runtime::{
    OpenedProviderStream, ProviderRuntime, extract_provider_code, join_url,
    prepend_encode_warnings, response_mode_mismatch_error,
};
use crate::runtime_error::RuntimeError;

pub(super) struct PlannedExecution {
    pub(super) plan: ProviderRequestPlan,
    pub(super) response_format: ResponseFormat,
    pub(super) url: String,
}

pub(super) fn plan_execution(
    runtime: &ProviderRuntime,
    request: Request,
) -> Result<PlannedExecution, RuntimeError> {
    let response_format = request.response_format.clone();
    let plan = runtime
        .adapter
        .plan_request(request)
        .map_err(RuntimeError::from_adapter)?;
    let endpoint_path = plan
        .endpoint_path_override
        .as_deref()
        .unwrap_or(runtime.adapter.descriptor().endpoint_path);
    let url = join_url(&runtime.platform.base_url, endpoint_path);

    Ok(PlannedExecution {
        plan,
        response_format,
        url,
    })
}

pub(super) async fn execute_planned_non_streaming(
    runtime: &ProviderRuntime,
    planned: PlannedExecution,
    adapter_context: &AdapterContext,
) -> Result<(Response, HttpJsonResponse), RuntimeError> {
    match (planned.plan.transport_kind, planned.plan.response_kind) {
        (ProviderTransportKind::HttpJson, ProviderResponseKind::JsonBody) => {
            execute_json_attempt(runtime, planned, adapter_context).await
        }
        (ProviderTransportKind::HttpSse, ProviderResponseKind::RawProviderStream) => {
            execute_sse_attempt(runtime, planned, adapter_context).await
        }
        (transport_kind, response_kind) => Err(RuntimeError::configuration(format!(
            "unsupported provider execution plan for {:?}: transport={transport_kind:?}, response={response_kind:?}",
            runtime.kind
        ))),
    }
}

pub(super) fn validate_streaming_plan(
    provider: ProviderId,
    plan: &ProviderRequestPlan,
) -> Result<(), RuntimeError> {
    match (plan.transport_kind, plan.response_kind) {
        (ProviderTransportKind::HttpSse, ProviderResponseKind::RawProviderStream) => Ok(()),
        (transport_kind, response_kind) => Err(RuntimeError::configuration(format!(
            "streaming API requires an SSE stream plan for {:?}: transport={transport_kind:?}, response={response_kind:?}",
            provider
        ))),
    }
}

pub(super) async fn open_planned_stream(
    runtime: &ProviderRuntime,
    planned: PlannedExecution,
    adapter_context: &AdapterContext,
) -> Result<OpenedProviderStream, RuntimeError> {
    open_sse_stream(
        runtime,
        planned.plan,
        planned.response_format,
        &planned.url,
        adapter_context,
    )
    .await
}

async fn execute_json_attempt(
    runtime: &ProviderRuntime,
    planned: PlannedExecution,
    adapter_context: &AdapterContext,
) -> Result<(Response, HttpJsonResponse), RuntimeError> {
    let PlannedExecution {
        plan,
        response_format,
        url,
    } = planned;
    let body = serialize_request_body(&plan)?;

    let mut provider_response = match runtime
        .transport
        .send(HttpSendRequest {
            platform: &runtime.platform,
            method: Method::POST,
            url: &url,
            body,
            ctx: adapter_context,
            options: plan.request_options.clone(),
            response_mode: HttpResponseMode::Json,
        })
        .await
        .map_err(|error| RuntimeError::from_transport(runtime.kind, error))?
    {
        HttpResponse::Json(response) => response,
        HttpResponse::Sse(response) => {
            return Err(response_mode_mismatch_error(
                runtime.kind,
                HttpResponseMode::Json,
                "SSE",
                &response.head,
            ));
        }
        HttpResponse::Bytes(response) => {
            return Err(response_mode_mismatch_error(
                runtime.kind,
                HttpResponseMode::Json,
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
    adapter_context: &AdapterContext,
) -> Result<(Response, HttpJsonResponse), RuntimeError> {
    let mut stream = open_planned_stream(runtime, planned, adapter_context).await?;
    while stream.next_envelope().await?.is_some() {}
    stream.finish()
}

async fn open_sse_stream(
    runtime: &ProviderRuntime,
    plan: ProviderRequestPlan,
    response_format: ResponseFormat,
    url: &str,
    adapter_context: &AdapterContext,
) -> Result<OpenedProviderStream, RuntimeError> {
    let body = serialize_request_body(&plan)?;

    let response = match runtime
        .transport
        .send(HttpSendRequest {
            platform: &runtime.platform,
            method: Method::POST,
            url,
            body,
            ctx: adapter_context,
            options: plan.request_options.clone(),
            response_mode: HttpResponseMode::Sse,
        })
        .await
        .map_err(|error| RuntimeError::from_transport(runtime.kind, error))?
    {
        HttpResponse::Sse(response) => *response,
        HttpResponse::Json(response) => {
            return Err(response_mode_mismatch_error(
                runtime.kind,
                HttpResponseMode::Sse,
                "JSON",
                &response.head,
            ));
        }
        HttpResponse::Bytes(response) => {
            return Err(response_mode_mismatch_error(
                runtime.kind,
                HttpResponseMode::Sse,
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
