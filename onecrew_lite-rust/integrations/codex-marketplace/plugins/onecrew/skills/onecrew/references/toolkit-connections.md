# Provider connections

Read this only while connecting, rotating or disconnecting a provider
operator account, mailbox, service credential or proxy,
or during a login lifecycle step. The selected live operation remains
authoritative.

- Treat connection references as tagged IDs: preserve the toolkit and
  reference kind returned by `onecrew_connection_list`, never translate one
  platform's account ID into another's, and never treat mailbox, credential
  and operator IDs as equal.
- Establish and validate the required proxy before starting a human operator
  login. Keep the same proxy for the entire attempt and never silently fall
  back to direct egress.
- Passwords, API keys, OTPs, 2FA values, proxy passwords, and X Chat passcodes
  are transient secrets. Supply them only in the exact secret field of the
  operation currently being executed. Never echo, log, save, or copy them into
  guidance or connection references.
- Browser and Telegram connections are human flows. A start response is not a
  connected account; follow the returned phase on the same live handle until
  completion, cancellation, or expiry.
- A wrong OTP, 2FA value, or passcode is retried only through the recovery path
  returned for the current live operation. Never manufacture a new flow while
  the owner says the existing flow remains resumable.
- An expired, cancelled, wrong-kind, or wrong-toolkit handle fails closed.
  Restart with a new intent only after the live operation directs recovery.
- Ask for provider-specific decisions, such as X Chat enablement, before the
  operation that requires them. Do not project one provider's fields onto
  another provider.
- Let the operation validate a service credential against the provider before
  it replaces the durable one, and keep returned status and detail projections
  secret-free.
- Rotation and disconnect/delete are distinct confirmed intents. Give each new
  intent a new idempotency key and read back the resulting account or
  credential state.
- Keep one idempotency key for retries of the same intent. Starting and
  completing a human flow are distinct operations and may have distinct keys
  when the live schema requires them.
- Treat a provider-accepted request as pending evidence, not completion. Use
  the selected operation's typed readback or status path.
- If the account needed for an operation is absent, stop and report the missing
  toolkit connection instead of guessing an ID or bypassing OneCrew.
