import { Route, Routes, useParams, type RoutesProps } from "react-router";

import { LandingPage } from "../landing-page";
import { getCompetitor } from "./competitors";
import { DocumentMeta } from "./document-meta";
import { FeaturePage, isFeatureSlug } from "./feature-page";
import { GuidePage } from "./guide-page";
import { UserGuidePage } from "./user-guide-page";
import { LegalPage } from "./legal-page";
import { MarketingShell } from "./marketing-shell";
import { AboutPage, ComparisonPage, PricingPage } from "./marketing-pages";
import { NotFoundPage } from "./not-found-page";
import { ProductPage } from "./product-page";
import { ResourcesPage } from "./resources-page";
import { UseCasePage, isUseCaseSlug } from "./use-case-page";
import { UseCasesPage } from "./use-cases-page";
import { useMarketingContent } from "./use-marketing-content";

export function HomeRoute() {
  const { content } = useMarketingContent();

  return <><DocumentMeta canonicalPath="/" description={content.home.description} title={content.home.documentTitle} /><LandingPage /></>;
}

function AboutRoute() {
  return <AboutPage />;
}

function PricingRoute() {
  return <PricingPage />;
}

function FeatureRoute() {
  const { feature } = useParams();
  return isFeatureSlug(feature) ? <FeaturePage slug={feature} /> : <NotFoundPage />;
}

function UseCaseRoute() {
  const { useCase } = useParams();
  return isUseCaseSlug(useCase) ? <UseCasePage slug={useCase} /> : <NotFoundPage />;
}

function ComparisonRoute() {
  const { competitor: slug } = useParams();
  const competitor = slug ? getCompetitor(slug) : undefined;
  const { comparisonContent, locale } = useMarketingContent();
  const copy = competitor ? comparisonContent.entries[competitor.slug] : undefined;
  return competitor && copy
    ? <ComparisonPage competitor={competitor} copy={copy} locale={locale} />
    : <NotFoundPage />;
}

export function MarketingRoutes({ location }: { location?: RoutesProps["location"] } = {}) {
  return (
    <Routes location={location}>
        <Route path="/" element={<HomeRoute />} />
        <Route element={<MarketingShell />}>
          <Route path="/about" element={<AboutRoute />} />
          <Route path="/pricing" element={<PricingRoute />} />
          <Route path="/product" element={<ProductPage />} />
          <Route path="/product/:feature" element={<FeatureRoute />} />
          <Route path="/use-cases" element={<UseCasesPage />} />
          <Route path="/use-cases/:useCase" element={<UseCaseRoute />} />
          <Route path="/resources" element={<ResourcesPage />} />
          <Route path="/resources/user-guide" element={<UserGuidePage />} />
          <Route path="/resources/kol-outreach-guide" element={<GuidePage />} />
          <Route path="/compare/:competitor" element={<ComparisonRoute />} />
          <Route path="/terms" element={<LegalPage slug="terms" />} />
          <Route path="/privacy" element={<LegalPage slug="privacy" />} />
          <Route path="*" element={<NotFoundPage />} />
        </Route>
    </Routes>
  );
}
