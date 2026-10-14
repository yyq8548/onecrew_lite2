import { z } from "zod";
import { IntegrationError } from "../../../devices/errors";
import { decryptToken, encryptToken } from "../../shared/crypto";
import { createMailOAuthRegistry, mailOAuthConfig, type MailOAuthRegistry } from "./adapters";
import type { MailOAuthProvider } from "./types";

const tokenSchema = z.object({access_token:z.string().min(1),refresh_token:z.string().min(1),expiry_date:z.number().int().positive()}).strict();
type Row = {id:string;user_id:string;provider:MailOAuthProvider;provider_subject:string;email:string;token_ciphertext:string|null;token_nonce:string|null;token_key_version:number|null;scope_json:string;status:"connected"|"reauth_required"|"disconnecting"|"disconnected";created_at:number;updated_at:number};
const select = "SELECT id,user_id,provider,provider_subject,email,token_ciphertext,token_nonce,token_key_version,scope_json,status,created_at,updated_at FROM mail_connection";
function publicConnection(row:Row,registry:MailOAuthRegistry) {
  const scopes = z.array(z.string()).parse(JSON.parse(row.scope_json));
  return {id:row.id,provider:row.provider,email:row.email,status:row.status,scopes,capabilities:registry[row.provider].capabilities(scopes),created_at:new Date(row.created_at).toISOString(),updated_at:new Date(row.updated_at).toISOString()};
}
async function owned(db:D1Database,userId:string,id:string) {
  const row=await db.prepare(`${select} WHERE id=? AND user_id=?`).bind(id,userId).first<Row>();
  if(!row) throw new IntegrationError("connection_not_found",404);
  return row;
}
async function decrypt(config:{encryptionKey:string},row:Row) {
  if(!row.token_ciphertext || !row.token_nonce || !row.token_key_version) throw new IntegrationError("connection_not_ready",409);
  try {return tokenSchema.parse(JSON.parse(await decryptToken(config.encryptionKey,{ciphertext:row.token_ciphertext,nonce:row.token_nonce,keyVersion:row.token_key_version},row.user_id,row.provider,row.id)));}
  catch {throw new IntegrationError("storage_unavailable",503);}
}
export async function listMailConnections(db:D1Database,userId:string,registry:MailOAuthRegistry=createMailOAuthRegistry()) {
  const result=await db.prepare(`${select} WHERE user_id=? AND status!='disconnected' ORDER BY created_at DESC LIMIT 100`).bind(userId).all<Row>();
  return {connections:result.results.map(row=>publicConnection(row,registry))};
}
export async function getMailConnection(db:D1Database,userId:string,id:string,registry:MailOAuthRegistry=createMailOAuthRegistry()) {return publicConnection(await owned(db,userId,id),registry);}
export async function issueMailAccessToken(db:D1Database,bindings:unknown,origin:string,userId:string,id:string,registry:MailOAuthRegistry=createMailOAuthRegistry()) {
  let row=await owned(db,userId,id);
  if(row.status!=="connected") throw new IntegrationError("connection_not_ready",409);
  const {adapter,config}=mailOAuthConfig(row.provider,bindings,origin,registry);
  let tokens=await decrypt(config,row);
  if(tokens.expiry_date<=Date.now()+120000) {
    let refreshed;
    try {refreshed=await adapter.refresh(config,tokens.refresh_token);} catch(error) {
      if(adapter.isReauthRequired(error)) {
        await db.prepare("UPDATE mail_connection SET status='reauth_required',updated_at=? WHERE id=? AND user_id=? AND status='connected' AND token_ciphertext=?").bind(Date.now(),id,userId,row.token_ciphertext).run();
        throw new IntegrationError("connection_not_ready",409);
      }
      throw new IntegrationError("provider_unavailable",502,true);
    }
    if(refreshed.scopes!==undefined&&refreshed.scopes.length===0) throw new IntegrationError("provider_response_invalid",502);
    tokens=tokenSchema.parse({access_token:refreshed.access_token,refresh_token:refreshed.refresh_token??tokens.refresh_token,expiry_date:Date.now()+refreshed.expires_in*1000});
    const encrypted=await encryptToken(config.encryptionKey,JSON.stringify(tokens),userId,row.provider,id);
    const updated=await db.prepare("UPDATE mail_connection SET token_ciphertext=?,token_nonce=?,token_key_version=?,scope_json=?,updated_at=? WHERE id=? AND user_id=? AND status='connected' AND token_ciphertext=?")
      .bind(encrypted.ciphertext,encrypted.nonce,encrypted.keyVersion,refreshed.scopes===undefined?row.scope_json:JSON.stringify(refreshed.scopes),Date.now(),id,userId,row.token_ciphertext).run();
    if(updated.meta.changes!==1) {
      row=await owned(db,userId,id);
      if(row.status!=="connected") throw new IntegrationError("connection_not_ready",409);
      tokens=await decrypt(config,row);
    } else row={...row,token_ciphertext:encrypted.ciphertext,scope_json:refreshed.scopes===undefined?row.scope_json:JSON.stringify(refreshed.scopes)};
  }
  const current=await owned(db,userId,id);
  if(current.status!=="connected") throw new IntegrationError("connection_not_ready",409);
  if(current.token_ciphertext!==row.token_ciphertext) tokens=await decrypt(config,current);
  if(tokens.expiry_date<=Date.now()+30000) throw new IntegrationError("provider_unavailable",502,true);
  return {access_token:tokens.access_token,expires_at:new Date(tokens.expiry_date).toISOString(),scopes:JSON.parse(current.scope_json) as string[],email:current.email};
}
export async function disconnectMailConnection(db:D1Database,bindings:unknown,origin:string,userId:string,id:string,registry:MailOAuthRegistry=createMailOAuthRegistry()) {
  const initial=await owned(db,userId,id);
  if(initial.status==="disconnected") return publicConnection(initial,registry);
  // A callback may already be exchanging its code. Invalidate every older
  // transaction in the same atomic D1 batch that blocks token issuance.
  const now=Date.now();
  await db.batch([
    db.prepare("UPDATE mail_connection SET status='disconnecting',updated_at=? WHERE id=? AND user_id=? AND status IN ('connected','reauth_required')")
      .bind(now,id,userId),
    db.prepare("DELETE FROM mail_oauth_transaction WHERE user_id=? AND provider=? AND EXISTS(SELECT 1 FROM mail_connection WHERE id=? AND user_id=? AND status='disconnecting')")
      .bind(userId,initial.provider,id,userId),
  ]);
  const row=await owned(db,userId,id);
  if(row.status==="disconnected") return publicConnection(row,registry);
  if(row.status!=="disconnecting") throw new IntegrationError("connection_not_ready",409,true);
  const {adapter,config}=mailOAuthConfig(row.provider,bindings,origin,registry);
  const tokens=await decrypt(config,row);
  try {await adapter.revoke(config,tokens.refresh_token);} catch {throw new IntegrationError("revocation_unknown",502,true);}
  // A late completion can only finalize the same connection and same token.
  // Reconnect after confirmation creates a different connection row and ID.
  await db.prepare("UPDATE mail_connection SET status='disconnected',token_ciphertext=NULL,token_nonce=NULL,token_key_version=NULL,updated_at=? WHERE id=? AND user_id=? AND status='disconnecting' AND token_ciphertext=?")
    .bind(Date.now(),id,userId,row.token_ciphertext).run();
  return getMailConnection(db,userId,id,registry);
}
