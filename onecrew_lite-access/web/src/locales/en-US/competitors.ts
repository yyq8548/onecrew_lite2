import type {
  ComparisonLocaleContent,
  ComparisonQuestion,
} from "@/features/landing/marketing/competitors";

const sharedQuestions: ComparisonQuestion[] = [
  {
    question: "Is this a feature-by-feature comparison?",
    answer:
      "No. Product capabilities change frequently, so this page focuses on each product’s starting point, operating model, and best-fit team. Validate any must-have workflow in a trial before buying.",
  },
  {
    question: "Does OneCrew replace the AI service I already pay for?",
    answer:
      "No. OneCrew runs in its own desktop client and connects the AI services you already have access to. Your AI account and subscription remain separate.",
  },
];

const competitors: ComparisonLocaleContent = {
  page: {
    breadcrumb: "Compare",
    breadcrumbLabel: "OneCrew vs. {{competitor}}",
    eyebrow: "PRODUCT COMPARISON",
    title: "OneCrew vs. {{competitor}}",
    documentTitle: "OneCrew vs. {{competitor}} — Which Fits Your Workflow?",
    officialLink: "View the official source",
    chooseCompetitor: "Choose {{competitor}} when…",
    chooseOneCrew: "Choose OneCrew when…",
    practicalEyebrow: "THE PRACTICAL DIFFERENCE",
    practicalTitle: "Compare the operating model, not just the feature count.",
    decisionPoint: "Decision point",
    verified: "How this comparison was built",
    updated:
      "Reviewed in September 2026. Products change, so confirm must-have capabilities in current documentation and a hands-on trial.",
    faqTitle: "Questions worth asking before you choose",
    moreComparisons: "Compare other platforms",
    comparisonLink: "OneCrew vs. {{competitor}}",
  },
  entries: {
    "tiktok-shop-affiliate": { officialSource: "TikTok Shop Affiliate official documentation", summary: "TikTok Shop Affiliate centers on products, affiliate commissions, and TikTok Shop transactions. OneCrew focuses on creator discovery, communication, and partnership follow-up.", competitorFit: "Your core need is managing TikTok Shop product promotion, commissions, and attribution.", oneCrewFit: "You need cross-platform creator discovery and a reviewable outreach workflow with the AI you already use.", rows: [{ label: "Starting point", competitor: "TikTok Shop products, affiliate partnerships, and conversion tracking.", oneCrew: "Creator discovery, matching, communication, and follow-up tasks." }, { label: "Operating model", competitor: "Configure products, creators, and commissions inside TikTok Shop.", oneCrew: "Connect your existing AI, inbox, and supported social channels." }, { label: "Better buying signal", competitor: "You need a complete commerce affiliate loop.", oneCrew: "You want to improve creator research and outreach execution first." }], questions: sharedQuestions },

    lev8: {
      officialSource: "Lev8 product and tools pages",
      summary:
        "Lev8 is a broader live-web intelligence and outbound platform for GTM teams. OneCrew is purpose-built around creator discovery, brand communication, and partnership follow-up.",
      competitorFit:
        "Your work spans sales prospects, recruiting, market research, and broader GTM automation.",
      oneCrewFit:
        "Your core job is finding creators, understanding fit, reaching out from brand channels, and keeping partnership context intact.",
      rows: [
        {
          label: "Starting point",
          competitor: "Live-web people and business intelligence across multiple GTM plays.",
          oneCrew: "A specific creator discovery or outreach task.",
        },
        {
          label: "Role of creator workflows",
          competitor: "Creator Finder sits inside a broader prospecting and outreach toolkit.",
          oneCrew:
            "Creator records, fit signals, brand email, supported social channels, and follow-up are the primary workflow.",
        },
        {
          label: "AI model",
          competitor: "Uses Lev8’s own agents, research, and smart actions.",
          oneCrew: "Connects AI services you already pay for inside the OneCrew desktop client.",
        },
        {
          label: "Better buying signal",
          competitor: "You want one tool for several people-research and GTM outbound jobs.",
          oneCrew: "You want a focused, reviewable workspace for creator partnerships.",
        },
      ],
      questions: sharedQuestions,
    },
    modash: {
      officialSource: "Modash platform and help center",
      summary:
        "Modash is an all-in-one influencer marketing platform with a strong Shopify focus. OneCrew centers the workflow on connecting your existing AI with creator data, brand email, and supported social channels.",
      competitorFit:
        "You run a Shopify brand and want discovery, relationship management, tracking, and payments in one platform.",
      oneCrewFit:
        "You want to run creator search and outreach through natural language while keeping the AI service of your choice.",
      rows: [
        {
          label: "Starting point",
          competitor: "A Shopify-first influencer program operating system.",
          oneCrew: "A conversation-led creator discovery and outreach workspace.",
        },
        {
          label: "Public product emphasis",
          competitor: "Discovery, CRM, email, tracking, payments, and Shopify data sync.",
          oneCrew:
            "Creator fit, connected communication context, AI-prepared next steps, and human approval for key actions.",
        },
        {
          label: "AI model",
          competitor: "Includes built-in capabilities such as AI Search.",
          oneCrew: "Connects AI services you already have access to inside OneCrew.",
        },
        {
          label: "Better buying signal",
          competitor:
            "Commerce tracking and creator payments need to live in the same influencer suite.",
          oneCrew:
            "The team’s immediate priority is reducing repetitive search, drafting, and follow-up work with AI.",
        },
      ],
      questions: sharedQuestions,
    },
    hypeauditor: {
      officialSource: "HypeAuditor platform and help center",
      summary:
        "HypeAuditor emphasizes creator analytics, audience quality, fraud detection, and the campaign lifecycle. OneCrew focuses on letting the AI you already use participate directly in everyday creator outreach.",
      competitorFit:
        "You need deep creator analytics, audience-authenticity checks, competitive research, and campaign reporting.",
      oneCrewFit:
        "You already have a preferred AI and want to connect it to creator records, brand communication, and ongoing follow-up.",
      rows: [
        {
          label: "Starting point",
          competitor: "Large-scale creator data, analytics, and the full influencer lifecycle.",
          oneCrew: "A real creator task and the context needed to carry it forward.",
        },
        {
          label: "Public product emphasis",
          competitor:
            "Audience quality, authenticity, analytics, discovery, campaign management, and tracking.",
          oneCrew:
            "Natural-language search, fit rationale, connected communication, and controllable AI next steps.",
        },
        {
          label: "AI model",
          competitor: "Uses built-in capabilities such as AI Scout and AI outreach.",
          oneCrew:
            "Connects an external AI service and gives it access to OneCrew’s working context.",
        },
        {
          label: "Better buying signal",
          competitor: "Analytics depth, vetting metrics, and campaign measurement come first.",
          oneCrew: "Daily discovery, communication, and follow-up efficiency is the immediate problem.",
        },
      ],
      questions: sharedQuestions,
    },
    upfluence: {
      officialSource: "Upfluence platform page",
      summary:
        "Upfluence combines influencer, affiliate, UGC, payments, commerce integrations, and built-in AI. OneCrew takes a lighter approach centered on your existing AI and a dedicated creator outreach workspace.",
      competitorFit:
        "You need one system for commerce attribution, creator payments, affiliates, and campaign automation.",
      oneCrewFit:
        "You want to start with discovery and outreach, keep your choice of AI, and retain human control over important actions.",
      rows: [
        {
          label: "Starting point",
          competitor: "End-to-end influencer and affiliate program management.",
          oneCrew: "AI-native creator discovery, communication, and follow-up.",
        },
        {
          label: "Public product emphasis",
          competitor: "Discovery, marketplace, program management, tracking, payments, API, and MCP.",
          oneCrew:
            "Connects creator data, brand email, supported social channels, and the operating workspace.",
        },
        {
          label: "AI model",
          competitor: "Uses Upfluence’s built-in Jaice AI.",
          oneCrew: "Uses an AI service you already pay for and are authorized to access.",
        },
        {
          label: "Better buying signal",
          competitor:
            "The purchase must cover commerce, payments, and end-to-end program operations.",
          oneCrew:
            "The purchase is about putting your existing AI to work inside creator outreach.",
        },
      ],
      questions: sharedQuestions,
    },
    saral: {
      officialSource: "SARAL platform and help center",
      summary:
        "SARAL is built around gifting, seeding, affiliate programs, and relationship management for consumer brands. OneCrew centers on connecting your existing AI with creator data and outreach channels.",
      competitorFit:
        "Your core motion is DTC gifting, seeding, or affiliate marketing, with shipping, commissions, and content tracking.",
      oneCrewFit:
        "Your first priority is bringing creator search, personalized outreach, reply triage, and follow-up into an AI-assisted workflow.",
      rows: [
        {
          label: "Starting point",
          competitor: "DTC seeding and long-term influencer program operations.",
          oneCrew: "A natural-language task to find, reach, and move creator relationships forward.",
        },
        {
          label: "Public product emphasis",
          competitor:
            "Bulk discovery, email, CRM, shipping, affiliate tracking, payments, and UGC rights.",
          oneCrew:
            "Creator discovery, communication context, AI-prepared follow-up, and approval for key actions.",
        },
        {
          label: "AI model",
          competitor: "Centers on in-product automation and influencer program tools.",
          oneCrew: "Connects your existing AI to OneCrew’s creator and channel capabilities.",
        },
        {
          label: "Better buying signal",
          competitor: "You need to systemize product seeding, commissions, and content assets.",
          oneCrew:
            "You first need to reduce tool switching across search, drafting, reply triage, and follow-up.",
        },
      ],
      questions: sharedQuestions,
    },
    influencity: {
      officialSource: "Influencity platform page",
      summary:
        "Influencity spans discovery, IRM, campaigns, reporting, social management, and listening. OneCrew stays focused on AI-driven creator discovery and outreach execution.",
      competitorFit:
        "Your team wants a broad suite for influencer strategy, social operations, listening, and reporting.",
      oneCrewFit:
        "You want a focused workspace that puts your preferred AI directly into creator search, communication, and follow-up.",
      rows: [
        {
          label: "Starting point",
          competitor: "A broad influencer marketing and social media suite.",
          oneCrew: "An AI workspace organized around creator outreach tasks.",
        },
        {
          label: "Public product emphasis",
          competitor: "Discover, IRM, Campaign Manager, Reports, Social Hub, and Monitoring.",
          oneCrew: "Match creators, connect conversations, recognize reply intent, and prepare the next step.",
        },
        {
          label: "Operating model",
          competitor: "Teams plan, execute, and report across a suite of product modules.",
          oneCrew: "Users state a goal in chat, then review results and approve actions in the workspace.",
        },
        {
          label: "Better buying signal",
          competitor: "You want to consolidate several influencer and social tools.",
          oneCrew:
            "You want less software complexity and need to fix outreach efficiency and broken context first.",
        },
      ],
      questions: sharedQuestions,
    },
    aspire: {
      officialSource: "Aspire platform overview",
      summary:
        "Aspire combines a creator marketplace, relationship workflows, and full program tooling for commerce brands. OneCrew is a better fit for teams that want to move discovery and outreach forward with the AI they already use.",
      competitorFit:
        "You run mature influencer, ambassador, affiliate, UGC, or product-seeding programs and need a marketplace plus full program operations.",
      oneCrewFit:
        "You need a more direct AI-assisted outreach workflow with fewer modules between search, communication, and follow-up.",
      rows: [
        {
          label: "Starting point",
          competitor:
            "Large creator programs, a marketplace, and customizable relationship workflows.",
          oneCrew: "A creator task and AI-assisted execution.",
        },
        {
          label: "Public product emphasis",
          competitor:
            "Discovery, marketplace, relationship management, campaigns, content, affiliates, and measurement.",
          oneCrew:
            "Creator matching, connected communication, AI-prepared next steps, and human review.",
        },
        {
          label: "Buying and rollout",
          competitor:
            "The official path is demo-led, suited to a structured platform evaluation and rollout.",
          oneCrew:
            "Starts with the team’s real outreach workflow so the core experience can be validated first.",
        },
        {
          label: "Better buying signal",
          competitor:
            "You need enterprise program breadth, a marketplace, commerce outcomes, and service support.",
          oneCrew:
            "You want a smaller, faster test of whether AI can improve creator outreach execution.",
        },
      ],
      questions: sharedQuestions,
    },
  },
};

export default competitors;
