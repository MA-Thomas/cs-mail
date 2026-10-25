//! Agreed receiver conjectures: real authentication cannot be replaced by headers
//! or a forwarded route; permission is current; success follows commit; lost
//! acknowledgements and fresh encryption cannot create duplicate deliveries.
use super::*;
use cs_mail_adapters::DomainIdentity;
use cs_mail_service::incoming::SmtpReceiver;
use cs_mail_smtp_gateway::{incoming::reply_address, receiver};
use support::smtp::{alternative, sign_message};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
};
use tokio_rustls::{
    TlsAcceptor, TlsConnector,
    rustls::{
        ClientConfig, RootCertStore, ServerConfig,
        pki_types::{CertificateDer, PrivateKeyDer, ServerName, pem::PemObject},
    },
};

struct Host {
    runtime: tokio::runtime::Runtime,
    address: std::net::SocketAddr,
    task: tokio::task::JoinHandle<()>,
    _dns: support::smtp::Dns,
}
impl Drop for Host {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Host {
    fn start(f: &External) -> Self {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let repository = f.repository().incoming();
        let clock = f.base.clock.clone();
        let (address, task, dns) = runtime.block_on(async {
            let (dns, verifier) = support::smtp::Dns::start().await;
            let domain = Arc::new(DomainIdentity::parse_ascii("mail.localhost").unwrap());
            let handler = Arc::new(
                SmtpReceiver::new(repository, verifier, (*domain).clone(), [88; 32], clock, 2)
                    .unwrap(),
            );
            let cert = CertificateDer::from_pem_slice(support::smtp::TLS_CERTIFICATE).unwrap();
            let key = PrivateKeyDer::from_pem_slice(support::smtp::TLS_PRIVATE_KEY).unwrap();
            let tls = TlsAcceptor::from(Arc::new(
                ServerConfig::builder()
                    .with_no_client_auth()
                    .with_single_cert(vec![cert], key)
                    .unwrap(),
            ));
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let task = tokio::spawn(async move {
                loop {
                    let (socket, peer) = listener.accept().await.unwrap();
                    let (domain, tls, handler) = (domain.clone(), tls.clone(), handler.clone());
                    tokio::spawn(async move {
                        let _ = receiver::session(socket, peer.ip(), domain, tls, handler).await;
                    });
                }
            });
            (address, task, dns)
        });
        Self {
            runtime,
            address,
            task,
            _dns: dns,
        }
    }
    fn send(&self, to: &str, raw: &[u8]) -> u16 {
        self.runtime.block_on(async {
            let mut peer = Peer::connect(self.address).await;
            peer.data(to, raw).await;
            peer.answer().await
        })
    }
    fn lose_acknowledgement(&self, to: &str, raw: &[u8]) {
        self.runtime.block_on(async {
            let mut peer = Peer::connect(self.address).await;
            peer.data(to, raw).await;
        });
    }
}
struct Peer(BufReader<tokio_rustls::client::TlsStream<TcpStream>>);
async fn answer<S: tokio::io::AsyncRead + Unpin>(io: &mut BufReader<S>) -> u16 {
    loop {
        let mut line = String::new();
        assert!(io.read_line(&mut line).await.unwrap() > 0);
        if line.as_bytes().get(3) == Some(&b' ') {
            return line[..3].parse().unwrap();
        }
    }
}
impl Peer {
    async fn connect(address: std::net::SocketAddr) -> Self {
        let mut io = BufReader::new(TcpStream::connect(address).await.unwrap());
        assert_eq!(answer(&mut io).await, 220);
        io.get_mut()
            .write_all(b"EHLO sender.example.test\r\n")
            .await
            .unwrap();
        assert_eq!(answer(&mut io).await, 250);
        io.get_mut().write_all(b"STARTTLS\r\n").await.unwrap();
        assert_eq!(answer(&mut io).await, 220);
        let mut roots = RootCertStore::empty();
        roots
            .add(CertificateDer::from_pem_slice(support::smtp::TLS_CA).unwrap())
            .unwrap();
        let tls = TlsConnector::from(Arc::new(
            ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth(),
        ));
        let mut peer = Self(BufReader::new(
            tls.connect(ServerName::try_from("localhost").unwrap(), io.into_inner())
                .await
                .unwrap(),
        ));
        peer.0
            .get_mut()
            .write_all(b"MAIL FROM:<carol@example.test>\r\n")
            .await
            .unwrap();
        assert_eq!(
            peer.answer().await,
            503,
            "TLS must discard pre-upgrade greeting state"
        );
        peer.0
            .get_mut()
            .write_all(b"EHLO sender.example.test\r\n")
            .await
            .unwrap();
        assert_eq!(peer.answer().await, 250);
        peer
    }
    async fn answer(&mut self) -> u16 {
        answer(&mut self.0).await
    }
    async fn data(&mut self, to: &str, raw: &[u8]) {
        self.0
            .get_mut()
            .write_all(b"MAIL FROM:<carol@example.test>\r\n")
            .await
            .unwrap();
        assert_eq!(self.answer().await, 250);
        self.0
            .get_mut()
            .write_all(format!("RCPT TO:<{to}>\r\n").as_bytes())
            .await
            .unwrap();
        assert_eq!(self.answer().await, 250);
        self.0.get_mut().write_all(b"DATA\r\n").await.unwrap();
        assert_eq!(self.answer().await, 354);
        let mut stuffed = vec![];
        for row in raw.split_inclusive(|b| *b == b'\n') {
            if row.starts_with(b".") {
                stuffed.push(b'.');
            }
            stuffed.extend_from_slice(row);
        }
        stuffed.extend_from_slice(b".\r\n");
        self.0.get_mut().write_all(&stuffed).await.unwrap();
        self.0.get_mut().flush().await.unwrap();
    }
}
fn route(id: u128) -> String {
    reply_address(
        ConversationId::new(id).unwrap(),
        &DomainIdentity::parse_ascii("mail.localhost").unwrap(),
    )
}

#[test]
#[ignore = "requires isolated PostgreSQL and local SMTP/DNS sockets"]
fn authenticated_wire_mail_cannot_escape_mailbox_conversation_or_current_permission() {
    let f = External::new();
    f.start(800);
    let host = Host::start(&f);
    let to = route(800);
    let raw = multipart(
        "carol",
        &to,
        "first",
        "wire-plaintext-marker\n.dot transparency",
    );
    assert_eq!(host.send(&to, &raw), 250);
    assert_eq!(f.read(800).1.len(), 2);
    let altered = String::from_utf8(raw.clone())
        .unwrap()
        .replace("=77=69=72=65", "=66=61=6B=65");
    assert_eq!(
        host.send(&to, altered.as_bytes()),
        550,
        "DKIM tampering must fail despite the reply route"
    );
    let forged=b"Authentication-Results: mail.localhost; dmarc=pass\r\nFrom: carol@example.test\r\nMessage-ID: <forged@example.test>\r\n\r\nForged\r\n";
    assert_eq!(host.send(&to, forged), 550);
    assert_eq!(
        host.send(&to, &multipart("other", &to, "other", "wrong mailbox")),
        550
    );
    assert_eq!(host.send(&route(801), &raw), 550);
    let ambiguous = [b"From: other@example.test\r\n".as_slice(), raw.as_slice()].concat();
    assert_eq!(host.send(&to, &ambiguous), 550);
    let temporary=b"From: carol@temporary.test\r\nMessage-ID: <temporary@temporary.test>\r\n\r\nDNS failure\r\n";
    assert_eq!(
        host.send(&to, temporary),
        451,
        "temporary DNS failure must remain retryable"
    );
    f.contact(
        ProtocolCommand::AcceptRelationship {
            expected_version: f.read(800).0.version.into(),
        },
        802,
    );
    f.contact(
        ProtocolCommand::RevokeRelationship {
            expected_version: f.read(800).0.version.into(),
        },
        803,
    );
    assert_eq!(
        host.send(
            &to,
            &multipart("carol", &to, "revoked", "no longer allowed")
        ),
        550
    );
    f.start(804);
    let new_route = route(804);
    f.contact(
        ProtocolCommand::BlockRelationship {
            expected_version: f.read(804).0.version.into(),
        },
        805,
    );
    assert_eq!(
        host.send(
            &new_route,
            &multipart("carol", &new_route, "blocked", "blocked")
        ),
        550
    );
    assert_eq!(f.read(800).1.len(), 2);
    assert_eq!(f.read(804).1.len(), 1);
    let mut db = postgres::Client::connect(&f.base.url, postgres::NoTls).unwrap();
    for row in db
        .query(
            "SELECT record::text,inbound_evidence::text FROM cs_external_messages",
            &[],
        )
        .unwrap()
    {
        assert!(!row.get::<_, String>(0).contains("wire-plaintext-marker"));
        if let Some(evidence) = row.get::<_, Option<String>>(1) {
            assert!(evidence.contains("example.test"));
            assert!(!evidence.contains("wire-plaintext-marker"));
        }
    }
}

#[test]
#[ignore = "requires isolated PostgreSQL and local SMTP/DNS sockets"]
fn smtp_success_follows_atomic_commit_and_lost_acknowledgements_do_not_duplicate() {
    let f = External::new();
    f.start(900);
    let host = Host::start(&f);
    let to = route(900);
    let raw = multipart("carol", &to, "durable", "one logical message");
    let mut db = postgres::Client::connect(&f.base.url, postgres::NoTls).unwrap();
    db.batch_execute("CREATE FUNCTION refuse_smtp() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.inbound_digest IS NOT NULL THEN RAISE EXCEPTION 'injected SMTP commit failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER refuse_smtp BEFORE INSERT ON cs_external_messages FOR EACH ROW EXECUTE FUNCTION refuse_smtp();").unwrap();
    assert_eq!(host.send(&to, &raw), 451);
    assert_eq!(
        f.read(900).1.len(),
        1,
        "failure cannot leave half a delivery"
    );
    db.batch_execute("DROP TRIGGER refuse_smtp ON cs_external_messages")
        .unwrap();
    host.lose_acknowledgement(&to, &raw);
    for _ in 0..100 {
        if f.read(900).1.len() == 2 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(f.read(900).1.len(), 2);
    f.base.advance(1);
    let retransmitted = [
        b"Received: retry via another hop\r\n".as_slice(),
        raw.as_slice(),
    ]
    .concat();
    assert_eq!(host.send(&to, &retransmitted), 250);
    assert_eq!(
        f.read(900).1.len(),
        2,
        "fresh encryption, time and trace headers cannot duplicate"
    );
    // A client can re-encode the same logical text on a later attempt.
    let reencoded = alternative("carol", &to, "durable", "one logical message");
    let start = reencoded
        .find("Content-Transfer-Encoding: quoted-printable")
        .unwrap();
    let end = start + reencoded[start..].find("\r\n--reply-boundary").unwrap();
    let mut reencoded = reencoded;
    reencoded.replace_range(
        start..end,
        "Content-Transfer-Encoding: base64\r\n\r\nb25lIGxvZ2ljYWwgbWVzc2FnZQ==",
    );
    assert_eq!(host.send(&to, &sign_message(&reencoded)), 250);
    assert_eq!(
        f.read(900).1.len(),
        2,
        "transfer encoding is not a new message"
    );
    assert_eq!(
        host.send(&to, &multipart("carol", &to, "durable", "altered retry")),
        550
    );
    assert_eq!(f.read(900).1.len(), 2);
    // Hold a real database lock across DATA: no success may escape while commit waits.
    let mut transaction = db.transaction().unwrap();
    transaction
        .batch_execute("LOCK TABLE cs_external_messages IN SHARE MODE")
        .unwrap();
    let raw = support::smtp::signed("carol", &to, "wait-for-commit", "second logical message");
    let mut peer = host.runtime.block_on(Peer::connect(host.address));
    host.runtime.block_on(peer.data(&to, &raw));
    assert!(
        host.runtime
            .block_on(async {
                tokio::time::timeout(std::time::Duration::from_millis(100), peer.answer()).await
            })
            .is_err()
    );
    transaction.commit().unwrap();
    assert_eq!(host.runtime.block_on(peer.answer()), 250);
    assert_eq!(f.read(900).1.len(), 3);
    challenge_alias_receiving(&f, &host);
}

fn multipart(local: &str, to: &str, id: &str, text: &str) -> Vec<u8> {
    sign_message(&alternative(local, to, id, text))
}

/// Counterexample: SMTP acknowledges a message after dropping an attachment,
/// additional body, or malformed alternative. The member would lose content.
#[test]
#[ignore = "requires isolated PostgreSQL and local SMTP/DNS sockets"]
fn unsupported_mime_cannot_be_acknowledged_by_selecting_only_its_plain_part() {
    let f = External::new();
    f.start(1000);
    let host = Host::start(&f);
    let to = route(1000);
    let good = alternative("carol", &to, "unsupported", "visible text");
    let cases = [
        ("mixed", good.replace("multipart/alternative", "multipart/mixed")),
        ("duplicate HTML alternatives", good.replace("text/plain", "text/html")),
        ("unknown text subtype", good.replace("text/plain", "text/calendar")),
        ("attachment disposition", good.replace("Content-Type: text/html", "Content-Disposition: attachment; filename=details.html\r\nContent-Type: text/html")),
        ("inline file", good.replace("text/html; charset=utf-8", "text/html; charset=utf-8; name=details.html")),
        ("duplicate part type", good.replace("Content-Type: text/html", "Content-Type: text/plain\r\nContent-Type: text/html")),
        ("unknown charset", good.replace("charset=utf-8", "charset=unknown-encoding")),
        ("unknown transfer encoding", good.replace("quoted-printable", "unknown-encoding")),
        ("truncated alternative", good.replace("--reply-boundary--\r\n", "")),
        ("malformed closing delimiter", good.replace("--reply-boundary--\r\n", "--reply-boundary--junk\r\nignored content\r\n--reply-boundary--\r\n")),
        ("invalid base64", good.replace("Content-Transfer-Encoding: base64\r\n\r\n", "Content-Transfer-Encoding: base64\r\n\r\n!invalid!")),
        ("extra part", good.replace("--reply-boundary--", "--reply-boundary\r\nContent-Type: text/plain\r\n\r\nextra content\r\n--reply-boundary--")),
        ("nested related", good.replace("Content-Type: text/html; charset=utf-8\r\nContent-Transfer-Encoding: base64\r\n\r\n", "Content-Type: multipart/related; boundary=nested\r\n\r\n--nested\r\nContent-Type: text/html\r\n\r\n")),
    ];
    for (name, raw) in cases {
        assert_eq!(host.send(&to, &sign_message(&raw)), 554, "{name}");
        assert_eq!(f.read(1000).1.len(), 1, "{name} must not partially deliver");
    }
    assert_eq!(
        host.send(&to, &sign_message(&good)),
        250,
        "a refusal must not reserve the message ID"
    );
}

/// Counterexample: the two transport adapters work separately but the emitted
/// Reply-To does not return a readable, authenticated reply to the same member.
#[test]
#[ignore = "requires isolated PostgreSQL and local HTTP/SMTP/DNS sockets"]
fn outbound_carrier_and_multipart_reply_form_one_readable_local_conversation() {
    use cs_mail_service::external::{Smtp2goTransport, dispatch};
    use cs_mail_smtp_gateway::smtp2go::Smtp2go;
    let f = External::new();
    f.start(1100);
    let carrier = support::smtp2go::Carrier::new();
    carrier.push(support::smtp2go::accepted("local-roundtrip"));
    let mut transport = Smtp2goTransport {
        carrier: Smtp2go::new(&carrier.endpoint, "api-local-test-credential").unwrap(),
        dkim: support::smtp::dkim("mail.localhost"),
        domain: DomainIdentity::parse_ascii("mail.localhost").unwrap(),
    };
    let relay = EndpointSecretKey::from_secret_bytes(ContentKeyRef(600), &[60; 32])
        .unwrap()
        .0;
    dispatch(&f.repository(), &relay, MessageId(1100), &mut transport).unwrap();
    let raw = carrier.mime(0);
    let outgoing = mail_parser::MessageParser::default().parse(&raw).unwrap();
    assert_eq!(
        outgoing.to().unwrap().first().unwrap().address(),
        Some("carol@example.test")
    );
    let to = outgoing
        .reply_to()
        .unwrap()
        .first()
        .unwrap()
        .address()
        .unwrap();
    assert_eq!(to, route(1100));
    assert!(
        outgoing
            .body_text(0)
            .unwrap()
            .contains("external fixture 1100")
    );
    let host = Host::start(&f);
    let text = "Thank you — café.\r\n.A reply from Carol";
    let reply = alternative("carol", to, "roundtrip", text).replace(
        "MIME-Version:",
        &format!(
            "In-Reply-To: <{}>\r\nReferences: <{}>\r\nMIME-Version:",
            outgoing.message_id().unwrap(),
            outgoing.message_id().unwrap()
        ),
    );
    let signed = sign_message(&reply);
    // Both preserved representations are part of the authentication input.
    let mut tampered = signed.clone();
    let marker = b"Content-Transfer-Encoding: base64\r\n\r\n";
    let offset = tampered
        .windows(marker.len())
        .position(|s| s == marker)
        .unwrap()
        + marker.len();
    tampered[offset] = b'Q';
    assert_eq!(host.send(to, &tampered), 550);
    assert_eq!(f.read(1100).1.len(), 1);
    assert_eq!(host.send(to, &signed), 250);
    let messages = f.read(1100).1;
    assert_eq!(messages.len(), 2);
    let incoming = &messages[1].message;
    let member = EndpointSecretKey::from_secret_bytes(ContentKeyRef(500), &[50; 32])
        .unwrap()
        .0;
    let bytes =
        cs_mail_content::decrypt(&member, incoming.sender_key, &incoming.member_copy).unwrap();
    let document: cs_mail_correspondence::Document = serde_json::from_slice(&bytes).unwrap();
    let [cs_mail_correspondence::DocumentPart::Email(email)] = document.parts() else {
        panic!("incoming email must preserve MIME")
    };
    assert_eq!(email.mime(), signed);
    let presentation = email.presentation().unwrap();
    assert!(presentation.supplied_text.unwrap().contains(text));
    assert!(presentation.html.unwrap().contains("café"));
    assert_eq!(host.send(to, &signed), 250);
    assert_eq!(f.read(1100).1.len(), 2);
    let parts: Vec<_> = reply.split("--reply-boundary\r\n").collect();
    let html = parts[2].strip_suffix("--reply-boundary--\r\n").unwrap();
    let reversed = format!(
        "{}--reply-boundary\r\n{html}--reply-boundary\r\n{}--reply-boundary--\r\n",
        parts[0], parts[1]
    );
    assert_eq!(host.send(to, &sign_message(&reversed)), 250);
    assert_eq!(
        f.read(1100).1.len(),
        2,
        "alternative order cannot replace or duplicate committed content"
    );
    assert_eq!(carrier.count(), 1);
}

/// Counterexample: an unknown sender reaches the inbox, forged mail creates a
/// response, retries create notice storms, or a later block cannot cancel a prompt.
#[test]
#[ignore = "requires isolated PostgreSQL and local SMTP/DNS sockets"]
fn member_address_prompts_are_authenticated_bounded_and_rechecked_before_sending() {
    let f = External::new();
    let host = Host::start(&f);
    let to = format!("member-{:032x}@mail.localhost", BOB.0);
    let mut db = postgres::Client::connect(&f.base.url, postgres::NoTls).unwrap();
    let count = |db: &mut postgres::Client, table: &str| -> i64 {
        db.query_one(&format!("SELECT count(*) FROM {table}"), &[])
            .unwrap()
            .get(0)
    };
    let forged = b"From: carol@example.test\r\nAuthentication-Results: mail.localhost; dmarc=pass\r\nMessage-ID: <forged@example.test>\r\n\r\nForged\r\n";
    assert_eq!(host.send(&to, forged), 550);
    assert_eq!(count(&mut db, "cs_mail_notices"), 0);
    let automatic = alternative("carol", &to, "automatic", "do not loop").replace(
        "MIME-Version:",
        "Auto-Submitted: auto-replied\r\nMIME-Version:",
    );
    assert_eq!(host.send(&to, &sign_message(&automatic)), 550);
    assert_eq!(count(&mut db, "cs_mail_notices"), 0);
    let raw = multipart("carol", &to, "unknown", "must not enter the inbox");
    assert_eq!(host.send(&to, &raw), 250);
    assert_eq!(host.send(&to, &raw), 250);
    assert_eq!(
        host.send(&to, &multipart("carol", &to, "unknown", "altered retry")),
        550
    );
    assert_eq!(
        host.send(&to, &multipart("carol", &to, "another", "no second prompt")),
        250
    );
    assert_eq!(count(&mut db, "cs_mail_notices"), 1);
    assert_eq!(count(&mut db, "cs_external_messages"), 0);
    assert_eq!(count(&mut db, "cs_relationship_aggregates"), 0);
    let deployment = f.base.gateway.deployment();
    let notice = deployment.waiting_notices(10).unwrap().pop().unwrap();
    // Permission changes while a queued prompt is waiting for a worker.
    f.start(1200);
    f.contact(
        ProtocolCommand::AcceptRelationship {
            expected_version: f.read(1200).0.version.into(),
        },
        1210,
    );
    assert_eq!(host.send(&to, &raw), 250);
    assert_eq!(
        host.send(
            &to,
            &multipart("carol", &to, "unknown", "changed after acceptance")
        ),
        550
    );
    assert_eq!(
        count(&mut db, "cs_external_messages"),
        1,
        "a historical prompt cannot become inbox admission after acceptance"
    );
    f.contact(
        ProtocolCommand::BlockRelationship {
            expected_version: f.read(1200).0.version.into(),
        },
        1201,
    );
    assert!(
        deployment
            .begin_queued_notice(notice.id, || f.base.clock.now())
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        deployment.notice_state(notice.id).unwrap(),
        Some(DeliveryState::Cancelled { .. })
    ));
    assert_eq!(
        host.send(
            &to,
            &multipart("carol", &to, "blocked-member-address", "blocked")
        ),
        550
    );
}

/// Counterexample: opening a new email conversation grants outbound disclosure,
/// consumes a lane twice, or leaves a partial route when admission fails.
#[test]
#[ignore = "requires isolated PostgreSQL and local HTTP/SMTP/DNS sockets"]
#[allow(clippy::too_many_lines)] // One causal experiment: admission, explicit send consent, rollback, replay and withdrawal.
fn member_address_lane_admission_is_atomic_and_member_reply_requires_consent() {
    let f = External::new();
    f.start(1300);
    f.grant_lane(1301);
    let host = Host::start(&f);
    let to = format!("member-{:032x}@mail.localhost", BOB.0);
    let raw = multipart("carol", &to, "new-thread", "lane message");
    let mut db = postgres::Client::connect(&f.base.url, postgres::NoTls).unwrap();
    db.batch_execute("CREATE FUNCTION refuse_new_mail() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.inbound_digest IS NOT NULL THEN RAISE EXCEPTION 'injected failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER fail_new_mail BEFORE INSERT ON cs_external_messages FOR EACH ROW EXECUTE FUNCTION refuse_new_mail();").unwrap();
    assert_eq!(host.send(&to, &raw), 451);
    assert_eq!(
        db.query_one("SELECT count(*) FROM cs_external_conversations", &[])
            .unwrap()
            .get::<_, i64>(0),
        1
    );
    db.batch_execute("DROP TRIGGER fail_new_mail ON cs_external_messages")
        .unwrap();
    assert_eq!(host.send(&to, &raw), 250);
    assert_eq!(host.send(&to, &raw), 250);
    assert_eq!(
        host.send(
            &to,
            &multipart("carol", &to, "over-quota", "second message")
        ),
        550
    );
    let id = db
        .query_one(
            "SELECT id FROM cs_external_conversations WHERE id <> '1300'",
            &[],
        )
        .unwrap()
        .get::<_, String>(0)
        .parse::<u128>()
        .unwrap();
    assert_eq!(f.read(id).1.len(), 1);
    let package = support::external::text(
        &f.preparation(),
        ConversationId::new(id).unwrap(),
        MessageId(1302),
        BOB,
        f.base.clock.now(),
    );
    let send = f.command(
        id,
        1302,
        app::Command::Send {
            message: Box::new(package),
            disclose_to_external_mail: false,
        },
    );
    let refused = app::Service(&f.repository()).handle(&send).unwrap();
    assert!(
        matches!(&refused, app::Response::Outcome(a) if matches!(a.receipt.outcome, app::Outcome::Refused(_)))
    );
    // Explicit Send must grant consent and queue the reply together. A commit
    // failure must leave neither grant nor message, and must not reopen a refusal.
    let mut replacement = send.clone();
    replacement.operation = IdempotencyKey(1307);
    if let app::Command::Send {
        disclose_to_external_mail,
        ..
    } = &mut replacement.command
    {
        *disclose_to_external_mail = true;
    }
    let replacement = replacement.sign(&[21; 32]).unwrap();
    db.batch_execute("CREATE FUNCTION refuse_reply() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected reply failure'; END $$; CREATE TRIGGER fail_reply BEFORE INSERT ON cs_external_messages FOR EACH ROW EXECUTE FUNCTION refuse_reply();").unwrap();
    assert!(app::Service(&f.repository()).handle(&replacement).is_err());
    assert_eq!(f.read(id).1.len(), 1);
    if let app::Response::Conversation { conversation, .. } = app::Service(&f.repository())
        .handle(&f.command(id, 900_001, app::Command::Read { after: 0 }))
        .unwrap()
    {
        assert!(
            !conversation.disclosure().permits_dispatch(),
            "failed Send must not leave an outgoing grant"
        );
    } else {
        panic!("expected conversation");
    }
    db.batch_execute("DROP TRIGGER fail_reply ON cs_external_messages")
        .unwrap();
    let accepted = app::Service(&f.repository()).handle(&replacement).unwrap();
    assert!(
        matches!(&accepted, app::Response::Outcome(a) if matches!(a.receipt.outcome, app::Outcome::Queued(_)))
    );
    assert_eq!(
        accepted,
        app::Service(&f.repository()).handle(&replacement).unwrap()
    );
    assert_eq!(
        refused,
        app::Service(&f.repository()).handle(&send).unwrap(),
        "consent restoration cannot reopen a terminal refusal"
    );
    let carrier = support::smtp2go::Carrier::new();
    carrier.push(support::smtp2go::accepted("consented-inbound-reply"));
    let mut transport = cs_mail_service::external::Smtp2goTransport {
        carrier: cs_mail_smtp_gateway::smtp2go::Smtp2go::new(
            &carrier.endpoint,
            "api-local-test-credential",
        )
        .unwrap(),
        dkim: support::smtp::dkim("mail.localhost"),
        domain: DomainIdentity::parse_ascii("mail.localhost").unwrap(),
    };
    let relay = EndpointSecretKey::from_secret_bytes(ContentKeyRef(600), &[60; 32])
        .unwrap()
        .0;
    cs_mail_service::external::dispatch(&f.repository(), &relay, MessageId(1302), &mut transport)
        .unwrap();
    let mime = String::from_utf8(carrier.mime(0)).unwrap();
    assert!(mime.contains("In-Reply-To: <new-thread@example.test>"));
    assert_eq!(carrier.count(), 1);
    // Re-granting consent cannot resurrect a message cancelled by withdrawal.
    let package = support::external::text(
        &f.preparation(),
        ConversationId::new(id).unwrap(),
        MessageId(1304),
        BOB,
        f.base.clock.now(),
    );
    app::Service(&f.repository())
        .handle(&f.command(
            id,
            1304,
            app::Command::Send {
                message: Box::new(package),
                disclose_to_external_mail: false,
            },
        ))
        .unwrap();
    app::Service(&f.repository())
        .handle(&f.command(id, 1305, app::Command::WithdrawDisclosure))
        .unwrap();
    app::Service(&f.repository())
        .handle(&f.command(id, 1306, app::Command::GrantDisclosure))
        .unwrap();
    assert!(f.repository().begin_dispatch(MessageId(1304)).is_err());
}

// Extend the existing atomic-commit experiment: envelope routing must survive a
// misleading To header, rollback, redelivery and retirement without reclassification.
#[allow(clippy::too_many_lines)] // One causal experiment: rollback, exact replay, then retirement.
#[allow(clippy::too_many_lines)] // One lifecycle experiment challenges exact authority, rollback and replay together.
fn challenge_alias_receiving(f: &External, host: &Host) {
    use cs_mail_application::aliases::{Command, Outcome, SignedCommand};
    let account = f
        .base
        .gateway
        .deployment()
        .accounts(|| CanonicalTime(0))
        .persona_account(BOB)
        .unwrap();
    let alias = cs_mail_primitives::AliasId::new(9800).unwrap();
    let signed = |operation, command| {
        SignedCommand {
            scope: f.base.gateway.scope(),
            product: "cs-mail/test".into(),
            account,
            owner: BOB,
            key: BOB_KEY,
            operation: IdempotencyKey(operation),
            command,
            signature: vec![],
        }
        .sign(&[21; 32])
        .unwrap()
    };
    let ack = f
        .base
        .gateway
        .alias_command(
            &signed(
                9800,
                Command::Create {
                    id: alias,
                    creation: cs_mail_application::aliases::AliasCreation {
                        address: cs_mail_application::aliases::AddressSelection::Generated {
                            address: "fixture482731@mail.localhost"
                                .to_owned()
                                .try_into()
                                .unwrap(),
                        },
                        configuration: cs_mail_accounts::aliases::AliasConfiguration {
                            receiving: cs_mail_accounts::aliases::ReceivingPolicy::AliasScoped,
                            description: "Care team".to_owned().try_into().unwrap(),
                        },
                    },
                },
            ),
            "mail.localhost",
        )
        .unwrap();
    let Outcome::Changed(aliases) = ack.receipt.outcome else {
        panic!("alias not created")
    };
    // Conjecture: DMARC-authenticated domain evidence, not knowledge of the alias,
    // controls sender-restricted admission; removal and retries cannot restore it.
    let restricted_id = cs_mail_primitives::AliasId::new(9810).unwrap();
    let created = f
        .base
        .gateway
        .alias_command(
            &signed(
                9810,
                Command::Create {
                    id: restricted_id,
                    creation: cs_mail_application::aliases::AliasCreation {
                        address: cs_mail_application::aliases::AddressSelection::Generated {
                            address: "fixture482732@mail.localhost"
                                .to_owned()
                                .try_into()
                                .unwrap(),
                        },
                        configuration: cs_mail_accounts::aliases::AliasConfiguration {
                            receiving: cs_mail_accounts::aliases::ReceivingPolicy::Bonded,
                            description: "Shopping".to_owned().try_into().unwrap(),
                        },
                    },
                },
            ),
            "mail.localhost",
        )
        .unwrap();
    let Outcome::Changed(restricted_addresses) = created.receipt.outcome else {
        panic!("shopping address")
    };
    let shopping = restricted_addresses[0].address().as_str();
    let shopping_mail = support::smtp::signed("shop", shopping, "shopping-domain", "domain canary");
    assert_eq!(
        host.send(shopping, &shopping_mail),
        550,
        "unpaired address admitted mail"
    );
    let domain_pair = |domain: &str, operation, expected: u64| {
        use cs_mail_capabilities::{
            DeclarationAuthorityConstraint, LaneMode, LaneSubject, RateLimit, SenderScopedGrant,
            SignedLaneGrant,
        };
        let domain = DomainIdentity::parse_ascii(domain).unwrap();
        let at = f.base.clock.now();
        let grant = SenderScopedGrant {
            id: LaneId(operation),
            subject: LaneSubject::LegacyDomain(domain.clone()),
            sender: domain
                .synthetic_protocol_identity(&f.base.gateway.scope().deployment_domain, 1),
            recipient: BOB,
            protocol_version: ProtocolVersion(2),
            deployment_domain: f.base.gateway.scope().deployment_domain,
            intended_provider: ProviderRef(30),
            recipient_operational_key: BOB_KEY,
            purpose: DeclaredPurpose::Unspecified,
            origin: Some(OriginMode::LegacyOrUnspecified),
            declaration_authority: DeclarationAuthorityConstraint::LegacyGateway,
            lifetime: Duration(10000),
            rate_limit: RateLimit {
                max_messages: 1,
                interval: Duration(10000),
            },
            mode: LaneMode::Expiring,
            issued_at: at,
            not_before: at,
            not_after: at.checked_add(Duration(10000)).unwrap(),
            version: Version(1),
        };
        let signature = ed25519_dalek::SigningKey::from_bytes(&[21; 32])
            .sign(&grant.signing_bytes().unwrap())
            .to_bytes();
        signed(
            operation,
            Command::Permission {
                id: restricted_id,
                expected_address: 0,
                expected_permission: expected.into(),
                action: cs_mail_application::aliases::PermissionAction::Senders(vec![
                    SignedLaneGrant { grant, signature },
                ]),
            },
        )
    };
    let wrong = domain_pair("other.test", 9811, 0);
    assert!(matches!(
        f.base
            .gateway
            .alias_command(&wrong, "mail.localhost")
            .unwrap()
            .receipt
            .outcome,
        Outcome::Changed(_)
    ));
    assert_eq!(
        host.send(shopping, &shopping_mail),
        550,
        "unselected domain admitted"
    );
    let matching = domain_pair("example.test", 9812, 1);
    let paired = f
        .base
        .gateway
        .alias_command(&matching, "mail.localhost")
        .unwrap();
    assert!(matches!(paired.receipt.outcome, Outcome::Changed(_)));
    assert_eq!(host.send(shopping, &shopping_mail), 250);
    assert_eq!(
        host.send(shopping, &shopping_mail),
        250,
        "exact retry consumed allowance twice"
    );
    assert_eq!(
        host.send(
            shopping,
            &support::smtp::signed("shop", shopping, "shopping-second", "over allowance")
        ),
        550
    );
    let removal = signed(
        9813,
        Command::Permission {
            id: restricted_id,
            expected_address: 0,
            expected_permission: 2.into(),
            action: cs_mail_application::aliases::PermissionAction::Remove,
        },
    );
    assert!(matches!(
        f.base
            .gateway
            .alias_command(&removal, "mail.localhost")
            .unwrap()
            .receipt
            .outcome,
        Outcome::Changed(_)
    ));
    assert_eq!(
        f.base
            .gateway
            .alias_command(&matching, "mail.localhost")
            .unwrap(),
        paired,
        "historical retry reactivated a grant"
    );
    assert_eq!(
        host.send(
            shopping,
            &support::smtp::signed("another", shopping, "shopping-after-remove", "removed")
        ),
        550
    );
    assert_eq!(
        host.send(shopping, &shopping_mail),
        250,
        "historical redelivery created a new authority check"
    );
    let address = aliases[0].address().as_str();
    assert!(matches!(
        aliases[0].permission().state(),
        cs_mail_accounts::alias_permission::PermissionState::Broad(_)
    ));
    let open = signed(
        9802,
        Command::Permission {
            id: alias,
            expected_address: 0,
            expected_permission: 1.into(),
            action: cs_mail_application::aliases::PermissionAction::Open,
        },
    );
    assert!(matches!(
        f.base
            .gateway
            .alias_command(&open, "mail.localhost")
            .unwrap()
            .receipt
            .outcome,
        Outcome::Changed(_)
    ));
    let mut db = postgres::Client::connect(&f.base.url, postgres::NoTls).unwrap();
    let raw = support::smtp::signed(
        "new-team",
        &route(900),
        "alias-delivery",
        "receiving route canary",
    );
    // Conjecture: removal committed after preparation but before delivery fences the prepared authority.
    let verified = host.runtime.block_on(async {
        let (_dns, verifier) = support::smtp::Dns::start().await;
        cs_mail_smtp_gateway::incoming::authenticate(
            &verifier,
            cs_mail_adapters::SmtpAuthenticationRequest {
                raw_message: &raw,
                remote_ip: "127.0.0.1".parse().unwrap(),
                helo_identity: "example.test",
                mail_from: Some("carol@example.test"),
                receiver_hostname: "mail.localhost",
                received_at: f.base.clock.now(),
            },
            address,
            &DomainIdentity::parse_ascii("mail.localhost").unwrap(),
        )
        .await
        .unwrap()
    });
    let incoming = f.repository().incoming();
    let preparation = incoming
        .prepare_mail(&verified, &[88; 32])
        .unwrap()
        .unwrap();
    let prepared = verified
        .encrypt_reply(
            cs_mail_smtp_gateway::incoming::ReplyEncryption {
                conversation: preparation.conversation.id(),
                deployment: incoming.scope.deployment_domain,
                provider: incoming.scope.provider,
                member: preparation.relationship.recipient,
                relationship: preparation.relationship.relationship,
                content_scope: preparation.relationship.content_scope,
                member_key: preparation.member_key,
                lane: preparation.lane,
            },
            &[88; 32],
            f.base.clock.now(),
        )
        .unwrap();
    let remove_prepared = signed(
        9803,
        Command::Permission {
            id: alias,
            expected_address: 0,
            expected_permission: 2.into(),
            action: cs_mail_application::aliases::PermissionAction::Remove,
        },
    );
    assert!(matches!(
        f.base
            .gateway
            .alias_command(&remove_prepared, "mail.localhost")
            .unwrap()
            .receipt
            .outcome,
        Outcome::Changed(_)
    ));
    assert!(
        incoming.receive(&prepared).is_err(),
        "prepared message bypassed committed removal"
    );
    let explicit_reopen = signed(
        9804,
        Command::Permission {
            id: alias,
            expected_address: 0,
            expected_permission: 3.into(),
            action: cs_mail_application::aliases::PermissionAction::Open,
        },
    );
    assert!(matches!(
        f.base
            .gateway
            .alias_command(&explicit_reopen, "mail.localhost")
            .unwrap()
            .receipt
            .outcome,
        Outcome::Changed(_)
    ));
    db.batch_execute("CREATE TRIGGER refuse_alias_smtp BEFORE INSERT ON cs_external_messages FOR EACH ROW EXECUTE FUNCTION refuse_smtp()").unwrap();
    assert_eq!(host.send(address, &raw), 451);
    assert_eq!(
        db.query_one(
            "SELECT count(*) FROM cs_alias_deliveries WHERE alias=$1",
            &[&alias.value().to_string()]
        )
        .unwrap()
        .get::<_, i64>(0),
        0,
        "failed message left alias delivery evidence"
    );
    db.batch_execute("DROP TRIGGER refuse_alias_smtp ON cs_external_messages")
        .unwrap();
    assert_eq!(
        host.send(address, &raw),
        250,
        "alias-scoped lane must admit an authenticated previously unknown sender"
    );
    let before: String = db
        .query_one(
            "SELECT row_to_json(d)::text FROM cs_alias_deliveries d WHERE alias=$1",
            &[&alias.value().to_string()],
        )
        .unwrap()
        .get(0);
    assert_eq!(
        host.send(&address.to_ascii_uppercase(), &raw),
        250,
        "canonical receiving address is stable across case"
    );
    let known = db
        .query_one(
            "SELECT count(*) FROM cs_alias_deliveries WHERE alias=$1 AND transport='external'",
            &[&alias.value().to_string()],
        )
        .unwrap()
        .get::<_, i64>(0);
    assert_eq!(known, 1);
    f.base
        .gateway
        .alias_command(
            &signed(
                9801,
                Command::Retire {
                    id: alias,
                    expected: aliases[0].revision(),
                },
            ),
            "mail.localhost",
        )
        .unwrap();
    assert_eq!(
        host.send(address, &raw),
        250,
        "historical exact redelivery is not a new admission"
    );
    assert_eq!(
        host.send(
            address,
            &support::smtp::signed(
                "new-team",
                address,
                "alias-new-after-retire",
                "must not deliver"
            )
        ),
        550
    );
    assert_eq!(
        before,
        db.query_one(
            "SELECT row_to_json(d)::text FROM cs_alias_deliveries d WHERE alias=$1",
            &[&alias.value().to_string()]
        )
        .unwrap()
        .get::<_, String>(0)
    );
}
