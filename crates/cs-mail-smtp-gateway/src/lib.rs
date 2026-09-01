//! Bounded SMTP authentication using Stalwart Labs' `mail-auth` implementation.
//!
//! This crate is the only cs-mail layer that knows the concrete DMARC library.
//! It converts SMTP session facts and an RFC5322 message into the small,
//! versioned evidence model consumed by express-lane admission.

use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use cs_mail_adapters::{
    DmarcAlignment, DmarcError, DmarcErrorKind, DmarcPass, DmarcVerificationFuture, DmarcVerifier,
    DomainIdentity, SmtpAuthenticationRequest, VerifiedDomain,
};
use cs_mail_primitives::Duration;
use mail_auth::dmarc::Alignment as MailAlignment;
use mail_auth::dmarc::verify::DmarcParameters;
use mail_auth::spf::verify::SpfParameters;
use mail_auth::{
    AuthenticatedMessage, DkimResult, DmarcOutput, DmarcResult, MessageAuthenticator, SpfResult,
};

/// Provider-side limits applied before any DNS-backed cryptographic work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DmarcLimits {
    pub max_message_bytes: usize,
    pub max_header_bytes: usize,
    pub max_header_count: usize,
    pub max_dkim_signatures: usize,
    /// Maximum difference between canonical receipt time and the system clock
    /// used internally by `mail-auth` for DKIM expiration checks.
    pub max_clock_skew: Duration,
}

impl Default for DmarcLimits {
    fn default() -> Self {
        Self {
            max_message_bytes: 25 * 1024 * 1024,
            max_header_bytes: 256 * 1024,
            max_header_count: 512,
            max_dkim_signatures: 32,
            max_clock_skew: Duration(5 * 60 * 1_000),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GatewayBuildError(String);

impl fmt::Display for GatewayBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "could not initialize the DNS resolver: {}",
            self.0
        )
    }
}

impl std::error::Error for GatewayBuildError {}

/// Production DMARC verifier backed by `mail-auth` 0.12.1 and Hickory DNS.
#[derive(Clone)]
pub struct StalwartDmarcVerifier {
    authenticator: MessageAuthenticator,
    limits: DmarcLimits,
}

impl StalwartDmarcVerifier {
    /// Uses the host's configured DNS resolvers.
    ///
    /// # Errors
    ///
    /// Returns an error when system resolver configuration is unavailable.
    pub fn from_system_config(limits: DmarcLimits) -> Result<Self, GatewayBuildError> {
        MessageAuthenticator::new_system_conf()
            .map(|authenticator| Self::with_authenticator(authenticator, limits))
            .map_err(|error| GatewayBuildError(error.to_string()))
    }

    /// Accepts an explicitly configured authenticator, useful for deployments
    /// that pin resolver transport and DNS policy outside this crate.
    pub const fn with_authenticator(
        authenticator: MessageAuthenticator,
        limits: DmarcLimits,
    ) -> Self {
        Self {
            authenticator,
            limits,
        }
    }

    pub const fn limits(&self) -> DmarcLimits {
        self.limits
    }

    async fn verify_request(
        &self,
        request: SmtpAuthenticationRequest<'_>,
    ) -> Result<VerifiedDomain, DmarcError> {
        let context = self.validate(&request)?;
        let message = AuthenticatedMessage::parse(request.raw_message).ok_or_else(|| {
            DmarcError::new(
                DmarcErrorKind::Malformed,
                "RFC5322 message could not be parsed",
            )
        })?;

        let dkim_output = self.authenticator.verify_dkim(&message).await;
        let spf_output = if let Some(mail_from) = request.mail_from {
            self.authenticator
                .verify_spf(SpfParameters::verify_mail_from(
                    request.remote_ip,
                    context.helo_identity.as_str(),
                    context.receiver_hostname.as_str(),
                    mail_from,
                ))
                .await
        } else {
            self.authenticator
                .verify_spf(SpfParameters::verify_ehlo(
                    request.remote_ip,
                    context.helo_identity.as_str(),
                    context.receiver_hostname.as_str(),
                ))
                .await
        };
        let authentication_lookup_temporary = dkim_output
            .iter()
            .any(|output| matches!(output.result(), DkimResult::TempError(_)))
            || spf_output.result() == SpfResult::TempError;

        let output = self
            .authenticator
            .verify_dmarc(DmarcParameters::new(
                &message,
                &dkim_output,
                context.mail_from_domain.as_str(),
                &spf_output,
            ))
            .await;
        evidence_from_output(
            &output,
            request.received_at,
            authentication_lookup_temporary,
        )
    }

