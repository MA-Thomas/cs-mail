//! Versioned deterministic CBOR for authenticated cs-mail commands.
//!
//! The format uses only fixed-length arrays, integers, text, and fixed-width
//! identifier byte strings. It emits preferred integer encodings and rejects
//! indefinite containers, unexpected lengths, unknown tags, and trailing data.

use std::convert::Infallible;
use std::fmt;

use cs_mail_primitives::{
    AttemptId, AttemptSubjectRef, BondId, CanonicalTime, ContentRef, DeliveryIntentRef, Duration,
    IdempotencyKey, LedgerAccountRef, MessageDeclarationDigest, MessageId, MessageValidityUntil,
    Money, OperationalKeyRef, PersistenceReserveId, PolicyVersion, PrivacyProfileVersion,
    ProtocolIdentity, ProtocolVersion, ProviderRef, QuoteId, RelationshipRef,
    RetentionPolicyVersion, SettlementUnit, Version, WireVersion,
};
use cs_mail_protocol::{ActorRef, CancellationReason, ContactTerms, ProtocolCommand};
use minicbor::{Decoder, Encoder};

const SIGNING_DOMAIN: &str = "cs-mail/command";
const QUOTE_SIGNING_DOMAIN: &str = "cs-mail/contact-terms";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandTarget {
    Relationship(RelationshipRef),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalCommandEnvelope {
    pub wire_version: WireVersion,
    pub protocol_version: ProtocolVersion,
    pub deployment_domain: [u8; 32],
    pub intended_provider: ProviderRef,
    pub target: CommandTarget,
    pub actor: ActorRef,
    pub operational_key: OperationalKeyRef,
    pub idempotency_key: IdempotencyKey,
    pub command: ProtocolCommand,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WireError {
    Codec(String),
    UnexpectedShape,
    UnsupportedVersion(ProtocolVersion),
    UnknownTag(u32),
    InvalidIdentifier,
    TrailingData,
}

impl fmt::Display for WireError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Codec(error) => write!(formatter, "CBOR codec error: {error}"),
            Self::UnexpectedShape => formatter.write_str("unexpected CBOR shape"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported protocol version {}", version.0)
            }
            Self::UnknownTag(tag) => write!(formatter, "unknown command tag {tag}"),
            Self::InvalidIdentifier => formatter.write_str("identifier has an invalid width"),
            Self::TrailingData => formatter.write_str("trailing bytes after command envelope"),
        }
    }
}

impl std::error::Error for WireError {}

impl From<minicbor::encode::Error<Infallible>> for WireError {
    fn from(value: minicbor::encode::Error<Infallible>) -> Self {
        Self::Codec(value.to_string())
    }
}

impl From<minicbor::decode::Error> for WireError {
    fn from(value: minicbor::decode::Error) -> Self {
        Self::Codec(value.to_string())
    }
}

/// Encodes the exact bytes covered by an operational-key signature.
///
/// # Errors
///
/// Returns an error only if the in-memory CBOR encoder rejects a value.
pub fn encode_command_envelope(envelope: &CanonicalCommandEnvelope) -> Result<Vec<u8>, WireError> {
    let mut encoder = Encoder::new(Vec::new());
    encoder.array(10)?.str(SIGNING_DOMAIN)?;
    encoder.u16(envelope.wire_version.0)?;
    encoder.u16(envelope.protocol_version.0)?;
    encoder.bytes(&envelope.deployment_domain)?;
    encode_id(&mut encoder, envelope.intended_provider.0)?;
    encode_target(&mut encoder, envelope.target)?;
    encode_actor(&mut encoder, envelope.actor)?;
    encode_id(&mut encoder, envelope.operational_key.0)?;
    encode_id(&mut encoder, envelope.idempotency_key.0)?;
    encode_command(&mut encoder, &envelope.command)?;
    Ok(encoder.into_writer())
}

