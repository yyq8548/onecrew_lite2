import { describe, expect, it } from "vitest";
import { renderDistributionEmail } from "../src/distribution/email-template";
import { renderVerificationEmail } from "../src/auth/email-template";
const accountUrl = "https://access.onecrewai.io/account?apply=1";

describe("distribution decision email", () => {
  it.each(["zh-CN", "en-US"] as const)("renders approved and rejected HTML/text with the OTP brand frame in %s", locale => {
    const otp = renderVerificationEmail({ locale, otp: "123456", type: "sign-in" });
    for (const decision of ["approved", "rejected"] as const) {
      const mail = renderDistributionEmail({ decision, locale, displayName: "Alex 陈", reviewReason: "Please describe your planned use.", accountUrl });
      expect(mail.html).toContain(`lang="${locale === "en-US" ? "en" : "zh-CN"}"`);
      for (const brand of ["cid:onecrew-mark", "background:#f4f5ef", "max-width:480px", "border-top:4px solid #ffde65"]) {
        expect(mail.html).toContain(brand); expect(otp.html).toContain(brand);
      }
      expect(mail.html).toContain(`href="${accountUrl}"`);
      expect(mail.text).toContain(accountUrl);
      expect(mail.html).not.toMatch(/\/d\/[A-Za-z0-9_-]/);
      expect(mail.text).not.toMatch(/\/d\/[A-Za-z0-9_-]/);
      expect(mail.subject).toContain("OneCrew");
      if (decision === "rejected") expect(mail.text).toContain("Please describe your planned use.");
      else expect(mail.text).not.toContain("Please describe your planned use.");
    }
  });
  it("escapes applicant names and user-visible review reasons without losing plain-text meaning", () => {
    const mail = renderDistributionEmail({ decision: "rejected", locale: "zh-CN", displayName: '<script>"A & B"</script>', reviewReason: '<img src=x onerror="alert(1)">\nKeep < > & \' " visible', accountUrl });
    expect(mail.html).not.toContain("<script>"); expect(mail.html).not.toContain("<img src=x");
    expect(mail.html).toContain("&lt;script&gt;&quot;A &amp; B&quot;&lt;/script&gt;");
    expect(mail.html).toContain("&lt;img src=x onerror=&quot;alert(1)&quot;&gt;<br>Keep &lt; &gt; &amp; &#39; &quot; visible");
    expect(mail.text).toContain('<img src=x onerror="alert(1)">');
  });
  it.each(["javascript:alert(1)", "https://access.onecrewai.io/d/AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", "https://access.onecrewai.io/account?apply=1&token=secret", "https://user:password@access.onecrewai.io/account?apply=1", "https://access.onecrewai.io/account?apply=1#secret", "http://external.test/account?apply=1"])("rejects an unsafe or non-account CTA %s", url => {
    expect(() => renderDistributionEmail({ decision: "approved", locale: "en-US", displayName: "Alex", accountUrl: url })).toThrow();
  });
  it("requires a visible reason for rejected notices", () => {
    expect(() => renderDistributionEmail({ decision: "rejected", locale: "zh-CN", displayName: "Alex", reviewReason: "  ", accountUrl })).toThrow("distribution_review_reason_required");
  });
  it("does not insert internal approval reasons into either MIME alternative", () => {
    const mail = renderDistributionEmail({ decision: "approved", locale: "en-US", displayName: "Alex", reviewReason: "internal-only", accountUrl });
    expect(mail.html).not.toContain("internal-only"); expect(mail.text).not.toContain("internal-only");
  });
});
