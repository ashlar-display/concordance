use super::*;
use crate::output::warning::Violation;

/// `Clone::clone` must compile and produce an engine that behaves identically
/// to the original. Calling it exercises the hand-written `Clone` impl that
/// delegates to the derived `Copy`.
#[test]
fn clone_produces_equivalent_engine() {
    let original = DefaultConstraintEngine::default();
    let cloned = original.clone();
    // Both should format identically — same check list, same display names.
    assert_eq!(alloc::format!("{original:?}"), alloc::format!("{cloned:?}"));
}

/// The `Debug` impl formats the engine as an ordered list of rule display names.
/// The output must be non-empty and contain at least one known built-in rule name.
#[test]
fn debug_lists_rule_names() {
    let engine = DefaultConstraintEngine::<Violation>::default();
    let output = alloc::format!("{engine:?}");
    assert!(!output.is_empty());
    // Spot-check one well-known built-in rule name.
    assert!(
        output.contains("frl_ceiling"),
        "expected 'frl_ceiling' in debug output, got: {output}"
    );
}
