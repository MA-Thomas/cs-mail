//! One explicit correction of the original fictional accepted samples.
//! Fresh signed consent is retained with its original operation through unknown commits.
use super::*;
use cs_mail_application::relationships::alignment::AcceptanceAlignmentCommand;
use cs_mail_capabilities::{
    DeclarationAuthorityConstraint, LaneGrant, LaneMode, LaneSubject, SignedLaneGrant,
};
use cs_mail_primitives::{IdempotencyKey, LaneId, OriginMode, Version};
use cs_mail_protocol::{PolicySnapshot, ProtocolEvent, RelationshipRequest, RequestLifecycle};
use ed25519_dalek::Signer;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Deserialize, Serialize)]
struct Prepared {
    account: Value,
    persona: Value,
    commands: Vec<AcceptanceAlignmentCommand>,
    preserved: Vec<String>,
}
fn id(request: &RelationshipRequest, domain: &[u8]) -> u128 {
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update(request.terms.relationship.as_bytes());
    hash.update(request.id.0.to_be_bytes());
    let bytes: [u8; 32] = hash.finalize().into();
    u128::from_be_bytes(bytes[..16].try_into().unwrap()) | 1
}
pub(super) fn run(
    db: &mut Client,
    database: &str,
    root: &Path,
    output: &Path,
    profile: &Value,
    simulate_ack_loss: bool,
) -> Result<()> {
    require(
        db.query_one("SELECT pg_try_advisory_lock(20261013,37)", &[])?
            .get::<_, bool>(0),
    )?;
    let prepared: Prepared = if output.exists() {
        serde_json::from_slice(&std::fs::read(output)?)?
    } else {
        let mut commands = Vec::new();
        let mut preserved = Vec::new();
        let now = CanonicalTime(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis()
                .try_into()?,
        );
        for subject in ["maya", "daniel", "elena", "jules"] {
            let peer = read_private(&root.join("participants").join(subject).join("member.json"))?;
            let ids = cs_mail_application::accounts::EnrollmentIdentifiers::for_operation(
                peer["enrollment"]
                    .as_str()
                    .ok_or("missing original enrollment")?,
            );
            let rows = db.query("SELECT aggregate_key,request FROM cs_requests WHERE request#>>'{terms,sender}'=$1 AND request#>>'{terms,recipient}'=$2 AND request#>'{terms,reply_lane}'='null'::jsonb", &[&ids.persona.0.to_string(),&number(&profile["persona"])?.to_string()])?;
            require(rows.len() == 1)?;
            let original: String = rows[0].get(0);
            let Json(request): Json<RelationshipRequest> = rows[0].get(1);
            let RequestLifecycle::Accepted { event, .. } = request.lifecycle else {
                preserved.push(format!("{subject}: original request not accepted"));
                continue;
            };
            let Json(acceptance): Json<ProtocolEvent> = db.query_one("SELECT event FROM cs_protocol_events WHERE aggregate_key=$1 AND event->'reference'=$2", &[&original,&Json(event)])?.get(0);
            let Json(protocol): Json<cs_mail_protocol::policy::ProtocolPolicy> = db.query_one("SELECT policy FROM cs_protocol_policies p JOIN cs_protocol_policy_current c USING(version) WHERE c.singleton", &[])?.get(0);
            let policy: PolicySnapshot = protocol
                .snapshot(cs_mail_protocol::policy::DeploymentTerms {
                    protocol_version: request.terms.protocol_version,
                    provider: request.terms.recipient_provider,
                    unit: request.terms.unit,
                    financial_scope: request.terms.financial.scope,
                    payment_provider_key: request.terms.payment_provider_key,
                })
                .map_err(|_| "invalid current protocol policy")?;
            let expires = acceptance
                .at
                .checked_add(policy.reply_lane.lifetime())
                .ok_or("expiry overflow")?;
            if expires <= now {
                preserved.push(format!("{subject}: original acceptance window expired"));
                continue;
            }
            let row = db.query_one("SELECT aggregate_key,relationship_state FROM cs_relationship_aggregates WHERE relationship_state#>>'{relationship,key,sender}'=$1 AND relationship_state#>>'{relationship,key,recipient}'=$2", &[&request.terms.recipient.0.to_string(), &ids.persona.0.to_string()])?;
            let reverse_key: String = row.get(0);
            let Json(state): Json<Value> = row.get(1);
            let reverse: cs_mail_protocol::Relationship =
                serde_json::from_value(state["relationship"].clone())?;
            let lane = db
                .query_opt(
                    "SELECT lane FROM cs_capability_lanes WHERE aggregate_key=$1",
                    &[&reverse_key],
                )?
                .map(|row| row.get::<_, Json<cs_mail_capabilities::Lane>>(0).0);
            let secret = Zeroizing::new(key(&peer["recipient_secret"])?);
            let verifying = ed25519_dalek::SigningKey::from_bytes(&secret)
                .verifying_key()
                .to_bytes();
            let Json(registry): Json<cs_mail_security::KeyRegistry> = db
                .query_one(
                    "SELECT registry FROM cs_product_accounts WHERE id=$1",
                    &[&number(&peer["account"])?.to_string()],
                )?
                .get(0);
            let recipient_key = registry
                .records()
                .find(|key| {
                    key.verifying_key == verifying
                        && key.actor == cs_mail_protocol::ActorRef::Recipient(ids.persona)
                })
                .ok_or("original recipient key missing")?
                .reference;
            registry.active_verifying_key(
                recipient_key,
                cs_mail_protocol::ActorRef::Recipient(ids.persona),
                now,
            )?;
            let grant = LaneGrant {
                id: lane.as_ref().map_or(
                    LaneId(id(&request, b"cs-mail/development-aligned-lane/v1")),
                    |lane| lane.grant().id,
                ),
                subject: LaneSubject::Native(cs_mail_privacy::ScopedHandle(
                    *reverse.key.reference.as_bytes(),
                )),
                sender: request.terms.recipient,
                recipient: ids.persona,
                protocol_version: policy.protocol_version,
                deployment_domain: request.terms.financial.scope.deployment_domain,
                intended_provider: policy.recipient_provider,
                recipient_operational_key: recipient_key,
                purpose: policy.reply_lane.purpose().clone(),
                origin: Some(OriginMode::HumanInitiated),
                declaration_authority: DeclarationAuthorityConstraint::NativeSender,
                lifetime: policy.reply_lane.lifetime(),
                rate_limit: policy.reply_lane.rate_limit(),
                mode: LaneMode::Expiring,
                issued_at: now,
                not_before: now,
                not_after: expires,
                version: Version(lane.as_ref().map_or(Ok(1), |lane| {
                    lane.grant()
                        .version
                        .0
                        .checked_add(1)
                        .ok_or("version overflow")
                })?),
            };
            let signature = ed25519_dalek::SigningKey::from_bytes(&secret)
                .sign(&grant.signing_bytes()?)
                .to_bytes();
            commands.push(AcceptanceAlignmentCommand {
                source: request.reservation(),
                operation: IdempotencyKey(id(
                    &request,
                    b"cs-mail/development-acceptance-alignment/v1",
                )),
                consent: SignedLaneGrant { grant, signature },
                policy,
            });
        }
        let prepared = Prepared {
            account: profile["account"].clone(),
            persona: profile["persona"].clone(),
            commands,
            preserved,
        };
        artifact::durable(output, &serde_json::to_vec(&prepared)?)?;
        prepared
    };
    require(prepared.account == profile["account"] && prepared.persona == profile["persona"])?;
    let deployment = cs_mail_storage_postgres::PostgresDeployment::connect_existing(database)?;
    for (index, command) in prepared.commands.iter().enumerate() {
        require(command.consent.grant.sender.0 == number(&profile["persona"])?)?;
        let outcome = deployment.align_development_acceptance(command, || {
            CanonicalTime(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
            )
        })?;
        println!(
            "Original accepted request {}: {:?}; replayed={}",
            command.source.request.0, outcome.disposition, outcome.replayed
        );
        if simulate_ack_loss && index == 0 {
            return Err("simulated commit acknowledgement loss; exact signed correction remains prepared for replay".into());
        }
    }
    for reason in prepared.preserved {
        println!("Preserved {reason}");
    }
    Ok(())
}

