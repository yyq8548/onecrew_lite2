import { Resend } from "resend";
import type { ResendAuthConfig } from "../config/env";
import { DEFAULT_EMAIL_LOCALE } from "./email-locale";
import { renderVerificationEmail } from "./email-template";
import { ONECREW_AI_MARK_PNG_BASE64 } from "./email-assets";
import type { EmailLocale, VerificationOTPType } from "./email-types";
export type { EmailLocale, VerificationOTPType } from "./email-types";

export async function sendVerificationOTP(
  config: ResendAuthConfig,
  input: { email: string; otp: string; type: VerificationOTPType },
  locale: EmailLocale = DEFAULT_EMAIL_LOCALE,
) {
  const resend = new Resend(config.apiKey);
  const content = renderVerificationEmail({ ...input, locale });
  const { error } = await resend.emails.send({
    from: config.from,
    to: [input.email],
    subject: content.subject,
    html: content.html,
    attachments: [{
      content: ONECREW_AI_MARK_PNG_BASE64,
      contentId: "onecrew-mark",
      contentType: "image/png",
      filename: "onecrew-ai-mark.png",
    }],
  });
  if (error) throw new Error("email_delivery_failed");
}
