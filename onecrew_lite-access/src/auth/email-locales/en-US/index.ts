import type { VerificationEmailCopy, VerificationOTPType } from "../../email-types";

const enUS: Record<VerificationOTPType, VerificationEmailCopy> = {
  "sign-in": {
    codeLabel: "Your sign-in code",
    description: "Use the code below to sign in to OneCrew.",
    expires: "This code is valid for 5 minutes.",
    securityNotice: "If you did not request this email, you can ignore it.",
    subject: "Your OneCrew sign-in code",
    title: "Sign in to OneCrew",
  },
  "email-verification": {
    codeLabel: "Your verification code",
    description: "Use the code below to verify your OneCrew email.",
    expires: "This code is valid for 5 minutes.",
    securityNotice: "If you did not request this email, you can ignore it.",
    subject: "Verify your OneCrew email",
    title: "Verify your OneCrew email",
  },
  "forget-password": {
    codeLabel: "Your password reset code",
    description: "Use the code below to reset your OneCrew password.",
    expires: "This code is valid for 5 minutes.",
    securityNotice: "If you did not request this email, you can ignore it.",
    subject: "Your OneCrew password reset code",
    title: "Reset your OneCrew password",
  },
  "change-email": {
    codeLabel: "Your email change code",
    description: "Use the code below to confirm your new email address.",
    expires: "This code is valid for 5 minutes.",
    securityNotice: "If you did not request this email, you can ignore it.",
    subject: "Your OneCrew email change code",
    title: "Confirm your new OneCrew email",
  },
};

export default enUS;
