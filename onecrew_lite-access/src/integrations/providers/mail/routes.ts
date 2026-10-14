import { Hono } from "hono";
import { html } from "hono/html";
import { bodyLimit } from "hono/body-limit";
import { loadSafeSession } from "../../../auth/session";
import { verifyDeviceCredential } from "../../../devices/credential-identity";
import { IntegrationError } from "../../../devices/errors";
import type { AppEnv } from "../../../types/bindings";
import { connectionIdSchema, MAIL_PREFIX } from "./contracts";
import { mailFailure, mailResponse, requestIdSchema, mailBodyLimit } from "./http";
import { beginMailOAuth, cancelMailOAuth, completeMailOAuth } from "./repository";
import { createMailOAuthRegistry, mailOAuthConfig, type MailOAuthRegistry } from "./adapters";
import { disconnectMailConnection, getMailConnection, issueMailAccessToken, listMailConnections } from "./service";
import type { MailOAuthProvider } from "./types";

const pageHeaders = {"Cache-Control":"no-store","X-Content-Type-Options":"nosniff","X-Robots-Tag":"noindex, nofollow","Referrer-Policy":"no-referrer","Content-Security-Policy":"default-src 'none'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'"};
async function page(title:string,body:ReturnType<typeof html>,status=200) {
  return new Response(await html`<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>${title}</title><main>${body}</main></html>`,{status,headers:{...pageHeaders,"Content-Type":"text/html; charset=UTF-8"}});
}
function pageError(error:unknown) {
  const typed=error instanceof IntegrationError?error:new IntegrationError("storage_unavailable",503,true);
  return page("Mailbox connection",html`<h1>Mailbox connection failed</h1><p>${typed.code}</p><p>Return to OneCrew and try again.</p>`,typed.status);
}
function providerFrom(url:URL):MailOAuthProvider {
  if(url.searchParams.getAll("provider").length!==1) throw new IntegrationError("invalid_request",400);
  const provider=url.searchParams.get("provider");
  if(provider!=="google"&&provider!=="outlook") throw new IntegrationError("invalid_request",400);
  return provider;
}
function expectedOwner(url:URL) {
  if(url.searchParams.getAll("expected_user_id").length>1) throw new IntegrationError("invalid_request",400);
  const expected=url.searchParams.get("expected_user_id");
  if(expected!==null&&(expected.length<1||expected.length>128)) throw new IntegrationError("invalid_request",400);
  return expected;
}

