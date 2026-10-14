import { env } from "cloudflare:workers";
import { applyD1Migrations, type D1Migration } from "cloudflare:test";
import { Hono } from "hono";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { exportJWK, generateKeyPair, SignJWT } from "jose";
import * as oidc from "openid-client";
import app from "../src/index";
import routeFixture from "./fixtures/mail-oauth-broker.json";
import { runtimeContextMiddleware } from "../src/middleware/api";
import { createMailRoutes } from "../src/integrations/providers/mail/routes";
import { createMailOAuthRegistry } from "../src/integrations/providers/mail/adapters";
import { GoogleMailOAuthAdapter, GOOGLE_MAIL_SCOPES } from "../src/integrations/providers/mail/adapters/google";
import { beginMailOAuth, completeMailOAuth, cancelMailOAuth } from "../src/integrations/providers/mail/repository";
import { issueMailAccessToken, disconnectMailConnection, listMailConnections, getMailConnection } from "../src/integrations/providers/mail/service";
import type { MailOAuthAdapter, MailOAuthConfig, MailOAuthTokens } from "../src/integrations/providers/mail/types";
import type { AppEnv } from "../src/types/bindings";
import { encryptToken, decryptToken } from "../src/integrations/shared/crypto";
import { accessApiMethods } from "../src/security/access-boundary";

const base=env as unknown as CloudflareBindings & {TEST_MIGRATIONS:D1Migration[]};
const secret="AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";
const apiOrigin="https://sync.identity.test";
const credentialId="11111111-1111-4111-8111-111111111111";
const deviceId=`dev_${"m".repeat(24)}`;
const pairingId=`pair_${"m".repeat(24)}`;
const bearer=`${credentialId}_${"a".repeat(43)}`;
let userId="";
let cookie="";
let failRevoke=false;
let revokeCount=0;
let revokeGate:Promise<void>|null=null;
let revokeEntered:(()=>void)|null=null;
let refreshGate:Promise<{access_token:string;expires_in:number}>|null=null;
let refreshEntered:(()=>void)|null=null;
let refreshInvalidGrant=false;
let refreshedScopes:string[]|undefined;
let exchangeGate:Promise<void>|null=null;
let exchangeEntered:(()=>void)|null=null;
let exchangeFails=false;
const projection={protocol:"reacher.creator_sync",protocol_version:1,installation_id:"22222222-2222-4222-8222-222222222222",credential_id:credentialId,access_user_id:"",access_device_id:deviceId,access_pairing_id:pairingId,source_instance_id:"33333333-3333-4333-8333-333333333333",capabilities:["creator_snapshots:write_chunked_quarantine"],accepted_bundle:{format:"reacher.creator_bundle",format_version:1,profile:"portable_public",max_snapshot_bytes:100,max_chunk_bytes:100,max_chunks:1,max_namespace_staged_bytes:100,max_access_user_staged_bytes:100}};
const bindings=()=>({...base,CREATOR_SYNC_API_ORIGIN:apiOrigin,SYNC_CREDENTIAL_VERIFIER:{fetch:async()=>Response.json({...projection,access_user_id:userId})} as unknown as Fetcher,GMAIL_CLIENT_ID:"test-client.apps.googleusercontent.com",GMAIL_CLIENT_SECRET:"test-secret",THIRD_PARTY_TOKEN_ENCRYPTION_KEY:secret});
const scopes=["openid","https://www.googleapis.com/auth/userinfo.email","https://www.googleapis.com/auth/gmail.send"];
const fake=(provider:"google"|"outlook"="google"):MailOAuthAdapter=>({
  provider,supported:true,...(provider==="google"?{callbackPath:"/integrations/gmail/callback"}:{}),
  createConfig:()=>({provider,encryptionKey:secret}),
  async authorizationUrl(_config,state,nonce,verifier){return new URL(`https://provider.test/authorize?${new URLSearchParams({state,nonce,verifier})}`);},
  callbackUrl(_config,params){return new URL(`https://access.onecrewai.io/integrations/gmail/callback?${params}`);},
  async exchangeCode(_config,_url,_state,_nonce,_verifier):Promise<MailOAuthTokens>{exchangeEntered?.();if(exchangeGate) await exchangeGate;if(exchangeFails) throw Error("bad token");return {access_token:"provider-access",refresh_token:"provider-refresh",expires_in:3600,subject:"provider-sub",email:"different@gmail.com",scopes};},
  async refresh(){refreshEntered?.();if(refreshInvalidGrant) throw Error("invalid_grant");const result=await(refreshGate??Promise.resolve({access_token:"refreshed-access",expires_in:1800}));return {...result,...(refreshedScopes===undefined?{}:{scopes:refreshedScopes})};},
  async revoke(){revokeCount++;revokeEntered?.();if(revokeGate) await revokeGate;if(failRevoke) throw Error("503");},
  isReauthRequired(error){return error instanceof Error&&error.message==="invalid_grant";},
  capabilities(granted){return {can_send:granted.includes("https://www.googleapis.com/auth/gmail.send"),can_read_thread:granted.includes("https://www.googleapis.com/auth/gmail.readonly"),can_read_attachment:false};},
});
function route(registry=createMailOAuthRegistry({google:fake()})) {return new Hono<AppEnv>().use("*",runtimeContextMiddleware).route("/",createMailRoutes(registry));}
function machine(path:string,init:RequestInit={}) {return new Request(`http://localhost:8787/api/desktop/integrations/mail/v1${path}`,{...init,headers:{Authorization:`Bearer ${bearer}`,...init.headers}});}
beforeEach(async()=>{
  failRevoke=false;revokeCount=0;refreshGate=null;refreshEntered=null;refreshInvalidGrant=false;refreshedScopes=undefined;revokeGate=null;revokeEntered=null;exchangeGate=null;exchangeEntered=null;exchangeFails=false;
  await applyD1Migrations(base.DB,base.TEST_MIGRATIONS);
  await base.DB.prepare("DELETE FROM mail_oauth_transaction").run();
  await base.DB.prepare("DELETE FROM mail_connection").run();
  await base.DB.prepare("DELETE FROM desktop_pairing_grant WHERE id=?").bind(pairingId).run();
  await base.DB.prepare("DELETE FROM access_device WHERE id=?").bind(deviceId).run();
  const email=`mail-${crypto.randomUUID()}@example.com`;
  const response=await app.request("http://localhost:8787/api/session/register",{method:"POST",headers:{"Content-Type":"application/json",Origin:"http://localhost:8787","cf-connecting-ip":`198.51.100.${Math.floor(Math.random()*254)+1}`},body:JSON.stringify({email,name:"Mail Tester",password:"correct-horse-battery-staple"})},base);
  expect(response.status).toBe(200);
  cookie=response.headers.get("Set-Cookie")!.split(";",1)[0]!;
  userId=(await base.DB.prepare("SELECT id FROM user WHERE email=?").bind(email).first<{id:string}>())!.id;
  await base.DB.prepare("INSERT INTO access_device(id,user_id,label,created_at) VALUES (?,?,'Mail Device',1000)").bind(deviceId,userId).run();
  await base.DB.prepare("INSERT INTO desktop_pairing_grant(id,device_id,request_hmac,code_hmac,code_challenge,platform,app_version,created_at,expires_at,consumed_at) VALUES (?,?,?,?,?,'macos','1.0',1000,91000,2000)")
    .bind(pairingId,deviceId,"d".repeat(43),"e".repeat(43),"f".repeat(43)).run();
});

