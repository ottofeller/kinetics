use super::{Feature, FeatureId};

pub struct SecretsV2;

pub struct SecretsParams<'a> {
    pub package: Option<&'a str>,
}

impl Feature for SecretsV2 {
    const ID: FeatureId = FeatureId::SecretsV2;

    type Params<'a> = SecretsParams<'a>;

    fn enabled(params: SecretsParams<'_>) -> bool {
        params.package.is_some()
    }
}
