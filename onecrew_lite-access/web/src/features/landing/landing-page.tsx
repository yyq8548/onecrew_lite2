import { HeroHeadline } from "./hero-headline";
import { LandingLink } from "./landing-link";
import { HeroPromptDemo } from "./hero-prompt-demo";
import { HomeFaq } from "./home-faq";
import { useAccessLoginUrl } from "./access-site";
import { localizedAbsoluteUrl, oneCrewOrganization, schemaGraph } from "./marketing/schema";
import {
  AudienceSection,
  ChannelCoverage,
  FeatureShowcase,
  OutreachComparison,
  WorkflowSteps,
} from "./home-sections";
import {
  LandingLanguageProvider,
  LangText,
  useLandingLocale,
} from "./language-switcher";
import { CTA, SiteFooter, SiteHeader } from "./site-chrome";

function LandingContent() {
  const ACCESS_LOGIN_URL = useAccessLoginUrl();
  const locale = useLandingLocale();
  const isEnglish = locale === "en";
  const schema = schemaGraph(oneCrewOrganization, {
    "@type": "SoftwareApplication",
    applicationCategory: "BusinessApplication",
    description: isEnglish
      ? "OneCrew connects creator data, brand email, supported social channels, and outreach workflows to the AI services teams already use."
      : "OneCrew 是面向出海品牌的 AI 达人营销工作台，帮助团队搜索和筛选 KOL、集中管理邮件与社交私信，并用 AI 检查回复、生成跟进建议和推进达人合作。",
    inLanguage: isEnglish ? "en" : "zh-CN",
    name: "OneCrew",
    operatingSystem: "Desktop",
    url: localizedAbsoluteUrl("/", locale),
  });

  return (
    <div className="landing-page">
      <main id="main-content" tabIndex={-1}>
        <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(schema) }} />
        <section className="home-hero-new">
          <SiteHeader />
          <div className="hero-new-main">
            <div className="hero-new-copy">
              <HeroHeadline locale={locale} />
              <div className="hero-new-support">
                <p>
                  <LangText
                    zh="OneCrew 是面向出海品牌的 AI 达人发现与外联工具。它将达人数据、品牌邮箱、社交渠道和工作面板接入你常用的 AI，让你用自然语言完成达人搜索、筛选、外联和持续跟进。"
                    en="OneCrew is an AI-native creator outreach workspace that connects creator data, brand email, and supported social channels to the AI you already use—so you can find, reach, and follow up with creators through conversation."
                  />
                </p>
                <div className="hero-new-actions">
                  <a className="button" href={ACCESS_LOGIN_URL}>
                    <LangText zh="立即体验" en="Try OneCrew" /> <span aria-hidden="true">→</span>
                  </a>
                  <LandingLink className="text-link" href="/resources/user-guide"><LangText zh="阅读使用指南" en="Read the user guide" /> <span aria-hidden="true">→</span></LandingLink>
                </div>
              </div>
            </div>
          </div>
          <HeroPromptDemo />
        </section>
        <OutreachComparison locale={locale} />
        <WorkflowSteps locale={locale} />
        <ChannelCoverage locale={locale} />
        <FeatureShowcase locale={locale} />
        <AudienceSection locale={locale} />
        <HomeFaq locale={locale} />
        <CTA />
      </main>
      <SiteFooter />
    </div>
  );
}

export function LandingPage() {
  return (
    <LandingLanguageProvider>
      <LandingContent />
    </LandingLanguageProvider>
  );
}
