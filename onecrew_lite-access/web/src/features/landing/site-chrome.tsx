import { PUBLIC_CONTACT } from "@/lib/public-contact";
import { PUBLIC_BLOG_URL } from "@/lib/public-site";
import { DesktopNav } from "./desktop-nav";
import { LandingLink as Link } from "./landing-link";
import { LanguageSwitcher, LangText, useLandingLocale } from "./language-switcher";
import { useAccessLoginUrl } from "./access-site";
import { FooterAiLinks } from "./footer-ai-links";

export function Logo() {
  return (
    <Link className="brand" href="/" aria-label="OneCrew">
      <img className="header-logo" src="/onecrew-logo-dark.svg" alt="OneCrew" />
    </Link>
  );
}

export function SiteHeader() {
  const ACCESS_LOGIN_URL = useAccessLoginUrl();
  return (
    <header className="site-header">
      <a
        className="sr-only focus:not-sr-only focus:fixed focus:left-4 focus:top-4 focus:z-[1200] focus:rounded-lg focus:bg-white focus:px-4 focus:py-2 focus:text-black focus:shadow-lg"
        href="#main-content"
      >
        <LangText zh="跳到主要内容" en="Skip to main content" />
      </a>

      <div className="site-header-inner">
        <Logo />
        <DesktopNav />

        <div className="header-actions">
          <LanguageSwitcher />
          <a className="button compact" href={ACCESS_LOGIN_URL}>
            <LangText zh="立即体验" en="Try OneCrew" />
          </a>
        </div>

        <details className="mobile-menu">
          <summary><LangText zh="菜单" en="Menu" /></summary>
          <div>
            <strong><LangText zh="能力" en="Capabilities" /></strong>
            <Link href="/product/creator-discovery">
              <LangText zh="达人发现" en="Creator discovery" />
            </Link>
            <Link href="/product/unified-inbox">
              <LangText zh="统一收件箱" en="Unified inbox" />
            </Link>
            <Link href="/product/ai-agent">AI Agent</Link>
            <Link href="/resources/user-guide"><LangText zh="使用指南" en="User guide" /></Link>
            <a href={PUBLIC_BLOG_URL}><LangText zh="博客" en="Blog" /></a>
            <Link href="/pricing"><LangText zh="定价" en="Pricing" /></Link>
            <Link href="/about"><LangText zh="关于" en="About" /></Link>
            <a href={ACCESS_LOGIN_URL}><LangText zh="立即体验" en="Try OneCrew" /></a>
          </div>
        </details>
      </div>
    </header>
  );
}