describe("panel-owned mailbox flow",()=>{
  it("shows the authenticated account, blocks mismatches, consumes OAuth once, and gives only the device a short token",async()=>{
    const handler=route();const b=bindings();
    const mismatch=await handler.request("http://localhost:8787/integrations/mail/connect?provider=google&expected_user_id=someone-else",{headers:{Cookie:cookie}},b);
    expect(mismatch.status).toBe(403);expect(await mismatch.text()).toContain("Different OneCrew account");
    const preview=await handler.request(`http://localhost:8787/integrations/mail/connect?provider=google&expected_user_id=${userId}`,{headers:{Cookie:cookie}},b);
    expect(preview.status).toBe(200);expect(await preview.text()).toContain("OneCrew account:");
    const start=await handler.request(`http://localhost:8787/integrations/mail/connect?provider=google&expected_user_id=${userId}`,{method:"POST",headers:{Cookie:cookie,Origin:"http://localhost:8787"}},b);
    expect(start.status).toBe(303);
    const state=new URL(start.headers.get("Location")!).searchParams.get("state")!;
    const callback=await handler.request(`http://localhost:8787/integrations/gmail/callback?code=code&state=${state}`,{headers:{Cookie:cookie}},b);
    expect(callback.status).toBe(200);const callbackHtml=await callback.text();expect(callbackHtml).toContain("different@gmail.com");expect(callbackHtml).not.toContain("provider-access");expect(callbackHtml).not.toContain("provider-refresh");
    const replay=await handler.request(`http://localhost:8787/integrations/gmail/callback?code=code&state=${state}`,{headers:{Cookie:cookie}},b);expect(replay.status).toBe(410);
    const list=await handler.request(machine("/connections"),{},b);expect(list.status).toBe(200);
    const listBody=await list.json() as any;expect(listBody.data.connections).toHaveLength(1);
    expect(JSON.stringify(listBody)).not.toContain("provider-refresh");
    const id=listBody.data.connections[0].id as string;
    const detail=await handler.request(machine(`/connections/${id}`),{},b);expect(detail.status).toBe(200);const detailBody=await detail.json();
    const token=await handler.request(machine(`/connections/${id}/access-token`,{method:"POST"}),{},b);expect(token.status).toBe(200);const tokenBody=await token.json() as any;expect(tokenBody.data.access_token).toBe("provider-access");
    const browserToken=await handler.request(`http://localhost:8787/api/desktop/integrations/mail/v1/connections/${id}/access-token`,{method:"POST",headers:{Cookie:cookie}},b);expect(browserToken.status).toBe(401);
    const invalidBody=await handler.request(machine(`/connections/${id}/access-token`,{method:"POST",body:"{}",headers:{"X-OneCrew-Request-ID":"44444444-4444-4444-8444-444444444444"}}),{},b);
    expect(invalidBody.status).toBe(400);expect(await invalidBody.json()).toMatchObject({requestId:"44444444-4444-4444-8444-444444444444",error:{code:"invalid_request"}});
    const rejectedSync=await handler.request(machine(`/connections/${id}`),{}, {...b,SYNC_CREDENTIAL_VERIFIER:{fetch:async()=>new Response(null,{status:401})} as unknown as Fetcher});expect(rejectedSync.status).toBe(401);
    const wrongCredential=await handler.request(machine(`/connections/${id}`),{}, {...b,SYNC_CREDENTIAL_VERIFIER:{fetch:async()=>Response.json({...projection,access_user_id:userId,credential_id:crypto.randomUUID()})} as unknown as Fetcher});expect(wrongCredential.status).toBe(503);
    expect(accessApiMethods("/api/desktop/integrations/mail/v1/authorizations")).toBeNull();
    expect(accessApiMethods("/api/desktop/integrations/tiktok-business/v1/connections")).toBeNull();
    expect((await app.request("http://localhost:8787/api/integrations/tiktok-business/connections",{},b)).status).toBe(404);
    expect((await base.DB.prepare("SELECT name FROM sqlite_master WHERE name='mail_desktop_authorization'").first())).toBeNull();
    const foreign=await handler.request(machine(`/connections/${id}`,{headers:{Authorization:`Bearer ${bearer}`}}),{}, {...b,SYNC_CREDENTIAL_VERIFIER:{fetch:async()=>Response.json({...projection,access_user_id:"foreign-user"})} as unknown as Fetcher});expect(foreign.status).toBe(403);
    const disconnected=await handler.request(machine(`/connections/${id}`,{method:"DELETE"}),{},b);expect(disconnected.status).toBe(200);const disconnectedBody=await disconnected.json() as any;expect(disconnectedBody.data.connection.status).toBe("disconnected");
    expect((await handler.request(machine(`/connections/${id}`,{method:"DELETE"}),{},b)).status).toBe(200);expect(revokeCount).toBe(1);
    expect((await handler.request(machine(`/connections/${id}/access-token`,{method:"POST"}),{},b)).status).toBe(409);
    const scrub=(value:any,key=""):any=>Array.isArray(value)?value.map(x=>scrub(x)):value&&typeof value==="object"?Object.fromEntries(Object.entries(value).map(([k,v])=>[k,scrub(v,k)])):key==="requestId"?"<request-id>":key==="id"?"<connection-id>":["created_at","updated_at","expires_at"].includes(key)?"<iso8601>":key==="access_token"?"<access-token>":value;
    expect({list:scrub(listBody),detail:scrub(detailBody),token:scrub(tokenBody),disconnected:scrub(disconnectedBody)}).toEqual(routeFixture);
  });
  it("keeps an existing connection when a later code exchange fails without revoking it",async()=>{
    const b=bindings();const registry=createMailOAuthRegistry({google:fake()});
    const initialUrl=await beginMailOAuth(base.DB,b,"http://localhost:8787",userId,"google",registry);
    const initialState=new URL(initialUrl).searchParams.get("state")!;
    const connected=await completeMailOAuth(base.DB,b,"http://localhost:8787",initialState,userId,new URL(`http://localhost:8787/integrations/gmail/callback?code=x&state=${initialState}`),registry);
    const before=await base.DB.prepare("SELECT token_ciphertext FROM mail_connection WHERE id=?").bind(connected.connection_id).first();
    const retryUrl=await beginMailOAuth(base.DB,b,"http://localhost:8787",userId,"google",registry);
    const retryState=new URL(retryUrl).searchParams.get("state")!;
    exchangeFails=true;
    await expect(completeMailOAuth(base.DB,b,"http://localhost:8787",retryState,userId,new URL(`http://localhost:8787/integrations/gmail/callback?code=x&state=${retryState}`),registry)).rejects.toMatchObject({code:"provider_unavailable"});
    expect((await getMailConnection(base.DB,userId,connected.connection_id,registry)).status).toBe("connected");
    expect(await base.DB.prepare("SELECT token_ciphertext FROM mail_connection WHERE id=?").bind(connected.connection_id).first()).toEqual(before);
    expect(revokeCount).toBe(0);
  });
  it("keeps failed revocation pending and prevents reconnection until a retry succeeds",async()=>{
    const b=bindings();const registry=createMailOAuthRegistry({google:fake()});
    const destination=await beginMailOAuth(base.DB,b,"http://localhost:8787",userId,"google",registry);
    const state=new URL(destination).searchParams.get("state")!;
    const result=await completeMailOAuth(base.DB,b,"http://localhost:8787",state,userId,new URL(`http://localhost:8787/integrations/gmail/callback?code=x&state=${state}`),registry);
    failRevoke=true;
    await expect(disconnectMailConnection(base.DB,b,"http://localhost:8787",userId,result.connection_id,registry)).rejects.toMatchObject({code:"revocation_unknown"});
    await expect(beginMailOAuth(base.DB,b,"http://localhost:8787",userId,"google",registry)).rejects.toMatchObject({code:"connection_not_ready"});
    await expect(issueMailAccessToken(base.DB,b,"http://localhost:8787",userId,result.connection_id,registry)).rejects.toMatchObject({code:"connection_not_ready"});
    failRevoke=false;
    expect((await disconnectMailConnection(base.DB,b,"http://localhost:8787",userId,result.connection_id,registry)).status).toBe("disconnected");
    expect(revokeCount).toBe(2);
    expect(await base.DB.prepare("SELECT token_ciphertext FROM mail_connection WHERE id=?").bind(result.connection_id).first()).toEqual({token_ciphertext:null});
  });
  it("allows one active Access owner for a provider mailbox subject without exposing the first owner",async()=>{
    const b=bindings();const registry=createMailOAuthRegistry({google:fake()});
    const other=`other-${crypto.randomUUID()}`;
    await base.DB.prepare("INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt) VALUES (?, 'Other', ?, 1, '2026-01-01', '2026-01-01')").bind(other,`${other}@example.com`).run();
    const firstUrl=await beginMailOAuth(base.DB,b,"http://localhost:8787",userId,"google",registry);
    const firstState=new URL(firstUrl).searchParams.get("state")!;
    const first=await completeMailOAuth(base.DB,b,"http://localhost:8787",firstState,userId,new URL(`http://localhost:8787/integrations/gmail/callback?code=x&state=${firstState}`),registry);
    const secondUrl=await beginMailOAuth(base.DB,b,"http://localhost:8787",other,"google",registry);
    const secondState=new URL(secondUrl).searchParams.get("state")!;
    await expect(completeMailOAuth(base.DB,b,"http://localhost:8787",secondState,other,new URL(`http://localhost:8787/integrations/gmail/callback?code=x&state=${secondState}`),registry)).rejects.toMatchObject({code:"account_not_authorized",status:403});
    expect((await listMailConnections(base.DB,other,registry)).connections).toEqual([]);
    await disconnectMailConnection(base.DB,b,"http://localhost:8787",userId,first.connection_id,registry);
    const retryUrl=await beginMailOAuth(base.DB,b,"http://localhost:8787",other,"google",registry);
    const retryState=new URL(retryUrl).searchParams.get("state")!;
    expect((await completeMailOAuth(base.DB,b,"http://localhost:8787",retryState,other,new URL(`http://localhost:8787/integrations/gmail/callback?code=x&state=${retryState}`),registry)).connection_id).toMatch(/^conn_/);
  });
  it("keeps old DELETE completions on the old row after a new connection is created",async()=>{
    const b=bindings();const registry=createMailOAuthRegistry({google:fake()});
    const url=await beginMailOAuth(base.DB,b,"http://localhost:8787",userId,"google",registry);
    const state=new URL(url).searchParams.get("state")!;
    const connected=await completeMailOAuth(base.DB,b,"http://localhost:8787",state,userId,new URL(`http://localhost:8787/integrations/gmail/callback?code=x&state=${state}`),registry);
    let release!:()=>void;
    revokeGate=new Promise(resolve=>{release=resolve;});
    const entered=new Promise<void>(resolve=>{revokeEntered=resolve;});
    const first=disconnectMailConnection(base.DB,b,"http://localhost:8787",userId,connected.connection_id,registry);
    await entered;
    await expect(beginMailOAuth(base.DB,b,"http://localhost:8787",userId,"google",registry)).rejects.toMatchObject({code:"connection_not_ready"});
    revokeGate=null;
    expect((await disconnectMailConnection(base.DB,b,"http://localhost:8787",userId,connected.connection_id,registry)).status).toBe("disconnected");
    const nextUrl=await beginMailOAuth(base.DB,b,"http://localhost:8787",userId,"google",registry);
    const nextState=new URL(nextUrl).searchParams.get("state")!;
    const next=await completeMailOAuth(base.DB,b,"http://localhost:8787",nextState,userId,new URL(`http://localhost:8787/integrations/gmail/callback?code=x&state=${nextState}`),registry);
    expect(next.connection_id).not.toBe(connected.connection_id);
    release();
    expect((await first).status).toBe("disconnected");
    expect((await getMailConnection(base.DB,userId,next.connection_id,registry)).status).toBe("connected");
    expect((await getMailConnection(base.DB,userId,connected.connection_id,registry)).status).toBe("disconnected");
    expect(revokeCount).toBe(2);
  });
  it("invalidates started OAuth during disconnect, including code exchanges already in flight",async()=>{
    const b=bindings();const registry=createMailOAuthRegistry({google:fake()});
    const firstUrl=await beginMailOAuth(base.DB,b,"http://localhost:8787",userId,"google",registry);
    const firstState=new URL(firstUrl).searchParams.get("state")!;
    const connection=await completeMailOAuth(base.DB,b,"http://localhost:8787",firstState,userId,new URL(`http://localhost:8787/integrations/gmail/callback?code=x&state=${firstState}`),registry);
    const staleUrl=await beginMailOAuth(base.DB,b,"http://localhost:8787",userId,"google",registry);
    const staleState=new URL(staleUrl).searchParams.get("state")!;
    await disconnectMailConnection(base.DB,b,"http://localhost:8787",userId,connection.connection_id,registry);
    await expect(completeMailOAuth(base.DB,b,"http://localhost:8787",staleState,userId,new URL(`http://localhost:8787/integrations/gmail/callback?code=x&state=${staleState}`),registry)).rejects.toMatchObject({code:"authorization_expired"});
    const inFlightUrl=await beginMailOAuth(base.DB,b,"http://localhost:8787",userId,"google",registry);
    const inFlightState=new URL(inFlightUrl).searchParams.get("state")!;
    const reconnected=await completeMailOAuth(base.DB,b,"http://localhost:8787",inFlightState,userId,new URL(`http://localhost:8787/integrations/gmail/callback?code=x&state=${inFlightState}`),registry);
    const pendingUrl=await beginMailOAuth(base.DB,b,"http://localhost:8787",userId,"google",registry);
    const pendingState=new URL(pendingUrl).searchParams.get("state")!;
    let release!:()=>void;
    exchangeGate=new Promise(resolve=>{release=resolve;});
    const entered=new Promise<void>(resolve=>{exchangeEntered=resolve;});
    const pending=completeMailOAuth(base.DB,b,"http://localhost:8787",pendingState,userId,new URL(`http://localhost:8787/integrations/gmail/callback?code=x&state=${pendingState}`),registry);
    await entered;
    await disconnectMailConnection(base.DB,b,"http://localhost:8787",userId,reconnected.connection_id,registry);
    release();
    await expect(pending).rejects.toMatchObject({code:"connection_not_ready"});
    expect((await getMailConnection(base.DB,userId,reconnected.connection_id,registry)).status).toBe("disconnected");
    const freshUrl=await beginMailOAuth(base.DB,b,"http://localhost:8787",userId,"google",registry);
    const freshState=new URL(freshUrl).searchParams.get("state")!;
    expect((await completeMailOAuth(base.DB,b,"http://localhost:8787",freshState,userId,new URL(`http://localhost:8787/integrations/gmail/callback?code=x&state=${freshState}`),registry)).connection_id).not.toBe(reconnected.connection_id);
  });
  it("does not revive a disconnected connection after an in-flight refresh finishes",async()=>{
    const b=bindings();const registry=createMailOAuthRegistry({google:fake()});
    const url=await beginMailOAuth(base.DB,b,"http://localhost:8787",userId,"google",registry);
    const state=new URL(url).searchParams.get("state")!;
    const connected=await completeMailOAuth(base.DB,b,"http://localhost:8787",state,userId,new URL(`http://localhost:8787/integrations/gmail/callback?code=x&state=${state}`),registry);
    const ciphertext=await encryptToken(secret,JSON.stringify({access_token:"expired",refresh_token:"provider-refresh",expiry_date:Date.now()+1000}),userId,"google",connected.connection_id);
    await base.DB.prepare("UPDATE mail_connection SET token_ciphertext=?,token_nonce=?,token_key_version=? WHERE id=?").bind(ciphertext.ciphertext,ciphertext.nonce,ciphertext.keyVersion,connected.connection_id).run();
    let release!:(value:{access_token:string;expires_in:number})=>void;
    refreshGate=new Promise(resolve=>{release=resolve;});
    const entered=new Promise<void>(resolve=>{refreshEntered=resolve;});
    const pending=issueMailAccessToken(base.DB,b,"http://localhost:8787",userId,connected.connection_id,registry);
    await entered;
    expect((await disconnectMailConnection(base.DB,b,"http://localhost:8787",userId,connected.connection_id,registry)).status).toBe("disconnected");
    release({access_token:"late-access",expires_in:1800});
    await expect(pending).rejects.toMatchObject({code:"connection_not_ready"});
    expect(await base.DB.prepare("SELECT status,token_ciphertext FROM mail_connection WHERE id=?").bind(connected.connection_id).first()).toEqual({status:"disconnected",token_ciphertext:null});
  });
  it("distinguishes invalid refresh grants from reduced or omitted scopes",async()=>{
    const b=bindings();const registry=createMailOAuthRegistry({google:fake()});
    const url=await beginMailOAuth(base.DB,b,"http://localhost:8787",userId,"google",registry);
    const state=new URL(url).searchParams.get("state")!;
    const connected=await completeMailOAuth(base.DB,b,"http://localhost:8787",state,userId,new URL(`http://localhost:8787/integrations/gmail/callback?code=x&state=${state}`),registry);
    async function expireAccess() {
      const encrypted=await encryptToken(secret,JSON.stringify({access_token:"expiring",refresh_token:"provider-refresh",expiry_date:Date.now()+1000}),userId,"google",connected.connection_id);
      await base.DB.prepare("UPDATE mail_connection SET token_ciphertext=?,token_nonce=?,token_key_version=? WHERE id=?").bind(encrypted.ciphertext,encrypted.nonce,encrypted.keyVersion,connected.connection_id).run();
    }
    await expireAccess();
    refreshedScopes=["https://www.googleapis.com/auth/gmail.readonly"];
    expect((await issueMailAccessToken(base.DB,b,"http://localhost:8787",userId,connected.connection_id,registry)).scopes).toEqual(refreshedScopes);
    expect((await getMailConnection(base.DB,userId,connected.connection_id,registry)).capabilities).toMatchObject({can_send:false,can_read_thread:true});
    await expireAccess();refreshedScopes=undefined;
    expect((await issueMailAccessToken(base.DB,b,"http://localhost:8787",userId,connected.connection_id,registry)).scopes).toEqual(["https://www.googleapis.com/auth/gmail.readonly"]);
    await expireAccess();refreshedScopes=[];
    await expect(issueMailAccessToken(base.DB,b,"http://localhost:8787",userId,connected.connection_id,registry)).rejects.toMatchObject({code:"provider_response_invalid"});
    expect((await getMailConnection(base.DB,userId,connected.connection_id,registry)).status).toBe("connected");
    refreshedScopes=undefined;refreshInvalidGrant=true;
    await expect(issueMailAccessToken(base.DB,b,"http://localhost:8787",userId,connected.connection_id,registry)).rejects.toMatchObject({code:"connection_not_ready"});
    expect((await getMailConnection(base.DB,userId,connected.connection_id,registry)).status).toBe("reauth_required");
    refreshInvalidGrant=false;
    expect((await disconnectMailConnection(base.DB,b,"http://localhost:8787",userId,connected.connection_id,registry)).status).toBe("disconnected");
  });
  it("expires and cancels transactions, and reuses the provider-neutral flow for a second adapter",async()=>{
    const b=bindings();const registry=createMailOAuthRegistry({outlook:fake("outlook")});
    const destination=await beginMailOAuth(base.DB,b,"http://localhost:8787",userId,"outlook",registry);
    const state=new URL(destination).searchParams.get("state")!;
    await cancelMailOAuth(base.DB,state,userId);
    await expect(completeMailOAuth(base.DB,b,"http://localhost:8787",state,userId,new URL(`http://localhost:8787/integrations/gmail/callback?code=x&state=${state}`),registry)).rejects.toMatchObject({code:"authorization_expired"});
    const next=await beginMailOAuth(base.DB,b,"http://localhost:8787",userId,"outlook",registry);
    const nextState=new URL(next).searchParams.get("state")!;
    const connected=await completeMailOAuth(base.DB,b,"http://localhost:8787",nextState,userId,new URL(`http://localhost:8787/integrations/gmail/callback?code=x&state=${nextState}`),registry);
    expect(connected.provider).toBe("outlook");
    await base.DB.prepare("UPDATE mail_oauth_transaction SET expires_at=0 WHERE user_id=?").bind(userId).run();
    await expect(cancelMailOAuth(base.DB,nextState,userId)).rejects.toMatchObject({code:"authorization_expired"});
    await expect(beginMailOAuth(base.DB,b,"http://localhost:8787",userId,"outlook")).rejects.toMatchObject({code:"not_supported"});
  });
});

