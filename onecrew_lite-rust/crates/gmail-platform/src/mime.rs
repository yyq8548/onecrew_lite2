use super::client::Message;
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use mailparse::{MailHeaderMap, ParsedMail};
use onecrew_email_contracts::{OutboundEmail, PlatformEmailMessage, PlatformEmailRequest};
use sha2::{Digest, Sha256};

pub(super) fn encode(from: &str, input: &PlatformEmailRequest) -> Result<String, String> {
    OutboundEmail {
        from_name: None,
        from_email: from.into(),
        to: input.to.clone(),
        cc: input.cc.clone(),
        bcc: input.bcc.clone(),
        reply_to: input.reply_to.clone(),
        in_reply_to: input.in_reply_to.clone(),
        references: input.references.clone(),
        idempotency_key: input.idempotency_key.clone(),
        subject: input.subject.clone(),
        text: input.text.clone(),
        html: input.html.clone(),
    }
    .validate()
    .map_err(|_| "gmail_message_invalid".to_owned())?;
    if input.in_reply_to.is_some() || !input.references.is_empty() {
        return Err("gmail_reply_not_supported".into());
    }
    if std::iter::once(from)
        .chain(std::iter::once(input.subject.as_str()))
        .chain(
            input
                .to
                .iter()
                .chain(&input.cc)
                .chain(&input.bcc)
                .chain(&input.reply_to)
                .map(String::as_str),
        )
        .any(|s| s.contains(['\r', '\n', '\0']))
    {
        return Err("gmail_header_invalid".into());
    }
    if input.text.as_ref().map_or(0, String::len) + input.html.as_ref().map_or(0, String::len)
        > 2 * 1024 * 1024
    {
        return Err("gmail_message_too_large".into());
    }
    // Stable per durable action; never interpreted as provider-side idempotency support.
    let digest = format!(
        "{:x}",
        Sha256::digest(format!("{from}\0{}", input.idempotency_key))
    );
    let mut mail = format!(
        "From: {from}\r\nMessage-ID: <{digest}@onecrew.local>\r\nDate: {}\r\nMIME-Version: 1.0\r\n",
        chrono::Utc::now().to_rfc2822()
    );
    for (name, values) in [
        ("To", &input.to),
        ("Cc", &input.cc),
        ("Bcc", &input.bcc),
        ("Reply-To", &input.reply_to),
    ] {
        if !values.is_empty() {
            mail.push_str(&format!("{name}: {}\r\n", values.join(", ")));
        }
    }
    // Split encoded words on UTF-8 character boundaries, keeping each below RFC 2047's 75 octets.
    let mut words = Vec::new();
    let mut chunk = String::new();
    for c in input.subject.chars() {
        if chunk.len() + c.len_utf8() > 42 {
            words.push(STANDARD.encode(chunk.as_bytes()));
            chunk.clear();
        }
        chunk.push(c);
    }
    if !chunk.is_empty() {
        words.push(STANDARD.encode(chunk.as_bytes()));
    }
    mail.push_str(&format!(
        "Subject: {}\r\n",
        words
            .iter()
            .map(|s| format!("=?UTF-8?B?{s}?="))
            .collect::<Vec<_>>()
            .join("\r\n ")
    ));
    let part = |kind: &str, body: &str| {
        let encoded = STANDARD.encode(body);
        let lines = encoded
            .as_bytes()
            .chunks(76)
            .map(|c| std::str::from_utf8(c).expect("base64 ASCII"))
            .collect::<Vec<_>>()
            .join("\r\n");
        format!(
            "Content-Type: {kind}; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n{lines}\r\n"
        )
    };
    match (&input.text, &input.html) {
        (Some(text), Some(html)) => {
            let boundary = format!("onecrew-{digest}");
            mail.push_str(&format!("Content-Type: multipart/alternative; boundary=\"{boundary}\"\r\n\r\n--{boundary}\r\n{}--{boundary}\r\n{}--{boundary}--\r\n", part("text/plain", text), part("text/html", html)));
        }
        (Some(text), None) => mail.push_str(&part("text/plain", text)),
        (None, Some(html)) => mail.push_str(&part("text/html", html)),
        _ => return Err("gmail_body_required".into()),
    }
    Ok(URL_SAFE_NO_PAD.encode(mail))
}

pub(super) fn decode(message: Message, expected: &str) -> Result<PlatformEmailMessage, String> {
    if message.id != expected || !message.label_ids.iter().any(|l| l == "SENT") {
        return Err("gmail_sent_message_mismatch".into());
    }
    decode_message(message, false, None)
}

pub(super) fn decode_inbox(
    message: Message,
    expected: &str,
    recipient: &str,
) -> Result<PlatformEmailMessage, String> {
    if message.id != expected || message.label_ids.iter().any(|l| l == "DRAFT") {
        return Err("gmail_received_message_mismatch".into());
    }
    decode_message(message, true, Some(recipient))
}