export function SiteFooter() {
  const locale = useLandingLocale();
  return (
    <div className="site-footer-shell">
      <footer className="site-footer">
        <div className="footer-top">
          <div className="footer-about">
            <span className="footer-logo" role="img" aria-label="OneCrew" />
            <div className="footer-ai">
              <h2>
                <LangText zh="在 AI 中了解我们" en="Learn about us with AI" />
              </h2>
              <FooterAiLinks />
            </div>
          </div>
          <div className="footer-column">
            <h2>
              <LangText zh="了解 OneCrew" en="Explore OneCrew" />
            </h2>
            <Link href="/product">
              <LangText zh="产品" en="Product" />
            </Link>
            <Link href="/use-cases">
              <LangText zh="适用团队" en="Use cases" />
            </Link>
            <Link href="/resources/user-guide">
              <LangText zh="使用指南" en="User guide" />
            </Link>
            <a href={PUBLIC_BLOG_URL}>
              <LangText zh="博客" en="Blog" />
            </a>
            <Link href="/about">
              <LangText zh="关于我们" en="About" />
            </Link>
            <Link href="/pricing">
              <LangText zh="定价" en="Pricing" />
            </Link>
          </div>
          <div className="footer-column">
            <h2>
              <LangText zh="核心能力" en="Capabilities" />
            </h2>
            <Link href="/product/creator-discovery">
              <LangText zh="达人发现" en="Creator discovery" />
            </Link>
            <Link href="/product/unified-inbox">
              <LangText zh="统一收件箱" en="Unified inbox" />
            </Link>
            <Link href="/product/ai-agent">AI Agent</Link>
          </div>
          <div className="footer-column footer-compare">
            <h2>
              <LangText zh="对比" en="Compare" />
            </h2>{" "}
            <Link href="/compare/tiktok-shop-affiliate">
              OneCrew vs. TikTok Shop Affiliate
            </Link>
            <Link href="/compare/lev8">OneCrew vs. Lev8</Link>
            <Link href="/compare/modash">OneCrew vs. Modash</Link>
            <Link href="/compare/hypeauditor">OneCrew vs. HypeAuditor</Link>
            <Link href="/compare/upfluence">OneCrew vs. Upfluence</Link>
            <Link href="/compare/saral">OneCrew vs. SARAL</Link>
            <Link href="/compare/influencity">OneCrew vs. Influencity</Link>
            <Link href="/compare/aspire">OneCrew vs. Aspire</Link>
          </div>
          <div className="footer-column footer-legal">
            <h2>
              <LangText zh="法律" en="Legal" />
            </h2>
            <Link href="/terms">
              <LangText zh="服务条款" en="Terms" />
            </Link>
            <Link href="/privacy">
              <LangText zh="隐私政策" en="Privacy" />
            </Link>
            <div className="footer-contact">
              <h3>
                <LangText zh="联系我们" en="Contact us" />
              </h3>
              <div>
                <a
                  className="footer-social-link"
                  href={PUBLIC_CONTACT.x}
                  target="_blank"
                  rel="noopener noreferrer"
                  aria-label={
                    locale === "en" ? "OneCrew on X" : "OneCrew 官方 X"
                  }
                  title="X"
                >
                  <svg viewBox="0 0 24 24" aria-hidden="true">
                    <path d="M18.9 2H22l-6.8 7.8L23.2 22h-6.3L12 15.6 6.4 22H3.2l7.3-8.4L2.8 2h6.5l4.4 5.8L18.9 2Zm-1.1 17.8h1.7L8.4 4.1H6.6l11.2 15.7Z" />
                  </svg>
                </a>
                <a
                  className="footer-social-link"
                  href={`mailto:${PUBLIC_CONTACT.email}`}
                  aria-label={
                    locale === "en" ? "Email OneCrew" : "邮件联系 OneCrew"
                  }
                  title={PUBLIC_CONTACT.email}
                >
                  <svg viewBox="0 0 24 24" aria-hidden="true">
                    <path d="M3 6.5h18v11H3zM4 7.5l8 6 8-6" />
                  </svg>
                </a>
              </div>
            </div>
          </div>
        </div>
        <div className="footer-wordmark" aria-hidden="true">
          ONECREW
        </div>
        <div className="footer-bottom">
          <p>© 2026 OneCrew. All rights reserved.</p>
        </div>
      </footer>
    </div>
  );
}

export function Breadcrumbs({
  items,
}: {
  items: Array<{ label: string; href?: string }>;
}) {
  const locale = useLandingLocale();

  return (
    <nav className="breadcrumbs" aria-label={locale === "en" ? "Breadcrumb" : "面包屑"}>
      {items.map((item, index) => (
        <span key={item.label}>
          {index > 0 && <b aria-hidden="true">/</b>}
          {item.href ? (
            <Link href={item.href}>{item.label}</Link>
          ) : (
            <span aria-current="page">{item.label}</span>
          )}
        </span>
      ))}
    </nav>
  );
}

export function CTA() {
  const ACCESS_LOGIN_URL = useAccessLoginUrl();
  return (
    <section className="final-cta">
      <span className="final-cta-eyebrow">
        <LangText
          zh="从一次真实的外联任务开始"
          en="Start with a real outreach task"
        />
      </span>
      <div>
        <h2>
          <span><LangText zh="让下一次达人合作，" en="Find the right creators." /></span>
          <span><LangText zh="始终有清晰的下一步" en="Turn outreach into partnerships." /></span>
        </h2>
        <p>
          <LangText
            zh="告诉我们你的团队如何做达人营销，看看 OneCrew 是否适合你的工作流。"
            en="Give your AI the context and tools to move creator relationships forward."
          />
        </p>
      </div>
      <a className="button acid" href={ACCESS_LOGIN_URL}>
        <LangText zh="立即体验" en="Try OneCrew" /> <span aria-hidden="true">→</span>
      </a>
    </section>
  );
}
