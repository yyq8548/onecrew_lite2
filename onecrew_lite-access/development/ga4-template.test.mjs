import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import vm from "node:vm";
import { webcrypto } from "node:crypto";
import test from "node:test";
import { renderGa4Init } from "./ga4-template.mjs";

const source = renderGa4Init(await readFile(new URL("../web/public/ga4-init.js", import.meta.url), "utf8"));
function page(surface, href) {
  const listeners = {};
  const notifications = [];
  const window = { location: { href }, dispatchEvent: event => notifications.push(event.type) };
  const document = { cookie: "", currentScript: { dataset: { surface } }, referrer: "https://www.google.com/search?q=private", title: "Marketing title", addEventListener: (name, fn) => { listeners[name] = fn; } };
  const context = vm.createContext({ window, document, URL, Date, Set, Promise, setTimeout, clearTimeout, crypto: webcrypto, TextEncoder, Uint8Array, Event });
  vm.runInContext(source, context);
  return { window, document, context, listeners, notifications, commands: () => JSON.parse(JSON.stringify(window.dataLayer.map(args => Array.from(args)))) };
}

test("both surfaces initialize the same tag and root cookie domain once", () => {
  for (const surface of ["marketing", "access"]) {
    const p = page(surface, surface === "marketing" ? "https://onecrewai.io/" : "https://access.onecrewai.io/account");
    vm.runInContext(source, p.context);
    const configs = p.commands().filter(args => args[0] === "config");
    assert.equal(configs.length, 1);
    assert.equal(configs[0][1], "G-E8PCYC26GN");
    assert.equal(configs[0][2].cookie_domain, "onecrewai.io");
    assert.equal(configs[0][2].cookie_path, "/");
    assert.equal(configs[0][2].send_page_view, false);
    assert.equal(p.commands().filter(args => args[0] === "event").length, 0);
  }
  assert.doesNotMatch(source, /__ONECREW_/);
});

test("page views keep channel parameters, redact sensitive context and dedupe each navigation", () => {
  const p = page("marketing", "https://onecrewai.io/?utm_source=qa&utm_medium=test&email=private%40example.com&token=SECRET#private");
  p.window.onecrewAnalytics.pageView("initial");
  p.window.onecrewAnalytics.pageView("initial");
  p.window.location.href = "https://onecrewai.io/pricing";
  p.document.title = "Pricing";
  p.window.onecrewAnalytics.pageView("pricing");
  const events = p.commands().filter(args => args[0] === "event");
  assert.equal(events.length, 2);
  assert.equal(events[0][2].page_location, "https://onecrewai.io/?utm_source=qa&utm_medium=test");
  assert.equal(events[0][2].page_referrer, "https://www.google.com/");
  assert.equal(events[1][2].page_title, "Pricing");
  assert.doesNotMatch(JSON.stringify(p.commands()), /SECRET|private/);
});

test("Access business events contain no raw form, download-token or referrer path values", () => {
  const p = page("access", "https://access.onecrewai.io/d/SECRET?returnTo=private&email=private%40example.com&utm_source=private%40example.com");
  p.document.title = "Private user name";
  p.window.onecrewAnalytics.business("sign_up");
  p.window.onecrewAnalytics.business("generate_lead");
  p.window.onecrewAnalytics.business("login");
  const events = p.commands().filter(args => args[0] === "event");
  assert.deepEqual(events.map(args => args[1]), ["sign_up", "generate_lead"]);
  assert.equal(events[0][2].page_location, "https://access.onecrewai.io/download");
  assert.equal(events[0][2].page_title, "OneCrew Access");
  assert.doesNotMatch(JSON.stringify(p.commands()), /SECRET|private|Private user/);
});

test("CTA is emitted only for an actual matching click and never blocks navigation", () => {
  const p = page("marketing", "https://onecrewai.io/");
  assert.equal(p.commands().filter(args => args[0] === "event").length, 0);
  const click = href => p.listeners.click({ target: { closest: () => ({ href }) } });
  click("https://onecrewai.io/pricing");
  click("https://access.onecrewai.io/account?apply=1&returnTo=SECRET");
  assert.deepEqual(p.commands().filter(args => args[0] === "event").map(args => args[1]), ["cta_click"]);
  assert.doesNotMatch(JSON.stringify(p.commands()), /SECRET/);
  p.window.gtag = () => { throw new Error("blocked"); };
  assert.doesNotThrow(() => click("https://access.onecrewai.io/account?apply=1"));
});

test("the first-party context binds both GA client and session without storing raw IDs", async () => {
  const p = page("access", "https://access.onecrewai.io/account");
  const callbacks = {};
  p.window.gtag = (_command, _measurement, field, callback) => { callbacks[field] = callback; };
  const ready = p.window.onecrewAnalytics.ready();
  callbacks.client_id("client-A"); callbacks.session_id("session-A");
  assert.equal(await ready, true);
  const cookie = p.document.cookie;
  assert.match(cookie, /^__Host-onecrew-ga-context=[a-f0-9]{64}; Path=\/; Secure; SameSite=Lax; Max-Age=1800$/);
  assert.doesNotMatch(cookie, /client-A|session-A/);
  const changed = p.window.onecrewAnalytics.ready();
  callbacks.client_id("client-A"); callbacks.session_id("session-B");
  assert.equal(await changed, true);
  assert.notEqual(p.document.cookie, cookie);
});

test("late tag readiness is signalled after the original timeout", async () => {
  const p = page("access", "https://access.onecrewai.io/account");
  let timeout;
  p.context.setTimeout = callback => { timeout = callback; return 1; };
  p.context.clearTimeout = () => undefined;
  const callbacks = {};
  p.window.gtag = (_command, _measurement, field, callback) => { callbacks[field] = callback; };
  const ready = p.window.onecrewAnalytics.ready(); timeout();
  assert.equal(await ready, false);
  const notified = new Promise(resolve => { p.window.dispatchEvent = event => resolve(event.type); });
  callbacks.client_id("client-A"); callbacks.session_id("session-A");
  assert.equal(await notified, "onecrew:analytics-ready");
});
