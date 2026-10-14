import type { VerificationEmailCopy, VerificationOTPType } from "../../email-types";

const zhCN: Record<VerificationOTPType, VerificationEmailCopy> = {
  "sign-in": {
    codeLabel: "你的登录验证码",
    description: "使用下面的验证码完成 OneCrew 登录。",
    expires: "验证码 5 分钟内有效。",
    securityNotice: "如果不是你本人操作，请忽略此邮件。",
    subject: "OneCrew 登录验证码",
    title: "登录 OneCrew",
  },
  "email-verification": {
    codeLabel: "你的邮箱验证码",
    description: "使用下面的验证码验证你的 OneCrew 邮箱。",
    expires: "验证码 5 分钟内有效。",
    securityNotice: "如果不是你本人操作，请忽略此邮件。",
    subject: "验证你的 OneCrew 邮箱",
    title: "验证你的 OneCrew 邮箱",
  },
  "forget-password": {
    codeLabel: "你的密码重置验证码",
    description: "使用下面的验证码重置你的 OneCrew 密码。",
    expires: "验证码 5 分钟内有效。",
    securityNotice: "如果不是你本人操作，请忽略此邮件。",
    subject: "OneCrew 重置密码验证码",
    title: "重置 OneCrew 密码",
  },
  "change-email": {
    codeLabel: "你的邮箱变更验证码",
    description: "使用下面的验证码确认你的新邮箱地址。",
    expires: "验证码 5 分钟内有效。",
    securityNotice: "如果不是你本人操作，请忽略此邮件。",
    subject: "OneCrew 更换邮箱验证码",
    title: "确认新的 OneCrew 邮箱",
  },
};

export default zhCN;
