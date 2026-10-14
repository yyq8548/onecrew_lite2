import { Breadcrumbs, SiteFooter, SiteHeader } from "../site-chrome";
import { LandingLink } from "../landing-link";
import { DocumentMeta } from "./document-meta";
import {
  breadcrumbList,
  localizedAbsoluteUrl,
  oneCrewOrganization,
  schemaGraph,
} from "./schema";
import { useMarketingContent } from "./use-marketing-content";

export function GuidePage() {
  const { content, locale } = useMarketingContent();
  const page = content.guide;
  const breadcrumbs = [
    { label: content.common.home, href: "/" },
    { label: content.common.resources, href: "/resources" },
    { label: page.title },
  ];
  const schema = schemaGraph({
    "@type": "Article",
    headline: page.title,
    description: page.description,
    datePublished: "2026-08-26",
    dateModified: "2026-09-05",
    inLanguage: locale === "en" ? "en" : "zh-CN",
    url: localizedAbsoluteUrl("/resources/kol-outreach-guide", locale),
    author: oneCrewOrganization,
    publisher: oneCrewOrganization,
  }, breadcrumbList(breadcrumbs, locale));

  return (
    <>
      <DocumentMeta canonicalPath="/resources/kol-outreach-guide" description={page.description} schema={schema} title={`${page.title}｜OneCrew`} />
      <SiteHeader />
      <main className="article-page" id="main-content">
        <Breadcrumbs items={breadcrumbs} />
        <article>
          <header><p className="page-label">{page.label}</p><h1>{page.title}</h1><p>{page.intro}</p><p>{page.authorLabel} {page.author} · {page.published} · {page.modified}</p></header>
          <nav className="toc" aria-label={page.toc}>
            <strong>{page.toc}</strong>
            {page.steps.map((step, index) => <a href={`#step-${index + 1}`} key={step.term}>{index + 1}. {step.term}</a>)}
          </nav>
          {page.steps.map((step, index) => (
            <section id={`step-${index + 1}`} key={step.term}>
              <h2>{index + 1}. {step.term}</h2>
              <p>{step.detail}</p>
              {index === 3 ? <div className="example"><strong>{page.exampleTitle}</strong><ul>{page.exampleItems.map((item) => <li key={item}>{item}</li>)}</ul></div> : null}
            </section>
          ))}
          <section><h2>{page.finalTitle}</h2><p>{page.finalBody}</p><p><LandingLink href="/product">{page.finalLink}</LandingLink></p></section>
        </article>
      </main>
      <SiteFooter />
    </>
  );
}
