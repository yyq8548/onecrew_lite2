import { escapeEmailHtml, renderEmailLayout } from "../auth/email-layout";
import type { EmailLocale } from "../auth/email-types";

const copy = {
  "zh-CN": {
    approved: {
      subject: "你的 OneCrew 体验申请已通过",
      title: "欢迎体验 OneCrew",
      description: "你的体验申请已通过审核。前往个人中心，开始使用 OneCrew。",
      action: "获取安装提示词",
      next: "登录个人中心，生成并复制 OneCrew 安装提示词，交给 AI 助手自动完成对应系统的安装。",
      notice: "安装链接有效期为 24 小时，过期后可在个人中心重新生成。",
    },
    rejected: {
      subject: "你的 OneCrew 体验申请审核结果",
      title: "体验申请审核结果",
      description: "本次申请暂未通过。你可以根据以下原因修改申请后重新提交。",
      action: "修改申请并重新提交",
      next: "我们已保留你的申请内容。前往个人中心修改后，可以再次提交审核。",
      notice: "此邮件中的审核原因也可在你的个人中心查看。",
    },
    greeting: (name: string) => name ? `你好，${name}。` : "你好。",
    reason: "审核原因",
    fallback: "如果按钮无法打开，请复制以下地址到浏览器：",
  },
  "en-US": {
    approved: {
      subject: "Your OneCrew early access is approved",
      title: "Welcome to OneCrew",
      description: "Your early-access application is approved. Visit your account to get started with OneCrew.",
      action: "Get your installation prompt",
      next: "Sign in to your account, generate and copy your OneCrew installation prompt, and give it to your AI assistant to install the version for your system.",
      notice: "Installation links expire after 24 hours. Generate a new link in your account when needed.",
    },
    rejected: {
      subject: "An update on your OneCrew application",
      title: "Your application review",
      description: "Your application was not approved this time. You can update it using the feedback below and apply again.",
      action: "Update your application and reapply",
      next: "Your application details have been saved. Visit your account to make changes and submit them for another review.",
      notice: "You can also view this review feedback in your account.",
    },
    greeting: (name: string) => name ? `Hello, ${name}.` : "Hello.",
    reason: "Review feedback",
    fallback: "If the button does not open, copy this address into your browser:",
  },
} as const;

export function renderDistributionEmail(input: {
  decision: "approved" | "rejected";
  locale: EmailLocale;
  displayName: string;
  reviewReason?: string;
  accountUrl: string;
}): { subject: string; html: string; text: string } {
  const url = new URL(input.accountUrl);
  const localHttp = url.protocol === "http:" && ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname);
  if ((url.protocol !== "https:" && !localHttp) || url.pathname !== "/account"
    || url.search !== "?apply=1" || url.hash || url.username || url.password) {
    throw new Error("invalid_distribution_account_url");
  }
  const reason = input.reviewReason?.trim() ?? "";
  if (input.decision === "rejected" && !reason) throw new Error("distribution_review_reason_required");
  const language = copy[input.locale];
  const message = language[input.decision];
  const greeting = language.greeting(input.displayName.trim());
  const accountUrl = url.toString();
  const safeUrl = escapeEmailHtml(accountUrl);
  const reasonHtml = input.decision === "rejected" ? `
              <div style="margin:24px 0 0;padding:18px 20px;background:#fff8dc;">
                <p style="margin:0 0 8px;color:#58655d;font-size:13px;line-height:1.5;">${escapeEmailHtml(language.reason)}</p>
                <p style="margin:0;color:#101712;font-size:15px;line-height:1.7;overflow-wrap:anywhere;word-break:break-word;">${escapeEmailHtml(reason).replace(/\r\n?|\n/g, "<br>")}</p>
              </div>` : "";
  const html = renderEmailLayout({
    locale: input.locale,
    title: message.title,
    description: message.description,
    contentHtml: `
              <p style="margin:24px 0 0;color:#101712;font-size:15px;line-height:1.7;">${escapeEmailHtml(greeting)}</p>${reasonHtml}
              <p style="margin:18px 0 0;color:#58655d;font-size:15px;line-height:1.7;">${escapeEmailHtml(message.next)}</p>
              <table role="presentation" width="100%" cellspacing="0" cellpadding="0" border="0" style="margin:24px 0 0;">
                <tr><td align="center" bgcolor="#ffde65" style="background:#ffde65;border-radius:6px;">
                  <a href="${safeUrl}" style="display:block;padding:15px 18px;color:#101712;font-size:15px;line-height:1.5;font-weight:700;text-align:center;text-decoration:none;">${escapeEmailHtml(message.action)}</a>
                </td></tr>
              </table>
              <p style="margin:18px 0 0;color:#7b877f;font-size:13px;line-height:1.7;text-align:center;">${escapeEmailHtml(message.notice)}</p>
              <p style="margin:22px 0 0;color:#7b877f;font-size:12px;line-height:1.7;">${escapeEmailHtml(language.fallback)}<br><a href="${safeUrl}" style="color:#58655d;overflow-wrap:anywhere;word-break:break-all;">${escapeEmailHtml(accountUrl)}</a></p>`,
  });
  const text = [message.title, message.description, greeting,
    ...(input.decision === "rejected" ? [`${language.reason}\n${reason}`] : []),
    message.next, `${message.action}\n${accountUrl}`, message.notice, "OneCrew"].join("\n\n");
  return { subject: message.subject, html, text };
}
