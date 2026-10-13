//! Signed delivery check using preserved owner identity and original fictional peer endpoints.
//! The check cache is durable ciphertext, so an interrupted send retains its exact operation.
use super::*;
pub(super) fn check(
    client: &cs_mail_client::member::MemberClient,
    root: &Path,
    owner: u128,
) -> Result<()> {
    use cs_mail_client::member::Transport;
    let transport = cs_mail_client::member::HttpTransport::new()?;
    for subject in ["maya", "daniel", "elena", "jules"] {
        let participant =
            read_private(&root.join("participants").join(subject).join("member.json"))?;
        let ids = cs_mail_application::accounts::EnrollmentIdentifiers::for_operation(
            participant["enrollment"]
                .as_str()
                .ok_or("missing original enrollment")?,
        );
        let other = ids.persona.0.to_string();
        let conversation = client.conversation(&other)?;
        let thread = conversation
            .threads
            .iter()
            .find(|thread| thread.request.is_some())
            .ok_or("missing original request conversation")?;
        require(
            thread.reply_allowed
                || matches!(
                    conversation.reply,
                    cs_mail_client::member::ReplyRoute::Standing
                        | cs_mail_client::member::ReplyRoute::Lane
                ),
        )?;
        let marker = format!("preserved-reply-check-{subject}");
        let existing = conversation.messages.iter().find(|message| {
            message.conversation.as_deref() == Some(thread.id.as_str())
                && message.outgoing
                && message.text == marker
        });
        let message = if let Some(existing) = existing {
            cs_mail_primitives::MessageId(existing.id.parse()?)
        } else {
            client.send_message(&other, Some(thread.id.parse()?), &marker, &[])?
        };
        let request = cs_mail_application::correspondence::SignedRequest {
            product: "cs-mail/dev".into(),
            account: cs_mail_primitives::AccountId(number(&participant["account"])?),
            persona: ids.persona,
            key: ids.key,
            operation: cs_mail_primitives::IdempotencyKey(message.0),
            command: cs_mail_application::correspondence::Command::Fetch { message },
            signature: Vec::new(),
        }
        .sign(&key(&participant["device_secret"])?)?;
        let bytes = transport.request(
            "http://127.0.0.1:9693",
            "v1/correspondence/commands",
            Some(&serde_json::to_vec(&request)?),
        )?;
        let response: cs_mail_application::correspondence::Response =
            serde_json::from_slice(&bytes)?;
        let cs_mail_application::correspondence::Response::Message(view) = response else {
            return Err("peer did not receive preserved-conversation reply".into());
        };
        require(
            view.record.author == ProtocolIdentity(owner)
                && view.record.manifest.message() == message,
        )?;
        let (secret, public) = EndpointSecretKey::from_secret_bytes(
            ContentKeyRef(number(&participant["content_key"])?),
            &key(&participant["content_secret"])?,
        )?;
        let endpoint = EndpointKeys {
            owner: ids.persona,
            secret: &secret,
            public,
        };
        let document = open(&endpoint, &view, view.record.sender_key)?;
        require(readable_text(&document) == marker)?;
    }
    println!(
        "All four preserved conversations accepted replies under the original owner identity; original fictional peer endpoints decrypted each exact reply."
    );
    Ok(())
}