describe("real openid-client Google adapter against mocked provider network",()=>{
  it("uses PKCE, validates signed identity and nonce, preserves granular scopes, refreshes and revokes",async()=>{
    const {privateKey,publicKey}=await generateKeyPair("RS256",{modulusLength:2048});
    const jwk={...await exportJWK(publicKey),kid:"test-key",alg:"RS256",use:"sig"};
    const calls:Array<{url:string;body:URLSearchParams|null}>=[];
    let claimOverride:Record<string,unknown>={};
    let revokeStatus=200;
    let corruptSignature=false;
    let invalidRefresh=false;
    let refreshScope: string | undefined = "https://www.googleapis.com/auth/gmail.readonly";
    let missingGrantRefresh=false;
    let verifiedEmail=true;
    const fetcher:typeof fetch=async(input,init)=>{
      const request=input instanceof Request?input.clone():new Request(input,init);
      const url=request.url;const body=request.method==="POST"?new URLSearchParams(Array.from((await request.formData()).entries()).map(([key,value])=>[key,String(value)])):null;calls.push({url,body});
      if(url==="https://accounts.google.com/.well-known/openid-configuration") return Response.json({issuer:"https://accounts.google.com",authorization_endpoint:"https://accounts.google.com/o/oauth2/v2/auth",token_endpoint:"https://oauth2.googleapis.com/token",jwks_uri:"https://www.googleapis.com/oauth2/v3/certs",revocation_endpoint:"https://oauth2.googleapis.com/revoke",response_types_supported:["code"],subject_types_supported:["public"],id_token_signing_alg_values_supported:["RS256"],token_endpoint_auth_methods_supported:["client_secret_post"],code_challenge_methods_supported:["S256"]});
      if(url==="https://www.googleapis.com/oauth2/v3/certs") return Response.json({keys:[jwk]});
      if(url==="https://oauth2.googleapis.com/token") {
        if(body?.get("grant_type")==="refresh_token") return invalidRefresh ? Response.json({error:"invalid_grant"},{status:400}) : Response.json({access_token:"refreshed",token_type:"Bearer",expires_in:1800,...(refreshScope===undefined?{}:{scope:refreshScope})});
        const token=await new SignJWT({email:"mail@gmail.com",email_verified:verifiedEmail,nonce:"nonce",...claimOverride}).setProtectedHeader({alg:"RS256",kid:"test-key"}).setIssuer((claimOverride.iss as string)||"https://accounts.google.com").setAudience((claimOverride.aud as string)||"test-client.apps.googleusercontent.com").setSubject("google-subject").setIssuedAt().setExpirationTime("1h").sign(privateKey);
        const idToken=corruptSignature?`${token.slice(0,token.lastIndexOf(".")+1)}${token.at(token.lastIndexOf(".")+1)==="A"?"B":"A"}${token.slice(token.lastIndexOf(".")+2)}`:token;
        return Response.json({access_token:"short",...(missingGrantRefresh?{}:{refresh_token:"refresh"}),token_type:"Bearer",expires_in:3600,scope:scopes.join(" "),id_token:idToken});
      }
      if(url==="https://oauth2.googleapis.com/revoke") return new Response(null,{status:revokeStatus});
      throw Error(`unexpected ${url}`);
    };
    const adapter=new GoogleMailOAuthAdapter(fetcher,false);
    const config=adapter.createConfig(bindings(),"http://localhost:8787")!;
    const verifier=oidc.randomPKCECodeVerifier();
    const url=await adapter.authorizationUrl(config,"state","nonce",verifier);
    expect(url.searchParams.get("code_challenge")).toBe(await oidc.calculatePKCECodeChallenge(verifier));
    expect(url.searchParams.get("redirect_uri")).toBe("http://localhost:8787/integrations/gmail/callback");
    const callback=adapter.callbackUrl(config,new URLSearchParams({state:"state",code:"code"}));
    const result=await adapter.exchangeCode(config,callback,"state","nonce",verifier);
    expect(result.scopes).toEqual(scopes);expect(adapter.capabilities(result.scopes)).toEqual({can_send:true,can_read_thread:false,can_read_attachment:false});
    expect(adapter.capabilities([GOOGLE_MAIL_SCOPES[3]])).toMatchObject({can_send:false,can_read_thread:true});
    expect(await adapter.refresh(config,"refresh")).toMatchObject({access_token:"refreshed",scopes:["https://www.googleapis.com/auth/gmail.readonly"]});
    refreshScope=undefined;expect(await adapter.refresh(config,"refresh")).not.toHaveProperty("scopes");
    refreshScope="";await expect(adapter.refresh(config,"refresh")).rejects.toMatchObject({code:"provider_response_invalid"});
    refreshScope="https://www.googleapis.com/auth/gmail.readonly";
    invalidRefresh=true;await expect(adapter.refresh(config,"refresh")).rejects.toSatisfy(error=>adapter.isReauthRequired(error));invalidRefresh=false;
    await adapter.revoke(config,"refresh");expect(calls.some(call=>call.url.endsWith("/revoke"))).toBe(true);
    revokeStatus=503;await expect(adapter.revoke(config,"refresh")).rejects.toThrow();
    revokeStatus=400;await expect(adapter.revoke(config,"refresh")).rejects.toThrow();
    revokeStatus=200;
    const priorRevocations=calls.filter(call=>call.url.endsWith("/revoke")).length;
    corruptSignature=true;await expect(adapter.exchangeCode(config,callback,"state","nonce",verifier)).rejects.toThrow();corruptSignature=false;
    missingGrantRefresh=true;await expect(adapter.exchangeCode(config,callback,"state","nonce",verifier)).rejects.toMatchObject({code:"provider_response_invalid"});missingGrantRefresh=false;
    verifiedEmail=false;await expect(adapter.exchangeCode(config,callback,"state","nonce",verifier)).rejects.toMatchObject({code:"provider_response_invalid"});verifiedEmail=true;
    expect(calls.filter(call=>call.url.endsWith("/revoke"))).toHaveLength(priorRevocations);
    claimOverride={nonce:"wrong"};await expect(adapter.exchangeCode(config,callback,"state","nonce",verifier)).rejects.toThrow();
    claimOverride={aud:"wrong-audience"};await expect(adapter.exchangeCode(config,callback,"state","nonce",verifier)).rejects.toThrow();
    claimOverride={iss:"https://wrong-issuer.test"};await expect(adapter.exchangeCode(config,callback,"state","nonce",verifier)).rejects.toThrow();
    await expect(adapter.exchangeCode(config,adapter.callbackUrl(config,new URLSearchParams({state:"wrong",code:"code"})),"state","nonce",verifier)).rejects.toThrow();
  });
});


describe("neutral credential encryption",()=>{
  it("binds encrypted material to owner, provider and connection",async()=>{
    const encrypted=await encryptToken(secret,"refresh-only","owner-a","google","conn-a");
    expect(encrypted.ciphertext).not.toContain("refresh-only");
    expect(await decryptToken(secret,encrypted,"owner-a","google","conn-a")).toBe("refresh-only");
    await expect(decryptToken(secret,encrypted,"owner-b","google","conn-a")).rejects.toThrow();
    await expect(decryptToken(secret,encrypted,"owner-a","outlook","conn-a")).rejects.toThrow();
    await expect(decryptToken(secret,encrypted,"owner-a","google","conn-b")).rejects.toThrow();
  });
});
