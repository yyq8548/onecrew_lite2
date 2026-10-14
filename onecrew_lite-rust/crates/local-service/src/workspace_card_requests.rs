//! Display-only, process-local notifications. Never search or approve anything.
//! An ambiguous receiving view is deliberately silent. Claims are consumed
//! before transport: an uncertain host response must not send a duplicate.
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::Mutex,
    time::{Duration, Instant},
};
use uuid::Uuid;

#[derive(Default)]
pub(crate) struct CardRequests(Mutex<Registry>);
#[derive(Default)]
struct Registry {
    views: HashMap<Uuid, View>,
    starts: HashMap<(String, String), Start>,
    retired: HashSet<Uuid>,
}
struct View {
    account: Option<String>,
    baseline: Option<String>,
    registered: Instant,
    seen: Option<Instant>,
    recovered: bool,
}
struct Start {
    at: Instant,
    claimed: bool,
    claimed_by: Option<Uuid>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    pub view_id: Uuid,
    #[serde(default)]
    pub manual: bool,
    #[serde(default)]
    pub baseline_campaign_id: Option<Uuid>,
}
#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Reply {
    None,
    Message { message: String },
}

impl CardRequests {
    pub(crate) fn request(
        &self,
        request: Request,
        account: &str,
        campaign: &str,
        chinese: bool,
    ) -> Reply {
        // After a service restart, an existing iframe may keep its server-issued view ID.
        // Rebind display-only state to the currently authenticated account; no business grant.
        if let Ok(mut registry) = self.0.lock() {
            if registry.retired.contains(&request.view_id) && !request.manual {
                return Reply::None;
            }
            if request.manual {
                registry.retired.remove(&request.view_id);
            }
            if !registry.views.contains_key(&request.view_id) && registry.views.len() < 256 {
                registry.views.insert(
                    request.view_id,
                    View {
                        account: Some(account.into()),
                        baseline: request.baseline_campaign_id.map(|id| id.to_string()),
                        registered: Instant::now(),
                        seen: None,
                        recovered: true,
                    },
                );
            }
            if request.manual {
                if let Some(view) = registry.views.get_mut(&request.view_id) {
                    view.account.get_or_insert_with(|| account.to_owned());
                }
                if !registry
                    .views
                    .get(&request.view_id)
                    .is_some_and(|v| v.account.as_deref() == Some(account))
                {
                    return Reply::None;
                }
                if let Some(start) = registry.starts.get_mut(&(account.into(), campaign.into())) {
                    start.claimed = true;
                    start.claimed_by = Some(request.view_id);
                }
                return display_message(chinese);
            }
        }
        self.claim(request.view_id, account, campaign, chinese)
    }
    pub(crate) fn register(&self, id: Uuid, account: Option<String>, baseline: Option<String>) {
        let Ok(mut registry) = self.0.lock() else {
            return;
        };
        // Bound optional display state; eviction can only disable notifications.
        if registry.views.len() >= 256 {
            registry.views.clear();
        }
        // A manually mounted current card already satisfies the display request.
        if let (Some(account), Some(campaign)) = (account.as_ref(), baseline.as_ref())
            && let Some(start) = registry
                .starts
                .get_mut(&(account.clone(), campaign.clone()))
        {
            start.claimed = true;
            if let Some(previous) = start.claimed_by.take() {
                registry.views.remove(&previous);
                registry.retired.insert(previous);
            }
        }
        registry.views.insert(
            id,
            View {
                account,
                baseline,
                registered: Instant::now(),
                seen: None,
                recovered: false,
            },
        );
    }
    pub(crate) fn started(&self, account: &str, campaign: &str) {
        let Ok(mut registry) = self.0.lock() else {
            return;
        };
        // Never forget a claimed plan and then notify again after a resume.
        if registry.starts.len() >= 4096 {
            return;
        }
        registry
            .starts
            .entry((account.into(), campaign.into()))
            .or_insert(Start {
                at: Instant::now(),
                claimed: false,
                claimed_by: None,
            });
    }
    pub(crate) fn claim(&self, id: Uuid, account: &str, campaign: &str, chinese: bool) -> Reply {
        let Ok(mut registry) = self.0.lock() else {
            return Reply::None;
        };
        let now = Instant::now();
        let Some(view) = registry.views.get_mut(&id) else {
            return Reply::None;
        };
        // A server-minted login card binds once, only after MCP authentication.
        if view
            .account
            .get_or_insert_with(|| account.to_owned())
            .as_str()
            != account
        {
            return Reply::None;
        }
        let previous = view.seen.replace(now);
        let registered = view.registered;
        let baseline = view.baseline.as_deref() == Some(campaign);
        // First observation opens a rendezvous window for competing chats.
        if previous.is_none_or(|at| now.duration_since(at) > Duration::from_secs(8)) || baseline {
            return Reply::None;
        }
        let visible = registry
            .views
            .values()
            .filter(|v| {
                v.account.as_deref() == Some(account)
                    && v.seen
                        .is_some_and(|at| now.duration_since(at) <= Duration::from_secs(8))
            })
            .count();
        if visible != 1 {
            return Reply::None;
        }
        let recovered = registry.views.get(&id).is_some_and(|view| view.recovered);
        let Some(start) = registry.starts.get_mut(&(account.into(), campaign.into())) else {
            return Reply::None;
        };
        if start.claimed || (start.at <= registered && !recovered) {
            return Reply::None;
        }
        start.claimed = true;
        start.claimed_by = Some(id);
        display_message(chinese)
    }
}

