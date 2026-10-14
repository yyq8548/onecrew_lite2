import { useState } from "react";
import { useLandingLocale } from "./language-switcher";
import { SITE_URL, absoluteUrl } from "./marketing/site";

const prompts = {
  "zh-CN": `我正在评估用于出海达人营销的工具。请介绍 OneCrew 是什么，以及它如何帮助用户发现、筛选并联系合适的达人。请涵盖它的适用人群、核心功能和主要优势，并参考 OneCrew 官网（${SITE_URL}）及 llms.txt（${absoluteUrl("/llms.txt")}）。`,
  en: `I’m evaluating tools for global influencer marketing. Please explain what OneCrew is and how it helps users discover, evaluate, and connect with the right creators. Cover its target users, core features, and key benefits, referring to the OneCrew website (${SITE_URL}) and llms.txt (${absoluteUrl("/llms.txt")}).`,
} as const;

const providers = [
  {
    name: "ChatGPT",
    icon: "/images/footer-ai/chatgpt.svg",
    createHref: (prompt: string) => `https://chat.openai.com/?prompt=${prompt}`,
  },
  {
    name: "Claude",
    icon: "/images/footer-ai/claude.svg",
    createHref: (prompt: string) => `https://claude.ai/?prompt=${prompt}`,
  },
  {
    name: "Gemini",
    icon: "/images/footer-ai/gemini.svg",
    createHref: (_prompt: string) => "https://gemini.google.com/",
  },
  {
    name: "Grok",
    icon: "/images/footer-ai/grok.svg",
    createHref: (prompt: string) => `https://grok.com/?q=${prompt}`,
  },
  {
    name: "Perplexity",
    icon: "/images/footer-ai/perplexity.svg",
    createHref: (prompt: string) =>
      `https://www.perplexity.ai/search/new?q=${prompt}`,
  },
] as const;

export function FooterAiLinks() {
  const locale = useLandingLocale();
  const isEnglish = locale === "en";
  const question = prompts[isEnglish ? "en" : "zh-CN"];
  const prompt = encodeURIComponent(question);
  const [copyResult, setCopyResult] = useState<{
    locale: typeof locale;
    status: "copied" | "manual";
  } | null>(null);
  const notice = copyResult?.locale === locale ? copyResult.status : null;
  const geminiLabel = isEnglish
    ? "Copy prompt and open Gemini"
    : "复制问题并打开 Gemini";

  function copyGeminiQuestion() {
    // Start copying during the click gesture, before the link opens the new tab.
    // Gemini's Chrome shortcut uses a browser-only header, not a public URL query.
    if (!navigator.clipboard) {
      setCopyResult({ locale, status: "manual" });
      return;
    }
    void navigator.clipboard.writeText(question).then(
      () => setCopyResult({ locale, status: "copied" }),
      () => setCopyResult({ locale, status: "manual" }),
    );
  }

  return (
    <>
      <div className="footer-ai-links">
        {providers.map(({ name, icon, createHref }) => (
          <a
            key={name}
            href={createHref(prompt)}
            target="_blank"
            rel="noopener noreferrer"
            onClick={name === "Gemini" ? copyGeminiQuestion : undefined}
            aria-label={
              name === "Gemini"
                ? geminiLabel
                : isEnglish
                  ? `Ask ${name} about OneCrew`
                  : `在 ${name} 中了解 OneCrew`
            }
            title={name === "Gemini" ? geminiLabel : name}
          >
            <img src={icon} alt="" width={31} height={31} />
          </a>
        ))}
      </div>
      {notice && (
        <div className="footer-ai-notice" role="status">
          <p>
            {notice === "copied"
              ? isEnglish
                ? "Prompt copied. Paste it in Gemini."
                : "问题已复制，请在 Gemini 中粘贴。"
              : isEnglish
                ? "Copy the question below and paste it in Gemini."
                : "请复制下方问题，并在 Gemini 中粘贴。"}
          </p>
          {notice === "manual" && (
            <textarea
              aria-label={
                isEnglish ? "Question for Gemini" : "用于 Gemini 的问题"
              }
              readOnly
              rows={5}
              value={question}
              onFocus={(event) => event.currentTarget.select()}
            />
          )}
        </div>
      )}
    </>
  );
}
