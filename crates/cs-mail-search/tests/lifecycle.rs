//! Agreed counterexamples: delayed publication/replay revives removed content,
//! a stale draft revision replaces newer writing, or incompatible vectors are ranked together.
use cs_mail_correspondence::{ConversationId, MessageVersion};
use cs_mail_primitives::{AccountId, CanonicalTime, MessageId};
use cs_mail_search::{
    CompatibilityProfile, CorrespondenceStream, DerivedSource, Embedding, Error, SourceIdentity,
    SourceOccurrence, SourceStatus, SourceVersion, WorkIntent, backup_removal_deadline,
};

#[test]
fn delayed_publication_cannot_revive_a_deleted_or_expired_source() {
    let source = SourceIdentity::new(
        AccountId(1),
        SourceOccurrence::Message {
            stream: CorrespondenceStream::Native,
            conversation: ConversationId::new(2).unwrap(),
            message: MessageId(3),
        },
    )
    .unwrap();
    let version = SourceVersion::Message(MessageVersion([4; 32]));
    let current = DerivedSource::observed(
        source.clone(),
        version,
        SourceStatus::Retained {
            expires_at: CanonicalTime(100),
        },
    )
    .unwrap();
    let ticket = current.next().generation();
    assert_eq!(
        current
            .next()
            .allow_publication(&source, version, ticket, CanonicalTime(100)),
        Err(Error::Expired)
    );
    let deletion = current
        .next()
        .advance(
            ticket,
            version,
            SourceStatus::Removed {
                removed_at: CanonicalTime(50),
            },
        )
        .unwrap();
    assert_eq!(
        deletion.work(),
        &WorkIntent::RemoveAndSynchronize {
            generation: deletion.next().generation(),
            backup_deadline: backup_removal_deadline(CanonicalTime(50)).unwrap(),
        }
    );
    assert!(
        deletion
            .next()
            .allow_publication(&source, version, ticket, CanonicalTime(60))
            .is_err()
    );
    assert_eq!(
        deletion.next().allow_publication(
            &source,
            version,
            deletion.next().generation(),
            CanonicalTime(60)
        ),
        Err(Error::Removed)
    );
    assert_eq!(
        deletion
            .next()
            .advance(
                deletion.next().generation(),
                version,
                SourceStatus::Retained {
                    expires_at: CanonicalTime(200)
                }
            )
            .unwrap_err(),
        Error::Removed
    );
    // Identical message IDs in a different account never establish the same source.
    let other = SourceIdentity::new(AccountId(2), source.occurrence().clone()).unwrap();
    assert_eq!(
        current
            .next()
            .allow_publication(&other, version, ticket, CanonicalTime(60)),
        Err(Error::Stale)
    );
}

#[test]
fn delayed_draft_revision_and_incompatible_profile_do_not_replace_current_evidence() {
    let source =
        SourceIdentity::new(AccountId(1), SourceOccurrence::PrivateDraft { id: 2 }).unwrap();
    let version = |n| SourceVersion::PrivateDraft {
        revision: std::num::NonZeroU64::new(n).unwrap(),
        checksum: [3; 32],
    };
    let initial =
        DerivedSource::observed(source.clone(), version(1), SourceStatus::PrivateDraft).unwrap();
    let updated = initial
        .next()
        .advance(
            initial.next().generation(),
            version(2),
            SourceStatus::PrivateDraft,
        )
        .unwrap();
    assert_eq!(
        updated.next().allow_publication(
            &source,
            version(1),
            initial.next().generation(),
            CanonicalTime(0)
        ),
        Err(Error::Stale)
    );
    assert_eq!(
        updated
            .next()
            .advance(
                updated.next().generation(),
                version(1),
                SourceStatus::PrivateDraft
            )
            .unwrap_err(),
        Error::Stale
    );
    let profile = CompatibilityProfile::new(
        [1; 32], [2; 32], [3; 32], [4; 32], [5; 32], [6; 32], 2, true,
    )
    .unwrap();
    let changed = CompatibilityProfile::new(
        [1; 32], [2; 32], [3; 32], [4; 32], [7; 32], [6; 32], 2, true,
    )
    .unwrap();
    let document = Embedding::new(&profile, vec![1., 0.]).unwrap();
    let query = Embedding::new(&changed, vec![1., 0.]).unwrap();
    assert_eq!(document.cosine(&query), Err(Error::Incompatible));
}
