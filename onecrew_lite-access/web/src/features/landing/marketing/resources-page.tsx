import { Breadcrumbs, CTA, SiteFooter, SiteHeader } from "../site-chrome";
import { LandingLink } from "../landing-link";
import { DocumentMeta } from "./document-meta";
import { breadcrumbList, localizedAbsoluteUrl, schemaGraph } from "./schema";
import { useMarketingContent } from "./use-marketing-content";

export function ResourcesPage() {
  const { content, locale } = useMarketingContent();
  const page = content.resources;
  const breadcrumbs = [{ label: content.common.home, href: "/" }, { label: page.label }];
  const schema = schemaGraph({
    "@type": "CollectionPage",
    name: page.title,
    description: page.description,
    url: localizedAbsoluteUrl("/resources", locale),
    inLanguage: locale === "en" ? "en" : "zh-CN",
  }, breadcrumbList(breadcrumbs, locale));

  return (
    <>
      <DocumentMeta canonicalPath="/resources" description={page.description} schema={schema} title={`${page.label}｜OneCrew`} />
      <SiteHeader />
      <main className="detail-page" id="main-content">
        <Breadcrumbs items={breadcrumbs} />
        <section className="feature-hero"><p className="page-label">{page.label}</p><h1>{page.title}</h1><p>{page.summary}</p></section>
        <section className="resource-list">
          {page.cards.map((card) => (
            <LandingLink href={card.href} key={card.href}>
              <span>{card.label}</span><strong>{card.title}</strong><p>{card.body}</p>
            </LandingLink>
          ))}
        </section>
        <CTA />
      </main>
      <SiteFooter />
    </>
  );
}