/// Decodes and structurally validates one canonical command envelope.
///
/// The caller can require a particular version after decoding. Signature
/// verification should re-encode the returned value and compare or verify the
/// signature over those deterministic bytes.
///
/// # Errors
///
/// Returns an error for malformed, indefinite, unknown, or trailing input.
pub fn decode_command_envelope(bytes: &[u8]) -> Result<CanonicalCommandEnvelope, WireError> {
    let mut decoder = Decoder::new(bytes);
    expect_array(&mut decoder, 10)?;
    if decoder.str()? != SIGNING_DOMAIN {
        return Err(WireError::UnexpectedShape);
    }
    let wire_version = WireVersion(decoder.u16()?);
    let protocol_version = ProtocolVersion(decoder.u16()?);
    let domain = decoder.bytes()?;
    let deployment_domain: [u8; 32] = domain.try_into().map_err(|_| WireError::UnexpectedShape)?;
    let envelope = CanonicalCommandEnvelope {
        wire_version,
        protocol_version,
        deployment_domain,
        intended_provider: ProviderRef(decode_id(&mut decoder)?),
        target: decode_target(&mut decoder)?,
        actor: decode_actor(&mut decoder)?,
        operational_key: OperationalKeyRef(decode_id(&mut decoder)?),
        idempotency_key: IdempotencyKey(decode_id(&mut decoder)?),
        command: decode_command(&mut decoder)?,
    };
    if decoder.position() != bytes.len() {
        return Err(WireError::TrailingData);
    }
    if encode_command_envelope(&envelope)? != bytes {
        return Err(WireError::UnexpectedShape);
    }
    Ok(envelope)
}

fn encode_target(encoder: &mut Encoder<Vec<u8>>, target: CommandTarget) -> Result<(), WireError> {
    encoder.array(2)?.u8(0)?;
    let CommandTarget::Relationship(reference) = target;
    encode_scoped_ref(
        encoder,
        reference.derivation_version(),
        reference.as_bytes(),
    )
}

fn decode_target(decoder: &mut Decoder<'_>) -> Result<CommandTarget, WireError> {
    expect_array(decoder, 2)?;
    match decoder.u8()? {
        0 => {
            let (version, bytes) = decode_scoped_ref(decoder)?;
            Ok(CommandTarget::Relationship(RelationshipRef::new(
                version, bytes,
            )))
        }
        tag => Err(WireError::UnknownTag(u32::from(tag))),
    }
}

fn encode_actor(encoder: &mut Encoder<Vec<u8>>, actor: ActorRef) -> Result<(), WireError> {
    encoder.array(2)?;
    match actor {
        ActorRef::Sender(identity) => {
            encoder.u8(0)?;
            encode_id(encoder, identity.0)?;
        }
        ActorRef::Recipient(identity) => {
            encoder.u8(1)?;
            encode_id(encoder, identity.0)?;
        }
        ActorRef::Provider(provider) => {
            encoder.u8(2)?;
            encode_id(encoder, provider.0)?;
        }
        ActorRef::Scheduler(provider) => {
            encoder.u8(3)?;
            encode_id(encoder, provider.0)?;
        }
    }
    Ok(())
}

fn decode_actor(decoder: &mut Decoder<'_>) -> Result<ActorRef, WireError> {
    expect_array(decoder, 2)?;
    let tag = decoder.u8()?;
    let id = decode_id(decoder)?;
    match tag {
        0 => Ok(ActorRef::Sender(ProtocolIdentity(id))),
        1 => Ok(ActorRef::Recipient(ProtocolIdentity(id))),
        2 => Ok(ActorRef::Provider(ProviderRef(id))),
        3 => Ok(ActorRef::Scheduler(ProviderRef(id))),
        tag => Err(WireError::UnknownTag(u32::from(tag))),
    }
}