fn decode_message(
    message: Message,
    inbound: bool,
    recipient: Option<&str>,
) -> Result<PlatformEmailMessage, String> {
    let bytes = URL_SAFE_NO_PAD
        .decode(message.raw.trim_end_matches('='))
        .map_err(|_| "gmail_mime_invalid")?;
    let mail = mailparse::parse_mail(&bytes).map_err(|_| "gmail_mime_invalid")?;
    let header = |name| mail.headers.get_first_value(name).unwrap_or_default();
    let addresses = |name| -> Result<Vec<String>, String> {
        let value = header(name);
        if value.is_empty() {
            return Ok(vec![]);
        }
        let parsed = mailparse::addrparse(&value).map_err(|_| "gmail_address_invalid")?;
        Ok(parsed
            .iter()
            .flat_map(|a| match a {
                mailparse::MailAddr::Single(a) => vec![a.addr.clone()],
                mailparse::MailAddr::Group(g) => g.addrs.iter().map(|a| a.addr.clone()).collect(),
            })
            .collect())
    };
    let mut text = None;
    let mut html = None;
    fn body(
        mail: &ParsedMail<'_>,
        text: &mut Option<String>,
        html: &mut Option<String>,
        depth: usize,
    ) -> Result<(), String> {
        if depth > 16 {
            return Err("gmail_mime_depth_exceeded".into());
        }
        // Attached text files and forwarded messages are not the message body.
        let disposition = mail.get_content_disposition();
        if disposition.disposition == mailparse::DispositionType::Attachment
            || disposition.params.contains_key("filename")
            || mail.ctype.params.contains_key("name")
        {
            return Ok(());
        }
        if mail.ctype.mimetype == "text/plain" && text.is_none() {
            *text = Some(mail.get_body().map_err(|_| "gmail_body_invalid")?);
        }
        if mail.ctype.mimetype == "text/html" && html.is_none() {
            *html = Some(mail.get_body().map_err(|_| "gmail_body_invalid")?);
        }
        for part in &mail.subparts {
            body(part, text, html, depth + 1)?;
        }
        Ok(())
    }
    body(&mail, &mut text, &mut html, 0)?;
    let timestamp = message
        .internal_date
        .parse::<i64>()
        .map_err(|_| "gmail_date_invalid")?;
    let created_at = chrono::DateTime::from_timestamp_millis(timestamp)
        .ok_or("gmail_date_invalid")?
        .to_rfc3339();
    let headers = mail
        .headers
        .iter()
        .map(|h| {
            (
                h.get_key().to_ascii_lowercase(),
                serde_json::Value::String(h.get_value()),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    Ok(PlatformEmailMessage {
        provider_message_id: message.id,
        internet_message_id: mail.headers.get_first_value("Message-ID"),
        from: header("From"),
        to: addresses("To")?,
        cc: addresses("Cc")?,
        bcc: addresses("Bcc")?,
        reply_to: addresses("Reply-To")?,
        received_for: recipient.map(|s| vec![s.to_owned()]).unwrap_or_default(),
        subject: header("Subject"),
        text,
        html,
        headers: headers.into(),
        last_event: if inbound { "received" } else { "sent" }.into(),
        created_at,
        inbound,
        attachments: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inbox_preserves_account_delivery_and_ignores_text_attachments() {
        let raw = concat!(
            "From: Sender <sender@example.com>\r\nTo: list@example.com\r\n",
            "Subject: =?UTF-8?B?5rWL6K+V?=\r\nMessage-ID: <incoming@example.com>\r\n",
            "Content-Type: multipart/mixed; boundary=parts\r\n\r\n",
            "--parts\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nBody\r\n",
            "--parts\r\nContent-Type: text/plain\r\nContent-Disposition: attachment; filename=private.txt\r\n\r\nAttachment\r\n--parts--\r\n"
        );
        let make = |labels| Message {
            id: "ab12".into(),
            raw: URL_SAFE_NO_PAD.encode(raw),
            label_ids: labels,
            internal_date: "1791786972000".into(),
        };
        let decoded =
            decode_inbox(make(vec!["INBOX".into()]), "ab12", "owner@example.com").unwrap();
        assert!(decoded.inbound);
        assert_eq!(decoded.received_for, ["owner@example.com"]);
        assert_eq!(decoded.to, ["list@example.com"]);
        assert_eq!(decoded.subject, "测试");
        assert_eq!(decoded.text.as_deref(), Some("Body"));
        assert_eq!(decoded.last_event, "received");
        assert!(decode_inbox(make(vec![]), "ab13", "owner@example.com").is_err());
        assert!(decode_inbox(make(vec!["DRAFT".into()]), "ab12", "owner@example.com").is_err());
    }
    fn request() -> PlatformEmailRequest {
        PlatformEmailRequest {
            to: vec!["receiver@example.com".into()],
            cc: vec![],
            bcc: vec![],
            reply_to: vec![],
            in_reply_to: None,
            references: vec![],
            subject: "OneCrew Gmail 接入测试".into(),
            text: Some("这是测试邮件。".into()),
            html: Some("<p>这是测试邮件。</p>".into()),
            idempotency_key: "test-once".into(),
        }
    }
    #[test]
    fn unicode_multipart_roundtrip_and_stable_message_identity() {
        let input = request();
        let raw = encode("sender@example.com", &input).unwrap();
        let make = |raw| Message {
            id: "abc123".into(),
            raw,
            label_ids: vec!["SENT".into()],
            internal_date: "1791786972000".into(),
        };
        let actual = decode(make(raw), "abc123").unwrap();
        assert_eq!(actual.subject, input.subject);
        assert_eq!(actual.text, input.text);
        assert_eq!(actual.html, input.html);
        assert_eq!(actual.to, input.to);
        assert_eq!(
            actual.internet_message_id,
            decode(
                make(encode("sender@example.com", &input).unwrap()),
                "abc123"
            )
            .unwrap()
            .internet_message_id
        );
    }
    #[test]
    fn rejects_header_injection_and_wrong_readback() {
        let mut input = request();
        input.subject = "hello\r\nBcc: other@example.com".into();
        assert!(encode("sender@example.com", &input).is_err());
        let message = Message {
            id: "abc123".into(),
            raw: "".into(),
            label_ids: vec!["INBOX".into()],
            internal_date: "0".into(),
        };
        assert!(decode(message, "abc123").is_err());
    }
}
