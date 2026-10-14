import type {
  ComparisonLocaleContent,
  ComparisonQuestion,
} from "@/features/landing/marketing/competitors";

const sharedQuestions: ComparisonQuestion[] = [
  {
    question: "这是一份功能清单对比吗？",
    answer:
      "不是。产品功能会持续变化，这里优先比较产品的起点、工作方式和适用团队。正式采购前仍应通过试用验证关键流程。",
  },
  {
    question: "OneCrew 会替代我已经购买的 AI 服务吗？",
    answer:
      "不会。OneCrew 在自己的桌面客户端中连接你已有使用权限的 AI 服务，再把达人数据、沟通渠道和工作台能力提供给它。AI 服务的账号与费用仍由你单独管理。",
  },
];

const competitors: ComparisonLocaleContent = {
  page: {
    breadcrumb: "产品对比",
    breadcrumbLabel: "OneCrew 与 {{competitor}}",
    eyebrow: "产品对比",
    title: "OneCrew 与 {{competitor}} 怎么选",
    documentTitle: "OneCrew 与 {{competitor}} 对比｜怎么选",
    officialLink: "查看官方来源",
    chooseCompetitor: "这些情况更适合 {{competitor}}",
    chooseOneCrew: "这些情况更适合 OneCrew",
    practicalEyebrow: "实际差别",
    practicalTitle: "不要只数功能，要比较工作方式",
    decisionPoint: "判断维度",
    verified: "比较依据",
    updated:
      "页面核对时间：2026 年 9 月。产品功能会变化，请以各平台最新公开说明和实际试用为准。",
    faqTitle: "选择前，值得先问清楚",
    moreComparisons: "继续比较其他平台",
    comparisonLink: "OneCrew 与 {{competitor}}",
  },
  entries: {
    "tiktok-shop-affiliate": { officialSource: "TikTok Shop Affiliate 官方说明", summary: "TikTok Shop Affiliate 以商品、联盟佣金和 TikTok Shop 交易为中心；OneCrew 专注于达人发现、沟通和合作跟进。", competitorFit: "你的核心目标是管理 TikTok Shop 商品推广、佣金和交易归因。", oneCrewFit: "你需要跨平台发现达人，并用已有 AI 推进可审核的外联流程。", rows: [{ label: "产品起点", competitor: "围绕 TikTok Shop 商品、联盟合作和订单转化。", oneCrew: "围绕达人发现、匹配、沟通和跟进任务。" }, { label: "工作方式", competitor: "在 TikTok Shop 体系内配置商品、达人和佣金。", oneCrew: "连接已有 AI、邮箱和支持的社交渠道。" }, { label: "更适合的采购判断", competitor: "需要完整的电商联盟交易闭环。", oneCrew: "需要先提升达人研究与外联执行效率。" }], questions: sharedQuestions },

    lev8: {
      officialSource: "Lev8 官方产品与工具页面",
      summary:
        "Lev8 是面向更广泛 GTM 场景的实时网络研究与外联平台；OneCrew 则把产品重点放在达人发现、品牌沟通与合作跟进。",
      competitorFit: "你的主要需求横跨销售线索、招聘、市场研究与多种 GTM 自动化。",
      oneCrewFit:
        "你的核心任务是寻找达人、理解内容与受众、从品牌渠道发起外联并持续维护合作上下文。",
      rows: [
        {
          label: "产品起点",
          competitor: "从实时网络中查找人物与商业信息，并支持多类 GTM playbook。",
          oneCrew: "从一次具体的达人发现或外联任务开始。",
        },
        {
          label: "达人能力的位置",
          competitor: "Creator Finder 是其更广泛人物搜索与外联工具集中的一部分。",
          oneCrew: "达人档案、匹配依据、品牌邮箱、支持的社媒渠道与跟进状态是产品主流程。",
        },
        {
          label: "AI 使用方式",
          competitor: "使用 Lev8 自身的 agents、research 与 smart actions。",
          oneCrew: "在 OneCrew 客户端中连接你已经购买并有权使用的 AI 服务。",
        },
        {
          label: "更适合的采购判断",
          competitor: "希望一套工具覆盖多种 GTM 人物研究与外联。",
          oneCrew: "希望围绕达人合作形成更专注、可审核的工作台。",
        },
      ],
      questions: sharedQuestions,
    },
    modash: {
      officialSource: "Modash 官方平台与帮助中心",
      summary:
        "Modash 是以 Shopify 品牌为重点的一体化网红营销平台；OneCrew 更强调把已有 AI 与达人数据、品牌邮箱和支持的社交渠道连接起来。",
      competitorFit: "你经营 Shopify 品牌，并希望在同一平台完成发现、关系管理、效果追踪和付款。",
      oneCrewFit: "你更重视通过自然语言推进达人搜索和外联，并希望继续使用自己选择的 AI 服务。",
      rows: [
        {
          label: "产品起点",
          competitor: "以 Shopify 品牌的 influencer program 为中心。",
          oneCrew: "以对话驱动的达人发现与外联工作台为中心。",
        },
        {
          label: "公开强调的能力",
          competitor: "发现、CRM、邮件、追踪、付款与 Shopify 数据同步。",
          oneCrew: "达人匹配、统一沟通上下文、AI 准备下一步与人工确认关键动作。",
        },
        {
          label: "AI 使用方式",
          competitor: "平台内提供 AI Search 等内置能力。",
          oneCrew: "连接你已有使用权限的 AI 服务，在 OneCrew 客户端中工作。",
        },
        {
          label: "更适合的采购判断",
          competitor: "电商品牌希望把商业追踪和付款纳入同一个 influencer suite。",
          oneCrew: "团队希望先用 AI 减少搜索、写信与跟进中的重复操作。",
        },
      ],
      questions: sharedQuestions,
    },
    hypeauditor: {
      officialSource: "HypeAuditor 官方平台与帮助中心",
      summary:
        "HypeAuditor 公开强调达人分析、受众质量、反欺诈与完整 campaign lifecycle；OneCrew 的重心是让你常用的 AI 直接参与日常达人外联工作。",
      competitorFit: "你需要深度达人分析、受众真实性判断、竞品研究和完整 campaign reporting。",
      oneCrewFit: "你已经有常用 AI，希望把它接入达人档案、品牌沟通和连续的跟进动作。",
      rows: [
        {
          label: "产品起点",
          competitor: "以大规模达人数据、分析和营销全周期为起点。",
          oneCrew: "以一次真实的达人任务和连续沟通上下文为起点。",
        },
        {
          label: "公开强调的能力",
          competitor: "受众质量、真实性、分析报告、发现、campaign 管理与追踪。",
          oneCrew: "自然语言搜索、匹配依据、统一沟通和可控的 AI 下一步。",
        },
        {
          label: "AI 使用方式",
          competitor: "使用平台内建的 AI Scout、AI outreach 等能力。",
          oneCrew: "连接外部 AI 服务，将 OneCrew 能力作为它可调用的工作上下文。",
        },
        {
          label: "更适合的采购判断",
          competitor: "分析深度、审计指标和 campaign measurement 是首要需求。",
          oneCrew: "日常查找、沟通与跟进效率是当前最需要解决的问题。",
        },
      ],
      questions: sharedQuestions,
    },
    upfluence: {
      officialSource: "Upfluence 官方平台页面",
      summary:
        "Upfluence 是覆盖 influencer、affiliate、UGC、付款和电商集成的综合平台，并提供内置 AI；OneCrew 更轻量地围绕已有 AI 与达人外联工作台组合。",
      competitorFit: "你需要电商归因、达人付款、affiliate 与 campaign automation 组成的一体化系统。",
      oneCrewFit: "你希望从达人发现和外联切入，保留 AI 选择权，并在重要动作上保持人工控制。",
      rows: [
        {
          label: "产品起点",
          competitor: "综合 influencer 与 affiliate program 管理。",
          oneCrew: "AI 原生的达人发现、沟通与持续跟进。",
        },
        {
          label: "公开强调的能力",
          competitor: "发现、marketplace、项目管理、效果追踪、付款、API 与 MCP。",
          oneCrew: "连接达人数据、品牌邮箱、支持的社交渠道和工作面板。",
        },
        {
          label: "AI 使用方式",
          competitor: "使用 Upfluence 内建的 Jaice AI。",
          oneCrew: "使用你已购买并有权调用的 AI 服务。",
        },
        {
          label: "更适合的采购判断",
          competitor: "采购目标是覆盖完整商业化、付款与电商闭环。",
          oneCrew: "采购目标是让现有 AI 真正进入达人外联执行。",
        },
      ],
      questions: sharedQuestions,
    },
    saral: {
      officialSource: "SARAL 官方平台与帮助中心",
      summary:
        "SARAL 以消费品牌的 gifting、seeding、affiliate 和关系管理为中心；OneCrew 以连接现有 AI、达人数据与外联渠道为中心。",
      competitorFit: "你的主要模式是 DTC gifting、seeding 或 affiliate，并需要发货、佣金与内容追踪。",
      oneCrewFit: "你的首要问题是把寻找达人、个性化外联、回复判断和跟进集中到 AI 协作流程。",
      rows: [
        {
          label: "产品起点",
          competitor: "从 DTC 品牌的 seeding 与长期 influencer program 出发。",
          oneCrew: "从一条自然语言任务出发，查找、触达并推进达人关系。",
        },
        {
          label: "公开强调的能力",
          competitor: "批量发现、邮件、CRM、发货、affiliate tracking、付款和 UGC rights。",
          oneCrew: "达人发现、沟通上下文、AI 跟进准备与关键动作确认。",
        },
        {
          label: "AI 使用方式",
          competitor: "以平台内的 automation 和 influencer program 工具为主。",
          oneCrew: "把你已有 AI 接入 OneCrew 的达人与渠道能力。",
        },
        {
          label: "更适合的采购判断",
          competitor: "要系统化商品寄送、佣金和内容资产管理。",
          oneCrew: "要先减少跨工具搜索、写信、判断回复和提醒跟进。",
        },
      ],
      questions: sharedQuestions,
    },
    influencity: {
      officialSource: "Influencity 官方平台页面",
      summary:
        "Influencity 提供从发现、IRM、campaign、报告到社媒管理与监听的宽产品套件；OneCrew 更专注于 AI 驱动的达人发现与外联执行。",
      competitorFit: "你的团队希望用较宽的软件套件覆盖 influencer strategy、社媒运营、监听与报告。",
      oneCrewFit: "你希望用更专注的工作台，把常用 AI 直接接入达人搜索、沟通和跟进。",
      rows: [
        {
          label: "产品起点",
          competitor: "覆盖 influencer marketing 与更广泛的 social media suite。",
          oneCrew: "围绕达人外联任务构建的 AI 工作台。",
        },
        {
          label: "公开强调的能力",
          competitor: "Discover、IRM、Campaign Manager、Reports、Social Hub 与 Monitoring。",
          oneCrew: "匹配达人、整合沟通、识别回复意向并准备下一步。",
        },
        {
          label: "工作方式",
          competitor: "团队在多个产品模块中规划、执行和报告。",
          oneCrew: "用户从对话提出目标，再在工作台检查结果并确认动作。",
        },
        {
          label: "更适合的采购判断",
          competitor: "希望减少多个 influencer 与 social 工具的采购数量。",
          oneCrew: "希望控制工具复杂度，并优先解决外联效率与上下文断裂。",
        },
      ],
      questions: sharedQuestions,
    },
    aspire: {
      officialSource: "Aspire 官方平台总览",
      summary:
        "Aspire 提供 creator marketplace、relationship workflow 与面向电商品牌的完整 program 工具；OneCrew 更适合希望用现有 AI 快速推进发现与外联的团队。",
      competitorFit:
        "你运营成熟的 influencer、ambassador、affiliate、UGC 或 product seeding 项目，需要 marketplace 与完整项目系统。",
      oneCrewFit: "你需要更直接的 AI 外联工作方式，并希望在较少模块中完成查找、沟通和跟进。",
      rows: [
        {
          label: "产品起点",
          competitor: "以大型 creator program、marketplace 与可定制 relationship workflow 为中心。",
          oneCrew: "以一次达人任务和 AI 协作执行为中心。",
        },
        {
          label: "公开强调的能力",
          competitor: "发现、marketplace、relationship management、campaign、content、affiliate 与 measurement。",
          oneCrew: "达人匹配、统一沟通、AI 下一步建议与人工审核。",
        },
        {
          label: "采购与上线方式",
          competitor: "官方主要引导品牌预约 demo，适合系统化评估与部署。",
          oneCrew: "从团队实际外联流程出发申请体验，先验证核心工作流。",
        },
        {
          label: "更适合的采购判断",
          competitor: "需要覆盖大型项目、marketplace、商业效果和服务生态。",
          oneCrew: "需要以更小切口验证 AI 能否直接提升达人外联效率。",
        },
      ],
      questions: sharedQuestions,
    },
  },
};

export default competitors;
