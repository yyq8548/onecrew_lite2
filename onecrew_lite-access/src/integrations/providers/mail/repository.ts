import { randomPKCECodeVerifier } from "openid-client";
import { IntegrationError } from "../../../devices/errors";
import { decryptToken, encryptToken, randomState, stateHash } from "../../shared/crypto";
import { createMailOAuthRegistry, mailOAuthConfig, type MailOAuthRegistry } from "./adapters";
import type { MailOAuthProvider } from "./types";

export type MailTransaction = {
  state_hash: string; provider: MailOAuthProvider; user_id: string;
  pkce_ciphertext: string; pkce_nonce: string; pkce_key_version: number;
  nonce: string; expires_at: number; consumed_at: number | null;
};
const TTL_MS = 10 * 60_000;

export async function beginMailOAuth(db: D1Database, bindings: unknown, origin: string, userId: string, provider: MailOAuthProvider, registry: MailOAuthRegistry = createMailOAuthRegistry()) {
  const { adapter, config } = mailOAuthConfig(provider, bindings, origin, registry);
  const pendingDisconnect = await db.prepare("SELECT 1 FROM mail_connection WHERE user_id=? AND provider=? AND status='disconnecting' LIMIT 1").bind(userId, provider).first();
  if (pendingDisconnect) throw new IntegrationError("connection_not_ready", 409, true);
  const state = randomState();
  const nonce = randomState();
  const verifier = randomPKCECodeVerifier();
  const authorizationUrl = await adapter.authorizationUrl(config, state, nonce, verifier);
  const hash = await stateHash(state);
  const encrypted = await encryptToken(config.encryptionKey, verifier, userId, provider, hash);
  const now = Date.now();
  await db.prepare("DELETE FROM mail_oauth_transaction WHERE state_hash IN (SELECT state_hash FROM mail_oauth_transaction WHERE expires_at<=? ORDER BY expires_at LIMIT 100)").bind(now).run();
  await db.prepare("INSERT INTO mail_oauth_transaction(state_hash,provider,user_id,pkce_ciphertext,pkce_nonce,pkce_key_version,nonce,created_at,expires_at) SELECT ?,?,?,?,?,?,?,?,? WHERE NOT EXISTS(SELECT 1 FROM mail_connection WHERE user_id=? AND provider=? AND status='disconnecting')")
    .bind(hash, provider, userId, encrypted.ciphertext, encrypted.nonce, encrypted.keyVersion, nonce, now, now + TTL_MS, userId, provider).run()
    .then((result) => { if (result.meta.changes !== 1) throw new IntegrationError("connection_not_ready", 409, true); });
  return authorizationUrl.href;
}

export async function claimMailTransaction(db: D1Database, state: string, userId: string, now = Date.now()) {
  const hash = await stateHash(state);
  const tx = await db.prepare("SELECT * FROM mail_oauth_transaction WHERE state_hash=? AND user_id=? AND consumed_at IS NULL AND expires_at>?")
    .bind(hash, userId, now).first<MailTransaction>();
  if (!tx) throw new IntegrationError("authorization_expired", 410);
  const claimed = await db.prepare("UPDATE mail_oauth_transaction SET consumed_at=? WHERE state_hash=? AND user_id=? AND consumed_at IS NULL AND expires_at>?")
    .bind(now, hash, userId, now).run();
  if (claimed.meta.changes !== 1) throw new IntegrationError("authorization_terminal", 409);
  return { ...tx, consumed_at: now };
}

export async function cancelMailOAuth(db: D1Database, state: string, userId: string, now = Date.now()) {
  await claimMailTransaction(db, state, userId, now);
}

