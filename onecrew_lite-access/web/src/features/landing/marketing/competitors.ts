export const competitorSlugs = [
  "tiktok-shop-affiliate",
  "lev8",
  "modash",
  "hypeauditor",
  "upfluence",
  "saral",
  "influencity",
  "aspire",
] as const;

export type CompetitorSlug = (typeof competitorSlugs)[number];

export type Competitor = {
  slug: CompetitorSlug;
  name: string;
  officialUrl: string;
};

export type ComparisonRow = {
  label: string;
  competitor: string;
  oneCrew: string;
};

export type ComparisonQuestion = {
  question: string;
  answer: string;
};

export type CompetitorCopy = {
  officialSource: string;
  summary: string;
  competitorFit: string;
  oneCrewFit: string;
  rows: ComparisonRow[];
  questions: ComparisonQuestion[];
};

export type ComparisonLocaleContent = {
  page: {
    breadcrumb: string;
    breadcrumbLabel: string;
    eyebrow: string;
    title: string;
    documentTitle: string;
    officialLink: string;
    chooseCompetitor: string;
    chooseOneCrew: string;
    practicalEyebrow: string;
    practicalTitle: string;
    decisionPoint: string;
    verified: string;
    updated: string;
    faqTitle: string;
    moreComparisons: string;
    comparisonLink: string;
  };
  entries: Record<CompetitorSlug, CompetitorCopy>;
};

export const competitors = [
  { slug: "tiktok-shop-affiliate", name: "TikTok Shop Affiliate", officialUrl: "https://seller-us.tiktok.com/" },
  { slug: "lev8", name: "Lev8", officialUrl: "https://lev8.com/" },
  { slug: "modash", name: "Modash", officialUrl: "https://www.modash.io/" },
  { slug: "hypeauditor", name: "HypeAuditor", officialUrl: "https://hypeauditor.com/" },
  { slug: "upfluence", name: "Upfluence", officialUrl: "https://www.upfluence.com/" },
  { slug: "saral", name: "SARAL", officialUrl: "https://www.getsaral.com/" },
  { slug: "influencity", name: "Influencity", officialUrl: "https://influencity.com/platform/" },
  { slug: "aspire", name: "Aspire", officialUrl: "https://www.aspire.io/platform-overview" },
] as const satisfies readonly Competitor[];

export function getCompetitor(slug: string): Competitor | undefined {
  return competitors.find((competitor) => competitor.slug === slug);
}