    fn validate(
        &self,
        request: &SmtpAuthenticationRequest<'_>,
    ) -> Result<ValidatedContext, DmarcError> {
        if request.raw_message.len() > self.limits.max_message_bytes {
            return Err(resource_limit("message exceeds the configured size limit"));
        }
        let header = header_section(request.raw_message).ok_or_else(|| {
            DmarcError::new(
                DmarcErrorKind::Malformed,
                "message has no complete header section",
            )
        })?;
        if header.len() > self.limits.max_header_bytes {
            return Err(resource_limit(
                "header section exceeds the configured size limit",
            ));
        }
        let (header_count, dkim_count) = count_headers(header);
        if header_count > self.limits.max_header_count {
            return Err(resource_limit("message has too many header fields"));
        }
        if dkim_count > self.limits.max_dkim_signatures {
            return Err(resource_limit("message has too many DKIM signatures"));
        }

        let system_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| {
                u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
            });
        if system_millis.abs_diff(request.received_at.0) > self.limits.max_clock_skew.0 {
            return Err(DmarcError::new(
                DmarcErrorKind::Temporary,
                "canonical receipt time is outside the permitted system-clock skew",
            ));
        }

        let helo_identity = normalize_helo_identity(request.helo_identity)?;
        let receiver_hostname =
            parse_smtp_domain(request.receiver_hostname, "invalid receiver hostname")?;
        let mail_from_domain = if let Some(mail_from) = request.mail_from {
            let (local, domain) = mail_from.rsplit_once('@').ok_or_else(|| {
                DmarcError::new(DmarcErrorKind::Malformed, "invalid MAIL FROM mailbox")
            })?;
            if local.is_empty() {
                return Err(DmarcError::new(
                    DmarcErrorKind::Malformed,
                    "invalid MAIL FROM mailbox",
                ));
            }
            parse_smtp_domain(domain, "invalid MAIL FROM domain")?
                .as_str()
                .to_owned()
        } else {
            helo_identity.clone()
        };

        Ok(ValidatedContext {
            helo_identity,
            receiver_hostname,
            mail_from_domain,
        })
    }
}

impl DmarcVerifier for StalwartDmarcVerifier {
    fn verify<'a>(&'a self, request: SmtpAuthenticationRequest<'a>) -> DmarcVerificationFuture<'a> {
        Box::pin(async move { self.verify_request(request).await })
    }
}

struct ValidatedContext {
    helo_identity: String,
    receiver_hostname: DomainIdentity,
    mail_from_domain: String,
}

fn parse_smtp_domain(input: &str, detail: &'static str) -> Result<DomainIdentity, DmarcError> {
    DomainIdentity::parse_ascii(input)
        .map_err(|_| DmarcError::new(DmarcErrorKind::Malformed, detail))
}

fn normalize_helo_identity(input: &str) -> Result<String, DmarcError> {
    if let Ok(domain) = DomainIdentity::parse_ascii(input) {
        return Ok(domain.as_str().to_owned());
    }
    let literal = input
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .ok_or_else(|| DmarcError::new(DmarcErrorKind::Malformed, "invalid EHLO identity"))?;
    let address = literal
        .get(..5)
        .filter(|prefix| prefix.eq_ignore_ascii_case("ipv6:"))
        .map_or(literal, |_| &literal[5..]);
    address
        .parse::<std::net::IpAddr>()
        .map_err(|_| DmarcError::new(DmarcErrorKind::Malformed, "invalid EHLO address literal"))?;
    Ok(input.to_ascii_lowercase())
}

fn resource_limit(detail: &'static str) -> DmarcError {
    DmarcError::new(DmarcErrorKind::ResourceLimit, detail)
}

fn header_section(message: &[u8]) -> Option<&[u8]> {
    message
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|end| &message[..end])
        .or_else(|| {
            message
                .windows(2)
                .position(|window| window == b"\n\n")
                .map(|end| &message[..end])
        })
}

fn count_headers(header: &[u8]) -> (usize, usize) {
    let mut header_count = 0;
    let mut dkim_count = 0;
    for line in header.split(|byte| *byte == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() || matches!(line.first(), Some(b' ' | b'\t')) {
            continue;
        }
        header_count += 1;
        if line
            .get(..15)
            .is_some_and(|name| name.eq_ignore_ascii_case(b"dkim-signature:"))
        {
            dkim_count += 1;
        }
    }
    (header_count, dkim_count)
}

