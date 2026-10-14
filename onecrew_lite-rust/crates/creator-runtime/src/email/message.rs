use std::collections::HashSet;

use chrono::DateTime;

use crate::{
    CreatorContactRouteRecord, EmailDirection, EmailMessageInput, EmailThread, MailboxDetail,
};
use onecrew_email_contracts::{PlatformEmailMessage, SentDelivery, sent_delivery};

pub(crate) fn reply_headers(
    reply_to: Option<&crate::EmailMessageView>,
) -> (Option<String>, Vec<String>) {
    let Some(reply_to) = reply_to else {
        return (None, Vec::new());
    };
    let message_id = reply_to.internet_message_id.clone();
    let mut references = reply_to.references.clone();
    if let Some(message_id) = message_id.as_ref()
        && !references.contains(message_id)
    {
        references.push(message_id.clone());
    }
    (message_id, references)
}

/// 一封回复要能独立读懂,所以正文下面附上被回复的原文。这是所有邮件
/// 客户端的通行做法,也是 `In-Reply-To`/`References` 之外唯一让人**看得
/// 见**关联的东西——那两个头只让客户端把消息归到一条线程里,收件人
/// 眼睛里什么都不会多出来。
///
/// 两个部分都要产出,而且是**同一封回复的两种表达**:
///
/// - `text/plain` 每行 `>` 前缀,属性行以 `wrote:` 结尾,和桌面端
///   `quoted-body.ts` 的解析器一致,纯文本客户端也是这么折叠的。
/// - `text/html` 用 Gmail 那一种 `div.gmail_quote` + `blockquote` 结构。
///   现代客户端(Gmail、Apple Mail、飞书)折叠历史靠的**只有**这个;
///   只发纯文本时它们没有可折叠的结构,那堵 `>` 就原样铺在收件人眼前。
///
/// 所以撰写时只写了纯文本,这里也要补出 html 部分——不是锦上添花,是
/// 收件人能不能读的分界线。
///
/// 属性行用英文:收件人是世界各地的 creator,而 "On … wrote:" 是识别度
/// 最广、也是 Gmail/Apple Mail 自己产出的写法。
pub(crate) fn quote_original(
    reply_to: Option<&crate::EmailMessageView>,
    text: Option<String>,
    html: Option<String>,
) -> (Option<String>, Option<String>) {
    let Some(parent) = reply_to else {
        return (text, html);
    };
    let attribution = attribution_line(parent);
    let original_text = parent
        .text
        .as_deref()
        .map(str::trim_end)
        .filter(|value| !value.is_empty());
    // 对方的 html 常常是一整份文档(飞书发来的就是 `<html><head></head>
    // <body>…`)。整份塞进引用块会嵌套出第二个 `<html>`,各家客户端各有
    // 各的收拾办法,没一个是我们想要的,所以只取 body 里的内容。
    let original_markup = parent
        .html
        .as_deref()
        .map(html_fragment)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| original_text.map(text_markup));
    // 已经带引文的正文不再追加:草稿里可能已经写好了,重试也不该让引文
    // 一层层叠上去。
    let carried = text.as_deref().is_some_and(already_quoted);
    let author_text = text.as_deref().map(str::trim_end);

    let quoted_text = author_text.map(|body| match original_text {
        Some(original) if !carried => {
            let quoted = original
                .lines()
                .map(|line| {
                    if line.is_empty() {
                        ">".to_string()
                    } else {
                        format!("> {line}")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!("{body}\n\n{attribution}\n{quoted}")
        }
        _ => body.to_owned(),
    });

    // 纯文本里的 `>` 只有纯文本客户端会折叠。Gmail、Apple Mail、飞书认
    // 的是 html 里的 `div.gmail_quote` 与 `blockquote`——一封回复要是只有
    // text/plain,收件人看到的就是一堵 `>`。所以回复一律附上 html 部分,
    // 哪怕撰写时只写了纯文本。
    let quoted_html = match html {
        Some(body) if body.contains("<blockquote") || body.contains("gmail_quote") => Some(body),
        Some(body) => Some(match original_markup.as_deref() {
            Some(original) => reply_markup(&body, &attribution, original),
            None => body,
        }),
        None => author_text.map(|body| {
            let markup = text_markup(body);
            match original_markup.as_deref() {
                Some(original) if !carried => reply_markup(&markup, &attribution, original),
                // 正文自己已经带着引文了:如实转成标记,不再多套一层。
                _ => markup,
            }
        }),
    };
    (quoted_text, quoted_html)
}

/// 纯文本正文转成标记:先转义,再把换行变成 `<br>`。少了这一步,引文里
/// 的分行会在 html 部分里全部塌成一段。
fn text_markup(value: &str) -> String {
    format!(
        "<div dir=\"ltr\">{}</div>",
        escape_html(value).replace('\n', "<br>")
    )
}

/// Gmail 产出的那一种回复结构。`div.gmail_quote` 与 `blockquote` 是各家
/// 客户端——以及 talon 这类解析库——共同认的两个标记,两个都给上;
/// `type="cite"` 是 Apple Mail 与 Thunderbird 认的那个。
fn reply_markup(body: &str, attribution: &str, original: &str) -> String {
    format!(
        "{body}<br><div class=\"gmail_quote\">\
         <div dir=\"ltr\" class=\"gmail_attr\">{}<br></div>\
         <blockquote class=\"gmail_quote\" type=\"cite\" \
         style=\"margin:0 0 0 .8ex;border-left:1px solid rgb(204,204,204);padding-left:1ex\">\
         {original}</blockquote></div>",
        escape_html(attribution)
    )
}

/// 取一份 html 文档里 `<body>` 的内容;本来就是片段就原样返回。
fn html_fragment(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    let Some(start) = lower.find("<body") else {
        return value.to_owned();
    };
    let Some(open) = value[start..].find('>').map(|offset| start + offset + 1) else {
        return value.to_owned();
    };
    let end = lower
        .rfind("</body>")
        .filter(|end| *end >= open)
        .unwrap_or(value.len());
    value[open..end].to_owned()
}

fn attribution_line(parent: &crate::EmailMessageView) -> String {
    let when = chrono::DateTime::from_timestamp(parent.occurred_at as i64, 0)
        .map(|instant| {
            instant
                .with_timezone(&chrono::Local)
                .format("%a, %b %-d, %Y at %-I:%M %p")
                .to_string()
        })
        .unwrap_or_default();
    if when.is_empty() {
        format!("{} wrote:", parent.from)
    } else {
        format!("On {when} {} wrote:", parent.from)
    }
}

/// 与 `quoted-body.ts` 同一条保守判据:只有当引用一直延续到正文结尾时
/// 才算,句中偶然出现的 `>` 不会被误判。
fn already_quoted(body: &str) -> bool {
    let mut saw_quote = false;
    for line in body.lines().rev() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('>') {
            saw_quote = true;
            continue;
        }
        if saw_quote && line.trim().is_empty() {
            continue;
        }
        break;
    }
    saw_quote
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Whether an accepted send actually went out, read from the provider's own
/// delivery status.
///
/// Identity is still proved here: a readback for a different message, or an
/// inbound one, says nothing about this send. The *content* of the readback
/// is not compared with the request: gateways may return normalized addresses,
/// subjects, or other provider-side rewrites. `last_event` is the gateway's
/// provider-neutral answer to the only question this step asks.
pub(super) fn verify_sent_readback(
    provider_message_id: &str,
    readback: &PlatformEmailMessage,
) -> Result<SentDelivery, String> {
    if readback.inbound || readback.provider_message_id != provider_message_id {
        return Err("provider sent-email readback did not match the accepted message".into());
    }
    Ok(sent_delivery(&readback.last_event))
}

pub(super) fn validate_inbound(
    mailbox: &MailboxDetail,
    message: &PlatformEmailMessage,
) -> Result<(), String> {
    if !message.inbound
        || message.provider_message_id.trim().is_empty()
        || message.from.trim().is_empty()
    {
        return Err("Resend receiving detail is not a valid inbound email".into());
    }
    let mailbox_address = normalize_email_address(&mailbox.email_address)?;
    let addressed = message
        .to
        .iter()
        .chain(message.cc.iter())
        .chain(message.bcc.iter())
        .chain(message.received_for.iter())
        .any(|address| normalize_email_address(address).as_deref() == Ok(mailbox_address.as_str()));
    if !addressed {
        return Err("received email was not addressed to the selected mailbox".into());
    }
    Ok(())
}

pub(crate) fn provider_message_input(
    message: PlatformEmailMessage,
    direction: EmailDirection,
    origin: crate::models::EmailOrigin,
    requested_in_reply_to: Option<String>,
    requested_references: Vec<String>,
    delivery_status: &str,
) -> Result<EmailMessageInput, String> {
    let in_reply_to = header_message_ids(&message.headers, "in-reply-to")
        .into_iter()
        .next()
        .or(requested_in_reply_to);
    let references = {
        let discovered = header_message_ids(&message.headers, "references");
        if discovered.is_empty() {
            requested_references
        } else {
            discovered
        }
    };
    Ok(EmailMessageInput {
        provider_message_id: message.provider_message_id,
        internet_message_id: message.internet_message_id.or_else(|| {
            header_message_ids(&message.headers, "message-id")
                .into_iter()
                .next()
        }),
        direction,
        from: message.from,
        to: message.to,
        cc: message.cc,
        bcc: message.bcc,
        received_for: message.received_for,
        reply_to: message.reply_to,
        in_reply_to,
        references,
        subject: message.subject,
        text: message.text,
        html: message.html,
        delivery_status: delivery_status.into(),
        headers: message.headers,
        occurred_at: parse_provider_time(&message.created_at)?,
        origin,
        attachments: message
            .attachments
            .into_iter()
            .map(|attachment| crate::models::EmailAttachmentInput {
                provider_attachment_id: attachment.provider_attachment_id,
                filename: attachment.filename,
                content_type: attachment.content_type,
                content_id: attachment.content_id,
                content_disposition: attachment.content_disposition,
                size_bytes: attachment.size_bytes,
                available: true,
            })
            .collect(),
    })
}

pub(super) fn safe_inbound_thread(
    thread: Option<EmailThread>,
    owner: Option<CreatorContactRouteRecord>,
) -> (Option<EmailThread>, Option<CreatorContactRouteRecord>) {
    match (thread, owner) {
        (Some(thread), Some(owner))
            if thread
                .creator_id
                .as_ref()
                .is_none_or(|id| *id == owner.route.creator_id)
                && thread
                    .contact_route_id
                    .as_ref()
                    .is_none_or(|id| *id == owner.route.id) =>
        {
            (Some(thread), Some(owner))
        }
        (Some(thread), None)
            if thread.creator_id.is_none() && thread.contact_route_id.is_none() =>
        {
            (Some(thread), None)
        }
        (None, owner) => (None, owner),
        _ => (None, None),
    }
}

pub(super) fn message_reference_ids(message: &PlatformEmailMessage) -> Vec<String> {
    let mut ids = header_message_ids(&message.headers, "references");
    ids.extend(header_message_ids(&message.headers, "in-reply-to"));
    ids.dedup();
    ids
}

pub(super) fn inbound_thread_key(message: &PlatformEmailMessage) -> String {
    message_reference_ids(message)
        .into_iter()
        .next()
        .map(|reference| format!("resend:reference:{reference}"))
        .or_else(|| {
            message
                .internet_message_id
                .as_ref()
                .map(|message_id| format!("resend:message:{message_id}"))
        })
        .unwrap_or_else(|| format!("resend:message:{}", message.provider_message_id))
}

pub(super) fn isolated_inbound_thread_key(message: &PlatformEmailMessage) -> String {
    format!("resend:unowned:{}", message.provider_message_id.trim())
}

pub(super) fn header_message_ids(headers: &serde_json::Value, name: &str) -> Vec<String> {
    let Some(object) = headers.as_object() else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for (key, value) in object {
        if !key.eq_ignore_ascii_case(name) {
            continue;
        }
        let values = match value {
            serde_json::Value::String(value) => vec![value.as_str()],
            serde_json::Value::Array(values) => values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect(),
            _ => Vec::new(),
        };
        for value in values {
            result.extend(split_message_ids(value));
        }
    }
    let mut seen = HashSet::new();
    result.retain(|value| seen.insert(value.clone()));
    result
}

fn split_message_ids(value: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut rest = value;
    while let Some(start) = rest.find('<') {
        let after = &rest[start..];
        let Some(end) = after.find('>') else {
            break;
        };
        result.push(after[..=end].to_owned());
        rest = &after[end + 1..];
    }
    if result.is_empty() {
        result.extend(value.split_whitespace().map(str::to_owned));
    }
    result
}

pub(crate) fn normalize_email_address(value: &str) -> Result<String, String> {
    crate::EmailAddress::parse(value).map(crate::EmailAddress::into_string)
}

pub(super) fn parse_provider_time(value: &str) -> Result<u64, String> {
    if value.trim().is_empty() {
        return Err("invalid Resend timestamp: timestamp is empty".into());
    }
    let timestamp = DateTime::parse_from_rfc3339(value)
        // Resend and resend-rs have also returned PostgreSQL timestamps such
        // as `2026-10-09 14:37:40.951732+00`. `%#z` intentionally accepts an
        // offset with or without minutes while the rest of the shape remains
        // strict.
        .or_else(|_| DateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f%#z"))
        .or_else(|_| DateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%#z"))
        .map_err(|error| format!("invalid Resend timestamp: {error}"))?
        .timestamp();
    u64::try_from(timestamp).map_err(|_| "Resend timestamp predates Unix epoch".into())
}

#[cfg(test)]
mod quoting_tests {
    use super::{already_quoted, quote_original, reply_headers};
    use crate::{EmailDirection, EmailId, EmailMessageView, EmailThreadId};

    fn parent(text: Option<&str>, html: Option<&str>) -> EmailMessageView {
        EmailMessageView {
            id: EmailId::new("email-1".to_string()),
            thread_id: EmailThreadId::new("thread-1".to_string()),
            provider_message_id: "provider-1".into(),
            internet_message_id: Some("<parent@mail>".into()),
            direction: EmailDirection::Inbound,
            from: "Alice <alice@example.com>".into(),
            to: vec!["me@example.com".into()],
            cc: Vec::new(),
            reply_to: Vec::new(),
            in_reply_to: None,
            references: vec!["<root@mail>".into()],
            subject: "合作".into(),
            text: text.map(str::to_string),
            html: html.map(str::to_string),
            delivery_status: "read".into(),
            occurred_at: 1_755_000_000,
        }
    }

    #[test]
    fn threading_headers_carry_the_whole_ancestry() {
        // References 是「祖先 + 父」,不是只有父:中间断一环,客户端就
        // 可能把后续回复挂成新线程。
        let (in_reply_to, references) = reply_headers(Some(&parent(Some("hi"), None)));
        assert_eq!(in_reply_to.as_deref(), Some("<parent@mail>"));
        assert_eq!(references, vec!["<root@mail>", "<parent@mail>"]);
    }

    #[test]
    fn a_reply_carries_the_original_beneath_it() {
        let (text, _) = quote_original(
            Some(&parent(Some("能寄样品吗？\n另外报个价"), None)),
            Some("可以，明天寄".into()),
            None,
        );
        let text = text.expect("text");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "可以，明天寄");
        // 属性行必须以 wrote: 结尾 —— 桌面端 quoted-body.ts 就是这么认的,
        // 收件人的客户端也是。
        assert!(lines[2].starts_with("On "), "{}", lines[2]);
        assert!(lines[2].ends_with(" wrote:"), "{}", lines[2]);
        assert!(lines[2].contains("Alice <alice@example.com>"));
        assert_eq!(&lines[3..], ["> 能寄样品吗？", "> 另外报个价"]);
    }

    #[test]
    fn the_whole_chain_is_carried_forward_not_just_the_last_turn() {
        // 邮件回复一轮轮累积:父邮件自己带的历史正是要继续带下去的东西。
        // 只引用父邮件「新写的那几句」,等于每回一次就砍掉一层。
        let parent_text = "收到,下周聊\n\nOn earlier, River wrote:\n> 最初那封";
        let (text, _) = quote_original(
            Some(&parent(Some(parent_text), None)),
            Some("好的".into()),
            None,
        );
        let text = text.expect("text");
        assert!(text.contains("> 收到,下周聊"), "{text}");
        assert!(text.contains("> On earlier, River wrote:"), "{text}");
        assert!(
            text.contains("> > 最初那封"),
            "父邮件的引用层级要保留:\n{text}"
        );
    }

    #[test]
    fn nothing_is_added_when_there_is_nothing_to_reply_to() {
        let (text, html) = quote_original(None, Some("首次触达".into()), Some("<p>x</p>".into()));
        assert_eq!(text.as_deref(), Some("首次触达"));
        assert_eq!(html.as_deref(), Some("<p>x</p>"));
    }

    #[test]
    fn a_body_that_already_quotes_is_left_alone() {
        // 重试同一次发送、或草稿里本来就写了引文,都不该让引文层层叠加。
        let body = "好的\n\nOn earlier, Bob wrote:\n> 原文";
        let (text, _) = quote_original(Some(&parent(Some("原文"), None)), Some(body.into()), None);
        assert_eq!(text.as_deref(), Some(body));
        assert!(already_quoted(body));
        assert!(!already_quoted("好的\n> 中间引一句\n然后继续写"));
    }

    #[test]
    fn html_replies_quote_as_a_blockquote_and_escape_plain_text() {
        let (_, html) = quote_original(
            Some(&parent(Some("1 < 2 & 3"), None)),
            None,
            Some("<p>收到</p>".into()),
        );
        let html = html.expect("html");
        assert!(html.starts_with("<p>收到</p>"));
        assert!(html.contains("<blockquote class=\"gmail_quote\" type=\"cite\""));
        // 原文是纯文本时必须转义,否则它会被当成标记渲染。
        assert!(html.contains("1 &lt; 2 &amp; 3"));
    }

    #[test]
    fn a_plain_text_reply_still_goes_out_with_an_html_part() {
        // 收件人抱怨过的那件事:只发 text/plain,飞书没有可折叠的结构,
        // 那堵 `>` 就原样铺在正文里。html 部分不是锦上添花。
        let (text, html) = quote_original(
            Some(&parent(Some("能寄样品吗？"), None)),
            Some("可以，明天寄".into()),
            None,
        );
        assert!(text.expect("text").contains("> 能寄样品吗？"));
        let html = html.expect("只写了纯文本的回复同样要带 html 部分");
        // 各家客户端认的两个标记都要在:talon 之类的解析库按 div.gmail_quote
        // 走,Apple Mail/Thunderbird 按 blockquote[type=cite] 走。
        assert!(html.contains("<div class=\"gmail_quote\">"), "{html}");
        assert!(html.contains("class=\"gmail_attr\">On "), "{html}");
        assert!(
            html.contains("<blockquote class=\"gmail_quote\" type=\"cite\""),
            "{html}"
        );
        // 引文里是原文本身,不是带 `>` 的那一份——`>` 只属于纯文本部分。
        assert!(html.contains("能寄样品吗？"), "{html}");
        assert!(
            !html.contains("&gt; 能寄样品吗？"),
            "html 部分不该再带 `>`:\n{html}"
        );
    }

    #[test]
    fn a_first_touch_stays_plain_text() {
        // 没有要回的信就没有要折叠的历史,别凭空造一份 html 出来。
        let (text, html) = quote_original(None, Some("首次触达".into()), None);
        assert_eq!(text.as_deref(), Some("首次触达"));
        assert_eq!(html, None);
    }

    #[test]
    fn only_the_body_of_the_originals_document_is_quoted() {
        // 飞书发来的是一整份文档。整份嵌进引用块就套出了第二个 `<html>`,
        // 各家客户端各有各的收拾办法,没一个是我们想要的。
        let document = "<html><head><style>p{color:red}</style></head>\
                        <body><div>Hi River,</div></body></html>";
        let (_, html) = quote_original(
            Some(&parent(Some("Hi River,"), Some(document))),
            Some("好的".into()),
            None,
        );
        let html = html.expect("html");
        assert!(html.contains("<div>Hi River,</div>"), "{html}");
        assert!(
            !html.contains("<html>"),
            "引文里不该再有一份文档外壳:\n{html}"
        );
        assert!(!html.contains("<head>"), "{html}");
        assert!(!html.contains("color:red"), "{html}");
    }

    #[test]
    fn a_body_that_already_quotes_gets_no_second_html_quote() {
        // 正文自己带着引文时,html 部分如实转就是,不能再套一层引用块,
        // 否则收件人会看到同一段历史出现两次。
        let body = "好的\n\nOn earlier, Bob wrote:\n> 原文";
        let (_, html) = quote_original(Some(&parent(Some("原文"), None)), Some(body.into()), None);
        let html = html.expect("html");
        assert!(!html.contains("gmail_quote"), "{html}");
        assert!(html.contains("&gt; 原文"), "{html}");
        // 换行不能塌成一段。
        assert!(html.contains("<br>"), "{html}");
    }

    #[test]
    fn the_attribution_reads_the_way_gmail_writes_it() {
        // 12 小时制带 AM/PM,日期与发件人之间只有空格——识别度最广的
        // 那一种写法,启发式解析器都是照它调的。
        let (text, _) =
            quote_original(Some(&parent(Some("原文"), None)), Some("好的".into()), None);
        let text = text.expect("text");
        let attribution = text.lines().nth(2).expect("属性行");
        assert!(attribution.starts_with("On "), "{attribution}");
        assert!(
            attribution.ends_with(" Alice <alice@example.com> wrote:"),
            "{attribution}"
        );
        assert!(
            attribution.contains(" AM ") || attribution.contains(" PM "),
            "{attribution}"
        );
    }

    #[test]
    fn an_empty_original_adds_no_quote() {
        let (text, _) = quote_original(Some(&parent(Some("   "), None)), Some("在的".into()), None);
        assert_eq!(text.as_deref(), Some("在的"));
    }
}
