pub mod secrets;

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// An activation rule with its own typed inputs, independent of runtime config.
pub trait Feature {
    /// Stable feature identifier.
    const ID: FeatureId;

    /// Inputs required by this feature.
    type Params<'a>;

    /// Evaluate whether the supplied inputs allow this feature to be enabled.
    fn enabled(params: Self::Params<'_>) -> bool;
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum FeatureId {
    SecretsV2,
    /// Accept feature names introduced by newer producers during deserialization.
    #[serde(other)]
    Unknown,
}

/// Computed decisions serialized as a list such as `["secrets_v2"]`.
/// The ordered set keeps runtime config serialization deterministic.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(transparent)]
pub struct Features(BTreeSet<FeatureId>);

impl Features {
    /// Evaluate a feature and update its stored decision.
    /// A false result also removes the feature if it was previously enabled.
    pub fn with<F: Feature>(mut self, params: F::Params<'_>) -> Self {
        if F::enabled(params) {
            self.0.insert(F::ID);
        } else {
            self.0.remove(&F::ID);
        }
        self
    }

    /// Read the stored decision without evaluating feature parameters again.
    pub fn enabled<F: Feature>(&self) -> bool {
        self.0.contains(&F::ID)
    }
}
