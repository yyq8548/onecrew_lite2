const connect = {
  page: {
    eyebrow: "onecrew_lite × Agent",
    title: "让 Agent 直接使用 onecrew_lite",
    description: "使用已连接的账号完成发现、管理与触达，同时保留本地运行与逐次授权边界。",
  },
  capabilities: {
    discover: {
      title: "发现 Creator",
      description: "跨平台按地区与主题寻找合适人选",
    },
    creators: {
      title: "维护 onecrew_lite",
      description: "沉淀 Creator、关系与沟通记录",
    },
    outreach: {
      title: "执行触达",
      description: "确认后发送私信或邮件",
    },
  },
  service: {
    label: "本地 MCP",
    supportCode: "支持代码：{{code}}",
    states: {
      stopped: {
        badge: "未启动",
        title: "等待启动",
        description: "启动本地 MCP 后，下方两种接入方式即可使用。",
        action: "启动本地 MCP",
      },
      starting: {
        badge: "正在启动",
        title: "正在启动本地 MCP",
        description: "正在完成 MCP 协议检查。",
        action: "正在启动…",
      },
      healthy: {
        badge: "健康",
        title: "Agent 连接已就绪",
        description: "Codex 与标准 MCP 客户端现在可以连接。",
        action: "停止本地 MCP",
      },
      degraded: {
        badge: "连接异常",
        title: "连接暂时异常",
        description: "健康检查失败，onecrew_lite 正在自动恢复。",
        action: "停止本地 MCP",
      },
      recovering: {
        badge: "正在恢复",
        title: "正在恢复连接",
        description: "正在重新建立本地 MCP 服务。",
        action: "正在恢复…",
      },
      stopping: {
        badge: "正在停止",
        title: "正在停止本地 MCP",
        description: "正在安全关闭本地 MCP 服务。",
        action: "正在停止…",
      },
      faulted: {
        badge: "需要处理",
        title: "服务需要处理",
        description: "查看错误详情后重新启动本地 MCP。",
        action: "重新启动本地 MCP",
      },
    },
    toasts: {
      stopped: "已停止本地 MCP 服务",
      started: "本地 MCP 已启动并通过协议检查",
      startFailed: "本地 MCP 启动失败",
    },
  },
  access: {
    eyebrow: "接入方式",
    title: "选择 Agent 如何连接",
    description: "Codex 使用插件；其他客户端使用一次性 MCP 指引",
  },
  codex: {
    name: "Codex",
    subtitle: "GPT Codex · onecrew_lite 插件",
    lifecycle: {
      desktopOnly: {
        badge: "仅桌面可用",
        title: "仅桌面应用可检查 Codex",
        description: "请在 onecrew_lite 桌面应用中管理 Codex 插件。",
      },
      stagingFailed: {
        badge: "生成失败",
        title: "onecrew_lite 插件生成失败",
        description: "本次启动没有生成可用插件包，因此不会沿用上一次运行的版本状态。请重新启动本地服务后再试。",
      },
      waiting: {
        badge: "等待服务",
        title: "等待本地服务",
        description: "启动后会为本次连接生成 onecrew_lite 插件，并核对 Codex 中实际安装的版本。",
      },
      checking: {
        badge: "检测中",
        title: "正在检查 onecrew_lite 插件",
        description: "正在生成或核对本次连接对应的 onecrew_lite 插件版本。",
      },
      installFailedDescription: "安装没有完成，请重试；现有版本不会被误报为最新。",
    },
    pluginStates: {
      transportFailed: {
        badge: "Codex 检查异常",
        title: "Codex 插件状态暂时不可用",
        description: "暂时无法读取 Codex 的插件状态。onecrew_lite 不会把缓存版本当成当前状态。",
        supportingDescription: "本地 MCP 状态不受此次检查影响，请稍后重新检查。",
      },
      unrelatedMarketplaceHealthy: {
        badge: "Codex 配置需处理",
        title: "Codex 无法读取插件列表",
        description: "本地 MCP 和 onecrew_lite 均正常。Codex 中另一个插件源「{{marketplace}}」已失效，因此暂时无法核对 onecrew_lite 插件版本。移除或修复该插件源后重新检查。",
        supportingDescription: "此问题仅影响 Codex 插件状态检查，不影响其他 MCP 客户端连接 onecrew_lite。",
      },
      unrelatedMarketplace: {
        badge: "Codex 配置需处理",
        title: "Codex 无法读取插件列表",
        description: "Codex 中另一个插件源「{{marketplace}}」已失效，因此暂时无法核对 onecrew_lite 插件版本。移除或修复该插件源后重新检查。",
        supportingDescription: "此错误来自 Codex 的本机插件配置，不表示 onecrew_lite 插件源已损坏。",
      },
      localMarketplaceInvalid: {
        badge: "插件源异常",
        title: "onecrew_lite 插件源异常",
        description: "Codex 中的 onecrew_lite 本地插件源已失效，暂时无法核对或更新插件版本。",
        supportingDescription: "请重新启动本地 MCP 以重新生成插件源，然后重新检查。",
      },
      genericFailure: {
        badge: "Codex 检查异常",
        title: "Codex 无法读取插件状态",
        description: "Codex 插件状态读取失败，暂时无法核对 onecrew_lite 插件版本。",
        supportingDescription: "本地 MCP 状态不受此次检查影响，请稍后重新检查。",
      },
      notDetected: {
        badge: "未检测",
        title: "未检测到 Codex",
        description: "在这台设备安装 Codex 后，即可从这里接入 onecrew_lite。",
        supportingDescription: "",
      },
      notReady: {
        badge: "未就绪",
        title: "onecrew_lite 插件尚未生成",
        description: "onecrew_lite 插件包尚未准备好，请重启本地服务后再试。",
        supportingDescription: "",
      },
      notInstalled: {
        badge: "可安装",
        title: "安装 onecrew_lite 插件",
        description: "安装 onecrew_lite 插件，在新对话中使用账号、Creator 与建联能力。",
        supportingDescription: "",
      },
      updateAvailable: {
        badge: "可更新",
        title: "更新 onecrew_lite 插件",
        description: "检测到新的 onecrew_lite 插件版本，更新后从新对话生效。",
        supportingDescription: "",
      },
      notRegistered: {
        badge: "需修复",
        title: "修复 onecrew_lite 插件连接",
        description: "插件版本已存在，但 onecrew_lite 本地插件源尚未注册。",
        supportingDescription: "",
      },
      ready: {
        badge: "已就绪",
        title: "onecrew_lite 插件已就绪",
        description: "onecrew_lite 插件已是最新版本，新对话会自动加载连接能力。",
        supportingDescription: "",
      },
    },
    actions: {
      install: "安装 onecrew_lite 插件",
      update: "更新 onecrew_lite 插件",
      repair: "修复插件连接",
    },
    versions: {
      ariaLabel: "onecrew_lite 插件当前与最新版本",
      current: "当前",
      latest: "最新",
      notInstalled: "未安装",
    },
    note: "插件变更从 Codex 新对话生效",
    buttons: {
      upToDate: "已是最新版本",
      recheck: "重新检查",
      installing: "正在安装…",
      unavailable: "暂不可用",
      configureAfterStart: "启动服务后配置",
    },
    toasts: {
      repaired: "Codex 插件连接已修复",
      upToDate: "onecrew_lite 插件已是最新",
      installed: "onecrew_lite 插件已安装或更新",
      restartConversation: "请从 Codex 新对话开始使用。",
      installFailed: "插件安装失败",
    },
    marketplace: {
      unknown: "未知插件源",
    },
  },
  standard: {
    name: "其他 Agent",
    subtitle: "标准 MCP 客户端",
    states: {
      ready: {
        badge: "可连接",
        title: "生成一次性连接指引",
        description: "复制给支持 Streamable HTTP 的客户端，按指引完成连接。",
        note: "5 分钟内有效且只能使用一次",
      },
      waiting: {
        badge: "等待服务",
        title: "等待本地服务",
        description: "启动后即可生成安全的短时 MCP 连接指引。",
        note: "连接信息不会长期暴露",
      },
    },
    buttonAfterStart: "启动服务后生成",
  },
  setupPrompt: {
    generating: "正在生成…",
    copied: "已复制连接指引",
    regenerate: "重新生成连接指引",
    generate: "生成并复制连接指引",
    copyFailed: "复制 MCP 连接指引失败",
  },
  footer: {
    privacy: "账号与凭据保留在 onecrew_lite；本地 Agent 只获得当前授权的能力",
  },
} as const;

export default connect;
