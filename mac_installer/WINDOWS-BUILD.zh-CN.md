# OneCrew Lite 0.9.0 · Windows 构建交接

源码分支：`codex/contact-collection-20261019`。本分支包含多平台优化和统一的 0.9.0 版本。`windows installer` 仍是历史 0.7.1 R8.3，未修改，也不代表已经包含本次代码。

已有仓库先确认无未保存修改，再执行：

```powershell
git fetch origin
git switch codex/contact-collection-20261019
git pull --ff-only origin codex/contact-collection-20261019
git rev-parse HEAD
```

新电脑可直接克隆该分支：

```powershell
git clone --branch codex/contact-collection-20261019 https://github.com/yyq8548/onecrew_lite2.git
```

先阅读根目录 AGENTS.md、development/README.md 和 [合并与部署流程](../docs/release/合并与部署流程.md)。固定 Node.js 26.7.0、pnpm 10.32.1，安装 Windows 原生依赖需要 Visual Studio C++ Build Tools。使用根目录锁文件：

```powershell
pnpm install --frozen-lockfile
pnpm check:independence
pnpm build:tables
pnpm typecheck
pnpm check:contracts
pnpm build:ui
pnpm test:app
pnpm test:opencli
```

先确认 Node 路径和版本，再提供真实的绝对路径构建 Windows 候选：

```powershell
$targetNode = (Get-Command node).Source
& $targetNode --version
pnpm build:candidate --output .artifacts/windows-x64-0.9.0-r1 --product-version 0.9.0 --target win32-x64 --node $targetNode --web-assets onecrew_lite-web/dist --library-web-assets onecrew_lite-library/web/dist
```

输出目录必须不存在。不要直接运行 development/package-windows-customer.mts：该历史脚本仍固定旧包的输入和哈希。制作新版用户 ZIP 时按 development/windows-customer-package 的既有安装流程更新版本、源码提交和实际哈希，并保留许可证、完整 SHA256SUMS 校验和独立插件验证。另用新文件名保存 0.9.0 成品；本次要求原 `windows installer` 目录保持不动。

Creator 数据已升级到 schema91，Vault schema4。升级沿用原数据目录、主密钥、端口、账号及日常 Chrome 身份。保留已验证成功的搜索方法，不因打包再次搜索各平台；Windows 验收只覆盖其安装、升级、服务、账号保留、配套插件及必要界面。

Facebook、Pinterest 已在 Mac 正常任务达到5位推荐；LinkedIn 角色筛选、Google/Yahoo 推荐数量和部分邮箱归属仍有已知限制，见 RELEASE-STATUS.md。没有 Windows 原生验收证据时保持 `productionReady=false`，不切换生产下载。
