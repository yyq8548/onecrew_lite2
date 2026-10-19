const common = {
  brand: {
    name: "onecrew_lite",
  },
  actions: {
    apply: "应用",
    clear: "清除",
    close: "关闭",
    refresh: "刷新",
    reset: "重置",
    retry: "重试",
  },
  dataTable: {
    empty: "暂无数据",
    noResults: "无结果",
    selectedCount: "已选 {{count}} 项",
    filter: {
      ariaLabel: "筛选{{title}}",
      ariaLabelWithSelection: "筛选{{title}}，当前 {{selection}}",
      clear: "清除筛选",
      datePrompt: "选择单日或日期区间",
    },
    sort: {
      ascending: "升序",
      descending: "降序",
      hide: "隐藏",
    },
    toolbar: {
      searchPlaceholder: "筛选…",
      view: "视图",
      showColumns: "显示列",
    },
    pagination: {
      totalRows: "共 {{count}} 条",
      rowsPerPage: "每页",
      pageStatus: "第 {{current}} / {{total}} 页",
      firstPage: "首页",
      previousPage: "上一页",
      nextPage: "下一页",
      lastPage: "末页",
    },
  },
  providerError: {
    unavailable: "服务未返回可验证的诊断信息。",
    notProvided: "未提供",
    labels: {
      ownerCode: "用例错误码", stage: "发生阶段", platform: "平台", protocol: "实际协议",
      origin: "来源", nativeCode: "原生错误码", localCode: "本地条件码",
      httpStatus: "HTTP 状态", rpcCode: "RPC 错误码",
    },
    origins: { provider: "平台调用", local: "本地检查" },
    technicalDetails: "技术详情",
  },
  runtimeUnavailable: {
    title: "本地 onecrew_lite 服务不可用",
    description: "请在 OneCrew 状态卡点击“打开管理端”，或请 Codex 使用 OneCrew 重新打开已授权页面。",
  },
  sidebar: {
    title: "侧边栏",
    mobileDescription: "显示移动端侧边栏。",
    toggle: "切换侧边栏",
  },
  errors: {
    clipboardUnavailable: "当前环境无法访问剪贴板",
    creatorIdRequired: "必须提供 Creator ID",
  },
} as const;

export default common;
