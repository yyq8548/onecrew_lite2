import { Breadcrumbs, CTA, SiteFooter, SiteHeader } from "../site-chrome";
import { LandingLink } from "../landing-link";
import type { UseCaseSlug } from "./content/types";
import { DocumentMeta } from "./document-meta";
import { breadcrumbList, localizedAbsoluteUrl, schemaGraph } from "./schema";
import { useMarketingContent } from "./use-marketing-content";

const useCaseSlugs = ["ecommerce-brands", "solo-founders", "agencies"] as const;

export function isUseCaseSlug(value: string | undefined): value is UseCaseSlug {
  return useCaseSlugs.includes(value as UseCaseSlug);
}

export function UseCasePage({ slug }: { slug: UseCaseSlug }) {
  const { content, locale } = useMarketingContent();
  const page = content.useCasePages[slug];
  const detail = content.useCases.detail;
  const currentPath = `/use-cases/${slug}`;
  const breadcrumbs = [
    { label: content.common.home, href: "/" },
    { label: content.common.solutions, href: "/use-cases" },
    { label: page.label },
  ];
  const schema = schemaGraph(
      { "@type": "WebPage", name: page.title, description: page.summary, url: localizedAbsoluteUrl(currentPath, locale), inLanguage: locale === "en" ? "en" : "zh-CN" },
      breadcrumbList(breadcrumbs, locale),
  );

  return (
    <>
      <DocumentMeta canonicalPath={currentPath} description={page.description} schema={schema} title={`${page.label}｜OneCrew`} />
      <SiteHeader />
      <main className="detail-page" id="main-content">
        <Breadcrumbs items={breadcrumbs} />
        <section className="feature-hero"><p className="page-label">{page.label}</p><h1>{page.title}</h1><p>{page.summary}</p></section>
        <section className="solution-section">
          <header><h2>{detail.problemTitle}</h2><p>{detail.problemBody}</p></header>
          <div>{page.problems.map((item) => <article key={item.term}><h3>{item.term}</h3><p>{item.detail}</p></article>)}</div>
        </section>
        <section className="how-section">
          <div><h2>{detail.workflowTitle}</h2><p>{detail.workflowBody}</p></div>
          <ol>{page.workflow.map((item, index) => <li key={item.term}><span>{index + 1}</span><div><h3>{item.term}</h3><p>{item.detail}</p></div></li>)}</ol>
        </section>
        <section className="facts-grid">{page.fit.map((item) => <article key={item.term}><h2>{item.term}</h2><p>{item.detail}</p></article>)}</section>
        <div className="related">
          <h2>{content.common.related}</h2>
          <div>
            <LandingLink href="/product">{content.common.product}</LandingLink>
            <LandingLink href="/product/creator-discovery">{content.featurePages["creator-discovery"].label}</LandingLink>
            <LandingLink href="/product/ai-agent">{content.featurePages["ai-agent"].label}</LandingLink>
            <LandingLink href="/resources/kol-outreach-guide">{content.guide.title}</LandingLink>
          </div>
        </div>
        <CTA />
      </main>
      <SiteFooter />
    </>
  );
}
