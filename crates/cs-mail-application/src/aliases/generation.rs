//! Readable alias candidates. Names are member-approved input, not identity evidence.
//! Randomness, reserved addresses and the serving domain are supplied facts.
use cs_mail_accounts::{aliases::AliasError, directory::MemberAddress};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

const SUFFIX_SPACE: u64 = 1_000_000;
const CROWDED_AT: usize = 10_000;
const ATTEMPTS: u64 = 32;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct NameFragment(String);
impl TryFrom<String> for NameFragment {
    type Error = AliasError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if !(2..=8).contains(&value.len()) || !value.bytes().all(|b| b.is_ascii_lowercase()) {
            return Err(AliasError::Invalid);
        }
        Ok(Self(value))
    }
}
impl From<NameFragment> for String {
    fn from(value: NameFragment) -> Self {
        value.0
    }
}
impl NameFragment {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct GenerationName(String);
impl TryFrom<String> for GenerationName {
    type Error = AliasError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.chars().count() > 120 || value.chars().any(char::is_control) {
            return Err(AliasError::Invalid);
        }
        let name = Self(value.trim().to_owned());
        if name.fragments().is_empty() {
            return Err(AliasError::Invalid);
        }
        Ok(name)
    }
}
impl From<GenerationName> for String {
    fn from(value: GenerationName) -> Self {
        value.0
    }
}
impl GenerationName {
    pub fn fragments(&self) -> Vec<NameFragment> {
        let normalized: String = self
            .0
            .nfkd()
            .filter(|c| !is_combining_mark(*c))
            .map(|c| {
                if c.is_ascii_alphabetic() {
                    c.to_ascii_lowercase()
                } else {
                    ' '
                }
            })
            .collect();
        let parts: Vec<_> = normalized.split_whitespace().collect();
        let mut fragments = BTreeSet::new();
        let mut add = |value: String| {
            if NameFragment::try_from(value.clone()).is_ok() {
                fragments.insert(value);
            }
        };
        for part in &parts {
            for length in 2..=part.len().min(8) {
                add(part[..length].to_owned());
            }
        }
        if let (Some(first), Some(last)) = (parts.first(), parts.last()) {
            if parts.len() > 1 {
                let initials = format!("{}{}", &first[..1], &last[..1]);
                add(initials.clone());
                add(initials.repeat(2));
                for length in 2..=last.len().min(7) {
                    add(format!("{}{}", &first[..1], &last[..length]));
                    add(format!("{}{}", &last[..length], &first[..1]));
                }
                for length in 2..=first.len().min(7) {
                    add(format!("{}{}", &first[..length], &last[..1]));
                }
            }
        }
        fragments
            .into_iter()
            .filter_map(|value| value.try_into().ok())
            .collect()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AddressSuggestion {
    pub address: MemberAddress,
    pub fragments: Vec<NameFragment>,
}
impl AddressSuggestion {
    pub fn matches(&self, name: &GenerationName, preferred: Option<&NameFragment>) -> bool {
        let Some(fragment) = generated_fragment(&self.address) else {
            return false;
        };
        self.fragments == name.fragments()
            && self.fragments.contains(&fragment)
            && preferred.is_none_or(|p| p == &fragment)
    }
}

pub fn generated_fragment(address: &MemberAddress) -> Option<NameFragment> {
    let (local, _) = address.as_str().split_once('@')?;
    let split = local.len().checked_sub(6)?;
    if !local.is_ascii() || !local[split..].bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    local[..split].to_owned().try_into().ok()
}

fn sample(seed: &[u8; 16], counter: &mut u64, bound: u64) -> u64 {
    let limit = u64::MAX - u64::MAX % bound;
    loop {
        let mut hash = Sha256::new();
        hash.update(b"cs-mail/readable-alias/v1");
        hash.update(seed);
        hash.update(counter.to_be_bytes());
        *counter += 1;
        let bytes = hash.finalize();
        let value = u64::from_be_bytes(bytes[..8].try_into().expect("eight digest bytes"));
        if value < limit {
            return value % bound;
        }
    }
}

/// # Errors
/// Refuses an invalid preferred fragment, exhausted namespace or bounded collision retries.
pub fn suggest(
    name: &GenerationName,
    preferred: Option<&NameFragment>,
    domain: &str,
    reserved: &BTreeSet<String>,
    seed: [u8; 16],
) -> Result<AddressSuggestion, AliasError> {
    let fragments = name.fragments();
    if preferred.is_some_and(|p| !fragments.contains(p)) {
        return Err(AliasError::Invalid);
    }
    let mut occupancy = BTreeMap::<String, usize>::new();
    for value in reserved {
        if let Ok(address) = MemberAddress::try_from(value.clone())
            && address
                .as_str()
                .split_once('@')
                .is_some_and(|(_, d)| d == domain)
            && let Some(fragment) = generated_fragment(&address)
        {
            *occupancy.entry(fragment.0).or_default() += 1;
        }
    }
    let mut eligible: Vec<_> = fragments
        .iter()
        .filter(|p| {
            preferred.is_none_or(|chosen| chosen == *p)
                && occupancy.get(p.as_str()).copied().unwrap_or(0) < 1_000_000
        })
        .collect();
    if preferred.is_none()
        && eligible
            .iter()
            .any(|p| occupancy.get(p.as_str()).copied().unwrap_or(0) < CROWDED_AT)
    {
        eligible.retain(|p| occupancy.get(p.as_str()).copied().unwrap_or(0) < CROWDED_AT);
    }
    if eligible.is_empty() {
        return Err(AliasError::Conflict);
    }
    let mut counter = 0;
    for _ in 0..ATTEMPTS {
        let index = usize::try_from(sample(
            &seed,
            &mut counter,
            u64::try_from(eligible.len()).map_err(|_| AliasError::Invalid)?,
        ))
        .map_err(|_| AliasError::Invalid)?;
        let suffix = sample(&seed, &mut counter, SUFFIX_SPACE);
        let address: MemberAddress =
            format!("{}{:06}@{}", eligible[index].as_str(), suffix, domain)
                .try_into()
                .map_err(|_| AliasError::Invalid)?;
        if !reserved.contains(address.as_str()) {
            return Ok(AddressSuggestion { address, fragments });
        }
    }
    Err(AliasError::Conflict)
}