fn display_message(chinese: bool) -> Reply {
    // No customer text, identifiers, URLs or authority enter this prompt.
    let message = if chinese {
        "请调用 onecrew_status，然后调用 onecrew_web，在当前聊天底部展示 OneCrew 工作台最新的只读状态卡。仅展示状态，不打开或切换浏览器页面；不要启动、恢复或批准搜索，不要修改任务条件，不要重复发送此请求。"
    } else {
        "Call onecrew_status, then onecrew_web to display the latest read-only OneCrew status card at the bottom of this chat. Display only: do not open or switch browser pages, start, resume or approve searches, change conditions, or resend this request."
    };
    Reply::Message {
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn surviving_view_recovers_after_restart_and_manual_refresh_is_display_only() {
        let owner = CardRequests::default();
        let id = Uuid::new_v4();
        let old = Uuid::new_v4();
        let campaign = Uuid::new_v4().to_string();
        owner.started("a", &campaign);
        let request = || Request {
            view_id: id,
            manual: false,
            baseline_campaign_id: Some(old),
        };
        assert_eq!(owner.request(request(), "a", &campaign, true), Reply::None);
        assert!(matches!(
            owner.request(request(), "a", &campaign, true),
            Reply::Message { .. }
        ));
        assert_eq!(owner.request(request(), "a", &campaign, true), Reply::None);
        assert_eq!(
            owner.request(
                Request {
                    manual: true,
                    ..request()
                },
                "b",
                &campaign,
                true
            ),
            Reply::None
        );
        let Reply::Message { message } = owner.request(
            Request {
                manual: true,
                ..request()
            },
            "a",
            &campaign,
            true,
        ) else {
            panic!("manual refresh missing");
        };
        assert!(message.contains("不要启动、恢复或批准搜索"));
    }
    #[test]
    fn once_per_started_plan_and_never_on_resume_or_new_card() {
        let owner = CardRequests::default();
        let id = Uuid::new_v4();
        owner.register(id, Some("a".into()), Some("old".into()));
        owner.started("a", "new");
        assert_eq!(owner.claim(id, "a", "new", true), Reply::None);
        assert!(matches!(
            owner.claim(id, "a", "new", true),
            Reply::Message { .. }
        ));
        owner.started("a", "new");
        assert_eq!(owner.claim(id, "a", "new", true), Reply::None);
        let fresh = Uuid::new_v4();
        owner.register(fresh, Some("a".into()), Some("new".into()));
        assert_eq!(owner.claim(fresh, "a", "new", true), Reply::None);
        owner.started("a", "next");
        assert_eq!(owner.claim(id, "a", "next", true), Reply::None);
        assert!(matches!(
            owner.claim(fresh, "a", "next", true),
            Reply::Message { .. }
        ));
    }
    #[test]
    fn ambiguous_chats_account_changes_and_unknown_views_are_silent() {
        let owner = CardRequests::default();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        owner.register(first, Some("a".into()), None);
        owner.register(second, Some("a".into()), None);
        owner.started("a", "new");
        assert_eq!(owner.claim(first, "a", "new", false), Reply::None);
        assert_eq!(owner.claim(second, "a", "new", false), Reply::None);
        assert_eq!(owner.claim(first, "a", "new", false), Reply::None);
        assert_eq!(owner.claim(first, "b", "new", false), Reply::None);
        assert_eq!(owner.claim(Uuid::new_v4(), "a", "new", false), Reply::None);
    }
    #[test]
    fn draft_selection_and_replayed_old_start_do_not_notify() {
        let owner = CardRequests::default();
        let id = Uuid::new_v4();
        owner.started("a", "old");
        owner.register(id, Some("a".into()), None);
        for _ in 0..2 {
            assert_eq!(owner.claim(id, "a", "draft", true), Reply::None);
            assert_eq!(owner.claim(id, "a", "old", true), Reply::None);
        }
    }
    #[test]
    fn login_card_binds_once_and_manual_mount_satisfies_pending_request() {
        let owner = CardRequests::default();
        let id = Uuid::new_v4();
        owner.register(id, None, None);
        owner.started("a", "new");
        assert_eq!(owner.claim(id, "a", "new", true), Reply::None);
        assert_eq!(owner.claim(id, "b", "new", true), Reply::None);
        let fresh = Uuid::new_v4();
        owner.register(fresh, Some("a".into()), Some("new".into()));
        assert_eq!(owner.claim(id, "a", "new", true), Reply::None);
    }
}