pub(super) fn check_withdrawal(
    database: &str,
    root: &Path,
    output: &Path,
    profile: &Value,
) -> Result<()> {
    // Conjecture: a signed correction prepared before a later block/unblock cannot
    // resurrect permission, even when the current direction is neutral again.
    let prepared: Prepared = serde_json::from_slice(&std::fs::read(output)?)?;
    require(prepared.account == profile["account"] && prepared.persona == profile["persona"])?;
    let command = prepared
        .commands
        .first()
        .ok_or("missing accepted fixture correction")?;
    let peer = read_private(&root.join("participants/daniel/member.json"))?;
    let ids = cs_mail_application::accounts::EnrollmentIdentifiers::for_operation(
        peer["enrollment"]
            .as_str()
            .ok_or("missing fixture enrollment")?,
    );
    require(command.consent.grant.recipient == ids.persona)?;
    let deployment = cs_mail_storage_postgres::PostgresDeployment::connect_existing(database)?;
    let stored = deployment
        .relationships_to(ids.persona)?
        .into_iter()
        .find(|relation| relation.sender.0 == number(&profile["persona"]).unwrap())
        .ok_or("missing reverse direction")?;
    let engine = deployment
        .existing_relationship(&stored.aggregate_key)?
        .ok_or("missing reverse engine")?;
    let secret = Zeroizing::new(key(&peer["recipient_secret"])?);
    let signer = cs_mail_security::CommandSigner::from_secret_bytes(
        cs_mail_protocol::ActorRef::Recipient(ids.persona),
        command.consent.grant.recipient_operational_key,
        &secret,
    );
    for block in [true, false] {
        let mut operation = [0_u8; 16];
        getrandom::fill(&mut operation).map_err(|_| "test operation randomness unavailable")?;
        let snapshot = engine.snapshot()?;
        let action = if block {
            cs_mail_protocol::ProtocolCommand::BlockRelationship {
                expected_version: snapshot.state.relationship.version.into(),
            }
        } else {
            cs_mail_protocol::ProtocolCommand::UnblockRelationship {
                expected_version: snapshot.state.relationship.version.into(),
            }
        };
        let signed = signer.sign(
            cs_mail_security::SigningScope {
                deployment_domain: command.consent.grant.deployment_domain,
                intended_provider: command.policy.recipient_provider,
                relationship: snapshot.state.relationship.key.reference,
            },
            command.policy.protocol_version,
            IdempotencyKey(u128::from_be_bytes(operation) | 1),
            action,
        )?;
        let handle = engine.receive_signed(
            &signed,
            command.consent.grant.deployment_domain,
            || {
                CanonicalTime(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64,
                )
            },
            command.policy.clone(),
        )?;
        require(!matches!(
            engine.process_received(&handle)?,
            cs_mail_application::receipts::ReceivedOutcome::Refused(_)
        ))?;
    }
    let at = || {
        CanonicalTime(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        )
    };
    let outcome = deployment.align_development_acceptance(command, at)?;
    require(
        outcome.disposition
            == cs_mail_application::relationships::alignment::AlignmentDisposition::Withdrawn
            && outcome.position.is_none(),
    )?;
    let replay = deployment.align_development_acceptance(command, at)?;
    require(replay.replayed && replay.disposition == outcome.disposition)?;
    require(
        matches!(
            engine.snapshot()?.state.relationship.state,
            cs_mail_protocol::RelationshipState::Unknown
                | cs_mail_protocol::RelationshipState::Rejected
        ) && engine.lane()?.is_none(),
    )?;
    let mut altered = command.clone();
    altered.consent.grant.not_after.0 += 1;
    require(matches!(
        deployment.align_development_acceptance(&altered, at),
        Err(cs_mail_storage_postgres::StorageError::DuplicateConflict)
    ))?;
    println!(
        "Later signed block/unblock preserved: prepared alignment and exact replay grant no lane; altered retry refused."
    );
    Ok(())
}
