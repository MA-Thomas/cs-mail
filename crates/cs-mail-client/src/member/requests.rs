//! Paid native request formation. Quote review and charge authorization are distinct
//! steps. Every accepted command and staged document is journaled before transmission.
use super::{
    Error, MemberClient, State,
    storage::{self, Secrets},
};
use crate::correspondence::{EndpointKeys, compose, prepare};
use cs_mail_application::{
    members::{RelationshipHandle, RequestView},
    requests::documents::{RequestDocument, SignedRequestDocument, StageReceipt},
};
use cs_mail_content::{ContentBinding, ContentKeyCertificate, content_key_certificate_digest};
use cs_mail_correspondence::{ConversationId, DocumentPart};
use cs_mail_finance::CaptureStatus;
use cs_mail_primitives::{
    ContentKeyVersion, ContentRef, DeclarationAuthority, DeclaredPurpose, DeliveryIntentRef,
    IdempotencyKey, KnownPurpose, MessageDeclarations, MessageId, MessageValidityUntil,
    OriginDeclaration, OriginMode, ProtocolIdentity, QuoteId, RequestClassId, RequestId, Version,
    WireVersion,
};
use cs_mail_protocol::{
    ActorRef, CancellationReason, ProtocolCommand, RequestLifecycle, RequestTerms, TermsOutcome,
};
use cs_mail_security::{
    CommandSigner, ReceiptKind, SignedCommandBytes, SignedContactTerms, SignedReceipt, SigningScope,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct RequestJourney {
    funding: Option<CaptureStatus>,
    other: ProtocolIdentity,
    handle: RelationshipHandle,
    request: RequestId,
    kind: RequestKind,
    stage: RequestStage,
}
#[derive(Clone, Copy)]
struct DocumentContext {
    other: ProtocolIdentity,
    handle: RelationshipHandle,
    request: RequestId,
    conversation: ConversationId,
}
#[derive(Clone, Deserialize, Serialize)]
enum RequestKind {
    Initial,
    Followup { policy: Version },
}
#[derive(Clone, Deserialize, Serialize)]
enum RequestStage {
    Quoting {
        class: RequestClassId,
        quote: QuoteId,
        command: SignedCommandBytes,
    },
    Quoted(SignedContactTerms),
    Preparing {
        terms: RequestTerms,
        document: Box<SignedRequestDocument>,
        work: PreparationWork,
    },
}
#[derive(Clone, Deserialize, Serialize)]
enum PreparationWork {
    Create(SignedCommandBytes),
    Funding,
    Stage,
    Submit(SignedCommandBytes),
    Cancel(SignedCommandBytes),
    Stopped,
}
#[derive(Clone, Deserialize, Serialize)]
pub(super) struct RequestResult {
    capture: Option<CaptureStatus>,
    other: ProtocolIdentity,
    request: RequestId,
    evidence: RequestEvidence,
    pub status: RequestStatus,
}
#[derive(Clone, Deserialize, Serialize)]
enum RequestEvidence {
    Receipt(Box<SignedReceipt>),
    Lifecycle(RequestLifecycle),
}
enum RequestAdvance {
    Wait,
    Funding(CaptureStatus),
    Next(Box<RequestStage>),
    Complete(RequestResult),
    ObservedChange(RequestResult),
    Refused {
        receipt: Box<SignedReceipt>,
        after: Option<Box<RequestStage>>,
    },
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum RequestStatus {
    Quoting,
    CommunicationAvailable,
    ExistingRequest,
    Review,
    Funding,
    FundingVoided,
    FundingReversed,
    Sending,
    Stopped,
    Submitted,
    FollowupSent,
    Cancelling,
    Cancelled,
    Refused,
    Changed,
}
#[derive(Clone, Debug)]
pub struct RequestProgress {
    pub capture: Option<CaptureStatus>,
    pub other: String,
    pub request: String,
    pub status: RequestStatus,
    pub quote: Option<RequestQuote>,
}
#[derive(Clone, Debug)]
pub struct RequestQuote {
    pub class_name: String,
    pub processing: u64,
    pub collateral: u64,
    pub total: u64,
    pub expires_at: u64,
    pub submission_window_ms: u64,
    pub decision_window_ms: u64,
    pub reply_lifetime_ms: u64,
    pub reply_max_messages: u32,
    pub reply_interval_ms: u64,
    pub reply_purpose: String,
}
#[derive(Deserialize)]
struct Answer {
    outcome: cs_mail_application::receipts::ReceivedOutcome,
    receipt: SignedReceipt,
    signed_terms: Option<SignedContactTerms>,
}

fn identifier() -> Result<u128, Error> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| Error::Unavailable)?;
    Ok(u128::from_be_bytes(bytes) | 1)
}
fn declarations(state: &State) -> MessageDeclarations {
    MessageDeclarations {
        purpose: DeclaredPurpose::Known(KnownPurpose::Personal),
        origin: OriginDeclaration {
            mode: OriginMode::HumanInitiated,
            authority: DeclarationAuthority::NativeSender(state.profile.sender_key),
        },
        payload_schema: None,
    }
}
impl RequestJourney {
    fn progress(&self) -> Result<RequestProgress, Error> {
        let (status, quote) = match &self.stage {
            RequestStage::Quoting { .. } => (RequestStatus::Quoting, None),
            RequestStage::Quoted(issued) => {
                let terms = &issued.terms;
                let reply = terms.reply_lane.agreed().ok_or(Error::InvalidData)?;
                (
                    RequestStatus::Review,
                    Some(RequestQuote {
                        class_name: terms.selected_class.description().into(),
                        processing: terms.processing_charge.minor_units(),
                        collateral: terms.collateral.minor_units(),
                        total: terms
                            .processing_charge
                            .checked_add(terms.collateral)
                            .ok_or(Error::InvalidData)?
                            .minor_units(),
                        expires_at: terms.expires_at.0,
                        submission_window_ms: terms.submission_window.0,
                        decision_window_ms: terms.decision_window.0,
                        reply_lifetime_ms: reply.lifetime().0,
                        reply_max_messages: reply.rate_limit().max_messages,
                        reply_interval_ms: reply.rate_limit().interval.0,
                        reply_purpose: match reply.purpose() {
                            DeclaredPurpose::Known(KnownPurpose::Personal) => "personal".into(),
                            other => format!("{other:?}"),
                        },
                    }),
                )
            }
            RequestStage::Preparing { work, .. } => (
                match work {
                    PreparationWork::Create(_) | PreparationWork::Funding => match self.funding {
                        Some(CaptureStatus::Voided) => RequestStatus::FundingVoided,
                        Some(CaptureStatus::Reversed) => RequestStatus::FundingReversed,
                        _ => RequestStatus::Funding,
                    },
                    PreparationWork::Stage | PreparationWork::Submit(_) => RequestStatus::Sending,
                    PreparationWork::Cancel(_) => RequestStatus::Cancelling,
                    PreparationWork::Stopped => RequestStatus::Stopped,
                },
                None,
            ),
        };
        Ok(RequestProgress {
            capture: self.funding,
            other: self.other.0.to_string(),
            request: self.request.0.to_string(),
            status,
            quote,
        })
    }
}
impl MemberClient {
    fn sender_command(
        state: &State,
        keys: &Secrets,
        handle: RelationshipHandle,
        command: ProtocolCommand,
    ) -> Result<SignedCommandBytes, Error> {
        CommandSigner::from_secret_bytes(
            ActorRef::Sender(state.profile.persona),
            state.profile.sender_key,
            &keys.device,
        )
        .sign(
            SigningScope {
                deployment_domain: state.profile.deployment.scope.deployment_domain,
                intended_provider: state.profile.deployment.scope.provider,
                relationship: handle.relationship,
            },
            state.profile.deployment.policy.protocol_version,
            IdempotencyKey(identifier()?),
            command,
        )
        .map_err(|_| Error::InvalidData)
    }
    fn request_answer(
        &self,
        state: &State,
        plan: &RequestJourney,
        signed: &SignedCommandBytes,
        expected: ReceiptKind,
    ) -> Result<Answer, Error> {
        let answer: Answer = self.call(
            &state.profile.server,
            &format!(
                "v1/relationships/{}/{}/commands",
                state.profile.persona.0, plan.other.0
            ),
            Some(&serde_json::to_vec(signed).map_err(|_| Error::InvalidData)?),
        )?;
        let receipt = &answer.receipt;
        answer
            .outcome
            .verify(receipt, &state.profile.deployment.provider_key)
            .map_err(|_| Error::InvalidData)?;
        if receipt.payload.command_digest.0 != <[u8; 32]>::from(Sha256::digest(&signed.payload))
            || receipt.payload.relationship != plan.handle.relationship
            || receipt.payload.deployment_domain != state.profile.deployment.scope.deployment_domain
            || receipt.payload.provider != state.profile.deployment.scope.provider
            || receipt.payload.protocol_version != state.profile.deployment.policy.protocol_version
            || !matches!(
                receipt.payload.kind,
                ReceiptKind::Refused | ReceiptKind::NoChange
            ) && receipt.payload.kind != expected
        {
            return Err(Error::InvalidData);
        }
        Ok(answer)
    }
    /// # Errors
    /// Pending different work, unavailable authority, quote refusal or unknown transport outcome.
    pub fn quote_request(
        &self,
        other: &str,
        class: RequestClassId,
    ) -> Result<RequestProgress, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let mut state = storage::open(self.store.as_ref(), &keys)?;
        super::require_idle(&state)?;
        let other = super::parse_sender(other)?;
        self.synchronize(&mut state, &keys)?;
        let (handle, _) = self.sending(&state.profile, &keys, other)?;
        let quote = QuoteId(identifier()?);
        let declaration_digest = cs_mail_content::message_declaration_digest(&declarations(&state))
            .map_err(|_| Error::InvalidData)?;
        let command = Self::sender_command(
            &state,
            &keys,
            handle,
            ProtocolCommand::IssueRequestTerms {
                class_id: class,
                quote_id: quote,
                declaration_digest: Some(declaration_digest),
            },
        )?;
        state.requesting = Some(RequestJourney {
            funding: None,
            other,
            handle,
            request: RequestId(identifier()?),
            kind: RequestKind::Initial,
            stage: RequestStage::Quoting {
                class,
                quote,
                command,
            },
        });
        state.last_request = None;
        storage::save(self.store.as_ref(), &keys, &state)?;
        self.advance_request(&mut state, &keys)?;
        Self::request_progress_of(&state)?.ok_or(Error::InvalidData)
    }
    /// Reviews the exact issued quote identified by the request plan. Nothing can
    /// silently substitute newly issued terms after the user's charge authorization.
    /// # Errors
    /// Changed plan, expired quote, pending work, unavailable custody or refused/unknown outcome.
    pub fn authorize_request(
        &self,
        request: RequestId,
        text: &str,
    ) -> Result<RequestProgress, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let mut state = storage::open(self.store.as_ref(), &keys)?;
        let plan = state.requesting.clone().ok_or(Error::InvalidData)?;
        let RequestStage::Quoted(issued) = &plan.stage else {
            return Err(Error::RequestPending);
        };
        issued
            .verify(&state.profile.deployment.provider_key)
            .map_err(|_| Error::InvalidData)?;
        if plan.request != request
            || issued.terms.sender != state.profile.persona
            || issued.terms.recipient != plan.other
            || issued.terms.relationship != plan.handle.relationship
        {
            return Err(Error::InvalidData);
        }
        if (self.clock)() >= issued.terms.expires_at {
            return Err(Error::QuoteExpired);
        }
        let document = self.request_document(
            &state,
            &keys,
            DocumentContext {
                other: plan.other,
                handle: plan.handle,
                request: plan.request,
                conversation: ConversationId::new(identifier()?).map_err(|_| Error::InvalidData)?,
            },
            text,
        )?;
        let account = self.account_view(&state.profile, &keys)?;
        let command = Self::sender_command(
            &state,
            &keys,
            plan.handle,
            ProtocolCommand::CreateRequest {
                payment_method: account.payment_method,
                request_id: request,
                message_id: document.document.package().manifest.message(),
                terms: Box::new(issued.terms.clone()),
            },
        )?;
        state.requesting.as_mut().ok_or(Error::InvalidData)?.stage = RequestStage::Preparing {
            terms: issued.terms.clone(),
            document: Box::new(document),
            work: PreparationWork::Create(command),
        };
        storage::save(self.store.as_ref(), &keys, &state)?;
        self.advance_request(&mut state, &keys)?;
        Self::request_progress_of(&state)?.ok_or(Error::InvalidData)
    }
    fn request_document(
        &self,
        state: &State,
        keys: &Secrets,
        context: DocumentContext,
        text: &str,
    ) -> Result<SignedRequestDocument, Error> {
        if text.trim().is_empty() || text.len() > super::correspondence::MAX_TEXT_BYTES {
            return Err(Error::InvalidData);
        }
        let now = (self.clock)();
        let expires = now
            .checked_add(super::correspondence::MESSAGE_LIFETIME)
            .ok_or(Error::InvalidData)?;
        let (secret, public) = super::correspondence::own_keys(&state.profile, keys)?;
        let signer = CommandSigner::from_secret_bytes(
            ActorRef::Sender(state.profile.persona),
            state.profile.sender_key,
            &keys.device,
        );
        let certificate = signer
            .sign_content_key_certificate(ContentKeyCertificate {
                wire_version: WireVersion(1),
                protocol_version: state.profile.deployment.policy.protocol_version,
                deployment_domain: state.profile.deployment.scope.deployment_domain,
                intended_provider: state.profile.deployment.scope.provider,
                relationship: context.handle.relationship,
                owner: state.profile.persona,
                operational_key: state.profile.sender_key,
                content_key_version: ContentKeyVersion(1),
                key: public,
                valid_from: now,
                valid_until: expires,
            })
            .map_err(|_| Error::InvalidData)?;
        let content_ref = ContentRef(identifier()?);
        let package = prepare(
            &EndpointKeys {
                owner: state.profile.persona,
                secret: &secret,
                public,
            },
            &compose(vec![DocumentPart::Text(text.into())]).map_err(|_| Error::InvalidData)?,
            context.conversation,
            self.endpoint_key(&state.profile, keys, context.other)?,
            state.profile.deployment.custody_key,
            ContentBinding {
                wire_version: WireVersion(1),
                content_ref,
                message_id: MessageId(identifier()?),
                sender: state.profile.persona,
                recipient: context.other,
                protocol_version: state.profile.deployment.policy.protocol_version,
                relationship: context.handle.relationship,
                content_scope: context.handle.content_scope,
                sender_certificate: content_key_certificate_digest(&certificate.certificate),
                declarations: declarations(state),
                message_valid_until: MessageValidityUntil(expires),
                capability: None,
            },
            (now, expires),
        )
        .map_err(|_| Error::InvalidData)?;
        SignedRequestDocument {
            scope: SigningScope {
                deployment_domain: state.profile.deployment.scope.deployment_domain,
                intended_provider: state.profile.deployment.scope.provider,
                relationship: context.handle.relationship,
            },
            key: state.profile.sender_key,
            document: RequestDocument::new(context.request, content_ref, package)
                .map_err(|_| Error::InvalidData)?,
            certificate,
            signature: Vec::new(),
        }
        .sign(&keys.device)
        .map_err(|_| Error::InvalidData)
    }
    fn outbound_request(
        &self,
        state: &State,
        keys: &Secrets,
        plan: &RequestJourney,
    ) -> Result<(RequestView, Version), Error> {
        let (_, view) = self.sending(&state.profile, keys, plan.other)?;
        let view = view.ok_or(Error::InvalidData)?;
        let request = view
            .requests
            .iter()
            .find(|request| request.id == plan.request)
            .cloned()
            .ok_or(Error::InvalidData)?;
        Ok((request, view.followup_policy.version))
    }
    fn finish_request(
        state: &mut State,
        plan: &RequestJourney,
        status: RequestStatus,
        receipt: SignedReceipt,
    ) {
        state.last_request = Some(RequestResult {
            capture: plan.funding,
            other: plan.other,
            request: plan.request,
            status,
            evidence: RequestEvidence::Receipt(Box::new(receipt)),
        });
        state.requesting = None;
    }
    fn record_changed_request(
        state: &mut State,
        plan: &RequestJourney,
        request: &RequestView,
    ) -> bool {
        if matches!(request.lifecycle, RequestLifecycle::PreparingSubmission(_)) {
            return false;
        }
        state.last_request = Some(RequestResult {
            capture: request.money.as_ref().map(|money| money.capture),
            other: plan.other,
            request: plan.request,
            status: RequestStatus::Changed,
            evidence: RequestEvidence::Lifecycle(request.lifecycle),
        });
        state.requesting = None;
        true
    }
    /// Advances only already authorized work. A quote always stops for review;
    /// pending funding returns control immediately rather than polling on the UI's behalf.
    pub(super) fn advance_request(&self, state: &mut State, keys: &Secrets) -> Result<(), Error> {
        for _ in 0..6 {
            let Some(plan) = state.requesting.clone() else {
                return Ok(());
            };
            let mut refused = false;
            match self.next_request_step(state, keys, &plan)? {
                RequestAdvance::Wait => return Ok(()),
                RequestAdvance::Funding(capture) => {
                    state.requesting.as_mut().ok_or(Error::InvalidData)?.funding = Some(capture);
                    storage::save(self.store.as_ref(), keys, state)?;
                    return Ok(());
                }
                RequestAdvance::Next(next_phase) => {
                    state.requesting.as_mut().ok_or(Error::InvalidData)?.stage = *next_phase;
                }
                RequestAdvance::Complete(result) => {
                    state.last_request = Some(result);
                    state.requesting = None;
                }
                RequestAdvance::ObservedChange(result) => {
                    state.last_request = Some(result);
                    state.requesting = None;
                    refused = true;
                }
                RequestAdvance::Refused { receipt, after } => {
                    state.last_request = Some(Self::request_result(
                        &plan,
                        RequestStatus::Refused,
                        *receipt,
                    ));
                    if let Some(after) = after {
                        state.requesting.as_mut().ok_or(Error::InvalidData)?.stage = *after;
                    } else {
                        state.requesting = None;
                    }
                    refused = true;
                }
            }
            storage::save(self.store.as_ref(), keys, state)?;
            if refused {
                return Err(Error::RequestRefused);
            }
        }
        Ok(())
    }
    fn request_result(
        plan: &RequestJourney,
        status: RequestStatus,
        receipt: SignedReceipt,
    ) -> RequestResult {
        RequestResult {
            capture: plan.funding,
            other: plan.other,
            request: plan.request,
            status,
            evidence: RequestEvidence::Receipt(Box::new(receipt)),
        }
    }
    fn observed_change(plan: &RequestJourney, request: &RequestView) -> RequestResult {
        RequestResult {
            capture: request.money.as_ref().map(|money| money.capture),
            other: plan.other,
            request: plan.request,
            status: RequestStatus::Changed,
            evidence: RequestEvidence::Lifecycle(request.lifecycle),
        }
    }
    fn next_request_step(
        &self,
        state: &State,
        keys: &Secrets,
        plan: &RequestJourney,
    ) -> Result<RequestAdvance, Error> {
        match &plan.stage {
            RequestStage::Quoted(_) => Ok(RequestAdvance::Wait),
            RequestStage::Quoting {
                class,
                quote,
                command,
            } => {
                let answer =
                    self.request_answer(state, plan, command, ReceiptKind::ContactTermsIssued)?;
                if answer.receipt.payload.kind == ReceiptKind::Refused {
                    return Ok(RequestAdvance::Refused {
                        receipt: Box::new(answer.receipt),
                        after: None,
                    });
                }
                // Interpret the domain owner's recorded terms result. Absence of a
                // payable quote is distinct from refusal and never authorizes a charge.
                let cs_mail_application::receipts::ReceivedOutcome::Protocol(outcome) =
                    &answer.outcome
                else {
                    return Err(Error::InvalidData);
                };
                let outcome = outcome
                    .transition
                    .terms_outcome
                    .clone()
                    .ok_or(Error::InvalidData)?;
                let raw_terms = match outcome {
                    TermsOutcome::NoChargeRequired => {
                        return Ok(RequestAdvance::Complete(Self::request_result(
                            plan,
                            RequestStatus::CommunicationAvailable,
                            answer.receipt,
                        )));
                    }
                    TermsOutcome::ExistingRequest(existing) => {
                        let mut result = Self::request_result(
                            plan,
                            RequestStatus::ExistingRequest,
                            answer.receipt,
                        );
                        result.request = existing;
                        return Ok(RequestAdvance::Complete(result));
                    }
                    TermsOutcome::ChargeRequired(terms) => terms,
                };
                let issued = answer.signed_terms.ok_or(Error::InvalidData)?;
                issued
                    .verify(&state.profile.deployment.provider_key)
                    .map_err(|_| Error::InvalidData)?;
                let terms = &issued.terms;
                if terms != raw_terms.as_ref()
                    || terms.sender != state.profile.persona
                    || terms.recipient != plan.other
                    || terms.relationship != plan.handle.relationship
                    || terms.quote_id != *quote
                    || terms.selected_class.id() != *class
                    || terms.recipient_provider != state.profile.deployment.scope.provider
                    || terms.protocol_version != state.profile.deployment.policy.protocol_version
                    || terms.declaration_digest
                        != Some(
                            cs_mail_content::message_declaration_digest(&declarations(state))
                                .map_err(|_| Error::InvalidData)?,
                        )
                    || terms.reply_lane.agreed().is_none()
                {
                    return Err(Error::InvalidData);
                }
                Ok(RequestAdvance::Next(Box::new(RequestStage::Quoted(issued))))
            }
            RequestStage::Preparing {
                terms,
                document,
                work,
            } => self.next_preparation_step(state, keys, plan, terms, document, work),
        }
    }
    fn preparation(
        terms: &RequestTerms,
        document: &SignedRequestDocument,
        work: PreparationWork,
    ) -> Box<RequestStage> {
        Box::new(RequestStage::Preparing {
            terms: terms.clone(),
            document: Box::new(document.clone()),
            work,
        })
    }
    fn next_preparation_step(
        &self,
        state: &State,
        keys: &Secrets,
        plan: &RequestJourney,
        terms: &RequestTerms,
        document: &SignedRequestDocument,
        work: &PreparationWork,
    ) -> Result<RequestAdvance, Error> {
        match work {
            PreparationWork::Stopped | PreparationWork::Funding => {
                let (request, _) = self.outbound_request(state, keys, plan)?;
                if !matches!(request.lifecycle, RequestLifecycle::PreparingSubmission(_)) {
                    return Ok(RequestAdvance::ObservedChange(Self::observed_change(
                        plan, &request,
                    )));
                }
                if matches!(work, PreparationWork::Stopped) {
                    return Ok(RequestAdvance::Wait);
                }
                let money = request.money.ok_or(Error::InvalidData)?;
                if money.capture != CaptureStatus::FundingFinalized {
                    return Ok(RequestAdvance::Funding(money.capture));
                }
                Ok(RequestAdvance::Next(Self::preparation(
                    terms,
                    document,
                    PreparationWork::Stage,
                )))
            }
            PreparationWork::Stage => {
                let (request, _) = self.outbound_request(state, keys, plan)?;
                if matches!(plan.kind, RequestKind::Initial)
                    && !matches!(request.lifecycle, RequestLifecycle::PreparingSubmission(_))
                    || matches!(plan.kind, RequestKind::Followup { .. })
                        && !request.lifecycle.is_awaiting_recipient_decision()
                {
                    return Ok(RequestAdvance::ObservedChange(Self::observed_change(
                        plan, &request,
                    )));
                }
                let command = self.stage_request_document(state, keys, plan, document)?;
                Ok(RequestAdvance::Next(Self::preparation(
                    terms,
                    document,
                    PreparationWork::Submit(command),
                )))
            }
            PreparationWork::Create(command)
            | PreparationWork::Submit(command)
            | PreparationWork::Cancel(command) => {
                let expected = match work {
                    PreparationWork::Create(_) => ReceiptKind::ReservationCommitted,
                    PreparationWork::Cancel(_) => ReceiptKind::SettlementCommitted,
                    _ => ReceiptKind::AdmissionCommitted,
                };
                let answer = self.request_answer(state, plan, command, expected)?;
                if answer.receipt.payload.kind == ReceiptKind::Refused {
                    let after = if matches!(work, PreparationWork::Create(_))
                        || matches!(plan.kind, RequestKind::Followup { .. })
                    {
                        None
                    } else {
                        Some(Self::preparation(terms, document, PreparationWork::Stopped))
                    };
                    return Ok(RequestAdvance::Refused {
                        receipt: Box::new(answer.receipt),
                        after,
                    });
                }
                if matches!(work, PreparationWork::Create(_)) {
                    return Ok(RequestAdvance::Next(Self::preparation(
                        terms,
                        document,
                        PreparationWork::Funding,
                    )));
                }
                let status = if matches!(work, PreparationWork::Cancel(_)) {
                    RequestStatus::Cancelled
                } else if matches!(plan.kind, RequestKind::Followup { .. }) {
                    RequestStatus::FollowupSent
                } else {
                    RequestStatus::Submitted
                };
                Ok(RequestAdvance::Complete(Self::request_result(
                    plan,
                    status,
                    answer.receipt,
                )))
            }
        }
    }
    fn stage_request_document(
        &self,
        state: &State,
        keys: &Secrets,
        plan: &RequestJourney,
        document: &SignedRequestDocument,
    ) -> Result<SignedCommandBytes, Error> {
        let receipt: StageReceipt = self.call(
            &state.profile.server,
            &format!(
                "v1/relationships/{}/{}/content",
                state.profile.persona.0, plan.other.0
            ),
            Some(&serde_json::to_vec(document).map_err(|_| Error::InvalidData)?),
        )?;
        if receipt.digest != document.digest().map_err(|_| Error::InvalidData)? {
            return Err(Error::InvalidData);
        }
        let (request, _) = self.outbound_request(state, keys, plan)?;
        let package = document.document.package();
        let binding = &package.recipient_copy.binding;
        let declaration_digest = cs_mail_content::message_declaration_digest(&binding.declarations)
            .map_err(|_| Error::InvalidData)?;
        let command = match plan.kind {
            RequestKind::Initial => ProtocolCommand::SubmitRequestToRecipient {
                request_id: plan.request,
                expected_request_version: request.version.into(),
                content_ref: document.document.content(),
                delivery_intent_ref: DeliveryIntentRef(identifier()?),
                declaration_digest,
                message_valid_until: binding.message_valid_until,
            },
            RequestKind::Followup { policy } => ProtocolCommand::AdmitFollowup {
                request_id: plan.request,
                message_id: package.manifest.message(),
                content_ref: document.document.content(),
                delivery_intent_ref: DeliveryIntentRef(identifier()?),
                declaration_digest,
                message_valid_until: binding.message_valid_until,
                expected_policy_version: policy,
            },
        };
        Self::sender_command(state, keys, plan.handle, command)
    }
    pub(super) fn request_progress_of(state: &State) -> Result<Option<RequestProgress>, Error> {
        if let Some(plan) = &state.requesting {
            return plan.progress().map(Some);
        }
        Ok(state.last_request.as_ref().map(|result| RequestProgress {
            capture: result.capture,
            other: result.other.0.to_string(),
            request: result.request.0.to_string(),
            status: result.status,
            quote: None,
        }))
    }
    /// # Errors
    /// Unavailable custody or malformed encrypted device progress.
    pub fn request_progress(&self) -> Result<Option<RequestProgress>, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        Self::request_progress_of(&storage::open(self.store.as_ref(), &keys)?)
    }
    /// # Errors
    /// A submission's original operation remains unknown, the request is terminal,
    /// or current custody/authority refuses cancellation.
    pub fn cancel_request(&self) -> Result<(), Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let mut state = storage::open(self.store.as_ref(), &keys)?;
        let plan = state.requesting.clone().ok_or(Error::InvalidData)?;
        if matches!(
            plan.stage,
            RequestStage::Quoting { .. } | RequestStage::Quoted(_)
        ) {
            state.requesting = None;
            storage::save(self.store.as_ref(), &keys, &state)?;
            return Ok(());
        }
        if self.reconcile_before_cancel(&mut state, &keys, &plan)? {
            return Ok(());
        }
        if matches!(plan.kind, RequestKind::Followup { .. }) {
            state.requesting = None;
            storage::save(self.store.as_ref(), &keys, &state)?;
            return Ok(());
        }
        let (request, _) = self.outbound_request(&state, &keys, &plan)?;
        if Self::record_changed_request(&mut state, &plan, &request) {
            storage::save(self.store.as_ref(), &keys, &state)?;
            return Ok(());
        }
        let command = Self::sender_command(
            &state,
            &keys,
            plan.handle,
            ProtocolCommand::CancelRequestSubmission {
                request_id: plan.request,
                expected_request_version: request.version.into(),
                reason: CancellationReason::SenderRequested,
            },
        )?;
        let RequestStage::Preparing {
            terms, document, ..
        } = plan.stage
        else {
            return Err(Error::InvalidData);
        };
        state.requesting.as_mut().ok_or(Error::InvalidData)?.stage = RequestStage::Preparing {
            terms,
            document,
            work: PreparationWork::Cancel(command),
        };
        storage::save(self.store.as_ref(), &keys, &state)?;
        self.advance_request(&mut state, &keys)
    }
    fn reconcile_before_cancel(
        &self,
        state: &mut State,
        keys: &Secrets,
        plan: &RequestJourney,
    ) -> Result<bool, Error> {
        if matches!(
            &plan.stage,
            RequestStage::Preparing {
                work: PreparationWork::Create(_)
                    | PreparationWork::Submit(_)
                    | PreparationWork::Cancel(_),
                ..
            }
        ) {
            // First recover the accepted command, but do not advance funding to delivery.
            let RequestStage::Preparing {
                terms,
                document,
                work,
            } = &plan.stage
            else {
                return Err(Error::InvalidData);
            };
            let (PreparationWork::Create(command)
            | PreparationWork::Submit(command)
            | PreparationWork::Cancel(command)) = work
            else {
                return Err(Error::InvalidData);
            };
            let expected = match work {
                PreparationWork::Create(_) => ReceiptKind::ReservationCommitted,
                PreparationWork::Cancel(_) => ReceiptKind::SettlementCommitted,
                _ => ReceiptKind::AdmissionCommitted,
            };
            let answer = self.request_answer(state, plan, command, expected)?;
            if matches!(
                work,
                PreparationWork::Submit(_) | PreparationWork::Cancel(_)
            ) && answer.receipt.payload.kind != ReceiptKind::Refused
            {
                Self::finish_request(
                    state,
                    plan,
                    if matches!(work, PreparationWork::Cancel(_)) {
                        RequestStatus::Cancelled
                    } else if matches!(plan.kind, RequestKind::Followup { .. }) {
                        RequestStatus::FollowupSent
                    } else {
                        RequestStatus::Submitted
                    },
                    answer.receipt,
                );
                storage::save(self.store.as_ref(), keys, state)?;
                return Ok(true);
            }
            if matches!(work, PreparationWork::Create(_))
                && answer.receipt.payload.kind == ReceiptKind::Refused
            {
                Self::finish_request(state, plan, RequestStatus::Refused, answer.receipt);
                storage::save(self.store.as_ref(), keys, state)?;
                return Ok(true);
            }
            state.requesting.as_mut().ok_or(Error::InvalidData)?.stage = RequestStage::Preparing {
                terms: terms.clone(),
                document: document.clone(),
                work: PreparationWork::Stopped,
            };
            storage::save(self.store.as_ref(), keys, state)?;
        }
        Ok(false)
    }
    /// # Errors
    /// Unresolved work, absent submitted request, unavailable custody or refused follow-up.
    pub fn send_followup(
        &self,
        other: &str,
        request_id: RequestId,
        text: &str,
    ) -> Result<RequestProgress, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let mut state = storage::open(self.store.as_ref(), &keys)?;
        super::require_idle(&state)?;
        self.synchronize(&mut state, &keys)?;
        let other = super::parse_sender(other)?;
        let (handle, view) = self.sending(&state.profile, &keys, other)?;
        let view = view.ok_or(Error::InvalidData)?;
        let request = view
            .requests
            .iter()
            .find(|r| r.id == request_id)
            .ok_or(Error::InvalidData)?;
        let conversation = super::correspondence::request_conversation(&state, other, request_id)
            .ok_or(Error::InvalidData)?;
        let document = self.request_document(
            &state,
            &keys,
            DocumentContext {
                other,
                handle,
                request: request_id,
                conversation,
            },
            text,
        )?;
        let plan = RequestJourney {
            funding: None,
            other,
            handle,
            request: request_id,
            kind: RequestKind::Followup {
                policy: view.followup_policy.version,
            },
            stage: RequestStage::Preparing {
                terms: request.terms.clone(),
                document: Box::new(document),
                work: PreparationWork::Stage,
            },
        };
        state.requesting = Some(plan);
        storage::save(self.store.as_ref(), &keys, &state)?;
        self.advance_request(&mut state, &keys)?;
        Self::request_progress_of(&state)?.ok_or(Error::InvalidData)
    }
}
