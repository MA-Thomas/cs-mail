//! Exact profile identity prevents mixing vector spaces. No runtime/model is selected here.
use crate::Error;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProfileId([u8; 32]);
/// All compatibility-affecting artifacts are content-addressed, not mutable model names.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ProfileRecord", into = "ProfileRecord")]
pub struct CompatibilityProfile(ProfileRecord);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileRecord {
    weights: [u8; 32],
    tokenizer: [u8; 32],
    preprocessing: [u8; 32],
    passage_rules: [u8; 32],
    instructions: [u8; 32],
    runtime_precision: [u8; 32],
    dimensions: u16,
    normalized: bool,
}
impl TryFrom<ProfileRecord> for CompatibilityProfile {
    type Error = Error;
    fn try_from(v: ProfileRecord) -> Result<Self, Error> {
        if v.dimensions == 0
            || v.dimensions > 8192
            || [
                v.weights,
                v.tokenizer,
                v.preprocessing,
                v.passage_rules,
                v.instructions,
                v.runtime_precision,
            ]
            .contains(&[0; 32])
        {
            return Err(Error::Invalid);
        }
        Ok(Self(v))
    }
}
impl From<CompatibilityProfile> for ProfileRecord {
    fn from(v: CompatibilityProfile) -> Self {
        v.0
    }
}
impl CompatibilityProfile {
    /// # Errors
    /// Requires artifact fingerprints and a bounded nonzero dimension count.
    #[allow(clippy::too_many_arguments)] // Each digest names a separate compatibility input.
    pub fn new(
        weights: [u8; 32],
        tokenizer: [u8; 32],
        preprocessing: [u8; 32],
        passage_rules: [u8; 32],
        instructions: [u8; 32],
        runtime_precision: [u8; 32],
        dimensions: u16,
        normalized: bool,
    ) -> Result<Self, Error> {
        ProfileRecord {
            weights,
            tokenizer,
            preprocessing,
            passage_rules,
            instructions,
            runtime_precision,
            dimensions,
            normalized,
        }
        .try_into()
    }
    pub fn id(&self) -> ProfileId {
        let mut hash = Sha256::new();
        hash.update(b"cs-mail/embedding-profile/v1");
        for digest in [
            self.0.weights,
            self.0.tokenizer,
            self.0.preprocessing,
            self.0.passage_rules,
            self.0.instructions,
            self.0.runtime_precision,
        ] {
            hash.update(digest);
        }
        hash.update(self.0.dimensions.to_be_bytes());
        hash.update([u8::from(self.0.normalized)]);
        ProfileId(hash.finalize().into())
    }
}
#[derive(Clone, Debug)]
pub struct Embedding {
    profile: ProfileId,
    values: Vec<f32>,
}
impl Embedding {
    pub fn values(&self) -> &[f32] {
        &self.values
    }
    /// Validate adapter output before indexing. Vectors remain sensitive derived data.
    /// # Errors
    /// Dimension mismatch, nonfinite/zero vectors or invalid declared normalization.
    pub fn new(profile: &CompatibilityProfile, values: Vec<f32>) -> Result<Self, Error> {
        let norm: f64 = values.iter().map(|v| f64::from(*v).powi(2)).sum();
        if values.len() != usize::from(profile.0.dimensions)
            || !norm.is_finite()
            || norm <= 0.
            || (profile.0.normalized && (norm - 1.).abs() > 0.01)
        {
            return Err(Error::Invalid);
        }
        Ok(Self {
            profile: profile.id(),
            values,
        })
    }
    /// Ranking evidence only; it is neither access authority nor an answer probability.
    /// # Errors
    /// Incompatible profiles must never be compared.
    pub fn cosine(&self, other: &Self) -> Result<f64, Error> {
        if self.profile != other.profile {
            return Err(Error::Incompatible);
        }
        let dot: f64 = self
            .values
            .iter()
            .zip(&other.values)
            .map(|(a, b)| f64::from(*a) * f64::from(*b))
            .sum();
        let norm = |v: &[f32]| v.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>().sqrt();
        Ok((dot / (norm(&self.values) * norm(&other.values))).clamp(-1., 1.))
    }
}
