use super::*;

#[test]
fn provider_id_reexport_matches_agent_core_type() {
    let provider_from_agent: ProviderId = ProviderId::OpenAi;
    let provider_from_core: agent_core::types::ProviderId = provider_from_agent;
    assert_eq!(provider_from_core, agent_core::types::ProviderId::OpenAi);
}
