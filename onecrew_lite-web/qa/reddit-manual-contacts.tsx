// Development-only visual fixture. Never writes customer data or calls Reddit.
import { createRoot } from "react-dom/client";
import { createInstance } from "i18next";
import { I18nextProvider } from "react-i18next";
import { DiscoveryManualContacts } from "../src/pages/connect/discovery-manual-contacts";
import zh from "../src/i18n/locales/zh-CN/shell";
import en from "../src/i18n/locales/en-US/shell";
import "../src/styles/index.css";

const locale = new URL(location.href).searchParams.get("locale") === "en-US" ? "en-US" : "zh-CN";
const i18n = createInstance();
await i18n.init({ lng: locale, resources: { "zh-CN": { shell: zh }, "en-US": { shell: en } } });
document.documentElement.lang = locale;
createRoot(document.getElementById("root")!).render(<I18nextProvider i18n={i18n}>
  <main className="mx-auto max-w-6xl space-y-6 p-6">
    <h1 className="text-xl font-semibold">{locale === "zh-CN" ? "Reddit 人工联系表 · 隔离模拟验收" : "Reddit manual contacts · isolated simulated QA"}</h1>
    <p>{locale === "zh-CN" ? "本页为模拟测试数据，不是客户搜索结果；不执行平台请求或发送。" : "Simulated test data, not customer search results. No provider requests or sending."}</p>
    <DiscoveryManualContacts candidates={[{
      id: "reddit-qa", platform: "reddit", platform_account_id: "t2_user123",
      display_name: "creator_1 (QA)", profile_url: "https://www.reddit.com/user/creator_1/", verdict: "qualified",
      evidence: { source_urls: ["https://www.reddit.com/comments/abc1/"], manual_profile_contact: true },
    }]} />
  </main>
</I18nextProvider>);
