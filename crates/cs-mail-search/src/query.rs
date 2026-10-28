//! Literal word/phrase meaning is independent of numerical retrieval and presentation.
use crate::{Error, TextPart};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct Query(QueryKind);
#[derive(Clone, Debug)]
enum QueryKind {
    Phrase(String),
    Words(BTreeSet<String>),
}
impl Query {
    /// # Errors
    /// Empty, oversized or wordless queries.
    pub fn parse(text: &str) -> Result<Self, Error> {
        let text = text.trim();
        if text.is_empty() || text.len() > 1024 {
            return Err(Error::Invalid);
        }
        if let Some(phrase) = text.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
            if phrase.is_empty() {
                return Err(Error::Invalid);
            }
            return Ok(Self(QueryKind::Phrase(phrase.to_owned())));
        }
        let words = words(text)
            .map(|(_, word)| word.to_lowercase())
            .collect::<BTreeSet<_>>();
        if words.is_empty() {
            return Err(Error::Invalid);
        }
        Ok(Self(QueryKind::Words(words)))
    }
    pub const fn is_phrase(&self) -> bool {
        matches!(self.0, QueryKind::Phrase(_))
    }
    /// Every word must occur somewhere in this message's canonical parts.
    /// Returns one supporting original range; separate parts remain separate sources.
    pub fn locate(&self, parts: &[(TextPart, String)]) -> Option<(TextPart, usize, usize)> {
        match &self.0 {
            QueryKind::Phrase(phrase) => parts
                .iter()
                .find_map(|(part, text)| literal_range(text, phrase).map(|(a, b)| (*part, a, b))),
            QueryKind::Words(required) => {
                let mut found = BTreeSet::new();
                let mut best = None;
                let mut best_count = 0;
                for (part, text) in parts {
                    let mut local = BTreeSet::new();
                    let mut start = None;
                    let mut end = 0;
                    for (at, word) in words(text) {
                        let end_word = at + word.len();
                        let word = word.to_lowercase();
                        if required.contains(&word) && local.insert(word.clone()) {
                            found.insert(word);
                            start.get_or_insert(at);
                            end = end_word;
                        }
                    }
                    if local.len() > best_count {
                        best_count = local.len();
                        best = Some((*part, start?, end));
                    }
                }
                (found == *required).then_some(best).flatten()
            }
        }
    }
}
fn words(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.split_inclusive(|c: char| !c.is_alphanumeric())
        .scan(0, |offset, segment| {
            let at = *offset;
            *offset += segment.len();
            Some((at, segment.trim_end_matches(|c: char| !c.is_alphanumeric())))
        })
        .filter(|(_, word)| !word.is_empty())
}
pub fn literal_range(text: &str, phrase: &str) -> Option<(usize, usize)> {
    let mut folded = String::new();
    let mut boundaries = BTreeMap::new();
    for (offset, ch) in text.char_indices() {
        boundaries.insert(folded.len(), offset);
        folded.extend(ch.to_lowercase());
    }
    boundaries.insert(folded.len(), text.len());
    let phrase = phrase.to_lowercase();
    if phrase.is_empty() {
        return None;
    }
    folded.match_indices(&phrase).find_map(|(start, _)| {
        Some((
            *boundaries.get(&start)?,
            *boundaries.get(&(start + phrase.len()))?,
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    // Conjecture: full word search requires all whole words across a message,
    // whereas quotes preserve literal order and punctuation across windows.
    #[test]
    fn all_words_across_parts_are_distinct_from_literal_phrases() {
        let parts = vec![
            (TextPart::Subject, "Doctors".into()),
            (
                TextPart::Body { part: 0 },
                "Your appointment is Tuesday.".into(),
            ),
        ];
        assert!(
            Query::parse("DOCTORS appointment")
                .unwrap()
                .locate(&parts)
                .is_some()
        );
        assert!(
            Query::parse("doctors application")
                .unwrap()
                .locate(&parts)
                .is_none()
        );
        assert!(
            Query::parse("doctor appointment")
                .unwrap()
                .locate(&parts)
                .is_none()
        );
        assert!(
            Query::parse("\"Doctors appointment\"")
                .unwrap()
                .locate(&parts)
                .is_none()
        );
        assert!(
            Query::parse("\"appointment is Tuesday.\"")
                .unwrap()
                .locate(&parts)
                .is_some()
        );
        assert_eq!(
            literal_range("İSTANBUL 😀", "i\u{307}stanbul"),
            Some((0, "İSTANBUL".len()))
        );
    }
}
