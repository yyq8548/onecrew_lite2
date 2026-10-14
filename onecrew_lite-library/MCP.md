# OneCrew MCP 接入

OneCrew 是通用 MCP 服务。HTTP `/mcp` 和本地 `mcp` 命令提供同一套工具，Codex 插件只是连接配置与 skill 的可选分发包。服务端 instruction、工具描述、输入 schema 和业务校验不依赖插件。

## 当前入口

| 方式 | 客户端做什么 | 地址与生命周期 |
|---|---|---|
| stdio | 启动 OneCrew 的 `mcp` 子命令，通过 stdin/stdout 收发协议消息 | 从同一数据目录的 `run/server.json` 发现已运行服务，验证实例，随服务端口变化重新连接 |
| Streamable HTTP | 直接连接服务 URL 下的 `/mcp` | 使用 `status` 显示的实际地址；客户端保存的 URL 不会自动随端口变化 |

`onecrew mcp` 的含义是“以前台标准输入/输出协议模式运行 MCP 桥接进程”，不是安装插件、后台启动服务或执行一次搜索。通常由 AI 客户端启动和管理，不是给用户在终端交互输入命令的界面。退出客户端会结束它管理的桥接进程；独立运行的 OneCrew 服务仍由 `start/stop/restart` 管理。

桥接进程转发工具列表和调用，不打开第二份数据库，不自行启动服务，不打开浏览器、不自动登录。业务操作要求 OneCrew Access 登录；`account` 的 `status/login/logout` 可在未登录时调用。

命令帮助：

```sh
onecrew --help
onecrew mcp --help
onecrew start
onecrew status
```

通用 MCP 客户端配置的 command 填已安装 OneCrew 可执行文件的绝对路径；Windows 使用 `onecrew.exe`。四平台分发包的二进制位于根目录，Codex 插件只包含 skill 与连接配置。双击 `install.command` / `install.cmd` 会将文件复制到用户安装目录，配置插件路径并启动服务；之后从安装目录添加 marketplace 并安装插件。移动程序后，运行 `onecrew configure-plugin --plugin-dir <插件目录> --data-dir <数据目录>` 并重新安装插件，让 Codex 缓存使用新路径。服务地址仍由 `onecrew mcp` 自动发现。

## 数据目录

默认位置：macOS `~/Library/Application Support/OneCrewConnect`，Windows `%LOCALAPPDATA%\OneCrewConnect`，Linux `$XDG_DATA_HOME/onecrew-connect` 或 `~/.local/share/onecrew-connect`。`--data-dir` / `ONECREW_DATA_DIR` 可覆盖默认位置；MCP 和服务必须选择同一个目录。

此数据空间独立于旧版 OneCrew 和 `io.onecrewai.onecrew` 目录；安装或首次启动不会读取、移动或删除旧数据。首次使用新空间需要重新登录。

服务与 MCP 使用安装后的可执行文件和同一个数据目录。手工配置通用 MCP 客户端时使用可执行文件的绝对路径，带空格的路径作为一个参数；Codex 分发插件也通过配置在插件清单中的安装后二进制路径连接。

本地 stdio 连接在首次业务调用时绑定已登录账号。端口变化后可继续连接同一账号；退出并换成另一个账号后，旧连接会拒绝业务调用，需要在 MCP 客户端重新连接 OneCrew。`account` 状态/登录工具仍可使用。表格、搜索记录与浏览器连接按个人租户隔离；MCP 不接受任意 `tenant_id` 切换数据空间。

## 客户端配置

每个客户端选择一种方式；已有插件连接同一服务时，无需重复注册。

### Codex

stdio：

```sh
codex mcp add onecrew -- /absolute/install/path/onecrew mcp
codex mcp get onecrew
```

HTTP：先将 `ONECREW_MCP_URL` 设置为 `status` 显示的实际服务地址加 `/mcp`：

```sh
codex mcp add onecrew --url "$ONECREW_MCP_URL"
```

