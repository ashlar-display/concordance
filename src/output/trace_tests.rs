use super::*;

#[test]
fn default_returns_empty_trace() {
    let trace = ReasoningTrace::default();
    assert!(trace.steps.is_empty());
}
