//! Application-owned publication and ranking. The model and `SQLite` supply evidence only.
use super::{Error, MemberClient, SearchSessionId, storage};
use cs_mail_search::{CompatibilityProfile, Embedding, Passage, ProfileId, TextPart};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Chunk {
    pub passage: Passage,
    pub text: String,
    pub values: Vec<f32>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) enum Projection {
    Pending,
    Building {
        profile: ProfileId,
        chunks: Vec<Chunk>,
    },
    Ready {
        profile: ProfileId,
        chunks: Vec<Chunk>,
    },
}
impl Projection {
    pub fn ready(&self, profile: &CompatibilityProfile) -> Option<&[Chunk]> {
        match self {
            Self::Ready {
                profile: id,
                chunks,
            } if *id == profile.id() => Some(chunks),
            _ => None,
        }
    }
    pub fn validate(&self) -> Result<(), Error> {
        match self {
            Self::Pending => Ok(()),
            Self::Building { chunks, .. } | Self::Ready { chunks, .. } => {
                if chunks.len() > 20000
                    || chunks.iter().any(|c| {
                        c.text.is_empty()
                            || c.values.len() != 512
                            || c.values.iter().any(|v| !v.is_finite())
                    })
                {
                    return Err(Error::InvalidData);
                }
                Ok(())
            }
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreparedChunk {
    part: usize,
    start: u32,
    end: u32,
    values: Vec<f32>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Prepared {
    chunks: Vec<PreparedChunk>,
    complete: bool,
}

impl MemberClient {
    pub(super) fn projection_key(cache: &[u8; 32]) -> Result<zeroize::Zeroizing<[u8; 32]>, Error> {
        let mut mac = Hmac::<Sha256>::new_from_slice(cache).map_err(|_| Error::InvalidData)?;
        mac.update(b"cs-mail/retained-search-projection/v1");
        Ok(zeroize::Zeroizing::new(mac.finalize().into_bytes().into()))
    }
    /// One bounded model turn; no foreground/domain lock is held across inference.
    #[allow(clippy::too_many_lines)] // One capture, bounded numerical turn and fresh atomic publication fence.
    pub(super) fn index_semantic(&self, session: SearchSessionId) -> Result<u64, Error> {
        let runtime = self.retrieval.as_ref().ok_or(Error::Unavailable)?;
        let (key, identity, version, expires, original, parts, title, prior, content_digest) = {
            let _workspace = self.workspace.try_lock().map_err(|_| Error::Unavailable)?;
            self.check_search_session(session)?;
            let keys = storage::secrets(self.store.as_ref())?;
            let state = storage::open(self.store.as_ref(), &keys)?;
            let retained = super::retained::Retained::load_at_epoch(
                self,
                &keys,
                &state,
                Some(session.value()),
            )?;
            let Some((key, indexed)) = retained.entries.iter().find(|(key, v)| {
                retained.entry(**key, (self.clock)()).is_some()
                    && v.projection.ready(runtime.profile()).is_none()
            }) else {
                return Ok(0);
            };
            let entry = retained
                .copy(self, &keys, *key, (self.clock)())?
                .ok_or(Error::SourceUnavailable)?;
            let content = self.search_content(&state, &keys, &entry)?;
            let mut parts = Vec::new();
            if let Some(subject) = &entry.subject {
                parts.push((TextPart::Subject, subject.text().to_owned()));
            }
            parts.extend(
                content
                    .reading
                    .text_parts()
                    .into_iter()
                    .map(|(part, text)| (part, text.to_owned())),
            );
            // One canonical string per location: duplicate equal parts are harmless,
            // conflicting representations cannot name a single source passage.
            let mut canonical = Vec::new();
            for (part, text) in parts {
                if let Some((_, existing)) = canonical.iter().find(|(p, _)| *p == part) {
                    if existing != &text {
                        return Err(Error::InvalidData);
                    }
                } else {
                    canonical.push((part, text));
                }
            }
            let parts = canonical;
            let title = parts
                .iter()
                .find(|(part, _)| *part == TextPart::Subject)
                .map_or_else(String::new, |(_, s)| s.clone());
            let (identity, version) = retained.source(*key)?;
            let prior = match &indexed.projection {
                Projection::Building { profile, chunks } if *profile == runtime.profile().id() => {
                    chunks.clone()
                }
                _ => Vec::new(),
            };
            (
                *key,
                identity,
                version,
                entry.expires,
                entry.text.clone(),
                parts,
                title,
                prior,
                indexed.content_digest,
            )
        };
        let model_key = {
            let keys = storage::secrets(self.store.as_ref())?;
            Self::projection_key(&keys.cache)?
        };
        let response:Prepared=runtime.request(session.value(),&model_key,&serde_json::json!({"op":"prepare","title":title,"parts":parts.iter().map(|(_,text)|text).collect::<Vec<_>>(),"after":prior.len()}))?;
        drop(model_key);
        if response.chunks.len() > 4 || (!response.complete && response.chunks.is_empty()) {
            return Err(Error::InvalidData);
        }
        let prior_length = prior.len();
        let mut chunks = prior;
        for raw in response.chunks {
            let (part, text) = parts.get(raw.part).ok_or(Error::InvalidData)?;
            let passage = Passage::new(
                identity.clone(),
                version,
                1,
                *part,
                text,
                raw.start,
                raw.end,
            )
            .map_err(|_| Error::InvalidData)?;
            let selected = passage
                .verify(text)
                .map_err(|_| Error::InvalidData)?
                .to_owned();
            Embedding::new(runtime.profile(), raw.values.clone())
                .map_err(|_| Error::InvalidData)?;
            chunks.push(Chunk {
                passage,
                text: selected,
                values: raw.values,
            });
        }
        if response.complete {
            for (part, text) in &parts {
                let mut intervals = chunks
                    .iter()
                    .filter(|c| c.passage.location() == *part)
                    .map(|c| c.passage.range())
                    .collect::<Vec<_>>();
                intervals.sort_unstable();
                let mut end = 0usize;
                for (a, b) in intervals {
                    if a as usize > end
                        && !text
                            .get(end..a as usize)
                            .ok_or(Error::InvalidData)?
                            .trim()
                            .is_empty()
                    {
                        return Err(Error::InvalidData);
                    }
                    end = end.max(b as usize);
                }
                if !text.get(end..).ok_or(Error::InvalidData)?.trim().is_empty() {
                    return Err(Error::InvalidData);
                }
            }
        }
        let _operation = self.operation.try_lock().map_err(|_| Error::Unavailable)?;
        let _workspace = self.workspace.try_lock().map_err(|_| Error::Unavailable)?;
        self.check_search_session(session)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let state = storage::open(self.store.as_ref(), &keys)?;
        let mut retained =
            super::retained::Retained::load_at_epoch(self, &keys, &state, Some(session.value()))?;
        let indexed = retained
            .entry(key, (self.clock)())
            .ok_or(Error::SourceUnavailable)?;
        let current_length = match &indexed.projection {
            Projection::Building { profile, chunks } if *profile == runtime.profile().id() => {
                chunks.len()
            }
            _ => 0,
        };
        if current_length != prior_length || indexed.projection.ready(runtime.profile()).is_some() {
            return Err(Error::Unavailable);
        }
        if indexed.content_digest != content_digest
            || retained.source(key)? != (identity.clone(), version)
            || (self.clock)() >= expires
        {
            return Err(Error::SourceUnavailable);
        }
        let copy = retained
            .copy(self, &keys, key, (self.clock)())?
            .ok_or(Error::SourceUnavailable)?;
        if copy.text != original {
            return Err(Error::InvalidData);
        }
        let projection = if response.complete {
            Projection::Ready {
                profile: runtime.profile().id(),
                chunks,
            }
        } else {
            Projection::Building {
                profile: runtime.profile().id(),
                chunks,
            }
        };
        retained.project(key, projection)?;
        retained.save(self, &keys)?;
        Ok(1)
    }
}
pub(super) fn content_digest(text: &str) -> [u8; 32] {
    Sha256::digest(text.as_bytes()).into()
}