fn encode_command(
    encoder: &mut Encoder<Vec<u8>>,
    command: &ProtocolCommand,
) -> Result<(), WireError> {
    match command {
        ProtocolCommand::IssueContactTerms {
            quote_id,
            declaration_digest,
        } => {
            encoder.array(3)?.u8(0)?;
            encode_id(encoder, quote_id.0)?;
            encode_optional_declaration_digest(encoder, *declaration_digest)?;
        }
        ProtocolCommand::ReserveAttempt {
            bond_id,
            reserve_id,
            attempt_id,
            message_id,
            terms,
        } => {
            encoder.array(6)?.u8(1)?;
            encode_id(encoder, bond_id.0)?;
            encode_id(encoder, reserve_id.0)?;
            encode_id(encoder, attempt_id.0)?;
            encode_id(encoder, message_id.0)?;
            encode_terms(encoder, terms)?;
        }
        ProtocolCommand::AdmitAttempt {
            bond_id,
            expected_bond_version,
            content_ref,
            delivery_intent_ref,
            declaration_digest,
            message_valid_until,
        } => {
            encoder.array(7)?.u8(2)?;
            encode_id(encoder, bond_id.0)?;
            encoder.u64(expected_bond_version.0)?;
            encode_id(encoder, content_ref.0)?;
            encode_id(encoder, delivery_intent_ref.0)?;
            encoder.bytes(&declaration_digest.0)?;
            encoder.u64(message_valid_until.0.0)?;
        }
        ProtocolCommand::CancelReservedAttempt {
            bond_id,
            expected_bond_version,
            reason,
        } => {
            encoder.array(4)?.u8(3)?;
            encode_id(encoder, bond_id.0)?;
            encoder.u64(expected_bond_version.0)?;
            encoder.u8(match reason {
                CancellationReason::SenderRequested => 0,
                CancellationReason::AdmissionTimeout => 1,
                CancellationReason::PreAdmissionFailure => 2,
            })?;
        }
        ProtocolCommand::AcceptRelationship { expected_version } => {
            encode_version_command(encoder, 4, *expected_version)?;
        }
        ProtocolCommand::RejectRelationship { expected_version } => {
            encode_version_command(encoder, 5, *expected_version)?;
        }
        ProtocolCommand::BlockRelationship { expected_version } => {
            encode_version_command(encoder, 6, *expected_version)?;
        }
        ProtocolCommand::UnblockRelationship { expected_version } => {
            encode_version_command(encoder, 7, *expected_version)?;
        }
        ProtocolCommand::ExpireBond {
            bond_id,
            expected_bond_version,
        } => {
            encoder.array(3)?.u8(8)?;
            encode_id(encoder, bond_id.0)?;
            encoder.u64(expected_bond_version.0)?;
        }
        ProtocolCommand::ReleasePersistenceReserve {
            reserve_id,
            expected_reserve_version,
        } => {
            encoder.array(3)?.u8(9)?;
            encode_id(encoder, reserve_id.0)?;
            encoder.u64(expected_reserve_version.0)?;
        }
        ProtocolCommand::RevokeRelationship { expected_version } => {
            encode_version_command(encoder, 10, *expected_version)?;
        }
    }
    Ok(())
}

fn decode_command(decoder: &mut Decoder<'_>) -> Result<ProtocolCommand, WireError> {
    let length = decoder.array()?.ok_or(WireError::UnexpectedShape)?;
    let tag = decoder.u8()?;
    match tag {
        0 if length == 3 => Ok(ProtocolCommand::IssueContactTerms {
            quote_id: QuoteId(decode_id(decoder)?),
            declaration_digest: decode_optional_declaration_digest(decoder)?,
        }),
        1 if length == 6 => Ok(ProtocolCommand::ReserveAttempt {
            bond_id: BondId(decode_id(decoder)?),
            reserve_id: PersistenceReserveId(decode_id(decoder)?),
            attempt_id: AttemptId(decode_id(decoder)?),
            message_id: MessageId(decode_id(decoder)?),
            terms: Box::new(decode_terms(decoder)?),
        }),
        2 if length == 7 => Ok(ProtocolCommand::AdmitAttempt {
            bond_id: BondId(decode_id(decoder)?),
            expected_bond_version: Version(decoder.u64()?),
            content_ref: ContentRef(decode_id(decoder)?),
            delivery_intent_ref: DeliveryIntentRef(decode_id(decoder)?),
            declaration_digest: MessageDeclarationDigest(
                decoder
                    .bytes()?
                    .try_into()
                    .map_err(|_| WireError::UnexpectedShape)?,
            ),
            message_valid_until: MessageValidityUntil(CanonicalTime(decoder.u64()?)),
        }),
        3 if length == 4 => {
            let bond_id = BondId(decode_id(decoder)?);
            let expected_bond_version = Version(decoder.u64()?);
            let reason = match decoder.u8()? {
                0 => CancellationReason::SenderRequested,
                1 => CancellationReason::AdmissionTimeout,
                2 => CancellationReason::PreAdmissionFailure,
                tag => return Err(WireError::UnknownTag(u32::from(tag))),
            };
            Ok(ProtocolCommand::CancelReservedAttempt {
                bond_id,
                expected_bond_version,
                reason,
            })
        }
        4 if length == 2 => Ok(ProtocolCommand::AcceptRelationship {
            expected_version: Version(decoder.u64()?),
        }),
        5 if length == 2 => Ok(ProtocolCommand::RejectRelationship {
            expected_version: Version(decoder.u64()?),
        }),
        6 if length == 2 => Ok(ProtocolCommand::BlockRelationship {
            expected_version: Version(decoder.u64()?),
        }),
        7 if length == 2 => Ok(ProtocolCommand::UnblockRelationship {
            expected_version: Version(decoder.u64()?),
        }),
        8 if length == 3 => Ok(ProtocolCommand::ExpireBond {
            bond_id: BondId(decode_id(decoder)?),
            expected_bond_version: Version(decoder.u64()?),
        }),
        9 if length == 3 => Ok(ProtocolCommand::ReleasePersistenceReserve {
            reserve_id: PersistenceReserveId(decode_id(decoder)?),
            expected_reserve_version: Version(decoder.u64()?),
        }),
        10 if length == 2 => Ok(ProtocolCommand::RevokeRelationship {
            expected_version: Version(decoder.u64()?),
        }),
        tag => Err(WireError::UnknownTag(u32::from(tag))),
    }
}

