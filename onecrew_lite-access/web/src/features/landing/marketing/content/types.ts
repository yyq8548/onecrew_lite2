export type Fact = {
  detail: string;
  term: string;
};

export type Question = {
  answer: string;
  question: string;
};

export type FeatureSlug = "ai-agent" | "creator-discovery" | "unified-inbox";

export type UseCaseSlug = "agencies" | "ecommerce-brands" | "solo-founders";

export type LegalSlug = "privacy" | "terms";

export type FeaturePageContent = {
  description: string;
  facts: Fact[];
  label: string;
  questions: Question[];
  steps: Fact[];
  summary: string;
  title: string;
};

export type UseCasePageContent = {
  description: string;
  fit: Fact[];
  label: string;
  problems: Fact[];
  summary: string;
  title: string;
  workflow: Fact[];
};

export type LegalSection = {
  bullets?: string[];
  paragraphs?: string[];
  title: string;
};

export type ProductPreviewContent = {
  approval: string;
  approve: string;
  aria: string;
  campaign: string;
  candidates: string;
  channels: string;
  checked: string;
  creatorMeta: Record<"ava" | "jonas", string>;
  draftAction: string;
  drafted: string;
  incoming: string;
  intent: string;
  intentBody: string;
  interested: string;
  matches: string;
  next: string;
  outgoing: string;
  reason: string;
  replied: string;
  steps: [string, string, string];
  why: string;
};

export type MarketingContent = {
  common: {
    faq: string;
    home: string;
    howItWorks: string;
    product: string;
    related: string;
    resources: string;
    solutions: string;
    tryOneCrew: string;
  };
  home: {
    description: string;
    documentTitle: string;
  };
  about: {
    description: string;
    documentTitle: string;
    label: string;
    eyebrow: string;
    title: [string, string];
    intro: string;
    explore: string;
    purposeTitle: string;
    purposeIntro: string;
    visionLabel: string;
    visionTitle: string;
    visionBody: string;
    missionLabel: string;
    missionTitle: string;
    missionBody: string;
    factsTitle: string;
    factsIntro: string;
    facts: Array<[value: string, label: string]>;
    request: string;
    principlesTitle: string;
    principlesIntro: string;
    principles: Array<[title: string, body: string]>;
    teamsTitle: string;
    teamsIntro: string;
    teams: Array<[title: string, body: string, href: string]>;
    storiesTitle: string;
    storiesIntro: string;
    stories: Array<[quote: string, role: string, company: string, avatar: string]>;
  };
  pricing: {
    access: { body: string; eyebrow: string; subtitle: string; title: string };
    description: string;
    documentTitle: string;
    eyebrow: string;
    factors: { eyebrow: string; items: Fact[]; title: string };
    faq: { eyebrow: string; questions: Question[]; title: string };
    included: { eyebrow: string; items: Fact[]; title: string };
    label: string;
    title: string;
  };
  featurePages: Record<FeatureSlug, FeaturePageContent>;
  legal: Record<LegalSlug, {
    description: string;
    intro: string;
    sections: LegalSection[];
    title: string;
    updated: string;
  }>;
  notFound: {
    back: string;
    description: string;
    title: string;
  };
  product: {
    description: string;
    documentTitle: string;
    facts: Fact[];
    preview: ProductPreviewContent;
    sections: Array<{ body: string; href: string; link: string; title: string }>;
    summary: string;
    title: string;
  };
  resources: {
    cards: Array<{ body: string; href: string; label: string; title: string }>;
    description: string;
    label: string;
    summary: string;
    title: string;
  };
  guide: {
    author: string;
    authorLabel: string;
    description: string;
    exampleItems: string[];
    exampleTitle: string;
    finalBody: string;
    finalLink: string;
    finalTitle: string;
    intro: string;
    label: string;
    modified: string;
    published: string;
    steps: Fact[];
    title: string;
    toc: string;
  };
  useCasePages: Record<UseCaseSlug, UseCasePageContent>;
  useCases: {
    cards: Array<{ body: string; href: string; link: string; title: string }>;
    description: string;
    label: string;
    summary: string;
    title: string;
    detail: {
      problemBody: string;
      problemTitle: string;
      workflowBody: string;
      workflowTitle: string;
    };
  };
};
