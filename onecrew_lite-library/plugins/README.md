# OneCrew Codex 插件

这是通用 OneCrew MCP 的可选 Codex 分发包，包含连接配置和 skill。直接配置 Codex、Claude Code、Claude Desktop、Cursor、VS Code 或 Gemini CLI 请看 [MCP 接入说明](../MCP.md)，不需要安装本插件。

插件源码与服务在同一仓库：`plugins/onecrew-tables`；marketplace 在项目根 `.agents/plugins/marketplace.json`。

本地源码调试，在项目根执行：

```sh
pnpm install
pnpm build
pnpm plugin:configure
codex plugin marketplace add "$PWD"
codex plugin add onecrew-tables@onecrew-tables
```

配置命令会把当前项目 CLI 的实际路径写入插件 MCP 配置，开发服务使用项目 `.data`。该文件包含机器路径，不提交；移动仓库后重新生成并安装插件。四平台分发包使用独立的 `onecrew` 二进制，并提供 `configure-plugin` 脚本写入其实际路径。

启动服务后，新建 Codex 对话测试。MCP stdio 读取项目 `.data/run/server.json` 发现端口；不负责启动服务。Codex 安装时复制到自己的缓存，这是客户端正常管理行为，不是另一份需要维护的源码。

## 四平台便携分发包

在 macOS 上从仓库根目录运行 `pnpm build:release`，生成 macOS arm64/x64 与 Windows arm64/x64 四个 ZIP，默认放在 `~/Downloads/OneCrew-<版本>-<提交短号>/`。只构建一个目标可运行 `pnpm build:release -- --target win32-x64`；用 `--output` 指定其他输出目录，服务端与网页资源已构建时可加 `--skip-build` 跳过 `pnpm build`。

打包器会下载官方 Node v26.7.0 的宿主 SEA 生成器和目标运行时，校验 `SHASUMS256.txt` 后使用；本机现有的 Node 25.8 可继续运行打包器，不需要全局升级 Node。脚本为每个平台生成独立二进制、Codex 插件、marketplace、Chrome 扩展、启动脚本和许可证文件，不会自动安装插件或扩展。

先双击 `install.command` / `install.cmd`。安装程序会把文件复制到 macOS `~/Applications/OneCrewConnect` 或 Windows `%LOCALAPPDATA%\Programs\OneCrewConnect`，配置插件路径，然后启动服务并打印本机 URL。它不安装 Codex 或 Chrome 扩展，也不修改 PATH。若目标已有服务，会先用明确的 OneCrewConnect 数据目录停止服务，再复制；重复运行安装程序会更新并重新启动服务。Chrome 扩展会自动发现本机默认端口范围内的服务。

服务和 MCP 使用同一个独立数据目录：macOS `~/Library/Application Support/OneCrewConnect`，Windows `%LOCALAPPDATA%\OneCrewConnect`，Linux `$XDG_DATA_HOME/onecrew-connect` 或 `~/.local/share/onecrew-connect`。安装不会读取、移动或删除旧版 OneCrew 或 `io.onecrewai.onecrew` 数据；新空间首次使用需要登录。

安装后，从任意目录执行下面的平台对应命令。macOS 终端：

```sh
codex plugin marketplace add "$HOME/Applications/OneCrewConnect"
codex plugin add onecrew-tables@onecrew-tables
```

Windows PowerShell：

```powershell
codex plugin marketplace add "${env:LOCALAPPDATA}\Programs\OneCrewConnect"
codex plugin add onecrew-tables@onecrew-tables
```

本地市场固定指向 OneCrewConnect 安装目录，其中 `.agents/plugins/marketplace.json` 引用 `plugins/onecrew-tables`。不要在 Downloads 中执行 `marketplace add .` 注册解压目录；安装结束时打印的命令已包含正确的安装路径。

安装后可从安装目录运行 `./onecrew status`、`./onecrew stop` 和 `./onecrew start` 管理服务（Windows 使用 `onecrew.exe`）。若移动已安装程序，在新位置运行 `onecrew configure-plugin --plugin-dir <插件目录> --data-dir <数据目录>`，再重新安装插件，让 Codex 缓存使用新路径。直接为 Claude Code、Cursor、VS Code 等通用 MCP 客户端手写连接时也可使用安装目录内二进制的绝对路径，见 [MCP 接入说明](../MCP.md)。

Windows 安装窗口会暂停显示结果并保留退出码。macOS 程序未公证；若系统拦截，请到“系统设置 → 隐私与安全性”选择“仍要打开”。Intel 版在 Apple Silicon 上通过 Rosetta 运行验收，尚未在 Intel Mac 实机验收。Windows 两版通过静态架构检查，尚待运行时验收，且未代码签名。

Chrome 扩展使用 OneCrewConnect 安装目录中的 `browser-extension`。在 Chrome 打开 `chrome://extensions`，开启开发者模式，点击“加载已解压的扩展程序”并选择该目录。市场和扩展均从安装目录加载后，可删除 Downloads 中的解压包。`SHA256SUMS` 校验各 ZIP；包内 `manifest.json` 记录来源 commit、目标、Node 版本和内容文件散列。第三方与 Node 许可证位于 `licenses/`。
