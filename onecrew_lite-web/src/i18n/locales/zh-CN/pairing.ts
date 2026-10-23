const pairing = {
  states: {
    checking: {
      title: "正在检查绑定状态",
      description: "正在确认这台设备是否可以进入 onecrew_lite。",
    },
    idle: {
      title: "连接你的 onecrew_lite 账号",
      description: "通过浏览器确认账号和这台设备，然后返回这里继续。",
    },
    waiting: {
      title: "等待浏览器确认",
      description: "请在浏览器中完成登录与授权。",
    },
    callback: {
      title: "已收到浏览器确认",
      description: "正在校验本次绑定，请保持 onecrew_lite 打开。",
    },
    enrolling: {
      title: "正在完成设备绑定",
      description: "正在建立这台设备的安全访问权限。",
    },
    expired: {
      title: "本次绑定已过期",
      description: "浏览器确认已超过有效时间。",
    },
    canceled: {
      title: "本次绑定已取消",
      description: "这次操作没有更改设备绑定状态。",
    },
    invalidated: {
      title: "绑定已失效",
      description: "这台设备的远端绑定已被解除。",
    },
    remoteUnavailable: {
      title: "网络连接失败",
      description: "桌面端无法连接 onecrew_lite 同步服务。",
    },
    rateLimited: {
      title: "操作过于频繁",
      description: "当前来源的绑定尝试已达到短时上限。",
    },
    deviceNameUnavailable: {
      title: "无法读取电脑名称",
      description: "请确认系统设置中的电脑名称可用，然后重新发起绑定。",
    },
    deviceNameInvalid: {
      title: "电脑名称不符合绑定要求",
      description: "请在系统设置中使用非空、最多 80 个字符的电脑名称，并移除控制字符和文字方向控制符。部分表情会占用多个字符。修改后请重新发起绑定。",
    },
    error: {
      title: "无法完成绑定",
      description: "onecrew_lite 未能安全确认本次绑定。",
    },
  },
  actions: {
    begin: "在浏览器中登录并绑定",
    reopen: "重新打开浏览器",
    restart: "重新开始",
    rebind: "重新绑定",
    retryNetwork: "重新检测网络",
    retry: "重新检测",
    cancel: "取消本次绑定",
  },
  progress: {
    checking: "正在检查绑定状态",
    callback: "正在校验浏览器回传",
    enrolling: "正在完成设备绑定",
  },
  alerts: {
    expired: {
      title: "绑定未完成",
      description: "请重新开始，并在浏览器中完成新的绑定流程。",
    },
    canceled: {
      title: "绑定未完成",
      description: "如需继续，请重新开始绑定。",
    },
    invalidated: {
      title: "设备需要重新绑定",
      description: "原有同步凭据已失效，重新绑定后才能继续使用 onecrew_lite 与 MCP。",
    },
    remoteUnavailable: {
      title: "请检查网络或代理",
      description: "请确认全局代理、TUN 或当前网络可以访问 onecrew_lite；网络恢复后再重新绑定。",
    },
    rateLimited: {
      title: "请稍后再试",
      description: "冷却期间不会再次打开网页授权；稍后重新检测即可。",
    },
    deviceNameUnavailable: {
      title: "无法读取电脑名称",
      description: "请确认系统设置中的电脑名称可用，然后重新发起绑定。",
    },
    deviceNameInvalid: {
      title: "电脑名称不符合绑定要求",
      description: "请在系统设置中使用非空、最多 80 个字符的电脑名称，并移除控制字符和文字方向控制符。部分表情会占用多个字符。修改后请重新发起绑定。",
    },
    error: {
      title: "绑定未完成",
      description: "请重新开始；如果问题持续，请稍后再试。",
    },
  },
} as const;

export default pairing;
