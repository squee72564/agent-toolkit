use super::*;

#[test]
fn send_options_with_observer_keeps_clone_and_partial_eq_pointer_semantics() {
    let observer: std::sync::Arc<dyn RuntimeObserver> = std::sync::Arc::new(ObserverStub);
    let options = SendOptions::for_target(Target::new(ProviderId::OpenAi)).with_observer(observer);

    let cloned = options.clone();
    assert_eq!(options, cloned);
    assert!(options.observer.is_some());

    let other_observer: std::sync::Arc<dyn RuntimeObserver> = std::sync::Arc::new(ObserverStub);
    let different =
        SendOptions::for_target(Target::new(ProviderId::OpenAi)).with_observer(other_observer);
    assert_ne!(options, different);
}
