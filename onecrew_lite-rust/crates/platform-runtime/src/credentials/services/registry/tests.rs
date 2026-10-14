use super::super::CredentialServices;
use super::*;
use onecrew_creator_runtime::{
    EmailAuthConnection, EmailSecretRecordId, MailboxProvider, MailboxRefId,
};
use onecrew_local_vault::EmailSecretRecordSecret;
use tokio::sync::Notify;

fn fixture() -> (
    tempfile::TempDir,
    Arc<CredentialServices>,
    MailboxAuthBinding,
) {
    let directory = tempfile::tempdir().unwrap();
    let vault = LocalVault::open(directory.path().join("vault.db")).unwrap();
    let secret = vault
        .upsert_email_secret_record(
            EmailSecretRecordKind::ResendApiKey,
            "Synthetic mailbox",
            serde_json::json!({"email_address":"sender@example.invalid"}),
            &EmailSecretRecordSecret::new("re_secret_key"),
        )
        .unwrap();
    let services = Arc::new(CredentialServices::new(
        vault,
        Arc::new(onecrew_email::ResendAdmission::new().unwrap()),
    ));
    let binding = MailboxAuthBinding {
        mailbox_ref_id: MailboxRefId::new("mailbox-1"),
        provider: MailboxProvider::Resend,
        label: "Synthetic mailbox".into(),
        email_address: "sender@example.invalid".into(),
        display_name: Some("Sender".into()),
        capabilities: vec![
            onecrew_creator_runtime::EmailCapability::Send,
            onecrew_creator_runtime::EmailCapability::Inbox,
        ],
        available: true,
        auth_connection: EmailAuthConnection {
            auth_connection_id: EmailAuthConnectionId::new("auth-1"),
            provider_kind: EmailProviderKind::Resend,
            secret_record_id: EmailSecretRecordId::new(secret.id.to_string()),
            label: "Synthetic auth".into(),
            available: true,
            created_at: 1,
            updated_at: 2,
        },
    };
    (directory, services, binding)
}

struct BindingResolver {
    binding: std::sync::Mutex<MailboxAuthBinding>,
    calls: std::sync::atomic::AtomicUsize,
    entered: Option<Arc<Notify>>,
    release: Option<Arc<Notify>>,
}
#[async_trait::async_trait]
impl EmailMailboxAuthResolver for BindingResolver {
    async fn resolve_mailbox_auth_connection(
        &self,
        mailbox_ref_id: &MailboxRefId,
    ) -> Result<MailboxAuthBinding, String> {
        assert_eq!(mailbox_ref_id.as_str(), "mailbox-1");
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if let Some(entered) = &self.entered {
            entered.notify_one();
        }
        if let Some(release) = &self.release {
            release.notified().await;
        }
        Ok(self.binding.lock().unwrap().clone())
    }
}
impl BindingResolver {
    fn new(binding: MailboxAuthBinding) -> Self {
        Self {
            binding: std::sync::Mutex::new(binding),
            calls: Default::default(),
            entered: None,
            release: None,
        }
    }
}
struct MissingResolver;
#[async_trait::async_trait]
impl EmailMailboxAuthResolver for MissingResolver {
    async fn resolve_mailbox_auth_connection(
        &self,
        _: &MailboxRefId,
    ) -> Result<MailboxAuthBinding, String> {
        Err("mailbox binding is missing".into())
    }
}