export async function completeMailOAuth(db: D1Database, bindings: unknown, origin: string, state: string, userId: string, callbackUrl: URL, registry: MailOAuthRegistry = createMailOAuthRegistry()) {
  const tx = await claimMailTransaction(db, state, userId);
  const { adapter, config } = mailOAuthConfig(tx.provider, bindings, origin, registry);
  const verifier = await decryptToken(config.encryptionKey, { ciphertext: tx.pkce_ciphertext, nonce: tx.pkce_nonce, keyVersion: tx.pkce_key_version }, userId, tx.provider, tx.state_hash);
  let tokens;
  try {
    tokens = await adapter.exchangeCode(config, adapter.callbackUrl(config, callbackUrl.searchParams), state, tx.nonce, verifier);
  } catch (error) {
    if (error instanceof IntegrationError) throw error;
    throw new IntegrationError("provider_unavailable", 502, true);
  }
  if (!tokens.access_token || !tokens.refresh_token || !tokens.subject || !tokens.email || !Number.isFinite(tokens.expires_in) || tokens.expires_in <= 0) {
    throw new IntegrationError("provider_response_invalid", 502);
  }
  const capabilities = adapter.capabilities(tokens.scopes);
  if (!capabilities.can_send && !capabilities.can_read_thread) throw new IntegrationError("provider_rejected", 403);
  const foreignGrant = await db.prepare("SELECT 1 FROM mail_connection WHERE user_id!=? AND provider=? AND provider_subject=? AND status!='disconnected' LIMIT 1").bind(userId, tx.provider, tokens.subject).first();
  if (foreignGrant) throw new IntegrationError("account_not_authorized", 403);
  const existing = await db.prepare("SELECT id,status,token_ciphertext FROM mail_connection WHERE user_id=? AND provider=? AND provider_subject=? AND status!='disconnected'")
    .bind(userId, tx.provider, tokens.subject).first<{id:string;status:string;token_ciphertext:string|null}>();
  if (existing?.status === "disconnecting") throw new IntegrationError("connection_not_ready", 409, true);
  const id = existing ? existing.id : `conn_${crypto.randomUUID()}`;
  const now = Date.now();
  const encrypted = await encryptToken(config.encryptionKey, JSON.stringify({ access_token: tokens.access_token, refresh_token: tokens.refresh_token, expiry_date: now + tokens.expires_in * 1000 }), userId, tx.provider, id);
  const parameters = [id, userId, tx.provider, tokens.subject, tokens.email, encrypted.ciphertext, encrypted.nonce, encrypted.keyVersion, JSON.stringify(tokens.scopes), now, now, userId, tx.provider, tx.state_hash, userId, tx.provider, tx.consumed_at, now] as const;
  let result;
  if (!existing) {
    result = await db.prepare("INSERT INTO mail_connection(id,user_id,provider,provider_subject,email,token_ciphertext,token_nonce,token_key_version,scope_json,status,created_at,updated_at) SELECT ?,?,?,?,?,?,?,?,?,'connected',?,? WHERE NOT EXISTS(SELECT 1 FROM mail_connection WHERE user_id=? AND provider=? AND status='disconnecting') AND EXISTS(SELECT 1 FROM mail_oauth_transaction WHERE state_hash=? AND user_id=? AND provider=? AND consumed_at=? AND expires_at>?) ON CONFLICT DO NOTHING").bind(...parameters).run();
  } else {
    result = await db.prepare("UPDATE mail_connection SET id=?,email=?,token_ciphertext=?,token_nonce=?,token_key_version=?,scope_json=?,status='connected',updated_at=? WHERE user_id=? AND provider=? AND provider_subject=? AND status IN ('connected','reauth_required') AND token_ciphertext IS ? AND NOT EXISTS(SELECT 1 FROM mail_connection WHERE user_id=? AND provider=? AND status='disconnecting') AND NOT EXISTS(SELECT 1 FROM mail_connection WHERE user_id!=? AND provider=? AND provider_subject=? AND status!='disconnected') AND EXISTS(SELECT 1 FROM mail_oauth_transaction WHERE state_hash=? AND user_id=? AND provider=? AND consumed_at=? AND expires_at>?)")
      .bind(id, tokens.email, encrypted.ciphertext, encrypted.nonce, encrypted.keyVersion, JSON.stringify(tokens.scopes), now, userId, tx.provider, tokens.subject, existing.token_ciphertext, userId, tx.provider, userId, tx.provider, tokens.subject, tx.state_hash, userId, tx.provider, tx.consumed_at, now).run();
  }
  if (result.meta.changes !== 1) throw new IntegrationError("connection_not_ready", 409, true);
  return { connection_id: id, provider: tx.provider, email: tokens.email, scopes: tokens.scopes };
}
