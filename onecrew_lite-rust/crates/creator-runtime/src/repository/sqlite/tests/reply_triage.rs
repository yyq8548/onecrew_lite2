use crate::repository::{ConversationRepository, EmailRepository, ReplyTriageRepository};
use crate::{
    AcknowledgeReplyTriageInput, ListReplyTriageInput, RecordReplyTriageInput, ReplyChannel,
    ReplyIntent,
};

use super::super::SqliteRepository;
use super::repository;

/// A conversation with one inbound and one outbound message, so the
/// direction gate has something to reject.
fn dm_fixture(repository: &SqliteRepository) -> (String, String) {
    let connection = repository.lock().expect("connection");
    connection
        .execute_batch(
            "INSERT INTO account_refs (id, platform, kind, label, created_at, updated_at)
               VALUES ('account-1', 'x', 'profile', 'Operator', 1, 1);
             INSERT INTO creators (id, display_name, created_at, updated_at, follower_counts_json)
               VALUES ('creator-1', '受访者', 1, 1, '{}');
             INSERT INTO contact_locators
               (id, channel, kind, normalized_value, display_value, created_at, updated_at)
               VALUES ('locator-1', 'x', 'handle', 'peer', 'peer', 1, 1);
             INSERT INTO creator_contact_routes
               (id, creator_id, locator_id, confidence, created_at, updated_at)
               VALUES ('route-1', 'creator-1', 'locator-1', 'exact', 1, 1);
             INSERT INTO resolved_targets
               (id, account_ref_id, platform, target_kind, external_target_id,
                created_at, updated_at)
               VALUES ('target-1', 'account-1', 'x', 'user', 'peer-1', 1, 1);
             INSERT INTO conversations
               (id, account_ref_id, resolved_target_id, channel, provider_thread_id,
                unread_count, created_at, updated_at)
               VALUES ('conv-1', 'account-1', 'target-1', 'x', 'thread-1', 0, 1, 1);
             INSERT INTO conversation_contact_routes
               (conversation_id, contact_route_id, created_at)
               VALUES ('conv-1', 'route-1', 1);
             INSERT INTO messages
               (id, conversation_id, external_message_id, direction, body, status, sent_at)
               VALUES ('msg-in', 'conv-1', 'ext-in', 'inbound', '有兴趣，报个价', 'delivered', 10),
                      ('msg-out', 'conv-1', 'ext-out', 'outbound', '你好', 'delivered', 5);",
        )
        .expect("dm fixture");
    ("conv-1".to_string(), "msg-in".to_string())
}

#[test]
fn a_verdict_names_exactly_one_inbound_message() {
    let (_directory, repository) = repository();
    let (_conversation_id, message_id) = dm_fixture(&repository);

    let neither = repository.record_reply_triage(RecordReplyTriageInput {
        email_id: None,
        message_id: None,
        intent: ReplyIntent::Interested,
        summary: "x".into(),
        suggested_reply: None,
    });
    assert!(
        neither.unwrap_err().contains("exactly one message"),
        "a verdict about nothing is not a verdict"
    );

    let outbound = repository.record_reply_triage(RecordReplyTriageInput {
        email_id: None,
        message_id: Some(crate::MessageId::new("msg-out".to_string())),
        intent: ReplyIntent::Interested,
        summary: "x".into(),
        suggested_reply: None,
    });
    assert!(
        outbound.unwrap_err().contains("inbound"),
        "judging our own message means nothing"
    );

    let missing = repository.record_reply_triage(RecordReplyTriageInput {
        email_id: None,
        message_id: Some(crate::MessageId::new("nope".to_string())),
        intent: ReplyIntent::Interested,
        summary: "x".into(),
        suggested_reply: None,
    });
    assert_eq!(missing.unwrap_err(), "message not found");

    let stored = repository
        .record_reply_triage(RecordReplyTriageInput {
            email_id: None,
            message_id: Some(crate::MessageId::new(message_id)),
            intent: ReplyIntent::Negotiating,
            summary: "对方问价格".into(),
            suggested_reply: Some("给出报价区间".into()),
        })
        .expect("verdict");
    assert_eq!(stored.channel, ReplyChannel::Dm);
    assert_eq!(stored.intent, ReplyIntent::Negotiating);
    assert!(stored.acknowledged_at.is_none());
}

#[test]
fn re_judging_a_message_replaces_the_reading_and_reopens_it() {
    let (_directory, repository) = repository();
    let (conversation_id, message_id) = dm_fixture(&repository);
    let record = |intent: ReplyIntent, summary: &str| {
        repository
            .record_reply_triage(RecordReplyTriageInput {
                email_id: None,
                message_id: Some(crate::MessageId::new(message_id.clone())),
                intent,
                summary: summary.into(),
                suggested_reply: None,
            })
            .expect("verdict")
    };

    let first = record(ReplyIntent::Question, "在问档期");
    repository
        .acknowledge_reply_triage(AcknowledgeReplyTriageInput {
            triage_id: first.id.clone(),
        })
        .expect("acknowledge");

    let second = record(ReplyIntent::Interested, "确认要合作");
    assert_eq!(second.id, first.id, "one message keeps one verdict row");
    assert_eq!(second.intent, ReplyIntent::Interested);
    assert!(
        second.acknowledged_at.is_none(),
        "what the human dismissed is not what is being said now"
    );

    // The row the inbox renders follows the same reading.
    let brief = repository
        .latest_triage_for_conversation(&crate::ConversationId::new(conversation_id))
        .expect("lookup")
        .expect("brief");
    assert_eq!(brief.intent, ReplyIntent::Interested);
    assert!(!brief.acknowledged);
}