export function createMailRoutes(registry:MailOAuthRegistry=createMailOAuthRegistry()) {
  const machine=new Hono<AppEnv>()
    .use("*",async(c,next)=>{
      let requestId:string=crypto.randomUUID();
      try {
        const supplied=c.req.header("X-OneCrew-Request-ID");
        if(supplied!==undefined) {
          const parsed=requestIdSchema.safeParse(supplied);
          if(!parsed.success) throw new IntegrationError("invalid_request_id",400);
          requestId=parsed.data;
        }
        c.set("mailRequestId",requestId);
        c.set("mailOwner",await verifyDeviceCredential(c.env,c.get("config").db,c.req.raw));
        await next();
      } catch(error) {return mailFailure(requestId,error instanceof IntegrationError?error:new IntegrationError("storage_unavailable",503,true));}
    })
    .get("/connections",async c=>mailResponse(c.get("mailRequestId"),await listMailConnections(c.get("config").db,c.get("mailOwner").userId,registry)))
    .get("/connections/:id",async c=>{
      const id=connectionIdSchema.safeParse(c.req.param("id")); if(!id.success) throw new IntegrationError("invalid_request",400);
      return mailResponse(c.get("mailRequestId"),{connection:await getMailConnection(c.get("config").db,c.get("mailOwner").userId,id.data,registry)});
    })
    .post("/connections/:id/access-token",mailBodyLimit,async c=>{
      const id=connectionIdSchema.safeParse(c.req.param("id")); if(!id.success) throw new IntegrationError("invalid_request",400);
      const token=await issueMailAccessToken(c.get("config").db,c.env,c.get("config").authOrigin,c.get("mailOwner").userId,id.data,registry);
      return mailResponse(c.get("mailRequestId"),token);
    })
    .delete("/connections/:id",mailBodyLimit,async c=>{
      const id=connectionIdSchema.safeParse(c.req.param("id")); if(!id.success) throw new IntegrationError("invalid_request",400);
      const connection=await disconnectMailConnection(c.get("config").db,c.env,c.get("config").authOrigin,c.get("mailOwner").userId,id.data,registry);
      return mailResponse(c.get("mailRequestId"),{connection});
    });
  machine.onError((error,c)=>mailFailure(c.get("mailRequestId")??crypto.randomUUID(),error instanceof IntegrationError?error:new IntegrationError("storage_unavailable",503,true)));
  const browser=new Hono<AppEnv>()
    .get("/integrations/mail/connect",async c=>{
      try {
        const url=new URL(c.req.url);const provider=providerFrom(url);const expected=expectedOwner(url);
        mailOAuthConfig(provider,c.env,c.get("config").authOrigin,registry);
        const session=await loadSafeSession(c.get("auth"),c.req.raw.headers);
        if(!session.session) return Response.redirect(new URL(`/login?${new URLSearchParams({returnTo:`${url.pathname}${url.search}`})}`,c.get("config").webOrigin),302);
        const current=session.session.user;
        if(expected&&expected!==current.id) return page("Switch OneCrew account",html`<h1>Different OneCrew account</h1><p>This browser is signed in as ${current.email}. Sign in with the OneCrew account active in your local panel, then reopen this link.</p>`,403);
        return page("Connect mailbox",html`<h1>Connect ${provider} mailbox</h1><p>OneCrew account: ${current.email}</p><p>The mailbox you choose at Google may have a different address. Access stores its refresh credential and gives your linked local device short-lived access tokens.</p><form method="post" action="${url.pathname}${url.search}"><button type="submit">Continue to ${provider}</button></form>`);
      } catch(error){return pageError(error);}
    })
    .post("/integrations/mail/connect",bodyLimit({maxSize:0,onError:()=>pageError(new IntegrationError("invalid_request",400))}),async c=>{
      try {
        if(c.req.header("Origin")!==c.get("config").webOrigin) throw new IntegrationError("invalid_request",403);
        const url=new URL(c.req.url);const provider=providerFrom(url);const expected=expectedOwner(url);
        const session=await loadSafeSession(c.get("auth"),c.req.raw.headers);
        if(!session.session) throw new IntegrationError("device_unauthorized",401);
        if(expected&&expected!==session.session.user.id) throw new IntegrationError("account_not_authorized",403);
        const destination=await beginMailOAuth(c.get("config").db,c.env,c.get("config").authOrigin,session.session.user.id,provider,registry);
        return Response.redirect(destination,303);
      } catch(error){return pageError(error);}
    })
    .get("/integrations/gmail/callback",async c=>{
      try {
        const url=new URL(c.req.url);
        if([...url.searchParams.keys()].some(key=>url.searchParams.getAll(key).length!==1)) throw new IntegrationError("invalid_request",400);
        const state=url.searchParams.get("state");if(!state||state.length<40||state.length>128) throw new IntegrationError("authorization_expired",410);
        const session=await loadSafeSession(c.get("auth"),c.req.raw.headers);
        if(!session.session) throw new IntegrationError("device_unauthorized",401);
        if(url.searchParams.has("error")) {
          await cancelMailOAuth(c.get("config").db,state,session.session.user.id);
          return page("Mailbox connection canceled",html`<h1>Mailbox connection canceled</h1><p>Return to OneCrew to continue.</p>`);
        }
        if(!url.searchParams.get("code")||url.searchParams.get("code")!.length>4096) throw new IntegrationError("invalid_request",400);
        const result=await completeMailOAuth(c.get("config").db,c.env,c.get("config").authOrigin,state,session.session.user.id,url,registry);
        return page("Mailbox connected",html`<h1>Mailbox connected</h1><p>${result.email} is ready in OneCrew.</p><p>Return to your local OneCrew panel and refresh your mailbox list.</p>`);
      } catch(error){return pageError(error);}
    });
  return new Hono<AppEnv>().route(MAIL_PREFIX,machine).route("/",browser);
}
export const mailRoutes=createMailRoutes();
