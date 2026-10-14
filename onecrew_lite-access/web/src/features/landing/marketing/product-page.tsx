import { Breadcrumbs, SiteFooter, SiteHeader } from "../site-chrome";
import { LandingLink } from "../landing-link";
import { DocumentMeta } from "./document-meta";
import { ProductFactTable, ProductPreview } from "./product-preview";
import { breadcrumbList, localizedAbsoluteUrl, schemaGraph } from "./schema";
import { useMarketingContent } from "./use-marketing-content";

export function ProductPage() {
  const { content, locale } = useMarketingContent();
  const page = content.product;
  const breadcrumbs = [{ label: content.common.home, href: "/" }, { label: content.common.product }];
  const schema = schemaGraph(
      { "@type": "WebPage", name: page.title, description: page.description, url: localizedAbsoluteUrl("/product", locale), inLanguage: locale === "en" ? "en" : "zh-CN" },
      breadcrumbList(breadcrumbs, locale),
  );

  return (
    <>
      <DocumentMeta canonicalPath="/product" description={page.description} schema={schema} title={page.documentTitle} />
      <SiteHeader />
      <main className="detail-page" id="main-content">
        <Breadcrumbs items={breadcrumbs} />
        <section className="detail-hero">
          <div><h1>{page.title}</h1><p>{page.summary}</p></div>
          <ProductFactTable facts={page.facts} />
        </section>
        <ProductPreview copy={page.preview} />
        <section className="detail-sections">
          {page.sections.map((section) => (
            <article key={section.href}>
              <h2>{section.title}</h2>
              <p>{section.body}</p>
              <LandingLink href={section.href}>{section.link}</LandingLink>
            </article>
          ))}
        </section>
      </main>
      <SiteFooter />
    </>
  );
}