#[test]
fn the_list_is_a_to_do_until_it_is_acknowledged() {
    let (_directory, repository) = repository();
    let (_conversation_id, message_id) = dm_fixture(&repository);
    let stored = repository
        .record_reply_triage(RecordReplyTriageInput {
            email_id: None,
            message_id: Some(crate::MessageId::new(message_id)),
            intent: ReplyIntent::Interested,
            summary: "有兴趣".into(),
            suggested_reply: None,
        })
        .expect("verdict");

    let open = repository
        .list_reply_triage(ListReplyTriageInput::default())
        .expect("list");
    assert_eq!(open.items.len(), 1);
    let view = &open.items[0];
    assert_eq!(view.creator_name.as_deref(), Some("受访者"));
    assert_eq!(view.excerpt, "有兴趣，报个价");
    assert!(view.conversation_id.is_some());

    repository
        .acknowledge_reply_triage(AcknowledgeReplyTriageInput {
            triage_id: stored.id.clone(),
        })
        .expect("acknowledge");
    assert!(
        repository
            .list_reply_triage(ListReplyTriageInput::default())
            .expect("list")
            .items
            .is_empty(),
        "an acknowledged verdict leaves the to-do"
    );
    assert_eq!(
        repository
            .list_reply_triage(ListReplyTriageInput {
                include_acknowledged: true,
                ..Default::default()
            })
            .expect("list")
            .items
            .len(),
        1,
        "but it is still readable when asked for"
    );

    // Acknowledging twice is the same as once.
    let again = repository
        .acknowledge_reply_triage(AcknowledgeReplyTriageInput {
            triage_id: stored.id,
        })
        .expect("acknowledge again");
    assert!(again.acknowledged_at.is_some());
}

#[test]
fn a_verdict_dies_with_the_message_it_judges() {
    let (_directory, repository) = repository();
    let (_conversation_id, message_id) = dm_fixture(&repository);
    repository
        .record_reply_triage(RecordReplyTriageInput {
            email_id: None,
            message_id: Some(crate::MessageId::new(message_id)),
            intent: ReplyIntent::Interested,
            summary: "有兴趣".into(),
            suggested_reply: None,
        })
        .expect("verdict");

    // A judgment about a message that no longer exists is not user data to
    // preserve; the cascade says so instead of a cleanup step someone has
    // to remember.
    repository
        .lock()
        .expect("connection")
        .execute("DELETE FROM messages WHERE id = 'msg-in'", [])
        .expect("delete message");
    assert!(
        repository
            .list_reply_triage(ListReplyTriageInput {
                include_acknowledged: true,
                ..Default::default()
            })
            .expect("list")
            .items
            .is_empty()
    );
}

#[test]
fn unused_repository_ports_stay_reachable() {
    // Keeps the email-side lookup honest without a full mailbox fixture:
    // an unknown thread simply has no verdict.
    let (_directory, repository) = repository();
    assert!(
        repository
            .latest_triage_for_email_thread(&crate::EmailThreadId::new("nope".to_string()))
            .expect("lookup")
            .is_none()
    );
    assert!(
        repository
            .get_conversation(
                &crate::AccountRefId::new("nope".to_string()),
                &crate::ResolvedTargetId::new("nope".to_string()),
                crate::ContactChannel::X,
            )
            .expect("lookup")
            .is_none()
    );
    assert!(
        repository
            .list_email_folder_counts(None)
            .expect("counts")
            .iter()
            .all(|entry| entry.count == 0)
    );
}

#[test]
fn the_inbox_badge_counts_what_is_waiting_on_a_human() {
    let (_directory, repository) = repository();
    let (_conversation_id, _message_id) = dm_fixture(&repository);

    // 私信侧数的是「最后一条是对方发的」:provider 不给未读数,
    // conversations.unread_count 一直是 0,而外联真正关心的是球在谁手里。
    // fixture 里 msg-in(inbound, sent_at 10)晚于 msg-out(5)。
    let waiting = repository.inbox_attention().expect("counts");
    assert_eq!(waiting.conversations, 1);
    assert_eq!(waiting.mail, 0, "没有邮箱线程");

    // 我们回了一句之后,球就不在我们手里了。
    repository
        .lock()
        .expect("connection")
        .execute(
            "INSERT INTO messages
               (id, conversation_id, external_message_id, direction, body, status, sent_at)
             VALUES ('msg-reply', 'conv-1', 'ext-reply', 'outbound', '好的', 'delivered', 20)",
            [],
        )
        .expect("reply");
    assert_eq!(
        repository.inbox_attention().expect("counts").conversations,
        0,
        "回过之后不再计数"
    );
}
