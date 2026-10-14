import { Breadcrumbs, SiteFooter, SiteHeader } from "../site-chrome";
import { DocumentMeta } from "./document-meta";
import styles from "./marketing-pages.module.css";
import { PageCta } from "./page-cta";
import { breadcrumbList, localizedAbsoluteUrl, schemaGraph } from "./schema";
import { useMarketingContent } from "./use-marketing-content";

export function PricingPage() {
  const { content, locale } = useMarketingContent();
  const page = content.pricing;
  const breadcrumbs = [
    { label: content.common.home, href: "/" },
    { label: page.label },
  ];
  const schema = schemaGraph({
    "@type": "WebPage",
    name: page.label,
    description: page.description,
    url: localizedAbsoluteUrl("/pricing", locale),
    inLanguage: locale === "en" ? "en" : "zh-CN",
  }, breadcrumbList(breadcrumbs, locale));

  return (
    <>
      <DocumentMeta
        canonicalPath="/pricing"
        description={page.description}
        schema={schema}
        title={page.documentTitle}
      />
      <SiteHeader />
      <main className={styles.page} id="main-content">
        <Breadcrumbs items={breadcrumbs} />
        <section className={styles.hero}>
          <p className={styles.eyebrow}>{page.eyebrow}</p>
          <h1>{page.title}</h1>
          <p>{page.description}</p>
          <PageCta />
        </section>

        <section className={styles.pricingLead}>
          <div>
            <span>{page.access.eyebrow}</span>
            <h2>{page.access.title}</h2>
          </div>
          <div>
            <strong>{page.access.subtitle}</strong>
            <p>{page.access.body}</p>
          </div>
        </section>

        <section className={styles.cardSection}>
          <div className={styles.sectionHeading}>
            <p>{page.included.eyebrow}</p>
            <h2>{page.included.title}</h2>
          </div>
          <div className={styles.threeCards}>
            {page.included.items.map((item, index) => (
              <article key={item.term}>
                <span>{String(index + 1).padStart(2, "0")}</span>
                <h3>{item.term}</h3>
                <p>{item.detail}</p>
              </article>
            ))}
          </div>
        </section>

        <section className={styles.priceFactors}>
          <div>
            <p className={styles.eyebrow}>{page.factors.eyebrow}</p>
            <h2>{page.factors.title}</h2>
          </div>
          <dl>
            {page.factors.items.map((item) => (
              <div key={item.term}>
                <dt>{item.term}</dt>
                <dd>{item.detail}</dd>
              </div>
            ))}
          </dl>
        </section>

        <section className={styles.faq}>
          <div>
            <p className={styles.eyebrow}>{page.faq.eyebrow}</p>
            <h2>{page.faq.title}</h2>
          </div>
          <div>
            {page.faq.questions.map((item, index) => (
              <details key={item.question} open={index === 0}>
                <summary>{item.question}</summary>
                <p>{item.answer}</p>
              </details>
            ))}
          </div>
        </section>
      </main>
      <SiteFooter />
    </>
  );
}
