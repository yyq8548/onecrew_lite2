import distribution from "./distribution";
import competitors from "./competitors";
import authDesign from "./auth-design";
import showcaseDemo from "./showcase-demo";
import marketing from "./marketing";

const zhCN = {
  distribution,
  common: { brand: "OneCrew", or: "或", cancel: "取消", save: "保存", sending: "发送中…", loading: "加载中…", switchToChinese: "切换到中文", switchToEnglish: "切换到 English" },
  auth: {
    design: authDesign,
    login: {
      title: "登录", description: "继续管理你的达人合作。", passwordMethod: "密码登录", otpMethod: "验证码登录", otpUnavailable: "暂未配置", email: "邮箱", emailPlaceholder: "name@company.com", password: "密码", passwordPlaceholder: "请输入密码", forgotPassword: "忘记密码？", submit: "登录", otpCode: "邮箱验证码", otpPlaceholder: "输入 6 位验证码", sendOtp: "发送验证码", resendOtp: "重新发送验证码", verifyOtp: "验证并登录", cooldown: "重新发送验证码（{{seconds}}s）", google: "使用 Google 继续", googleUnavailable: "Google 登录（暂未接入）", noAccount: "还没有账号？", createAccount: "创建账号", newIpChallenge: "检测到登录环境变化，请输入邮箱验证码。", emailVerificationChallenge: "请先完成邮箱验证，再继续登录。", validEmailForOtp: "请输入有效邮箱后再获取验证码。", passwordMismatch: "两次输入的密码不一致。",
    },
    register: {
      title: "创建账号", verificationTitle: "验证邮箱", description: "集中管理达人发现、沟通和下一步。", verificationEnabledDescription: "注册后将发送验证码验证邮箱。", verificationDescription: "验证码已发送到 {{email}}。", name: "姓名", namePlaceholder: "你的姓名", email: "邮箱", emailPlaceholder: "name@company.com", password: "密码", passwordPlaceholder: "请输入密码", passwordHelp: "至少 8 个字符。", confirmation: "确认密码", submit: "创建账号", verify: "验证并继续", resendOtp: "重新发送验证码", cooldown: "重新发送验证码（{{seconds}}s）", wrongEmail: "邮箱填错了？", backToRegister: "返回注册", hasAccount: "已经有账号？", backToLogin: "返回登录", otpCode: "邮箱验证码", otpPlaceholder: "输入 6 位验证码",
    },
    forgot: {
      title: "忘记密码", description: "通过邮箱验证码设置新密码。", sentDescription: "验证码已发送到 {{email}}。", email: "邮箱", emailPlaceholder: "name@company.com", otpCode: "邮箱验证码", otpPlaceholder: "输入 6 位验证码", newPassword: "新密码", confirmPassword: "确认新密码", sendOtp: "发送验证码", resendOtp: "重发", cooldown: "{{seconds}}s", reset: "重置密码", completeTitle: "密码已重置", completeDescription: "请使用新密码登录 OneCrew。", backToLogin: "返回登录", remembered: "想起密码了？",
    },
    otp: { validity: "验证码 5 分钟内有效，每次重发间隔 60 秒。", firstLoginHint: "未注册邮箱验证后会自动创建账号，可在账号中心设置密码。" },
  },
  account: {
    center: "个人中心", overviewNav: "概览", accessNav: "体验与下载", backHome: "返回 OneCrew 官网", viewProfile: "查看资料", snapshot: "账号一览", snapshotHint: "快速查看和进入常用设置。", accessSummaryHint: "查看体验申请、审核状态与客户端下载。", openPanel: "管理",
    context: "账号中心", signOut: "退出登录", greeting: "你好，{{name}}", defaultUser: "OneCrew 用户", manage: "管理你的 OneCrew 账号、官网会话和已连接设备。", sessionOk: "官网会话正常", currentAccount: "当前登录账号", safeSignedIn: "已安全登录 OneCrew", settingsTitle: "账号设置", settingsDescription: "更新资料、邮箱和登录密码。", profileNav: "个人资料", securityNav: "登录安全", emailNav: "邮箱地址", devicesNav: "设备管理", profileTitle: "个人资料", profileDescription: "这些信息会显示在你的 OneCrew 账号中。", name: "姓名", nameDescription: "这个名字会显示在你的 OneCrew 账号中。", saveProfile: "保存资料", profileSaved: "资料已保存。", securityTitle: "登录安全", securityDescription: "验证码登录创建的账号可以在这里设置密码。", currentPassword: "当前密码", currentPasswordPlaceholder: "输入当前密码", newPassword: "新密码", newPasswordPlaceholder: "至少 8 个字符", confirmPassword: "确认新密码", setPassword: "设置密码", changePassword: "修改密码", passwordSetHint: "当前账号还没有密码，设置后可以使用密码登录。", passwordSaved: "密码已保存。", emailTitle: "邮箱地址", emailDescription: "先验证当前邮箱，再验证新邮箱。", currentEmail: "当前邮箱", newEmail: "新邮箱", newEmailPlaceholder: "new@example.com", currentOtp: "当前邮箱验证码", currentEmailVerifyDescription: "验证码用于确认你仍能访问当前邮箱。", verifyCurrentEmail: "验证当前邮箱", currentEmailVerified: "当前邮箱已验证，请填写新邮箱。", sendNewEmailOtp: "发送新邮箱验证码", restartEmailChange: "重新验证当前邮箱", newOtp: "新邮箱验证码", sendCurrentOtp: "发送当前邮箱验证码", currentCodeSent: "验证码已发送到当前邮箱。", requestEmailChange: "发送新邮箱验证码", resendEmailChange: "重新发送新邮箱验证码", verifyEmailChange: "确认新邮箱", emailChangeRequested: "验证码已发送到新邮箱。", emailUpdated: "邮箱已更新。", devicesTitle: "已连接设备", devicesDescription: "查看和解除绑定的设备。",
    devices: { identity: "当前账号身份", listUnavailable: "设备列表暂时不可用", operationConfirm: "设备操作结果需要确认", loading: "正在读取设备…", empty: "暂无设备", loadMore: "加载更多设备", system: "系统", bindingIp: "绑定 IP", bindingLocation: "绑定地区", connectedAt: "连接时间", addedAt: "添加时间", revokedAt: "解除时间", active: "已激活", pending: "等待完成", expired: "已过期", revoked: "已撤销", delete: "删除", deleteButton: "删除设备", revoke: "解除绑定", revokeButton: "解除绑定", deleteTitle: "删除设备“{{label}}”？", revokeTitle: "解除绑定设备“{{label}}”？", deleteDescription: "删除后，这条已失效的设备记录将从账号列表中永久移除。", revokeDescription: "解除后，这台设备将无法继续访问你的 OneCrew 账号；如需恢复，必须重新绑定。", confirmDelete: "确认删除", confirmRevoke: "确认解除绑定", authFailure: "登录状态已失效，请重新登录。", notFound: "设备不存在或已不属于当前账号。", mustRevoke: "活动设备必须先解除绑定，才能删除记录。", revokeConflict: "同步服务中的设备身份不一致，请刷新后重试。", revokeUnavailable: "同步服务暂时不可用，解除绑定尚未完成，请稍后重试。", rateLimited: "请求过于频繁，请稍后再试。", revokeUnknown: "无法确认解除绑定结果，请刷新设备列表确认服务端状态。", deleteUnknown: "无法确认删除结果，请刷新设备列表确认服务端状态。", listUnknown: "无法读取安全设备状态，请检查网络后重试。", revokedRefresh: "设备已撤销，但完整列表刷新失败；当前状态来自服务端撤销响应，请稍后再次刷新。", deletedRefresh: "设备已删除，但完整列表刷新失败；当前列表已采用服务端删除响应，请稍后再次刷新。" },
  },
  errors: {
    invalidCredentials: "邮箱或密码不正确。", invalidPassword: "当前密码不正确。", emailNotVerified: "请先完成邮箱验证后再登录。", userAlreadyExists: "这个邮箱已经注册，请直接登录。", invalidEmail: "请输入有效的邮箱地址。", passwordTooShort: "密码至少需要 8 个字符。", tooManyRequests: "尝试次数过多，请稍后再试。", emailOtpInvalid: "验证码错误或已过期，请重新获取。", emailOtpUnavailable: "验证码服务暂不可用，请稍后重试。", emailChangeFailed: "邮箱修改未完成，请检查验证码后重试。", passwordResetFailed: "密码重置未完成，请检查验证码后重试。", passwordUpdateFailed: "密码更新未完成，请检查当前密码后重试。", profileUpdateFailed: "资料保存未完成，请稍后重试。", accountSettingsFailed: "账号设置暂不可用，请稍后重试。", generic: "操作未完成，请稍后重试。",
    oauth: { accessDenied: "Google 登录已取消，你可以重试或改用邮箱登录。", accountAlreadyLinked: "这个 Google 账号已连接到其他用户。", emailNotFound: "Google 账号没有提供可用邮箱，请改用邮箱注册。", emailNotVerified: "Google 账号邮箱尚未验证，请改用其他账号。", invalidCode: "Google 登录请求已失效，请重新开始。", noCode: "Google 登录未完成，请重新尝试。", callback: "Google 登录未完成，请重新尝试。", providerUnavailable: "Google 登录当前不可用，请改用邮箱登录。", stateNotFound: "登录请求已失效，请重新开始。", stateMismatch: "登录请求已失效，请重新开始。", unableToReadUser: "无法读取 Google 账号信息，请重新尝试。", signupDisabled: "当前暂时无法创建账号。", generic: "第三方登录未完成，请重新尝试。" },
  },
  layout: { homeAria: "OneCrew 首页", legalNotice: "登录即表示你同意仅在自己的设备和授权范围内使用 OneCrew。", showcaseKicker: "AI 达人发现与外联", showcaseTitle: "让你的 AI 帮你找到对的人。", showcaseDescription: "搜索、筛选、外联和持续跟进，在一个可掌控的工作台中完成。", showcaseDemo, sessionLoading: "正在检查登录状态…", sessionErrorTitle: "无法检查登录状态", sessionErrorDescription: "请检查网络连接后重试。", retry: "重新尝试" },
  competitors,
  marketing,
} as const;

export default zhCN;
