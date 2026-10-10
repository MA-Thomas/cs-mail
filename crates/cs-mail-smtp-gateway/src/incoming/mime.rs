//! The receiver's supported MIME profile. Selection never synthesizes text from
//! HTML or accepts a message by dropping an unsupported sibling part.
use super::Refusal;
use mail_parser::{HeaderName, Message, MessagePart, MimeHeaders, PartType};

pub(super) fn plain_text<'a>(message: &'a Message<'_>) -> Result<&'a str, Refusal> {
    if !message.attachments.is_empty() {
        return Err(Refusal::Unsupported);
    }
    for part in &message.parts {
        validate_headers(part)?;
    }
    match message.parts.as_slice() {
        [part] if plain(part) => text(part),
        [root, first, second]
            if root.is_content_type("multipart", "alternative")
                && matches!(&root.body, PartType::Multipart(ids) if ids.as_slice() == [1, 2]) =>
        {
            complete_alternative(message, root, first, second)?;
            match (plain(first), plain(second)) {
                (true, false) if second.is_content_type("text", "html") => text(first),
                (false, true) if first.is_content_type("text", "html") => text(second),
                _ => Err(Refusal::Unsupported),
            }
        }
        _ => Err(Refusal::Unsupported),
    }
}

fn plain(part: &MessagePart<'_>) -> bool {
    // MIME's default type is text/plain. An explicit unknown text subtype is not.
    part.content_type().is_none() || part.is_content_type("text", "plain")
}

fn text<'a>(part: &'a MessagePart<'_>) -> Result<&'a str, Refusal> {
    match &part.body {
        PartType::Text(body) => Ok(body),
        _ => Err(Refusal::Unsupported),
    }
}

fn validate_headers(part: &MessagePart<'_>) -> Result<(), Refusal> {
    if part.is_encoding_problem || part.attachment_name().is_some() {
        return Err(Refusal::Unsupported);
    }
    for name in [
        HeaderName::ContentType,
        HeaderName::ContentTransferEncoding,
        HeaderName::ContentDisposition,
    ] {
        let mut headers = part.headers.iter().filter(|h| h.name == name);
        if let Some(header) = headers.next() {
            let parsed = match name {
                HeaderName::ContentTransferEncoding => header.value.as_text().is_some(),
                _ => header.value.as_content_type().is_some(),
            };
            if !parsed || headers.next().is_some() {
                return Err(Refusal::Unsupported);
            }
        }
    }
    if part
        .content_disposition()
        .is_some_and(|d| !d.c_type.eq_ignore_ascii_case("inline"))
    {
        return Err(Refusal::Unsupported);
    }
    if let Some(kind) = part.content_type() {
        if let Some(attributes) = kind.attributes() {
            for (index, attribute) in attributes.iter().enumerate() {
                if attributes[..index]
                    .iter()
                    .any(|earlier| earlier.name.eq_ignore_ascii_case(&attribute.name))
                {
                    return Err(Refusal::Unsupported);
                }
            }
        }
        if let Some(charset) = kind.attribute("charset") {
            if !charset.eq_ignore_ascii_case("utf-8")
                && !charset.eq_ignore_ascii_case("utf8")
                && mail_parser::decoders::charsets::map::charset_decoder(charset.as_bytes())
                    .is_none()
            {
                return Err(Refusal::Unsupported);
            }
        }
    }
    if let Some(encoding) = part.content_transfer_encoding() {
        let identity = ["7bit", "8bit", "binary"]
            .iter()
            .any(|e| encoding.eq_ignore_ascii_case(e));
        let encoded = ["base64", "quoted-printable"]
            .iter()
            .any(|e| encoding.eq_ignore_ascii_case(e));
        if !identity && (!encoded || matches!(part.body, PartType::Multipart(_))) {
            return Err(Refusal::Unsupported);
        }
    }
    // The parser's charset decoders can substitute U+FFFD without reporting an
    // encoding error. Conservatively refuse that ambiguity rather than save loss.
    if matches!(&part.body, PartType::Text(s) | PartType::Html(s) if s.contains('\u{fffd}')) {
        return Err(Refusal::Unsupported);
    }
    Ok(())
}

fn complete_alternative(
    message: &Message<'_>,
    root: &MessagePart<'_>,
    first: &MessagePart<'_>,
    second: &MessagePart<'_>,
) -> Result<(), Refusal> {
    let boundary = root
        .content_type()
        .and_then(|ct| ct.attribute("boundary"))
        .ok_or(Refusal::Unsupported)?;
    // RFC 2046 boundary grammar; delimiters must occupy a complete line.
    if boundary.is_empty()
        || boundary.len() > 70
        || boundary.ends_with(' ')
        || !boundary
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"'()+_,-./:=? ".contains(&b))
    {
        return Err(Refusal::Unsupported);
    }
    let opening = format!("--{boundary}");
    let closing = format!("--{boundary}--");
    let mut position = root.offset_body as usize;
    let mut delimiters = 0;
    // mail-parser recovers missing closing boundaries. Validate framing against
    // its two child offsets, without independently parsing headers or payloads.
    for line in message.raw_message[position..].split_inclusive(|b| *b == b'\n') {
        position += line.len();
        let line = line.strip_suffix(b"\n").unwrap_or(line);
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let line = line.trim_ascii_end();
        if line.starts_with(opening.as_bytes()) {
            let valid = match delimiters {
                0 => line == opening.as_bytes() && position == first.offset_header as usize,
                1 => line == opening.as_bytes() && position == second.offset_header as usize,
                2 => line == closing.as_bytes(),
                _ => false,
            };
            if !valid {
                return Err(Refusal::Unsupported);
            }
            delimiters += 1;
        }
    }
    if delimiters != 3 {
        return Err(Refusal::Unsupported);
    }
    Ok(())
}