fn encode_version_command(
    encoder: &mut Encoder<Vec<u8>>,
    tag: u8,
    version: Version,
) -> Result<(), WireError> {
    encoder.array(2)?.u8(tag)?.u64(version.0)?;
    Ok(())
}

fn encode_terms(encoder: &mut Encoder<Vec<u8>>, terms: &ContactTerms) -> Result<(), WireError> {
    encoder.array(26)?;
    encode_id(encoder, terms.quote_id.0)?;
    encoder.u16(terms.protocol_version.0)?;
    encoder.u64(terms.policy_version.0)?;
    encoder.u16(terms.privacy_profile_version.0)?;
    encoder.u16(terms.retention_policy_version.0)?;
    encode_scoped_ref(
        encoder,
        terms.relationship.derivation_version(),
        terms.relationship.as_bytes(),
    )?;
    encode_scoped_ref(
        encoder,
        terms.attempt_subject.derivation_version(),
        terms.attempt_subject.as_bytes(),
    )?;
    encode_scoped_ref(
        encoder,
        terms.sender_account.derivation_version(),
        terms.sender_account.as_bytes(),
    )?;
    encode_id(encoder, terms.sender.0)?;
    encode_id(encoder, terms.recipient.0)?;
    encode_id(encoder, terms.recipient_provider.0)?;
    encoder.u64(terms.relationship_version.0)?;
    encoder.u64(terms.attempt_version.0)?;
    encoder.u64(terms.processing_charge.minor_units())?;
    encoder.u64(terms.collateral.minor_units())?;
    encoder.u64(terms.persistence.minor_units())?;
    encoder.u32(terms.attempt_level)?;
    encoder.u64(terms.eligibility_time.0)?;
    encoder.u32(terms.unit.0)?;
    encoder.u64(terms.admission_window.0)?;
    encoder.u64(terms.decision_window.0)?;
    encoder.u64(terms.persistence_release_at.0)?;
    encoder.u64(terms.earliest_final_expiry_at.0)?;
    encoder.u64(terms.issued_at.0)?;
    encoder.u64(terms.expires_at.0)?;
    encode_optional_declaration_digest(encoder, terms.declaration_digest)?;
    Ok(())
}

