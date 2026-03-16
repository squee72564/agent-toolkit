use crate::observability::RuntimeObserver;

mod agent_toolkit_builder_test;
mod agent_toolkit_execution_test;
mod agent_toolkit_routing_test;
mod agent_toolkit_test_fixtures;

#[derive(Debug)]
struct ObserverStub;
impl RuntimeObserver for ObserverStub {}
