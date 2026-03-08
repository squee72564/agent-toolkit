use super::*;

#[test]
fn resolve_observer_for_request_uses_expected_precedence() {
    let client_observer: std::sync::Arc<dyn RuntimeObserver> = std::sync::Arc::new(ObserverStub);
    let toolkit_observer: std::sync::Arc<dyn RuntimeObserver> = std::sync::Arc::new(ObserverStub);
    let send_observer: std::sync::Arc<dyn RuntimeObserver> = std::sync::Arc::new(ObserverStub);

    let resolved_send = resolve_observer_for_request(
        Some(&client_observer),
        Some(&toolkit_observer),
        Some(&send_observer),
    )
    .expect("send observer should resolve");
    assert!(std::sync::Arc::ptr_eq(resolved_send, &send_observer));

    let resolved_toolkit =
        resolve_observer_for_request(Some(&client_observer), Some(&toolkit_observer), None)
            .expect("toolkit observer should resolve");
    assert!(std::sync::Arc::ptr_eq(resolved_toolkit, &toolkit_observer));

    let resolved_client = resolve_observer_for_request(Some(&client_observer), None, None)
        .expect("client observer should resolve");
    assert!(std::sync::Arc::ptr_eq(resolved_client, &client_observer));

    let resolved_none = resolve_observer_for_request(None, None, None);
    assert!(resolved_none.is_none());
}
