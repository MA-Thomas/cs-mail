//! Controlled DNS and signed messages for the receiver's authentication experiments.
use base64::Engine;
use cs_mail_primitives::Duration;
use cs_mail_smtp_gateway::{
    DmarcLimits, StalwartDmarcVerifier,
    outbound::{DkimIdentity, OutboundMail, render_signed},
};
use hickory_proto::{
    op::{Message, MessageType, ResponseCode},
    rr::{RData, Record, RecordType, rdata::TXT},
};
use hickory_resolver::config::{NameServerConfig, ResolverConfig, ResolverOpts};
use std::sync::Arc;

pub struct Dns {
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Dns {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Dns {
    pub async fn start() -> (Self, StalwartDmarcVerifier) {
        let socket = Arc::new(tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let address = socket.local_addr().unwrap();
        let public = base64::engine::general_purpose::STANDARD.encode(
            ed25519_dalek::SigningKey::from_bytes(&[73; 32])
                .verifying_key()
                .as_bytes(),
        );
        let rsa_public =
            base64::engine::general_purpose::STANDARD.encode(dkim("example.test").public_key());
        let task = tokio::spawn(async move {
            let mut bytes = [0; 4096];
            loop {
                let (len, peer) = socket.recv_from(&mut bytes).await.unwrap();
                let query = Message::from_vec(&bytes[..len]).unwrap();
                let mut response = Message::new(query.id, MessageType::Response, query.op_code);
                response.metadata.authoritative = true;
                response.metadata.recursion_desired = true;
                response.metadata.recursion_available = true;
                for q in &query.queries {
                    response.add_query(q.clone());
                    let name = q.name().to_string();
                    if name.ends_with("temporary.test.") {
                        response.metadata.response_code = ResponseCode::ServFail;
                        continue;
                    }
                    let txt = match name.as_str() {
                        "_dmarc.example.test." => {
                            Some("v=DMARC1; p=reject; adkim=s; aspf=s".to_owned())
                        }
                        "example.test." => Some("v=spf1 -all".to_owned()),
                        "test._domainkey.example.test." => {
                            Some(format!("v=DKIM1; k=ed25519; p={public}"))
                        }
                        "rsa._domainkey.example.test." => {
                            Some(format!("v=DKIM1; k=rsa; p={rsa_public}"))
                        }
                        _ => None,
                    };
                    if let Some(txt) = txt.filter(|_| q.query_type() == RecordType::TXT) {
                        response.add_answer(Record::from_rdata(
                            q.name().clone(),
                            60,
                            RData::TXT(TXT::new(
                                txt.as_bytes()
                                    .chunks(200)
                                    .map(|chunk| std::str::from_utf8(chunk).unwrap().to_owned())
                                    .collect(),
                            )),
                        ));
                    } else {
                        response.metadata.response_code = ResponseCode::NXDomain;
                    }
                }
                socket
                    .send_to(&response.to_vec().unwrap(), peer)
                    .await
                    .unwrap();
            }
        });
        let mut ns = NameServerConfig::udp(address.ip());
        ns.connections[0].port = address.port();
        let mut options = ResolverOpts::default();
        options.timeout = std::time::Duration::from_secs(1);
        options.attempts = 1;
        let authenticator = mail_auth::MessageAuthenticator::new(
            ResolverConfig::from_parts(None, vec![], vec![ns]),
            options,
        )
        .unwrap();
        (
            Self { task },
            StalwartDmarcVerifier::with_authenticator(
                authenticator,
                DmarcLimits {
                    max_clock_skew: Duration(u64::MAX),
                    ..DmarcLimits::default()
                },
            ),
        )
    }
}
/// Public development fixture only. Never use this key in a deployment.
pub fn dkim(domain: &str) -> DkimIdentity {
    DkimIdentity::from_pkcs8_pem(
        domain,
        "rsa",
        include_bytes!("../fixtures/dkim-rsa2048.pem"),
    )
    .unwrap()
}

pub fn signed(local: &str, recipient: &str, id: &str, text: &str) -> Vec<u8> {
    render_signed(
        &OutboundMail {
            thread: None,
            from_local: local.into(),
            to: recipient.into(),
            subject: "Receiver experiment".into(),
            body: text.into(),
            message_id: id.into(),
            date: "Wed, 07 Oct 2026 12:00:00 +0000".into(),
        },
        &dkim("example.test"),
    )
    .unwrap()
}

/// A realistic flat alternative: quoted-printable UTF-8 text and base64 HTML.
/// Returned unsigned so experiments can introduce malformed MIME before signing.
pub fn alternative(local: &str, recipient: &str, id: &str, text: &str) -> String {
    let plain = text.as_bytes().iter().fold(String::new(), |mut out, b| {
        use std::fmt::Write;
        write!(out, "={b:02X}").unwrap();
        out
    });
    let plain = plain
        .as_bytes()
        .chunks(60)
        .map(|line| std::str::from_utf8(line).unwrap())
        .collect::<Vec<_>>()
        .join("=\r\n");
    let html = base64::engine::general_purpose::STANDARD.encode(format!("<p>{text}</p>"));
    let html = html
        .as_bytes()
        .chunks(76)
        .map(|line| std::str::from_utf8(line).unwrap())
        .collect::<Vec<_>>()
        .join("\r\n");
    format!(
        "From: {local}@example.test\r\nTo: {recipient}\r\nSubject: Receiver experiment\r\nDate: Wed, 07 Oct 2026 12:00:00 +0000\r\nMessage-ID: <{id}@example.test>\r\nMIME-Version: 1.0\r\nContent-Type: multipart/alternative; boundary=reply-boundary\r\n\r\nThis is a MIME message.\r\n--reply-boundary\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\n{plain}\r\n--reply-boundary\r\nContent-Type: text/html; charset=utf-8\r\nContent-Transfer-Encoding: base64\r\n\r\n{html}\r\n--reply-boundary--\r\n"
    )
}

pub fn sign_message(message: &str) -> Vec<u8> {
    use mail_auth::{
        common::{crypto::Ed25519Key, headers::HeaderWriter},
        dkim::DkimSigner,
    };
    let public = ed25519_dalek::SigningKey::from_bytes(&[73; 32])
        .verifying_key()
        .to_bytes();
    let key = Ed25519Key::from_seed_and_public_key(&[73; 32], &public).unwrap();
    let signature = DkimSigner::from_key(key)
        .domain("example.test")
        .selector("test")
        .headers([
            "From",
            "To",
            "Subject",
            "Date",
            "Message-ID",
            "MIME-Version",
            "Content-Type",
            "Content-Transfer-Encoding",
        ])
        .sign(message.as_bytes())
        .unwrap();
    [signature.to_header().as_bytes(), message.as_bytes()].concat()
}
pub const TLS_CERTIFICATE: &[u8] = include_bytes!("../fixtures/smtp-certificate.pem");
pub const TLS_PRIVATE_KEY: &[u8] = include_bytes!("../fixtures/smtp-private-key.pem");

pub const TLS_CA: &[u8] = include_bytes!("../fixtures/smtp-ca.pem");
