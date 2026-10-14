import { SiteFooter, SiteHeader } from "../site-chrome";
import { LandingLink } from "../landing-link";
import { DocumentMeta } from "./document-meta";
import { useMarketingContent } from "./use-marketing-content";

export function NotFoundPage() {
  const { content } = useMarketingContent();
  const page = content.notFound;

  return (
    <>
      <DocumentMeta canonicalPath="/" description={page.description} noIndex title={`${page.title}｜OneCrew`} />
      <SiteHeader />
      <main className="detail-page" id="main-content">
        <section className="feature-hero">
          <p className="page-label">404</p>
          <h1>{page.title}</h1>
          <p>{page.description}</p>
          <LandingLink className="button" href="/">{page.back}</LandingLink>
        </section>
      </main>
      <SiteFooter />
    </>
  );
}
