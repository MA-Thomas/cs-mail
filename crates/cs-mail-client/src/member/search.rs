//! Source-bound hybrid retrieval. Similarity never grants reading or reuse authority.
use super::retained::CopyKey;
use super::semantic::Chunk;
use super::{Error, MailKey, MemberClient, storage};
use cs_mail_search::{Embedding, Passage, Query, TextPart};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::Ordering,
};

/// Captured before dispatch; a later query or clearing search invalidates it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct SearchRequestId(u64);
impl TryFrom<u64> for SearchRequestId {
    type Error = Error;
    fn try_from(value: u64) -> Result<Self, Error> {
        if value == 0 {
            Err(Error::InvalidData)
        } else {
            Ok(Self(value))
        }
    }
}
impl SearchRequestId {
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Suggestions rank broadly; word results are exhaustive; related results exclude word matches.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum SearchMode {
    Suggestions,
    Words,
    Related,
}

#[derive(Clone, Debug)]
pub struct SearchMatch {
    pub key: MailKey,
    pub message: String,
    pub excerpt: String,
    pub expires_at: u64,
    pub passage: cs_mail_search::Passage,
    pub selected_text: String,
    pub start_utf16: u64,
    pub end_utf16: u64,
    pub subject: Option<String>,
    pub committed_at: u64,
}
pub(super) struct VerifiedSearchContent {
    pub reading: super::ReadingContent,
    pub version: cs_mail_correspondence::MessageVersion,
    pub committed_at: cs_mail_primitives::CanonicalTime,
}
#[derive(Clone, Debug)]
pub struct SearchPage {
    pub matches: Vec<SearchMatch>,
    pub literal_query: bool,
    pub searched: u64,
    pub unavailable: u64,
    pub not_retained: u64,
    pub preparing: u64,
    pub next: u64,
    pub total: u64,
}
#[derive(Clone)]
struct Candidate {
    key: CopyKey,
    passage: Passage,
    text: String,
    digest: [u8; 32],
}
pub(super) struct SearchSession {
    request: SearchRequestId,
    query: String,
    scope: super::InboxScope,
    groups: BTreeMap<SearchMode, Vec<Candidate>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rankings {
    semantic: Vec<(u64, f64)>,
    lexical: Vec<u64>,
    vector: Vec<f32>,
}
fn chunk_id(chunk: &Chunk) -> Result<u64, Error> {
    let hash: [u8; 32] =
        Sha256::digest(serde_json::to_vec(&chunk.passage).map_err(|_| Error::InvalidData)?).into();
    let id = u64::from_be_bytes(hash[..8].try_into().map_err(|_| Error::InvalidData)?)
        & 0x7fff_ffff_ffff_ffff;
    if id == 0 {
        return Err(Error::InvalidData);
    }
    Ok(id)
}
fn model_row(id: u64, alias: Option<String>, chunk: &Chunk) -> serde_json::Value {
    serde_json::json!({"id":id,"alias":alias.unwrap_or_default(),"text":chunk.text,"values":chunk.values})
}
impl MemberClient {
    /// Prepare the local numerical engine and current ready projections without
    /// creating a query or granting disclosure. No foreground lock spans model work.
    /// # Errors
    /// Closed lifetime, unavailable custody/resources or malformed projections.
    pub fn prepare_local_search(&self, session: super::SearchSessionId) -> Result<(), Error> {
        let runtime = self.retrieval.as_ref().ok_or(Error::RetrievalUnavailable)?;
        let (keys, model_key, rows) = {
            let _workspace = self.workspace.lock().map_err(|_| Error::Unavailable)?;
            self.check_search_session(session)?;
            let keys = storage::secrets(self.store.as_ref())?;
            let state = storage::open(self.store.as_ref(), &keys)?;
            let retained = super::retained::Retained::load_at_epoch(
                self,
                &keys,
                &state,
                Some(session.value()),
            )?;
            let mut ids = BTreeSet::new();
            let mut rows = Vec::new();
            let now = (self.clock)();
            for (copy, indexed) in &retained.entries {
                if retained.entry(*copy, now).is_none() {
                    continue;
                }
                if let Some(chunks) = indexed.projection.ready(runtime.profile()) {
                    for chunk in chunks {
                        Embedding::new(runtime.profile(), chunk.values.clone())
                            .map_err(|_| Error::InvalidData)?;
                        let id = chunk_id(chunk)?;
                        if !ids.insert(id) {
                            return Err(Error::InvalidData);
                        }
                        let alias = retained.received_keys.iter().find_map(|(alias, copies)| {
                            copies.contains(copy).then(|| alias.value().to_string())
                        });
                        rows.push(model_row(id, alias, chunk));
                    }
                }
            }
            let model_key = Self::projection_key(&keys.cache)?;
            (keys, model_key, rows)
        };
        for batch in rows.chunks(128) {
            self.check_search_session(session)?;
            let _: serde_json::Value = runtime.request(
                session.value(),
                &model_key,
                &serde_json::json!({"op":"upsert","rows":batch}),
            )?;
        }
        // Exercise the existing encoder with static, non-mail text. Its temporary
        // vector is discarded: this publishes neither a source nor a query. Reusing
        // the numerical contract preserves the existing embedding fingerprint.
        let _: serde_json::Value = runtime.request(
            session.value(),
            &model_key,
            &serde_json::json!({"op":"prepare","title":"","parts":["Local search preparation."],"after":0}),
        )?;
        storage::check_keys(self.store.as_ref(), &keys)?;
        self.check_search_session(session)
    }
    /// Capture intent before dispatch, also cancelling an active superseded query.
    /// # Errors
    /// The lifetime is closed or query identities are exhausted.
    pub fn begin_search(&self, lifetime: super::SearchSessionId) -> Result<SearchRequestId, Error> {
        self.check_search_session(lifetime)?;
        self.supersede_search()
    }
    fn supersede_search(&self) -> Result<SearchRequestId, Error> {
        let previous = self
            .search_revision
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| v.checked_add(1))
            .map_err(|_| Error::Unavailable)?;
        let next = SearchRequestId(previous + 1);
        if let Some(runtime) = &self.retrieval {
            runtime.supersede_search(next);
        }
        Ok(next)
    }
    /// # Errors
    /// Exhausted query identities or a poisoned query mutex.
    pub fn clear_search(&self) -> Result<(), Error> {
        let next = self.supersede_search()?;
        let mut current = self.search.lock().map_err(|_| Error::Unavailable)?;
        if self.search_revision.load(Ordering::SeqCst) == next.value() {
            *current = None;
        }
        Ok(())
    }
    /// Retained source authority is sampled before model work and checked again at disclosure.
    /// # Errors
    /// Invalid query/cursor, unavailable local model, custody or substituted source.
    #[allow(clippy::too_many_lines, clippy::too_many_arguments)] // Explicit lifetime, query, result group and page boundaries.
    pub fn search_mail(
        &self,
        lifetime: super::SearchSessionId,
        request: SearchRequestId,
        query: &str,
        mode: SearchMode,
        scope: super::InboxScope,
        after: u64,
        limit: u16,
    ) -> Result<SearchPage, Error> {
        if query.trim().is_empty() || query.len() > 1024 || limit == 0 || limit > 25 {
            return Err(Error::InvalidData);
        }
        let expression = Query::parse(query).map_err(|_| Error::InvalidData)?;
        self.check_search_session(lifetime)?;
        let fence = super::retrieval_runtime::QueryFence {
            id: request,
            revision: &self.search_revision,
        };
        fence.check()?;
        #[cfg(test)]
        if after == 0 && self.retrieval.is_some() {
            while self.index_semantic(lifetime)? > 0 {}
        }
        let runtime = self.retrieval.as_ref();
        let mut coverage = (0, 0, 0, 0);
        if after == 0 {
            let (all, allowed, rows, key, literals) = {
                let _workspace = self.workspace.lock().map_err(|_| Error::Unavailable)?;
                self.check_search_session(lifetime)?;
                let keys = storage::secrets(self.store.as_ref())?;
                if let super::InboxScope::Alias(id) = scope {
                    if !self
                        .receiving
                        .lock()
                        .map_err(|_| Error::Unavailable)?
                        .as_ref()
                        .ok_or(Error::Unavailable)?
                        .contains(id)
                    {
                        return Err(Error::InvalidData);
                    }
                }
                let state = storage::open(self.store.as_ref(), &keys)?;
                let retained = super::retained::Retained::session(self, &keys, lifetime)?;
                let now = (self.clock)();
                let mut all = BTreeMap::new();
                let mut allowed = BTreeSet::new();
                let mut rows = Vec::new();
                let mut literals = Vec::new();
                let in_scope = |key: &CopyKey| match scope {
                    super::InboxScope::All => true,
                    super::InboxScope::Alias(id) => retained
                        .received_keys
                        .get(&id)
                        .is_some_and(|s| s.contains(key)),
                };
                let progress = retained.progress(now);
                coverage.1 = progress.unavailable;
                coverage.2 = progress.remaining;
                for (copy, indexed) in &retained.entries {
                    fence.check()?;
                    if retained.entry(*copy, now).is_none() {
                        continue;
                    }
                    let canonical = if !in_scope(copy) {
                        Vec::new()
                    } else if let super::semantic::Projection::Ready { chunks, .. } =
                        &indexed.projection
                    {
                        canonical_parts(chunks)?
                    } else {
                        let entry = retained
                            .copy(self, &keys, *copy, now)?
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
                        parts
                    };
                    let word_match = expression.locate(&canonical);
                    if in_scope(copy)
                        && let Some((location, start, end)) = word_match
                    {
                        let (identity, version) = retained.source(*copy)?;
                        let text = canonical
                            .iter()
                            .find(|(part, _)| *part == location)
                            .ok_or(Error::InvalidData)?
                            .1
                            .as_str();
                        let passage = Passage::new(
                            identity,
                            version,
                            1,
                            location,
                            text,
                            u32::try_from(start).map_err(|_| Error::InvalidData)?,
                            u32::try_from(end).map_err(|_| Error::InvalidData)?,
                        )
                        .map_err(|_| Error::InvalidData)?;
                        literals.push(Candidate {
                            key: *copy,
                            text: passage
                                .verify(text)
                                .map_err(|_| Error::InvalidData)?
                                .to_owned(),
                            passage,
                            digest: indexed.content_digest,
                        });
                    }
                    if mode == SearchMode::Words || expression.is_phrase() {
                        if in_scope(copy) {
                            coverage.0 += 1;
                        }
                        continue;
                    }
                    let runtime = runtime.ok_or(Error::RetrievalUnavailable)?;
                    let Some(chunks) = indexed.projection.ready(runtime.profile()) else {
                        if in_scope(copy) {
                            coverage.3 += 1;
                        }
                        continue;
                    };
                    if in_scope(copy) {
                        coverage.0 += 1;
                    }
                    for chunk in chunks {
                        Embedding::new(runtime.profile(), chunk.values.clone())
                            .map_err(|_| Error::InvalidData)?;
                        let id = chunk_id(chunk)?;
                        if id == 0
                            || all
                                .insert(
                                    id,
                                    Candidate {
                                        key: *copy,
                                        passage: chunk.passage.clone(),
                                        text: chunk.text.clone(),
                                        digest: indexed.content_digest,
                                    },
                                )
                                .is_some()
                        {
                            return Err(Error::InvalidData);
                        }
                        if in_scope(copy) {
                            allowed.insert(id);
                        }
                        let alias = retained.received_keys.iter().find_map(|(alias, keys)| {
                            keys.contains(copy).then(|| alias.value().to_string())
                        });
                        rows.push(model_row(id, alias, chunk));
                    }
                }
                (
                    all,
                    allowed,
                    rows,
                    Self::projection_key(&keys.cache)?,
                    literals,
                )
            };
            let candidates = if mode == SearchMode::Words || expression.is_phrase() {
                if mode == SearchMode::Related {
                    Vec::new()
                } else {
                    literals
                }
            } else {
                let runtime = runtime.ok_or(Error::RetrievalUnavailable)?;
                let literal_keys = literals.iter().map(|c| c.key).collect::<BTreeSet<_>>();
                for batch in rows.chunks(128) {
                    let _: serde_json::Value = runtime.request_query(
                        lifetime.value(),
                        &key,
                        &serde_json::json!({"op":"upsert","rows":batch}),
                        fence,
                    )?;
                }
                let alias = match scope {
                    super::InboxScope::All => None,
                    super::InboxScope::Alias(id) => Some(id.value().to_string()),
                };
                let ranks:Rankings=runtime.request_query(lifetime.value(),&key,&serde_json::json!({"op":"query","query":query,"scope":alias,"all":all.keys().collect::<Vec<_>>(),"allowed":allowed}),fence)?;
                drop(key);
                Embedding::new(runtime.profile(), ranks.vector).map_err(|_| Error::InvalidData)?;
                let mut scores = BTreeMap::<CopyKey, (f64, u64)>::new();
                let semantic = ranks
                    .semantic
                    .into_iter()
                    .map(|(id, d)| {
                        if d.is_finite() {
                            Ok(id)
                        } else {
                            Err(Error::InvalidData)
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                for list in [&semantic, &ranks.lexical] {
                    let mut seen = BTreeSet::new();
                    let mut rank = 0u32;
                    for id in list {
                        let candidate = all
                            .get(id)
                            .filter(|_| allowed.contains(id))
                            .ok_or(Error::InvalidData)?;
                        if !seen.insert(candidate.key) {
                            continue;
                        }
                        rank += 1;
                        let entry = scores.entry(candidate.key).or_insert((0., *id));
                        entry.0 += 1. / (60. + f64::from(rank));
                    }
                }
                let mut ranked = scores.into_iter().collect::<Vec<_>>();
                ranked.sort_by(|a, b| b.1.0.total_cmp(&a.1.0).then_with(|| a.0.cmp(&b.0)));
                ranked
                    .into_iter()
                    .filter(|(key, _)| mode != SearchMode::Related || !literal_keys.contains(key))
                    .take(100)
                    .map(|(_, (_, id))| all.get(&id).cloned().ok_or(Error::InvalidData))
                    .collect::<Result<Vec<_>, _>>()?
            };
            let mut current = self.search.lock().map_err(|_| Error::Unavailable)?;
            self.check_search_session(lifetime)?;
            fence.check()?;
            if current
                .as_ref()
                .is_none_or(|s| s.request != request || s.query != query || s.scope != scope)
            {
                *current = Some(SearchSession {
                    request,
                    query: query.to_owned(),
                    scope,
                    groups: BTreeMap::new(),
                });
            }
            current
                .as_mut()
                .ok_or(Error::InvalidData)?
                .groups
                .insert(mode, candidates);
        }
        let (candidates, total) = {
            let session = self.search.lock().map_err(|_| Error::Unavailable)?;
            let candidates = &session
                .as_ref()
                .filter(|s| s.request == request && s.query == query && s.scope == scope)
                .ok_or(Error::InvalidData)?
                .groups
                .get(&mode)
                .ok_or(Error::InvalidData)?;
            let start = usize::try_from(after).map_err(|_| Error::InvalidData)?;
            if start > candidates.len() {
                return Err(Error::InvalidData);
            }
            (
                candidates
                    .iter()
                    .skip(start)
                    .take(usize::from(limit))
                    .cloned()
                    .collect::<Vec<_>>(),
                candidates.len() as u64,
            )
        };
        let _workspace = self.workspace.lock().map_err(|_| Error::Unavailable)?;
        fence.check()?;
        self.check_search_session(lifetime)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let state = storage::open(self.store.as_ref(), &keys)?;
        let retained =
            super::retained::Retained::load_at_epoch(self, &keys, &state, Some(lifetime.value()))?;
        let mut result = SearchPage {
            matches: Vec::new(),
            literal_query: expression.is_phrase(),
            searched: coverage.0,
            unavailable: coverage.1,
            not_retained: coverage.2,
            preparing: coverage.3,
            next: after,
            total,
        };
        for candidate in candidates {
            result.next += 1;
            let Some(indexed) = retained.entry(candidate.key, (self.clock)()) else {
                continue;
            };
            if indexed.content_digest != candidate.digest
                || retained.source(candidate.key)?
                    != (
                        candidate.passage.source().clone(),
                        candidate.passage.version(),
                    )
            {
                continue;
            }
            if let super::InboxScope::Alias(id) = scope {
                if !retained
                    .received_keys
                    .get(&id)
                    .is_some_and(|s| s.contains(&candidate.key))
                {
                    continue;
                }
            }
            let Some(entry) = retained.copy(self, &keys, candidate.key, (self.clock)())? else {
                continue;
            };
            let content = self.search_content(&state, &keys, &entry)?;
            let mut parts = Vec::new();
            if let Some(subject) = &entry.subject {
                parts.push((TextPart::Subject, subject.text()));
            }
            parts.extend(content.reading.text_parts());
            let text = parts
                .iter()
                .find_map(|(part, text)| (*part == candidate.passage.location()).then_some(*text))
                .ok_or(Error::InvalidData)?;
            let selected = candidate
                .passage
                .verify(text)
                .map_err(|_| Error::InvalidData)?;
            if selected != candidate.text
                || cs_mail_search::SourceVersion::Message(content.version)
                    != candidate.passage.version()
            {
                return Err(Error::InvalidData);
            }
            let (start, end) = candidate.passage.range();
            result.matches.push(SearchMatch {
                key: candidate.key.thread,
                message: candidate.key.message.0.to_string(),
                excerpt: excerpt(text, start),
                expires_at: entry.expires.0,
                selected_text: selected.to_owned(),
                start_utf16: text[..start as usize].encode_utf16().count() as u64,
                end_utf16: text[..end as usize].encode_utf16().count() as u64,
                subject: parts.iter().find_map(|(part, text)| {
                    (*part == TextPart::Subject).then(|| (*text).to_owned())
                }),
                passage: candidate.passage,
                committed_at: content.committed_at.0,
            });
        }
        storage::check_keys(self.store.as_ref(), &keys)?;
        self.check_search_session(lifetime)?;
        let now = (self.clock)();
        result.matches.retain(|m| now.0 < m.expires_at);
        fence.check()?;
        Ok(result)
    }
}

impl MemberClient {
    pub(super) fn search_content(
        &self,
        state: &super::State,
        keys: &storage::Secrets,
        entry: &super::retained::Entry,
    ) -> Result<VerifiedSearchContent, Error> {
        let (content, version, committed_at) = if let Some(view) = &entry.native {
            let (secret, public) = super::correspondence::own_keys(&state.profile, keys)?;
            let doc = crate::correspondence::open(
                &crate::correspondence::EndpointKeys {
                    owner: state.profile.persona,
                    secret: &secret,
                    public,
                },
                view,
                view.record.sender_key,
            )
            .map_err(|_| Error::InvalidData)?;
            let content = super::ReadingContent::from_document(&doc);
            if format!(
                "{}\n\n{}",
                doc.subject().ok_or(Error::InvalidData)?.text(),
                content.search_text()?
            ) != entry.text
            {
                return Err(Error::InvalidData);
            }
            (
                content,
                view.record.manifest.version(),
                view.record.committed_at,
            )
        } else {
            let (doc, version, committed_at) =
                self.external_source(state, keys, entry.key.thread, entry.key.message)?;
            let content = super::ReadingContent::from_external_document(&doc);
            if content.search_text()? != entry.text {
                return Err(Error::InvalidData);
            }
            (content, version, committed_at)
        };
        Ok(VerifiedSearchContent {
            reading: content,
            version,
            committed_at,
        })
    }
}
fn excerpt(text: &str, start_byte: u32) -> String {
    let chars: Vec<_> = text.chars().collect();
    let at = text[..start_byte as usize].chars().count();
    let start = at.saturating_sub(50);
    let end = (start + 240).min(chars.len());
    format!(
        "{}{}{}",
        if start > 0 { "…" } else { "" },
        chars[start..end].iter().collect::<String>(),
        if end < chars.len() { "…" } else { "" }
    )
}

/// Reassemble overlapping canonical part ranges, then bind the literal range.
/// Original authenticated content is still required at disclosure.
#[cfg(test)]
pub(super) fn literal_passage(
    chunks: &[Chunk],
    phrase: &str,
) -> Result<Option<(Passage, String)>, Error> {
    let parts = canonical_parts(chunks)?;
    let Some((location, start, end)) = Query::parse(&format!("\"{phrase}\""))
        .map_err(|_| Error::InvalidData)?
        .locate(&parts)
    else {
        return Ok(None);
    };
    let first = chunks
        .iter()
        .find(|c| c.passage.location() == location)
        .ok_or(Error::InvalidData)?;
    let text = &parts
        .iter()
        .find(|(part, _)| *part == location)
        .ok_or(Error::InvalidData)?
        .1;
    let passage = Passage::new(
        first.passage.source().clone(),
        first.passage.version(),
        1,
        location,
        text,
        u32::try_from(start).map_err(|_| Error::InvalidData)?,
        u32::try_from(end).map_err(|_| Error::InvalidData)?,
    )
    .map_err(|_| Error::InvalidData)?;
    Ok(Some((passage, text[start..end].to_owned())))
}
fn canonical_parts(chunks: &[Chunk]) -> Result<Vec<(TextPart, String)>, Error> {
    let mut locations = Vec::new();
    let mut parts = Vec::new();
    for chunk in chunks {
        if !locations.contains(&chunk.passage.location()) {
            locations.push(chunk.passage.location());
        }
    }
    for location in locations {
        let mut ranges = chunks
            .iter()
            .filter(|c| c.passage.location() == location)
            .collect::<Vec<_>>();
        ranges.sort_by_key(|c| c.passage.range());
        let first = ranges[0];
        let mut text = String::new();
        for chunk in ranges {
            let (start, end) = chunk.passage.range();
            let start = start as usize;
            let end = end as usize;
            if chunk.passage.source() != first.passage.source()
                || chunk.passage.version() != first.passage.version()
                || start > text.len()
                || end - start != chunk.text.len()
            {
                return Err(Error::InvalidData);
            }
            let overlap = text.len().min(end) - start;
            if text.get(start..start + overlap) != chunk.text.get(..overlap) {
                return Err(Error::InvalidData);
            }
            text.push_str(chunk.text.get(overlap..).ok_or(Error::InvalidData)?);
        }
        parts.push((location, text));
    }
    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cs_mail_correspondence::{ConversationId, MessageVersion};
    use cs_mail_primitives::{AccountId, MessageId};
    use cs_mail_search::literal_range;
    use cs_mail_search::{CorrespondenceStream, SourceIdentity, SourceOccurrence, SourceVersion};

    // Conjecture: a literal occurrence crossing embedding windows remains locatable
    // in original UTF-8/UTF-16 coordinates, including lowercase expansion.
    #[test]
    fn literal_occurrence_survives_passage_boundaries_and_unicode_expansion() {
        let text = "İSTANBUL 😀 before unique crossing phrase after";
        let phrase = "unique crossing phrase";
        let identity = SourceIdentity::new(
            AccountId(1),
            SourceOccurrence::Message {
                stream: CorrespondenceStream::Native,
                conversation: ConversationId::new(2).unwrap(),
                message: MessageId(3),
            },
        )
        .unwrap();
        let chunk = |start: usize, end: usize| Chunk {
            passage: Passage::new(
                identity.clone(),
                SourceVersion::Message(MessageVersion([4; 32])),
                1,
                TextPart::Body { part: 0 },
                text,
                u32::try_from(start).unwrap(),
                u32::try_from(end).unwrap(),
            )
            .unwrap(),
            text: text[start..end].to_owned(),
            values: vec![],
        };
        let split = text.find("crossing").unwrap();
        let chunks = vec![chunk(0, split + 5), chunk(split, text.len())];
        assert!(chunks.iter().all(|c| !c.text.contains(phrase)));
        let (passage, selected) = literal_passage(&chunks, phrase).unwrap().unwrap();
        assert_eq!(selected, phrase);
        assert_eq!(passage.verify(text).unwrap(), phrase);
        let start = text.find(phrase).unwrap();
        assert_eq!(
            passage.range(),
            (
                u32::try_from(start).unwrap(),
                u32::try_from(start + phrase.len()).unwrap()
            )
        );
        assert_eq!(
            literal_range(text, "i\u{307}stanbul"),
            Some((0, "İSTANBUL".len()))
        );
        assert_eq!(
            literal_range("İ", "i"),
            None,
            "partial lowercase expansion fabricated a source range"
        );
        assert!(
            literal_passage(&chunks, "absent literal")
                .unwrap()
                .is_none()
        );
    }
}
