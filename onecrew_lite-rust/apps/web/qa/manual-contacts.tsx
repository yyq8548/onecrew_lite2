// Development-only entry, not the production index. No service, account or provider calls.
import { createRoot } from "react-dom/client";
import { createInstance } from "i18next";
import { I18nextProvider } from "react-i18next";
import { DiscoveryManualContacts } from "../src/pages/connect/discovery-manual-contacts";
import type { DiscoveryCandidate } from "../src/lib/opencli-discovery";
import zh from "../src/i18n/locales/zh-CN/shell";
import en from "../src/i18n/locales/en-US/shell";
import audit from "../../../../../audits/lite-youtube-hidden-email-2026-10-09.json";
import linkedinAudit from "../../../../../audits/lite-linkedin-ashley-owner-publication-2026-10-09.json";
import "../src/styles/index.css";

const saved = audit.manual_display_candidate;
const candidate: DiscoveryCandidate = {
  id: saved.id, platform: saved.platform, platform_account_id: saved.platform_account_id,
  display_name: saved.display_name, profile_url: saved.profile_url,
  verdict: saved.verdict as DiscoveryCandidate["verdict"],
  evidence: { source_urls: [audit.attempts[0].source_url], contact_status: saved.contact_status },
};
const locale = new URL(location.href).searchParams.get("locale") === "en-US" ? "en-US" : "zh-CN";
// Exact public owner projection saved after real scoring/contact/reopen acceptance.
const linkedin = linkedinAudit.displayed_candidates[0] as DiscoveryCandidate;
const i18n = createInstance();
await i18n.init({ lng: locale, resources: { "zh-CN": { shell: zh }, "en-US": { shell: en } } });
document.documentElement.lang = locale;
createRoot(document.getElementById("root")!).render(
  <I18nextProvider i18n={i18n}>
    <main className="mx-auto max-w-6xl space-y-6 p-6">
      <h1 className="text-xl font-semibold">{locale === "zh-CN" ? "隔离验收：已保存的公开证据" : "Isolated QA: saved public evidence"}</h1>
      <p>{locale === "zh-CN" ? "使用 YouTube 和 LinkedIn 真实验收保存的公开证据；这是隔离组件预览，安装版整页仍需验收。" : "Saved public evidence from real YouTube and LinkedIn acceptance; this isolated component preview does not replace installed-page acceptance."}</p>
      <DiscoveryManualContacts candidates={[candidate, linkedin]} />
    </main>
  </I18nextProvider>,
);
