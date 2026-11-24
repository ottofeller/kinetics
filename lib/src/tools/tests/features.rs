use crate::tools::features::{Feature, FeatureId, Features};
struct TestFeature;
impl Feature for TestFeature {
    const ID: FeatureId = FeatureId::Unknown;
    type Params<'a> = bool;
    fn enabled(enabled: bool) -> bool {
        enabled
    }
}
#[test]
fn feature_rules_and_unknown_names_remain_supported() {
    let enabled = Features::default().with::<TestFeature>(true);
    assert!(enabled.enabled::<TestFeature>());
    assert!(!enabled.with::<TestFeature>(false).enabled::<TestFeature>());
    let future: Features = serde_json::from_str(r#"["future_feature"]"#).unwrap();
    assert!(future.enabled::<TestFeature>());
}
