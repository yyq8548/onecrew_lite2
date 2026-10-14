import { Breadcrumbs, SiteFooter, SiteHeader } from "../site-chrome";
import { LandingLink } from "../landing-link";
import type { FeatureSlug } from "./content/types";
import { DocumentMeta } from "./document-meta";
import { breadcrumbList, localizedAbsoluteUrl, schemaGraph } from "./schema";
import { useMarketingContent } from "./use-marketing-content";

const featureSlugs = ["creator-discovery", "unified-inbox", "ai-agent"] as const;

export function isFeatureSlug(value: string | undefined): value is FeatureSlug {
  return featureSlugs.includes(value as FeatureSlug);
}

export function FeaturePage({ slug }: { slug: FeatureSlug }) {
  const { content, locale } = useMarketingContent();
  const page = content.featurePages[slug];
  const currentPath = `/product/${slug}`;
  const breadcrumbs = [
    { label: content.common.home, href: "/" },
    { label: content.common.product, href: "/product" },
    { label: page.label },
  ];
  const schema = schemaGraph(
      { "@type": "WebPage", name: page.title, description: page.summary, url: localizedAbsoluteUrl(currentPath, locale), inLanguage: locale === "en" ? "en" : "zh-CN" },
      breadcrumbList(breadcrumbs, locale),
      {
        "@type": "FAQPage",
        mainEntity: page.questions.map((item) => ({
          "@type": "Question",
          name: item.question,
          acceptedAnswer: { "@type": "Answer", text: item.answer },
        })),
      },
  );

  return (
    <>
      <DocumentMeta canonicalPath={currentPath} description={page.description} schema={schema} title={`${page.label}｜OneCrew`} />
      <SiteHeader />
      <main className="detail-page" id="main-content">
        <Breadcrumbs items={breadcrumbs} />
        <section className="feature-hero"><p className="page-label">{page.label}</p><h1>{page.title}</h1><p>{page.summary}</p></section>
        <section className="facts-grid">
          {page.facts.map((item) => <article key={item.term}><h2>{item.term}</h2><p>{item.detail}</p></article>)}
        </section>
        <section className="how-section">
          <div><h2>{content.common.howItWorks}</h2></div>
          <ol>{page.steps.map((item, index) => <li key={item.term}><span>{index + 1}</span><div><h3>{item.term}</h3><p>{item.detail}</p></div></li>)}</ol>
        </section>
        <section className="faq-section"><h2>{content.common.faq}</h2>{page.questions.map((item) => <details key={item.question}><summary>{item.question}</summary><p>{item.answer}</p></details>)}</section>
        <div className="related">
          <h2>{content.common.related}</h2>
          <div>
            {featureSlugs.map((relatedSlug) => <LandingLink href={`/product/${relatedSlug}`} key={relatedSlug}>{content.featurePages[relatedSlug].label}</LandingLink>)}
            <LandingLink href="/resources/kol-outreach-guide">{content.guide.title}</LandingLink>
          </div>
        </div>
      </main>
      <SiteFooter />
    </>
  );
}
