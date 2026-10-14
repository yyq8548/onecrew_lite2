import type { EmailLocale } from "./email-types";

/** Shared brand frame for transactional email; contentHtml is trusted renderer output. */
export function renderEmailLayout(input: {
  locale: EmailLocale;
  title: string;
  description: string;
  preview?: string;
  contentHtml: string;
}) {
  const lang = input.locale === "en-US" ? "en" : "zh-CN";
  const title = escapeEmailHtml(input.title);
  const description = escapeEmailHtml(input.description);
  const preview = escapeEmailHtml(input.preview ?? input.description);
  const html = `<!doctype html>
<html lang="${lang}">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width,initial-scale=1">
    <title>${title}</title>
  </head>
  <body style="margin:0;background:#f4f5ef;color:#101712;font-family:Arial,'PingFang SC','Microsoft YaHei',sans-serif;">
    <div style="display:none;max-height:0;overflow:hidden;opacity:0;">${preview}</div>
    <table role="presentation" width="100%" cellspacing="0" cellpadding="0" border="0" style="background:#f4f5ef;">
      <tr><td align="center" style="padding:32px 16px;">
        <table role="presentation" width="100%" cellspacing="0" cellpadding="0" border="0" style="max-width:480px;">
          <tr><td align="center" style="padding:0 0 24px;">
            <img src="cid:onecrew-mark" width="48" height="48" alt="OneCrew" style="display:block;width:48px;height:48px;border:0;border-radius:12px;">
          </td></tr>
          <tr><td style="border-top:4px solid #ffde65;background:#ffffff;">
            <div style="padding:32px 28px 30px;">
              <h1 style="margin:0;color:#101712;font-size:26px;line-height:1.25;font-weight:700;text-align:center;">${title}</h1>
              <p style="margin:14px 0 0;color:#58655d;font-size:16px;line-height:1.65;text-align:center;">${description}</p>
${input.contentHtml}
            </div>
          </td></tr>
          <tr><td style="padding:20px 8px 0;color:#7b877f;font-size:12px;line-height:1.6;text-align:center;">OneCrew</td></tr>
        </table>
      </td></tr>
    </table>
  </body>
</html>`;
  return html;
}

export function escapeEmailHtml(value: string) {
  return value.replace(/[&<>'"]/g, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    "'": "&#39;",
    '"': "&quot;",
  })[character] ?? character);
}