注册后按客户端需要重载工具或新建对话。[官方配置说明](https://developers.openai.com/codex/mcp)

### Claude Code

```sh
claude mcp add --scope user --transport stdio onecrew -- /absolute/install/path/onecrew mcp
claude mcp get onecrew
```

HTTP 使用 `claude mcp add --scope user --transport http onecrew "$ONECREW_MCP_URL"`。在 Claude Code 中通过 `/mcp` 查看连接。[官方配置说明](https://code.claude.com/docs/en/mcp)

### Cursor

在用户级 `~/.cursor/mcp.json` 合并这个条目，保留已有服务器：

```json
{
  "mcpServers": {
    "onecrew": {
      "type": "stdio",
      "command": "/absolute/install/path/onecrew",
      "args": ["mcp"]
    }
  }
}
```

HTTP 条目使用 `{"url":"<实际服务地址>/mcp"}`，替换整个 stdio 条目。[官方配置说明](https://cursor.com/docs/mcp)

### VS Code

通过 `MCP: Open User Configuration` 打开本机用户配置，合并以下条目。其顶层键是 `servers`：

```json
{
  "servers": {
    "onecrew": {
      "type": "stdio",
      "command": "/absolute/install/path/onecrew",
      "args": ["mcp"]
    }
  }
}
```

HTTP 条目使用 `{"type":"http","url":"<实际服务地址>/mcp"}`。OneCrew 在用户本机运行时，应使用本机配置，不要把它作为远程开发主机的工作区命令。[官方配置说明](https://code.visualstudio.com/docs/agent-customization/mcp-servers)

### Gemini CLI

```sh
gemini mcp add --scope user --transport stdio onecrew /absolute/install/path/onecrew mcp
gemini mcp list
```

HTTP 使用 `gemini mcp add --scope user --transport http onecrew "$ONECREW_MCP_URL"`。手动写配置时 Streamable HTTP 使用 `httpUrl`，不要误用对应旧 SSE 的 `url`。[官方配置说明](https://geminicli.com/docs/tools/mcp-server/)

### Claude Desktop

通过本地 MCP 开发配置编辑 `claude_desktop_config.json`，合并：

```json
{
  "mcpServers": {
    "onecrew": {
      "command": "/absolute/install/path/onecrew",
      "args": ["mcp"]
    }
  }
}
```

这属于本地 stdio 接入。Claude 的“远程连接器”从其云端发起请求，不能直接访问用户电脑的 `127.0.0.1`，不应在那里填本地 HTTP 地址。[本地接入说明](https://modelcontextprotocol.io/docs/develop/connect-local-servers)、[远程连接器说明](https://support.claude.com/en/articles/11175166-get-started-with-custom-connectors-using-remote-mcp)

## 登录与密钥

当前实现使用本地 Access 登录会话控制业务调用，尚未实现独立的 MCP API Key 签发、轮换或 Bearer 校验。不要把 Lev8 的 API Key 配置直接换名后宣称可用，也不要将 Access 设备凭据或 `server.json` 的进程控制 token 填入客户端配置。HTTP 模式当前仅面向本机。

## 没装 skill 时的使用与验收

服务端 instruction 介绍能力、登录与基本调用流程。具体工具描述包含以下必要约定：

- 搜索先 create，再按状态等待，completed 后 fetch、评价并写入一张新的结果表；默认保存，用户明确仅预览时才跳过。当前 agent 按流程调用独立工具，搜索接口不隐式建表。
- YouTube 固定先搜视频、按 channelId 去重、获取频道资料与近期内容；AI 按需求和返回证据判断匹配程度。
- 创作者搜索默认三列：people 名称与头像、website 频道链接、score 匹配评价。原始平台 identity 放在名称对象里，评价依据放 score.reason；除非用户要求，不追加来源、抓取时间等辅助列。
- 保存使用建表/查表返回的真实列 ID；未知信息用 null 或省略可选字段，不猜测头像、ID 或表现数据。
- 只报告已确认写入的记录，返回真实 web_url；写入结果不确定时先查询核实再重试。

可复制给任意已接入客户端的验收提示：

> 使用已连接的 OneCrew MCP，先调用 account 检查登录状态；若未登录，返回登录页面让我完成授权。登录后列出已有表格，验证连接。不要索取或输出密钥，不要寻找本机源码或自行启动其他服务。

进一步验收时，让 agent 查找指定主题的创作者，检查默认保存、标准三列、原始 identity、score.reason、写入条数和详情链接。继续查看同次结果不应重复建表。连接验证本身不创建测试数据、不退出账号。

skill 保留详细范例和长流程，作为可选增强。MCP resources/prompts 没有跨客户端自动加载 skill 的保证，目前不新增只为读取说明而存在的工具，也不复制第二套业务接口。
