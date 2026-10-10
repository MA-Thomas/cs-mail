//! Bounded receive-only SMTP transport. Permission and persistence are delegated.
use crate::incoming::{MAX_EMAIL_BYTES, Refusal, destination_address};
use cs_mail_adapters::DomainIdentity;
use std::{future::Future, io, net::IpAddr, pin::Pin, sync::Arc, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::TlsAcceptor;

pub struct IncomingMail {
    pub remote_ip: IpAddr,
    pub helo: String,
    pub mail_from: Option<String>,
    pub recipient: String,
    pub data: Vec<u8>,
}
pub trait Receiver: Send + Sync {
    /// Success MUST mean the complete indicated transaction has committed.
    fn deliver(
        &self,
        mail: IncomingMail,
    ) -> Pin<Box<dyn Future<Output = Result<Received, Refusal>> + Send + '_>>;
}
pub enum Received {
    Admitted,
    RequestLinkHandled,
}
trait Socket: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Socket for T {}
type Stream = Box<dyn Socket>;
enum Phase {
    Upgrade(Stream),
    Done,
}
fn failure() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid SMTP framing")
}
async fn line(reader: &mut BufReader<Stream>, max: usize) -> io::Result<Vec<u8>> {
    timeout(Duration::from_secs(30), async {
        let mut result = Vec::new();
        loop {
            let available = reader.fill_buf().await?;
            if available.is_empty() {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            let used = available
                .iter()
                .position(|b| *b == b'\n')
                .map_or(available.len(), |n| n + 1);
            if result.len() + used > max {
                return Err(failure());
            }
            result.extend_from_slice(&available[..used]);
            reader.consume(used);
            if result.ends_with(b"\n") {
                if !result.ends_with(b"\r\n") || result.contains(&0) {
                    return Err(failure());
                }
                return Ok(result);
            }
        }
    })
    .await
    .map_err(|_| io::ErrorKind::TimedOut)?
}
async fn reply(reader: &mut BufReader<Stream>, text: &str) -> io::Result<()> {
    timeout(
        Duration::from_secs(30),
        reader.get_mut().write_all(text.as_bytes()),
    )
    .await
    .map_err(|_| io::ErrorKind::TimedOut)??;
    reader.get_mut().flush().await
}
fn path<'a>(argument: &'a str, prefix: &str) -> Option<(&'a str, &'a str)> {
    if !argument.get(..prefix.len())?.eq_ignore_ascii_case(prefix) {
        return None;
    }
    let rest = argument[prefix.len()..].trim_start().strip_prefix('<')?;
    let (address, params) = rest.split_once('>')?;
    Some((address, params))
}
async fn data(reader: &mut BufReader<Stream>) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    loop {
        let row = line(reader, 1001).await?;
        if row == b".\r\n" {
            return Ok(bytes);
        }
        let row = row.strip_prefix(b".").unwrap_or(&row);
        if row.len() > 1000 || bytes.len() + row.len() > MAX_EMAIL_BYTES {
            return Err(failure());
        }
        bytes.extend_from_slice(row);
    }
}
#[allow(clippy::too_many_lines)] // The protocol state and its resets are reviewed together.
async fn phase(
    stream: Stream,
    encrypted: bool,
    peer: IpAddr,
    domain: &DomainIdentity,
    receiver: &dyn Receiver,
) -> io::Result<Phase> {
    let mut io = BufReader::new(stream);
    let mut helo = None;
    let mut sender: Option<Option<String>> = None;
    let mut recipient = None;
    if !encrypted {
        reply(
            &mut io,
            &format!("220 {} ESMTP cs-mail\r\n", domain.as_str()),
        )
        .await?;
    }
    for _ in 0..100 {
        let raw = match line(&mut io, 512).await {
            Ok(line) => line,
            Err(e) => {
                let _ = reply(&mut io, "421 4.5.0 Closing connection\r\n").await;
                return Err(e);
            }
        };
        let Ok(command) = std::str::from_utf8(&raw[..raw.len() - 2]) else {
            return Err(failure());
        };
        if !command.is_ascii() {
            return Err(failure());
        }
        let (verb, arg) = command.split_once(' ').unwrap_or((command, ""));
        match verb.to_ascii_uppercase().as_str() {
            "QUIT" if arg.is_empty() => {
                reply(&mut io, "221 2.0.0 Goodbye\r\n").await?;
                return Ok(Phase::Done);
            }
            "NOOP" => reply(&mut io, "250 2.0.0 OK\r\n").await?,
            "RSET" if arg.is_empty() => {
                sender = None;
                recipient = None;
                reply(&mut io, "250 2.0.0 Reset\r\n").await?;
            }
            "EHLO" | "HELO" if !arg.is_empty() && !arg.bytes().any(|b| b <= 32 || b >= 127) => {
                helo = Some(arg.to_owned());
                sender = None;
                recipient = None;
                let answer = if verb.eq_ignore_ascii_case("HELO") {
                    format!("250 {}\r\n", domain.as_str())
                } else {
                    format!(
                        "250-{}\r\n250-SIZE {MAX_EMAIL_BYTES}\r\n250-8BITMIME\r\n{}250 ENHANCEDSTATUSCODES\r\n",
                        domain.as_str(),
                        if encrypted { "" } else { "250-STARTTLS\r\n" }
                    )
                };
                reply(&mut io, &answer).await?;
            }
            "STARTTLS" if arg.is_empty() && !encrypted && helo.is_some() && sender.is_none() => {
                // Bytes pipelined in cleartext must never become post-TLS commands.
                if !io.buffer().is_empty() {
                    reply(&mut io, "554 5.5.0 STARTTLS must end the command group\r\n").await?;
                    return Ok(Phase::Done);
                }
                reply(&mut io, "220 2.0.0 Ready for TLS\r\n").await?;
                return Ok(Phase::Upgrade(io.into_inner()));
            }
            "MAIL" if helo.is_some() && sender.is_none() => {
                let Some((address, params)) = path(arg, "FROM:") else {
                    reply(&mut io, "501 5.5.2 Invalid reverse path\r\n").await?;
                    continue;
                };
                let valid =
                    address.is_empty() || cs_mail_guests::GuestMailbox::parse(address).is_ok();
                let parameters = params.split_whitespace().all(|p| {
                    p.eq_ignore_ascii_case("BODY=8BITMIME")
                        || p.eq_ignore_ascii_case("BODY=7BIT")
                        || p.get(..5).is_some_and(|s| s.eq_ignore_ascii_case("SIZE="))
                            && p[5..]
                                .parse::<usize>()
                                .is_ok_and(|size| size <= MAX_EMAIL_BYTES)
                });
                if !valid || !parameters {
                    reply(&mut io, "501 5.5.4 Unsupported sender or parameters\r\n").await?;
                    continue;
                }
                sender = Some((!address.is_empty()).then(|| address.to_owned()));
                reply(&mut io, "250 2.1.0 Sender noted\r\n").await?;
            }
            "RCPT" if sender.is_some() => {
                if recipient.is_some() {
                    reply(&mut io, "452 4.5.3 One recipient per transaction\r\n").await?;
                    continue;
                }
                let Some((address, "")) = path(arg, "TO:") else {
                    reply(&mut io, "501 5.5.2 Invalid recipient\r\n").await?;
                    continue;
                };
                if destination_address(address, domain).is_err() {
                    reply(&mut io, "550 5.1.1 Unknown route\r\n").await?;
                    continue;
                }
                recipient = Some(address.to_owned());
                reply(&mut io, "250 2.1.5 Recipient noted\r\n").await?;
            }
            "DATA" if arg.is_empty() && sender.is_some() && recipient.is_some() => {
                reply(&mut io, "354 End with <CRLF>.<CRLF>\r\n").await?;
                let bytes = match timeout(Duration::from_secs(120), data(&mut io)).await {
                    Ok(Ok(bytes)) => bytes,
                    Ok(Err(error)) if error.kind() == io::ErrorKind::InvalidData => {
                        reply(&mut io, "552 5.3.4 Message too large or malformed\r\n").await?;
                        return Ok(Phase::Done);
                    }
                    _ => {
                        reply(&mut io, "451 4.4.2 Message transfer interrupted\r\n").await?;
                        return Ok(Phase::Done);
                    }
                };
                let mail = IncomingMail {
                    remote_ip: peer,
                    helo: helo.clone().ok_or_else(failure)?,
                    mail_from: sender.take().ok_or_else(failure)?,
                    recipient: recipient.take().ok_or_else(failure)?,
                    data: bytes,
                };
                let status = match receiver.deliver(mail).await {
                    Ok(Received::Admitted) => "250 2.0.0 Durably admitted\r\n",
                    Ok(Received::RequestLinkHandled) => {
                        "250 2.0.0 Not delivered to member; request instructions queued or already requested today\r\n"
                    }
                    Err(Refusal::Temporary) => "451 4.3.0 Please retry later\r\n",
                    Err(Refusal::Permanent) => "550 5.7.1 Message not admitted\r\n",
                    Err(Refusal::TooLarge) => "552 5.3.4 Message too large\r\n",
                    Err(Refusal::Unsupported) => "554 5.6.3 Only plain text is supported\r\n",
                };
                reply(&mut io, status).await?;
            }
            "AUTH" | "VRFY" | "EXPN" => {
                reply(&mut io, "502 5.5.1 Command not available\r\n").await?;
            }
            _ => reply(&mut io, "503 5.5.1 Invalid command or sequence\r\n").await?,
        }
    }
    reply(&mut io, "421 4.5.0 Session limit\r\n").await?;
    Ok(Phase::Done)
}
/// # Errors
/// Closes on framing, I/O, handshake or timeout failure. No content is logged.
pub async fn session(
    stream: TcpStream,
    peer: IpAddr,
    domain: Arc<DomainIdentity>,
    tls: TlsAcceptor,
    receiver: Arc<dyn Receiver>,
) -> io::Result<()> {
    timeout(Duration::from_secs(300), async {
        if let Phase::Upgrade(stream) =
            phase(Box::new(stream), false, peer, &domain, &*receiver).await?
        {
            let encrypted = timeout(Duration::from_secs(15), tls.accept(stream))
                .await
                .map_err(|_| io::ErrorKind::TimedOut)??;
            phase(Box::new(encrypted), true, peer, &domain, &*receiver).await?;
        }
        Ok(())
    })
    .await
    .map_err(|_| io::ErrorKind::TimedOut)?
}
