use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::paging::PageLimits;
use super::{ConversationId, CreatorId, EmailId, EmailThreadId, MessageId, ReplyTriageId};

pub const REPLY_TRIAGE_PAGE_LIMITS: PageLimits = PageLimits::new(25, 100);

/// What an inbound reply is actually asking for. Six values, chosen so a
/// human scanning a list can act on the tag alone: three that want a reply
/// now, one that closes the thread, one that is noise, and an escape hatch
/// so an agent is never forced to mislabel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReplyIntent {
    /// Wants to work together; the next move is ours.
    Interested,
    /// Needs something answered before deciding.
    Question,
    /// Discussing terms — price, scope, timing.
    Negotiating,
    /// Said no. The thread is closed unless something changes.
    Declined,
    /// Auto-reply, unrelated, or spam.
    Irrelevant,
    /// Real but none of the above; the summary carries the meaning.
    Other,
}

impl ReplyIntent {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Interested => "interested",
            Self::Question => "question",
            Self::Negotiating => "negotiating",
            Self::Declined => "declined",
            Self::Irrelevant => "irrelevant",
            Self::Other => "other",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "interested" => Ok(Self::Interested),
            "question" => Ok(Self::Question),
            "negotiating" => Ok(Self::Negotiating),
            "declined" => Ok(Self::Declined),
            "irrelevant" => Ok(Self::Irrelevant),
            "other" => Ok(Self::Other),
            other => Err(format!("unknown ReplyIntent: {other}")),
        }
    }

    /// Whether this verdict wants a human to look. Declined and irrelevant
    /// are answers, not requests.
    pub fn needs_attention(&self) -> bool {
        matches!(self, Self::Interested | Self::Question | Self::Negotiating)
    }
}

/// Which inbox the judged reply arrived in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReplyChannel {
    Email,
    Dm,
}

impl ReplyChannel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Email => "email",
            Self::Dm => "dm",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "email" => Ok(Self::Email),
            "dm" => Ok(Self::Dm),
            other => Err(format!("unknown ReplyChannel: {other}")),
        }
    }
}

/// One agent verdict about one inbound message. Recording a second verdict
/// for the same message replaces the first: a message has one current
/// reading, not a history of guesses.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReplyTriage {
    pub id: ReplyTriageId,
    pub channel: ReplyChannel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email_id: Option<EmailId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<MessageId>,
    pub intent: ReplyIntent,
    /// Why the agent read it that way, in one or two sentences.
    pub summary: String,
    /// Drafted wording the human can adopt or edit. Absent when the intent
    /// does not call for a reply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_reply: Option<String>,
    pub created_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acknowledged_at: Option<u64>,
}

/// The verdict as an inbox row shows it: enough for a tag and a hover, not
/// the whole record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReplyTriageBrief {
    pub intent: ReplyIntent,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_reply: Option<String>,
    pub created_at: u64,
    pub acknowledged: bool,
}

/// 一条还欠着判断的入站消息:同步把它收进来了,但没有人——也没有
/// agent——说过它是什么意思。
///
/// 它跟着同步结果一起返回,因为提醒放在指令里只是请求,放在返回值里
/// 就是事实:agent 必然读到工具调用的结果,而这份清单要么是空的,要么
/// 明摆着还有活没干。判断完再同步一次,清单自己会空掉。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PendingReplyTriage {
    pub channel: ReplyChannel,
    /// 邮箱回信:这封信。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email_id: Option<EmailId>,
    /// 私信:这条消息。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<MessageId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator_id: Option<CreatorId>,
    /// 发信人,足够让 agent 认出这是谁而不必再取一次。
    pub from: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    pub excerpt: String,
    pub occurred_at: u64,
}

/// 一个收件箱欠下的判断:有界的清单加上真实总数,免得积压很多时把
/// 返回撑爆,又不至于让被截断的部分看起来不存在。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PendingReplyTriageDebt {
    pub items: Vec<PendingReplyTriage>,
    /// 全部欠着的条数,可能大于 items.len()。
    pub total: u32,
}

/// items 的上限。够 agent 一轮处理完,又不会让同步结果变成一份长名单。
pub const PENDING_REPLY_TRIAGE_LIMIT: u32 = 20;

/// A verdict with the context needed to render it away from its inbox —
/// which creator, which thread — resolved at read time rather than stored,
/// so a re-linked thread stays correct.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReplyTriageView {
    #[serde(flatten)]
    pub triage: ReplyTriage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator_id: Option<CreatorId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email_thread_id: Option<EmailThreadId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<ConversationId>,
    /// A bounded excerpt of the message the verdict is about.
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordReplyTriageInput {
    /// The inbound email this verdict judges. Exactly one of `email_id`
    /// and `message_id` must be set.
    #[serde(default)]
    pub email_id: Option<EmailId>,
    /// The inbound direct message this verdict judges.
    #[serde(default)]
    pub message_id: Option<MessageId>,
    pub intent: ReplyIntent,
    pub summary: String,
    #[serde(default)]
    pub suggested_reply: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListReplyTriageInput {
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub cursor: Option<String>,
    /// Default false: the list is a to-do, so acknowledged verdicts are
    /// out of the way unless explicitly asked for.
    #[serde(default)]
    pub include_acknowledged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AcknowledgeReplyTriageInput {
    pub triage_id: ReplyTriageId,
}
