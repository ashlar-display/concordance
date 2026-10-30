use super::*;

/// A policy serialized before `frl_marginal`/`frl_unverified` existed still
/// deserializes, with the default policy's values for them.
#[test]
fn policy_without_frl_fields_deserializes_with_defaults() {
    let json = r#"{"prefer_native_resolution":true,"prefer_color_fidelity":false,"prefer_high_refresh":true,"penalize_dsc":false}"#;
    let p: NegotiationPolicy = serde_json::from_str(json).unwrap();
    assert!(p.prefer_high_refresh);
    assert_eq!(p.frl_marginal, FrlCandidateHandling::Deprioritize);
    assert_eq!(p.frl_unverified, FrlCandidateHandling::Keep);
}
