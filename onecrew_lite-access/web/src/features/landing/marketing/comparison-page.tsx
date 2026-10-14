import { useAccessI18n } from "@/lib/access-i18n";

import { Breadcrumbs, SiteFooter, SiteHeader } from "../site-chrome";
import type { Competitor, CompetitorCopy } from "./competitors";
import { competitors } from "./competitors";
import { DocumentMeta } from "./document-meta";
import { LandingLink as Link } from "../landing-link";
import styles from "./marketing-pages.module.css";
import { PageCta } from "./page-cta";
import { breadcrumbList, localizedAbsoluteUrl, schemaGraph } from "./schema";
import type { MarketingLocale } from "./use-marketing-content";
import { useMarketingContent } from "./use-marketing-content";

export function ComparisonPage({
  locale,
  competitor,
  copy,
}: {
  locale: MarketingLocale;
  competitor: Competitor;
  copy: CompetitorCopy;
}) {
  const { t } = useAccessI18n();
  const { content } = useMarketingContent();
  const title = t("competitors.page.title", { competitor: competitor.name });
  const breadcrumbs = [
    { label: content.common.home, href: "/" },
    { label: t("competitors.page.breadcrumb") },
    { label: t("competitors.page.breadcrumbLabel", { competitor: competitor.name }) },
  ];
  const schema = schemaGraph(
      {
        "@type": "WebPage",
        name: title,
        description: copy.summary,
        url: localizedAbsoluteUrl(`/compare/${competitor.slug}`, locale),
        inLanguage: locale === "en" ? "en" : "zh-CN",
        dateModified: "2026-09-05",
      },
      {
        "@type": "FAQPage",
        mainEntity: copy.questions.map((item) => ({
          "@type": "Question",
          name: item.question,
          acceptedAnswer: { "@type": "Answer", text: item.answer },
        })),
      },
      breadcrumbList(breadcrumbs, locale),
  );

  return (
    <>
      <DocumentMeta
        canonicalPath={`/compare/${competitor.slug}`}
        description={copy.summary}
        schema={schema}
        title={t("competitors.page.documentTitle", { competitor: competitor.name })}
      />
      <SiteHeader />
      <main className={styles.page} id="main-content">
        <Breadcrumbs items={breadcrumbs} />
        <section className={`${styles.hero} ${styles.compareHero}`}>
          <p className={styles.eyebrow}>{t("competitors.page.eyebrow")}</p>
          <h1>{title}</h1>
          <p>{copy.summary}</p>
          <div className={styles.heroActions}>
            <PageCta />
            <a href={competitor.officialUrl} target="_blank" rel="noreferrer">
              {t("competitors.page.officialLink")} ↗
            </a>
          </div>
        </section>

        <section className={styles.decisionGrid}>
          <article>
            <span>{competitor.name}</span>
            <h2>{t("competitors.page.chooseCompetitor", { competitor: competitor.name })}</h2>
            <p>{copy.competitorFit}</p>
          </article>
          <article>
            <span>OneCrew</span>
            <h2>{t("competitors.page.chooseOneCrew")}</h2>
            <p>{copy.oneCrewFit}</p>
          </article>
        </section>

        <section className={styles.comparisonSection}>
          <div className={styles.sectionHeading}>
            <p>{t("competitors.page.practicalEyebrow")}</p>
            <h2>{t("competitors.page.practicalTitle")}</h2>
          </div>
          <div className={styles.comparisonTable} role="table" aria-label={title}>
            <div className={styles.tableHeader} role="row">
              <span role="columnheader">{t("competitors.page.decisionPoint")}</span>
              <strong role="columnheader">{competitor.name}</strong>
              <strong role="columnheader">OneCrew</strong>
            </div>
            {copy.rows.map((row) => (
              <div className={styles.tableRow} role="row" key={row.label}>
                <span role="cell">{row.label}</span>
                <p role="cell" data-label={competitor.name}>{row.competitor}</p>
                <p role="cell" data-label="OneCrew">{row.oneCrew}</p>
              </div>
            ))}
          </div>
        </section>

        <section className={styles.sourceNote}>
          <div>
            <p className={styles.eyebrow}>{t("competitors.page.practicalEyebrow")}</p>
            <h2>{t("competitors.page.chooseOneCrew")}</h2>
            <p>{t("这篇文章从产品起点、工作方式和适用团队解释两者差异。", "This article explains the difference through each product’s starting point, operating model, and best-fit team.")}</p>
            <p>{copy.oneCrewFit}</p>
          </div>
        </section>

        <section className={styles.sourceNote}>
          <div>
            <p className={styles.eyebrow}>{t("competitors.page.verified")}</p>
            <h2>{copy.officialSource}</h2>
            <p>{t("competitors.page.updated")}</p>
          </div>
          <a href={competitor.officialUrl} target="_blank" rel="noreferrer">
            {t("competitors.page.officialLink")} ↗
          </a>
        </section>

        <section className={styles.faq}>
          <div>
            <p className={styles.eyebrow}>{content.common.faq}</p>
            <h2>{t("competitors.page.faqTitle")}</h2>
          </div>
          <div>
            {copy.questions.map((item, index) => (
              <details key={item.question} open={index === 0}>
                <summary>{item.question}</summary>
                <p>{item.answer}</p>
              </details>
            ))}
          </div>
        </section>

        <section className={styles.moreComparisons}>
          <h2>{t("competitors.page.moreComparisons")}</h2>
          <div>
            {competitors
              .filter(({ slug }) => slug !== competitor.slug)
              .map(({ slug, name }) => (
                <Link key={slug} href={`/compare/${slug}`}>
                  {t("competitors.page.comparisonLink", { competitor: name })}
                </Link>
              ))}
          </div>
        </section>
      </main>
      <SiteFooter />
    </>
  );
}