fn decode_terms(decoder: &mut Decoder<'_>) -> Result<ContactTerms, WireError> {
    expect_array(decoder, 26)?;
    let quote_id = QuoteId(decode_id(decoder)?);
    let protocol_version = ProtocolVersion(decoder.u16()?);
    let policy_version = PolicyVersion(decoder.u64()?);
    let privacy_profile_version = PrivacyProfileVersion(decoder.u16()?);
    let retention_policy_version = RetentionPolicyVersion(decoder.u16()?);
    let (relationship_version, relationship_bytes) = decode_scoped_ref(decoder)?;
    let (subject_version, subject_bytes) = decode_scoped_ref(decoder)?;
    let (account_version, account_bytes) = decode_scoped_ref(decoder)?;
    Ok(ContactTerms {
        quote_id,
        protocol_version,
        policy_version,
        privacy_profile_version,
        retention_policy_version,
        relationship: RelationshipRef::new(relationship_version, relationship_bytes),
        attempt_subject: AttemptSubjectRef::new(subject_version, subject_bytes),
        sender_account: LedgerAccountRef::new(account_version, account_bytes),
        sender: ProtocolIdentity(decode_id(decoder)?),
        recipient: ProtocolIdentity(decode_id(decoder)?),
        recipient_provider: ProviderRef(decode_id(decoder)?),
        relationship_version: Version(decoder.u64()?).into(),
        attempt_version: Version(decoder.u64()?).into(),
        processing_charge: Money::from_minor_units(decoder.u64()?),
        collateral: Money::from_minor_units(decoder.u64()?),
        persistence: Money::from_minor_units(decoder.u64()?),
        attempt_level: decoder.u32()?,
        eligibility_time: CanonicalTime(decoder.u64()?),
        unit: SettlementUnit(decoder.u32()?),
        admission_window: Duration(decoder.u64()?),
        decision_window: Duration(decoder.u64()?),
        persistence_release_at: CanonicalTime(decoder.u64()?),
        earliest_final_expiry_at: CanonicalTime(decoder.u64()?),
        issued_at: CanonicalTime(decoder.u64()?),
        expires_at: CanonicalTime(decoder.u64()?),
        declaration_digest: decode_optional_declaration_digest(decoder)?,
    })
}

fn encode_optional_declaration_digest(
    encoder: &mut Encoder<Vec<u8>>,
    digest: Option<MessageDeclarationDigest>,
) -> Result<(), WireError> {
    match digest {
        Some(digest) => {
            encoder.array(1)?.bytes(&digest.0)?;
        }
        None => {
            encoder.array(0)?;
        }
    }
    Ok(())
}

fn decode_optional_declaration_digest(
    decoder: &mut Decoder<'_>,
) -> Result<Option<MessageDeclarationDigest>, WireError> {
    match decoder.array()? {
        Some(0) => Ok(None),
        Some(1) => Ok(Some(MessageDeclarationDigest(
            decoder
                .bytes()?
                .try_into()
                .map_err(|_| WireError::UnexpectedShape)?,
        ))),
        _ => Err(WireError::UnexpectedShape),
    }
}

/// Encodes immutable provider-issued terms for signing.
///
/// # Errors
///
/// Returns an error if a field cannot be represented canonically.
pub fn encode_contact_terms_artifact(terms: &ContactTerms) -> Result<Vec<u8>, WireError> {
    let mut encoder = Encoder::new(Vec::new());
    encoder.array(2)?.str(QUOTE_SIGNING_DOMAIN)?;
    encode_terms(&mut encoder, terms)?;
    Ok(encoder.into_writer())
}

fn encode_scoped_ref(
    encoder: &mut Encoder<Vec<u8>>,
    version: u16,
    bytes: &[u8; 32],
) -> Result<(), WireError> {
    encoder.array(2)?.u16(version)?.bytes(bytes)?;
    Ok(())
}

fn decode_scoped_ref(decoder: &mut Decoder<'_>) -> Result<(u16, [u8; 32]), WireError> {
    expect_array(decoder, 2)?;
    let version = decoder.u16()?;
    let bytes = decoder.bytes()?;
    let bytes = bytes.try_into().map_err(|_| WireError::InvalidIdentifier)?;
    Ok((version, bytes))
}

fn encode_id(encoder: &mut Encoder<Vec<u8>>, value: u128) -> Result<(), WireError> {
    encoder.bytes(&value.to_be_bytes())?;
    Ok(())
}

fn decode_id(decoder: &mut Decoder<'_>) -> Result<u128, WireError> {
    let bytes = decoder.bytes()?;
    let bytes: [u8; 16] = bytes.try_into().map_err(|_| WireError::InvalidIdentifier)?;
    Ok(u128::from_be_bytes(bytes))
}

