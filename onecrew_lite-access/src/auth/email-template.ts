import { escapeEmailHtml, renderEmailLayout } from "./email-layout";
import type { EmailLocale, VerificationOTPType } from "./email-types";
import { emailLocales } from "./email-locales";

export function renderVerificationEmail(input: {
  locale: EmailLocale;
  otp: string;
  type: VerificationOTPType;
}) {
  const copy = emailLocales[input.locale][input.type];
  const otp = escapeEmailHtml(input.otp);
  const codeLabel = escapeEmailHtml(copy.codeLabel);
  const expires = escapeEmailHtml(copy.expires);
  const securityNotice = escapeEmailHtml(copy.securityNotice);
  const html = renderEmailLayout({
    locale: input.locale,
    title: copy.title,
    description: copy.description,
    contentHtml: `              <p style="margin:28px 0 10px;color:#58655d;font-size:13px;line-height:1.5;text-align:center;">${codeLabel}</p>
              <p style="margin:0;padding:18px 12px;color:#101712;background:#fff8dc;font-size:32px;line-height:1.2;font-weight:700;letter-spacing:.28em;text-align:center;">${otp}</p>
              <p style="margin:18px 0 0;color:#58655d;font-size:14px;line-height:1.7;text-align:center;">${expires}</p>
              <p style="margin:16px 0 0;color:#7b877f;font-size:13px;line-height:1.7;text-align:center;">${securityNotice}</p>`,
  });
  return { html, subject: copy.subject };
}