fn evidence_from_output(
    output: &DmarcOutput,
    evaluated_at: cs_mail_primitives::CanonicalTime,
    authentication_lookup_temporary: bool,
) -> Result<VerifiedDomain, DmarcError> {
    let record = output.dmarc_record();
    let dkim_passed = matches!(output.dkim_result(), DmarcResult::Pass);
    let spf_passed = matches!(output.spf_result(), DmarcResult::Pass);
    let (dkim, spf) = if dkim_passed || spf_passed {
        let pass_record = record.ok_or_else(|| {
            DmarcError::new(
                DmarcErrorKind::Failed,
                "DMARC produced a pass without its policy record",
            )
        })?;
        (
            dkim_passed.then(|| DmarcPass {
                alignment: map_alignment(pass_record.adkim),
            }),
            spf_passed.then(|| DmarcPass {
                alignment: map_alignment(pass_record.aspf),
            }),
        )
    } else {
        (None, None)
    };

    if dkim.is_some() || spf.is_some() {
        let author_domain = DomainIdentity::parse_ascii(output.domain()).map_err(|_| {
            DmarcError::new(
                DmarcErrorKind::Malformed,
                "DMARC returned an invalid RFC5322 author domain",
            )
        })?;
        return VerifiedDomain::passed(author_domain, dkim, spf, evaluated_at)
            .map_err(|error| DmarcError::new(DmarcErrorKind::Failed, error.to_string()));
    }

    if matches!(output.dkim_result(), DmarcResult::TempError(_))
        || matches!(output.spf_result(), DmarcResult::TempError(_))
        || (record.is_some() && authentication_lookup_temporary)
    {
        return Err(DmarcError::new(
            DmarcErrorKind::Temporary,
            "a transient DNS error prevented a conclusive DMARC result",
        ));
    }

    let detail = if record.is_none() {
        "no applicable DMARC policy record was found"
    } else {
        "neither SPF nor DKIM produced a DMARC-aligned pass"
    };
    Err(DmarcError::new(DmarcErrorKind::Failed, detail))
}

const fn map_alignment(alignment: MailAlignment) -> DmarcAlignment {
    match alignment {
        MailAlignment::Strict => DmarcAlignment::Strict,
        MailAlignment::Relaxed => DmarcAlignment::Relaxed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cs_mail_primitives::CanonicalTime;
    use mail_auth::Version;
    use mail_auth::dmarc::{Dmarc, Policy, Psd, Report};
    use std::sync::Arc;

    #[test]
    fn counts_folded_dkim_headers_once() {
        let headers = b"From: a@example.com\r\nDKIM-Signature: v=1;\r\n b=abc\r\nSubject: hi";
        assert_eq!(count_headers(headers), (3, 1));
    }

    #[test]
    fn accepts_domain_and_address_literal_helo_identities() {
        assert_eq!(
            normalize_helo_identity("MX.Example.COM.").unwrap(),
            "mx.example.com"
        );
        assert_eq!(normalize_helo_identity("[IPv6:::1]").unwrap(), "[ipv6:::1]");
        assert!(normalize_helo_identity("[not-an-address]").is_err());
    }

    #[test]
    fn no_policy_is_a_typed_authentication_failure() {
        let error =
            evidence_from_output(&DmarcOutput::default(), CanonicalTime(1), false).unwrap_err();
        assert_eq!(error.kind, DmarcErrorKind::Failed);
    }

    #[test]
    fn retains_dkim_and_spf_alignment_independently() {
        let record = Dmarc {
            v: Version::V1,
            adkim: MailAlignment::Strict,
            aspf: MailAlignment::Relaxed,
            fo: Report::All,
            np: Policy::None,
            p: Policy::None,
            psd: Psd::Default,
            rua: Vec::new(),
            ruf: Vec::new(),
            sp: Policy::None,
            t: false,
        };
        let output = DmarcOutput::default()
            .with_domain("mail.example.com")
            .with_dkim_result(DmarcResult::Pass)
            .with_spf_result(DmarcResult::Pass)
            .with_record(Arc::new(record));
        let evidence = evidence_from_output(&output, CanonicalTime(4), false).unwrap();
        assert_eq!(
            evidence.dkim,
            Some(DmarcPass {
                alignment: DmarcAlignment::Strict
            })
        );
        assert_eq!(
            evidence.spf,
            Some(DmarcPass {
                alignment: DmarcAlignment::Relaxed
            })
        );
    }

    #[test]
    fn lower_level_dns_failure_remains_retryable_when_policy_exists() {
        let record = Dmarc {
            v: Version::V1,
            adkim: MailAlignment::Relaxed,
            aspf: MailAlignment::Relaxed,
            fo: Report::All,
            np: Policy::None,
            p: Policy::None,
            psd: Psd::Default,
            rua: Vec::new(),
            ruf: Vec::new(),
            sp: Policy::None,
            t: false,
        };
        let output = DmarcOutput::default()
            .with_domain("example.com")
            .with_record(Arc::new(record));
        let error = evidence_from_output(&output, CanonicalTime(4), true).unwrap_err();
        assert_eq!(error.kind, DmarcErrorKind::Temporary);
    }
}
