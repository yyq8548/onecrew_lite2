export type VerificationOTPType =
  | "sign-in"
  | "email-verification"
  | "forget-password"
  | "change-email";

export type EmailLocale = "zh-CN" | "en-US";

export type VerificationEmailCopy = {
  codeLabel: string;
  description: string;
  expires: string;
  securityNotice: string;
  subject: string;
  title: string;
};
