use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use agent_core::{
    ExecutionPlan, PlatformConfig, ProviderCapabilities, ProviderDescriptor, ProviderFamilyId,
    ProviderInstanceId, ProviderKind,
};
use agent_providers::{ProviderAdapterHandle, adapter_for, test_support::TestAdapterBuilder};
use agent_transport::HttpTransport;
use reqwest::header::{HeaderMap, HeaderName};

use crate::provider::{ProviderClient, ProviderConfig, RegisteredProvider};
use crate::provider_runtime::ProviderRuntime;

pub(crate) fn default_instance_id(provider: ProviderKind) -> ProviderInstanceId {
    match provider {
        ProviderKind::OpenAi => ProviderInstanceId::openai_default(),
        ProviderKind::Anthropic => ProviderInstanceId::anthropic_default(),
        ProviderKind::OpenRouter => ProviderInstanceId::openrouter_default(),
        ProviderKind::GenericOpenAiCompatible => {
            ProviderInstanceId::generic_openai_compatible_default()
        }
    }
}

pub(crate) fn test_provider_client(provider: ProviderKind) -> ProviderClient {
    let adapter = adapter_for(provider);
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("test client should build");
    let transport = HttpTransport::builder(client).build();
    let instance_id = default_instance_id(provider);
    let registered = RegisteredProvider::new(
        instance_id.clone(),
        provider,
        ProviderConfig::new("test-key"),
    );
    let platform = registered
        .platform_config(adapter.descriptor())
        .expect("test platform should build");

    ProviderClient::new(ProviderRuntime {
        instance_id,
        kind: provider,
        registered,
        adapter,
        platform,
        transport,
        observer: None,
    })
}

pub(crate) fn test_provider_client_with_base_url(
    provider: ProviderKind,
    base_url: &str,
    default_model: Option<&str>,
) -> ProviderClient {
    let adapter = adapter_for(provider);
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("test client should build");
    let transport = HttpTransport::builder(client).build();
    let instance_id = default_instance_id(provider);
    let mut config = ProviderConfig::new("test-key").with_base_url(base_url);
    if let Some(default_model) = default_model {
        config = config.with_default_model(default_model);
    }
    let registered = RegisteredProvider::new(instance_id.clone(), provider, config);
    let platform = registered
        .platform_config(adapter.descriptor())
        .expect("test platform should build");

    ProviderClient::new(ProviderRuntime {
        instance_id,
        kind: provider,
        registered,
        adapter,
        platform,
        transport,
        observer: None,
    })
}

pub(crate) fn test_provider_client_with_streaming_support(
    provider: ProviderKind,
    default_model: Option<&str>,
    supports_streaming: bool,
) -> ProviderClient {
    let adapter = streaming_capability_test_adapter(supports_streaming);
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("test client should build");
    let transport = HttpTransport::builder(client).build();
    let platform = PlatformConfig {
        protocol: adapter.descriptor().protocol.clone(),
        base_url: "http://127.0.0.1:1".to_string(),
        auth_style: adapter.descriptor().default_auth_style.clone(),
        request_id_header: adapter.descriptor().default_request_id_header.clone(),
        default_headers: adapter.descriptor().default_headers.clone(),
    };
    let instance_id = default_instance_id(provider);
    let mut config = ProviderConfig::new("test-key");
    if let Some(default_model) = default_model {
        config = config.with_default_model(default_model);
    }
    let registered = RegisteredProvider::new(instance_id.clone(), provider, config);

    ProviderClient::new(ProviderRuntime {
        instance_id,
        kind: provider,
        registered,
        adapter,
        platform,
        transport,
        observer: None,
    })
}

pub(crate) fn test_provider_runtime(
    provider: ProviderKind,
    base_url: &str,
    default_model: Option<&str>,
) -> ProviderRuntime {
    let adapter = adapter_for(provider);
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("test client should build");
    let transport = HttpTransport::builder(client).build();
    let instance_id = default_instance_id(provider);
    let mut config = ProviderConfig::new("test-key").with_base_url(base_url);
    if let Some(default_model) = default_model {
        config = config.with_default_model(default_model);
    }
    let registered = RegisteredProvider::new(instance_id.clone(), provider, config);
    let platform = registered
        .platform_config(adapter.descriptor())
        .expect("test platform should build");

    ProviderRuntime {
        instance_id,
        kind: provider,
        registered,
        adapter,
        platform,
        transport,
        observer: None,
    }
}

pub(crate) async fn spawn_sse_stub(content_type: &str, body: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test listener");
    let addr = listener.local_addr().expect("local addr");
    let content_type = content_type.to_string();
    let body = body.to_string();

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept test stream");
        let mut scratch = [0_u8; 8192];
        let _ = stream.read(&mut scratch).await;

        let http = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nx-request-id: req_sse\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(http.as_bytes()).await;
        let _ = stream.shutdown().await;
    });

    format!("http://{addr}")
}

fn streaming_capability_test_adapter(supports_streaming: bool) -> ProviderAdapterHandle {
    let capabilities = ProviderCapabilities {
        supports_streaming,
        supports_family_native_options: true,
        supports_provider_native_options: true,
    };
    let descriptor = ProviderDescriptor {
        kind: ProviderKind::OpenAi,
        family: ProviderFamilyId::OpenAiCompatible,
        protocol: agent_core::ProtocolKind::OpenAI,
        default_base_url: "https://api.openai.com",
        endpoint_path: "/v1/responses",
        default_auth_style: agent_core::AuthStyle::Bearer,
        default_request_id_header: HeaderName::from_static("x-request-id"),
        default_headers: HeaderMap::new(),
        capabilities,
    };

    TestAdapterBuilder::new(descriptor, |execution: &ExecutionPlan| {
        adapter_for(ProviderKind::OpenAi).plan_request(execution)
    })
    .delegate_to_builtin(ProviderKind::OpenAi)
    .build()
}
