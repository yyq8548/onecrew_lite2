import { useEffect, useRef } from "react";
import { useAccessLoginUrl } from "../access-site";
import { LandingLink as Link } from "../landing-link";
import { CTA, SiteFooter, SiteHeader } from "../site-chrome";
import { DocumentMeta } from "./document-meta";
import { breadcrumbList, localizedAbsoluteUrl, schemaGraph } from "./schema";
import { useMarketingContent } from "./use-marketing-content";
import styles from "./about-page.module.css";

function ArrowUpRight() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M7 17 17 7M7 7h10v10" />
    </svg>
  );
}

function StarRow() {
  return (
    <div className={styles.stars}>
      <span className={styles.srOnly}>5 / 5</span>
      {Array.from({ length: 5 }, (_, i) => (
        <svg key={i} viewBox="0 0 24 24" aria-hidden="true">
          <path d="m12 2.8 2.75 5.57 6.15.9-4.45 4.33 1.05 6.13L12 16.84l-5.5 2.89 1.05-6.13L3.1 9.27l6.15-.9L12 2.8Z" />
        </svg>
      ))}
    </div>
  );
}

export function AboutPage() {
  const { content, locale } = useMarketingContent();
  const t = content.about;
  const breadcrumbs = [
    { label: content.common.home, href: "/" },
    { label: t.label },
  ];
  const schema = schemaGraph(
    {
      "@type": "AboutPage",
      name: t.label,
      description: t.description,
      url: localizedAbsoluteUrl("/about", locale),
      inLanguage: locale === "en" ? "en" : "zh-CN",
    },
    breadcrumbList(breadcrumbs, locale),
  );
  const rootRef = useRef<HTMLElement>(null);
  const accessHref = useAccessLoginUrl();

  // Translated list keys replace DOM nodes; observe the current content after each locale change.
  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    root.classList.add(styles.enhanced);
    const items = Array.from(
      root.querySelectorAll<HTMLElement>("[data-reveal]"),
    );
    if (!("IntersectionObserver" in window)) {
      items.forEach((item) => (item.dataset.visible = "true"));
      return;
    }
    const observer = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (entry.isIntersecting) {
            (entry.target as HTMLElement).dataset.visible = "true";
            observer.unobserve(entry.target);
          }
        });
      },
      { threshold: 0.12, rootMargin: "0px 0px -8% 0px" },
    );
    items.forEach((item, index) => {
      item.dataset.revealStep = String(index % 4);
      observer.observe(item);
    });
    return () => {
      observer.disconnect();
      root.classList.remove(styles.enhanced);
    };
  }, [locale]);

  return (
    <>
      <DocumentMeta
        canonicalPath="/about"
        description={t.description}
        schema={schema}
        title={t.documentTitle}
      />
      <SiteHeader />
      <main ref={rootRef} id="main-content" className={styles.page}>
        <section className={styles.hero} aria-labelledby="about-title">
          <img
            className={styles.heroImage}
            src="/images/about/hero.png"
            alt={
              locale === "zh"
                ? "创作者在工作室准备内容"
                : "Creators preparing content in a studio"
            }
            width={1672}
            height={941}
            fetchPriority="high"
          />
          <div className={styles.heroShade} aria-hidden="true" />
          <div className={`${styles.wrap} ${styles.heroContent}`}>
            <p className={styles.eyebrow}>{t.eyebrow}</p>
            <h1 id="about-title">
              <span>{t.title[0]}</span>{" "}
              <span>{t.title[1]}</span>
            </h1>
            <p className={styles.heroDescription}>{t.intro}</p>
            <a
              className={`${styles.button} ${styles.yellowButton}`}
              href="#purpose"
            >
              {t.explore}
              <ArrowUpRight />
            </a>
          </div>
          <div className={`${styles.wrap} ${styles.heroFoot}`}>
            <a href="#purpose" aria-label={t.explore}>
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <path d="M12 5v14m7-7-7 7-7-7" />
              </svg>
            </a>
          </div>
        </section>

        <section
          className={`${styles.wrap} ${styles.section} ${styles.purpose}`}
          id="purpose"
        >
          <header className={styles.sectionHeading} data-reveal>
            <h2>{t.purposeTitle}</h2>
            <p>{t.purposeIntro}</p>
          </header>
          <div className={styles.purposeGrid}>
            <figure className={styles.purposeFigure} data-reveal>
              <img
                src="/images/about/collaboration.png"
                alt={
                  locale === "zh"
                    ? "两位创作者一起查看合作简报"
                    : "Two creators reviewing a collaboration brief"
                }
                width={1122}
                height={1402}
                loading="lazy"
              />
            </figure>
            <div className={styles.purposeStatements} data-reveal>
              <article>
                <p className={styles.eyebrow}>{t.visionLabel}</p>
                <h3>{t.visionTitle}</h3>
                <p>{t.visionBody}</p>
              </article>
              <article>
                <p className={styles.eyebrow}>{t.missionLabel}</p>
                <h3>{t.missionTitle}</h3>
                <p>{t.missionBody}</p>
              </article>
            </div>
          </div>
        </section>

        <section
          className={`${styles.facts} ${styles.section}`}
          id="brand-facts"
        >
          <div className={styles.wrap}>
            <header className={styles.sectionHeading} data-reveal>
              <h2>{t.factsTitle}</h2>
              <p>{t.factsIntro}</p>
            </header>
            <div className={styles.factGrid}>
              {t.facts.map(([value, label]) => (
                <div key={label} data-reveal>
                  <strong>{value}</strong>
                  <p>{label}</p>
                </div>
              ))}
            </div>
            <a
              className={`${styles.button} ${styles.darkButton}`}
              href={accessHref}
              data-reveal
            >
              {t.request}
            </a>
          </div>
        </section>

        <section
          className={`${styles.wrap} ${styles.section} ${styles.principles}`}
        >
          <header className={styles.sectionHeading} data-reveal>
            <h2>{t.principlesTitle}</h2>
            <p>{t.principlesIntro}</p>
          </header>
          <div className={styles.principleGrid}>
            {t.principles.map(([title, body], index) => (
              <article key={title} data-reveal>
                <div className={styles.principleIcon} aria-hidden="true">
                  <span>{String(index + 1).padStart(2, "0")}</span>
                </div>
                <h3>{title}</h3>
                <p>{body}</p>
              </article>
            ))}
          </div>
        </section>

        <section className={styles.teams} id="teams">
          <div className={`${styles.wrap} ${styles.section}`}>
            <header className={styles.sectionHeading} data-reveal>
              <h2>{t.teamsTitle}</h2>
              <p>{t.teamsIntro}</p>
            </header>
            <div className={styles.teamList}>
              {t.teams.map(([title, body, href]) => (
                <Link key={title} href={href} data-reveal>
                  <div>
                    <h3>{title}</h3>
                    <p>{body}</p>
                  </div>
                  <ArrowUpRight />
                </Link>
              ))}
            </div>
          </div>
        </section>

        <section
          className={`${styles.section} ${styles.stories}`}
          aria-labelledby="stories-title"
        >
          <div className={styles.wrap}>
            <header className={styles.sectionHeading} data-reveal>
              <h2 id="stories-title">{t.storiesTitle}</h2>
              <p>{t.storiesIntro}</p>
            </header>
          </div>
          <div
            className={styles.marquee}
            tabIndex={0}
            role="region"
            aria-label={t.storiesTitle}
          >
            <div className={styles.marqueeTrack}>
              {[0, 1].map((set) => (
                <div
                  key={set}
                  className={styles.storyStrip}
                  aria-hidden={set === 1 ? "true" : undefined}
                >
                  {t.stories.map(([quote, role, company, avatar]) => (
                    <figure key={`${set}-${role}`}>
                      <StarRow />
                      <blockquote>
                        <p>“{quote}”</p>
                      </blockquote>
                      <figcaption>
                        <img
                          src={avatar}
                          alt=""
                          width={48}
                          height={48}
                          loading="lazy"
                        />
                        <span>
                          <strong>{role}</strong>
                          <small>{company}</small>
                        </span>
                      </figcaption>
                    </figure>
                  ))}
                </div>
              ))}
            </div>
          </div>
        </section>

        <CTA />
      </main>
      <SiteFooter />
    </>
  );
}