#[tokio::test]
async fn resolver_lock_is_released_before_await_and_each_admission_reads_the_binding() {
    let (_directory, services, binding) = fixture();
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    services.bind_email_mailbox_resolver(Arc::new(BindingResolver {
        entered: Some(entered.clone()),
        release: Some(release.clone()),
        ..BindingResolver::new(binding)
    }));
    let owner = services.clone();
    let first = tokio::spawn(async move {
        owner
            .resend_mailbox("mailbox-1", EmailCapability::Inbox)
            .await
    });
    entered.notified().await;
    let replacement = services.clone();
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        tokio::task::spawn_blocking(move || {
            replacement.bind_email_mailbox_resolver(Arc::new(MissingResolver))
        }),
    )
    .await
    .expect("resolver write lock was held across callback await")
    .unwrap();
    release.notify_one();
    assert_eq!(
        first.await.unwrap().unwrap().email_address,
        "sender@example.invalid"
    );
    assert_eq!(
        services
            .resend_mailbox("mailbox-1", EmailCapability::Inbox)
            .await
            .err()
            .unwrap(),
        "mailbox binding is missing"
    );
}

#[tokio::test]
async fn cached_sender_does_not_cache_binding_availability_or_sender_identity() {
    let (_directory, services, binding) = fixture();
    let resolver = Arc::new(BindingResolver::new(binding));
    services.bind_email_mailbox_resolver(resolver.clone());
    assert_eq!(
        services
            .resend_mailbox("mailbox-1", EmailCapability::Inbox)
            .await
            .unwrap()
            .email_address,
        "sender@example.invalid"
    );
    resolver.binding.lock().unwrap().available = false;
    assert!(
        services
            .resend_mailbox("mailbox-1", EmailCapability::Inbox)
            .await
            .err()
            .unwrap()
            .contains("unavailable")
    );
    {
        let mut binding = resolver.binding.lock().unwrap();
        binding.available = true;
        binding.email_address = "alias@example.invalid".into();
        binding.display_name = Some("Changed alias".into());
    }
    let changed = services
        .resend_mailbox("mailbox-1", EmailCapability::Inbox)
        .await
        .unwrap();
    assert_eq!(changed.email_address, "alias@example.invalid");
    assert_eq!(changed.display_name.as_deref(), Some("Changed alias"));
    assert_eq!(resolver.calls.load(std::sync::atomic::Ordering::SeqCst), 3);
    assert_eq!(
        services.email_registry.resend.lock().unwrap().len(),
        1,
        "one cache entry for the unchanged secret identity"
    );
}

#[tokio::test]
async fn a_blocked_sender_cache_does_not_block_the_async_worker() {
    let (_directory, services, binding) = fixture();
    let entered = Arc::new(Notify::new());
    services.bind_email_mailbox_resolver(Arc::new(BindingResolver {
        entered: Some(entered.clone()),
        ..BindingResolver::new(binding)
    }));
    let (holding, held) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let cache_owner = services.clone();
    let holder = tokio::task::spawn_blocking(move || {
        let guard = cache_owner.email_registry.resend.lock().unwrap();
        holding.send(()).unwrap();
        // Bounded even if a broken implementation blocks the single async worker.
        assert!(matches!(
            wait.recv_timeout(std::time::Duration::from_secs(2)),
            Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ));
        drop(guard);
    });
    held.await.unwrap();
    let lookup_owner = services.clone();
    let lookup = tokio::spawn(async move {
        lookup_owner
            .resend_mailbox("mailbox-1", EmailCapability::Inbox)
            .await
    });
    tokio::time::timeout(std::time::Duration::from_millis(250), entered.notified())
        .await
        .expect("provider admission blocked the async worker on its cache lock");
    tokio::task::yield_now().await;
    assert!(
        !lookup.is_finished(),
        "held cache must still block only the credential segment"
    );
    release.send(()).unwrap();
    holder.await.unwrap();
    assert!(lookup.await.unwrap().is_ok());
}

#[tokio::test]
async fn an_unbound_resolver_fails_without_provider_construction() {
    let (_directory, services, _binding) = fixture();
    assert_eq!(
        services
            .resend_mailbox("mailbox-1", EmailCapability::Send)
            .await
            .err()
            .unwrap(),
        "email mailbox resolver is unavailable"
    );
    assert!(services.email_registry.resend.lock().unwrap().is_empty());
}
