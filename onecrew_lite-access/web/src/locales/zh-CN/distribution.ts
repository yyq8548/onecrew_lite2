export default {
  stepsLabel: "体验流程", reviewStep: "审核结果",
  packagePreparing: "安装包准备中",
  applicationsClosedAction: "申请暂未开放", applicationsClosed: "申请暂未开放。已填写的内容会保留；开放后刷新状态即可提交。",
  reviewReason: "审核原因",
  requestError: "暂时无法验证下载权限", requestErrorDetail: "服务或网络暂时不可用，请重试。当前无法确认此链接的状态。",
  requiredFields: "请填写称呼和主要用途，不能仅输入空格。",
  eyebrow: "ONECREW EARLY ACCESS", title: "开始你的 OneCrew 体验", intro: "填写用途并提交申请，通过后在这里下载 OneCrew。",
  state: { closed: "暂未开放", none: "尚未申请", pending: "审核中", approved: "已获批", rejected: "申请未通过", revoked: "体验权限已撤销" },
  description: { approvedPreparing: "你的体验权限已开通。安装包正在准备中，发布后刷新状态即可下载。", none: "只需填写称呼和主要用途，邮箱将使用当前登录账户。", pending: "申请已收到。审核结果会发送至你的邮箱，也会更新在这里。", approved: "你的体验申请已通过。点击前往下载，安装 OneCrew 后按界面连接 OpenCLI。", rejected: "本次申请未通过。你可以更新主要用途后重新提交申请。", revoked: "当前账户已失去下载权限。已有链接也无法继续下载。" },
  apply: "申请体验", reapply: "更新申请", email: "登录邮箱", emailHint: "自动使用当前账户邮箱，无法在申请中修改。", displayName: "怎么称呼你", purpose: "主要用途", purposePlaceholder: "例如：为出海品牌寻找创作者，并管理合作沟通。", company: "团队 / 公司", optional: "选填", submit: "提交申请", submitting: "正在提交…", cancel: "取消", privacy: "我们仅使用这些信息审核体验申请。无需提供电话、地址或身份证明。", summary: "你的申请", refresh: "刷新状态", loading: "正在读取体验权限…", retry: "重新加载", error: "暂时无法读取状态，请稍后重试。", submitError: "申请未能确认，请重试。重复重试不会重复创建同一份申请。", verification: "请先验证当前登录邮箱，再提交体验申请。", conflict: "申请状态已更新，请刷新后查看。", signIn: "重新登录", sessionExpired: "登录已过期，请重新登录。",
  getLink: "获取下载链接", generating: "正在生成…", openDownload: "前往下载", linkNotice: "链接有效期为 24 小时，下载时仍需登录当前账户。", expires: "有效期至 {{date}}", downloadTitle: "下载 OneCrew", downloadIntro: "此链接仅供获批账户使用。每次下载都会验证当前权限。", signedIn: "当前登录：{{email}}", account: "返回账户中心", loadingDownload: "正在验证下载权限…", download: "下载安装包", noAssets: "暂无可用安装包", release: "版本 {{version}}", releaseNotes: "更新说明", expired: "下载链接已过期", expiredDetail: "请返回账户中心，重新获取下载链接。", wrongAccount: "当前登录账户不匹配", wrongAccountDetail: "请使用获批并收到此链接的账户登录。", unavailable: "下载链接不可用", unavailableDetail: "链接不存在，或当前登录账户与链接不匹配。请使用收到链接的账户登录，或返回账户中心获取新链接。", revokedTitle: "下载权限已撤销", revokedDetail: "当前账户无法继续下载，请返回账户中心查看状态。", noRelease: "安装包尚未发布", noReleaseDetail: "你的权限验证成功，但暂时没有可下载的版本。请稍后刷新。", invalid: "下载链接格式无效", invalidDetail: "请使用邮件或账户中心提供的完整链接。", copy: "复制链接", copied: "已复制", copyError: "未能复制，请打开下载页面。", fetchError: "暂时无法生成下载链接，请稍后重试。", switchAccount: "切换账户", switching: "正在退出…", mock: "本地视觉预览 · 模拟数据 · 不连接真实后台",
} as const;
