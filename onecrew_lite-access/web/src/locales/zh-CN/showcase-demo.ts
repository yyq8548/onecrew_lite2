const showcaseDemo = {
  addBrief: "添加品牌资料",
  ariaLabel: "OneCrew AI 达人搜索任务演示",
  creatorDiscovery: "达人发现与外联任务",
  creators: {
    ava: { channel: "Instagram · 美妆", detail: "北美受众 · 高互动", handle: "@MiaMakes" },
    claire: { channel: "Instagram · 美妆", detail: "北美受众 · 邮箱可用", handle: "@ClaireCreates" },
    ethan: { channel: "Instagram · 生活方式", detail: "品牌调性匹配", handle: "@EthanDaily" },
    imani: { channel: "YouTube · 测评", detail: "长视频 · 邮箱可用", handle: "@ImaniTests" },
    jonas: { channel: "YouTube · 日常护理", detail: "长视频 · 邮箱可用", handle: "@JonasReviews" },
    lina: { channel: "TikTok · 清洁护肤", detail: "近期活跃 · 可私信", handle: "@LinaGlow" },
    marcus: { channel: "YouTube · 成分测评", detail: "专业内容 · 建议联系", handle: "@MarcusLab" },
    mei: { channel: "TikTok · 护肤", detail: "教程内容 · 可触达", handle: "@MeiRoutine" },
    nora: { channel: "Instagram · 成分测评", detail: "专业内容 · 建议联系", handle: "@NoraNotes" },
    sofia: { channel: "TikTok · 护肤教程", detail: "品牌匹配 · 高互动", handle: "@SofiaSkin" },
  },
  promptStart: "为北美市场的护肤新品，寻找匹配的 TikTok 和 ",
  prompt: "为北美市场的护肤新品，寻找匹配的 TikTok 和 Instagram 达人，并准备第一轮个性化外联。",
  resultDescription: "已整理匹配依据与可触达渠道，外联前由你确认",
  resultTitle: "已找到 48 位潜在匹配达人",
  runTask: "运行任务",
} as const;

export default showcaseDemo;
