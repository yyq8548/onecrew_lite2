import { Breadcrumbs, SiteFooter, SiteHeader } from "../site-chrome";
import type { LegalSlug } from "./content/types";
import { DocumentMeta } from "./document-meta";
import { breadcrumbList, localizedAbsoluteUrl, schemaGraph } from "./schema";
import { useMarketingContent } from "./use-marketing-content";

export function isLegalSlug(value: string | undefined): value is LegalSlug {
  return value === "terms" || value === "privacy";
}

export function LegalPage({ slug }: { slug: LegalSlug }) {
  const { content, locale } = useMarketingContent();
  const page = content.legal[slug];
  const currentPath = `/${slug}`;
  const breadcrumbs = [{ label: content.common.home, href: "/" }, { label: page.title }];
  const schema = schemaGraph({
    "@type": "WebPage",
    name: `${page.title}｜OneCrew`,
    description: page.intro,
    dateModified: "2026-08-28",
    inLanguage: locale === "en" ? "en" : "zh-CN",
    url: localizedAbsoluteUrl(currentPath, locale),
  }, breadcrumbList(breadcrumbs, locale));

  return (
    <>
      <DocumentMeta canonicalPath={currentPath} description={page.description} schema={schema} title={`${page.title}｜OneCrew`} />
      <SiteHeader />
      <main className="legal-page" id="main-content">
        <Breadcrumbs items={breadcrumbs} />
        <article>
          <header className="legal-header"><h1>{page.title}</h1><p>{page.intro}</p><small>{page.updated}</small></header>
          <div className="legal-content">
            {page.sections.map((section, index) => (
              <section id={`section-${index + 1}`} key={section.title}>
                <h2>{index + 1}. {section.title}</h2>
                {section.paragraphs?.map((paragraph) => <p key={paragraph}>{paragraph}</p>)}
                {section.bullets ? <ul>{section.bullets.map((item) => <li key={item}>{item}</li>)}</ul> : null}
              </section>
            ))}
          </div>
        </article>
      </main>
      <SiteFooter />
    </>
  );
}