fn expect_array(decoder: &mut Decoder<'_>, expected: u64) -> Result<(), WireError> {
    match decoder.array()? {
        Some(actual) if actual == expected => Ok(()),
        _ => Err(WireError::UnexpectedShape),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(command: ProtocolCommand) -> CanonicalCommandEnvelope {
        CanonicalCommandEnvelope {
            wire_version: WireVersion(2),
            protocol_version: ProtocolVersion(1),
            deployment_domain: [9; 32],
            intended_provider: ProviderRef(5),
            target: CommandTarget::Relationship(RelationshipRef::from_u128_for_test(44)),
            actor: ActorRef::Recipient(ProtocolIdentity(2)),
            operational_key: OperationalKeyRef(3),
            idempotency_key: IdempotencyKey(4),
            command,
        }
    }

    #[test]
    fn every_command_variant_round_trips_canonically() {
        let commands = [
            ProtocolCommand::IssueContactTerms {
                quote_id: QuoteId(1),
                declaration_digest: Some(MessageDeclarationDigest([1; 32])),
            },
            ProtocolCommand::ReserveAttempt {
                bond_id: BondId(1),
                reserve_id: PersistenceReserveId(2),
                attempt_id: AttemptId(3),
                message_id: MessageId(4),
                terms: Box::new(ContactTerms {
                    quote_id: QuoteId(5),
                    protocol_version: ProtocolVersion(1),
                    policy_version: PolicyVersion(2),
                    privacy_profile_version: PrivacyProfileVersion(1),
                    retention_policy_version: RetentionPolicyVersion(1),
                    relationship: RelationshipRef::from_u128_for_test(6),
                    attempt_subject: AttemptSubjectRef::from_u128_for_test(23),
                    sender_account: LedgerAccountRef::from_u128_for_test(24),
                    sender: ProtocolIdentity(7),
                    recipient: ProtocolIdentity(8),
                    recipient_provider: ProviderRef(9),
                    relationship_version: Version(10).into(),
                    attempt_version: Version(11).into(),
                    processing_charge: Money::from_minor_units(12),
                    collateral: Money::from_minor_units(13),
                    persistence: Money::from_minor_units(14),
                    attempt_level: 15,
                    eligibility_time: CanonicalTime(16),
                    unit: SettlementUnit(17),
                    admission_window: Duration(18),
                    decision_window: Duration(19),
                    persistence_release_at: CanonicalTime(20),
                    issued_at: CanonicalTime(21),
                    expires_at: CanonicalTime(22),
                    earliest_final_expiry_at: CanonicalTime(122),
                    declaration_digest: Some(MessageDeclarationDigest([2; 32])),
                }),
            },
            ProtocolCommand::AdmitAttempt {
                bond_id: BondId(1),
                expected_bond_version: Version(2),
                content_ref: ContentRef(3),
                delivery_intent_ref: DeliveryIntentRef(4),
                declaration_digest: MessageDeclarationDigest([3; 32]),
                message_valid_until: MessageValidityUntil(CanonicalTime(40)),
            },
            ProtocolCommand::CancelReservedAttempt {
                bond_id: BondId(1),
                expected_bond_version: Version(2),
                reason: CancellationReason::AdmissionTimeout,
            },
            ProtocolCommand::AcceptRelationship {
                expected_version: Version(1),
            },
            ProtocolCommand::RejectRelationship {
                expected_version: Version(1),
            },
            ProtocolCommand::BlockRelationship {
                expected_version: Version(1),
            },
            ProtocolCommand::UnblockRelationship {
                expected_version: Version(1),
            },
            ProtocolCommand::ExpireBond {
                bond_id: BondId(1),
                expected_bond_version: Version(2),
            },
            ProtocolCommand::ReleasePersistenceReserve {
                reserve_id: PersistenceReserveId(1),
                expected_reserve_version: Version(2),
            },
            ProtocolCommand::RevokeRelationship {
                expected_version: Version(1),
            },
        ];
        for command in commands {
            let expected = envelope(command);
            let bytes = encode_command_envelope(&expected).unwrap();
            assert_eq!(decode_command_envelope(&bytes).unwrap(), expected);
        }
    }

    #[test]
    fn decoder_rejects_trailing_and_non_preferred_input() {
        let expected = envelope(ProtocolCommand::AcceptRelationship {
            expected_version: Version(1),
        });
        let mut bytes = encode_command_envelope(&expected).unwrap();
        bytes.push(0);
        assert_eq!(
            decode_command_envelope(&bytes),
            Err(WireError::TrailingData)
        );
    }
}
