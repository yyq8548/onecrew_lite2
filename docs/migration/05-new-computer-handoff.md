# 新电脑环境与开发接手流程

## 1. 获取正确源码

本仓库私有，先在新电脑完成自己的GitHub登录或SSH授权。不要从旧电脑复制token、Cookie或密钥。以下命令适用于安装了Git的Mac终端与Windows PowerShell；选择一个新的目录。

```sh
git clone --branch codex/typescript-migration-handoff --single-branch https://github.com/yyq8548/onecrew_lite.git onecrew_lite-migration
cd onecrew_lite-migration
git status --short
git log -1 --oneline
git merge-base --is-ancestor 26eefcab4dab3b836937c2922ec33789c6c10975 HEAD
```

首次克隆的状态应为空。最后命令应退出0。记录实际HEAD到本机接手日志；后续若推送新文档，HEAD可以变化，但基线祖先必须保留。

准备Node26.7.0后执行：

```sh
node docs/migration/verify-handoff.mjs
```

验证器只读Git树、当前源码和说明文件，核对已知源码目录与发布基线一致，不访问账号或生产服务。未来开始改动运行代码后，该验证器会报告基线不同，这是它检查“原始交接源码”的范围；届时改用实际迁移测试验证，不通过修改期望值隐藏差异。

## 2. 环境清单

| 工具 | 基线要求 | 用途 |
| --- | --- | --- |
| Git | 支持常规分支与完整克隆 | 源码与来源核验 |
| Node.js | 26.7.0 | 当前Library SEA与原生候选构建 |
| pnpm | 10.32.1 | Library workspace与锁文件 |
| npm | 随匹配Node提供 | 旧工作台/MCP UI的npm ci |
| Rust | 1.98.0，rust-toolchain.toml | 对照构建旧版Rust |
| uv | 现有workflow固定0.8.22 | Python构建辅助依赖 |
| CMake/Ninja/NASM/LLVM | 满足现有native脚本检查 | TLS及其他原生依赖 |
| Mac Xcode CLT | 对应本机SDK | Mac编译与系统工具 |
| Windows VS C++ Build Tools + SDK | x64 MSVC环境 | Windows原生构建 |
| Codex与Chrome | 已安装并分别授权 | 真实宿主/语义/浏览器验收 |

不要把旧机器的绝对工具路径写入仓库或当作新机默认路径。工具缺失时按官方安装方式准备；不要关闭安全保护。先记录 `node --version`、`pnpm --version`、`rustc --version` 与本机CPU/OS。

Mac：Rust target须匹配本机，ARM为aarch64-apple-darwin，Intel为x86_64-apple-darwin。Windows使用x86_64-pc-windows-msvc。交叉生成包不能替代原生运行验收。

## 3. 建立独立开发分支

```sh
git switch -c migration/typescript-runtime
```

若该分支已经存在，读取其当前状态后继续，不重置已有工作。所有中间文件置于仓库已有忽略的 `.scratch/` 或子项目 `.cache/`，不要使用 `/tmp`。Mac构建/测试时按需要将TMPDIR指向该专用目录。运行环境变量仅用于对应进程，不覆盖HOME或CODEX_HOME。

当前任务是迁移开发，日常安装、真实数据库、Cloudflare和公司共享分支都不是默认测试目标。

## 4. 重现已知基线

先阅读 `onecrew_lite-rust/AGENTS.md` 及任务涉及目录的规则。现有原生构建主入口是：

```sh
cd onecrew_lite-rust
node development/scripts/workspace-release.mjs build --output dist/native-candidates
```

此入口要求Git工作区干净，负责当前系统的配套主程序和Library候选；不自动生产发布。详细要求以 `development/NATIVE_RELEASE.md` 和脚本实际参数为准。源码仍有旧命名文档，遇到不一致以当前脚本、manifest和本交接记录核实。

单组件开发可以分别使用：

```sh
npm --prefix onecrew_lite-rust/apps/web ci
npm --prefix onecrew_lite-rust/apps/mcp-ui ci
pnpm --dir onecrew_lite-library install --frozen-lockfile
pnpm --dir onecrew_lite-library build
```

上述命令从仓库根目录执行。旧Rust完整单目标构建从 `onecrew_lite-rust` 运行 `bash development/scripts/build.sh --release --target aarch64-apple-darwin`（仅Mac ARM示例；按实际机器选目标）。Windows原生使用workspace-release入口和MSVC开发环境，避免套用Mac命令。

现有GitHub候选工作流为 `.github/workflows/workspace-candidates.yml`，手动触发、包含Windows与两种Mac runner，输出pending-native-acceptance候选。它的存在不表示已在本次交接运行或成功。

基线构建和测试应使用隔离数据目录。完整业务基线检查按现有开发文档执行，记录失败及环境原因；不通过fixture/mock结果声称已完成真实平台验收。新迁移验收必须按本计划使用真实服务与授权账号。

## 5. 第一轮开发顺序

1. 完成M00：确认所有源码和契约，建逐功能台账，标记Windows r3来源缺口。
2. 完成M01：先做Chrome、X网络、Telegram、X Chat四项原生实证，锁定依赖。
3. 完成M02：固定旧CLI/Web/MCP/表格同步契约与兼容窗口。
4. 按M03建立新TypeScript工作区，复用Library基础，先启动和停止真实服务。
5. 按M04/M05验证旧数据和统一身份，再逐业务迁移；不要先做漂亮界面再处理数据。

每次交付记录“改了哪些模块、实际检查、未完成项、下一步”。功能台账的实现、单元检查、集成检查、实机检查应分别记录。

## 6. 公司仓库与线上边界

需要查阅Pro或修改网站时，使用新机自己的公司GitLab权限：

```sh
git clone --branch onecrew-pro git@git.digifinex.org:onecrew/onecrew.git onecrew-pro-reference
git clone --branch codex/release-lite-r3-preserve-pro git@git.digifinex.org:onecrew/onecrew-access.git onecrew-access-production-reference
```

上面是参考检出；分析固定提交见source-ledger，后续公司分支可能更新，必须重新核对差异。Access的发布记录另在lite分支MR !6。没有权限时可继续本地Lite迁移，网站发布相关工作保持阻塞；不能把内附旧快照当成生产替代。

Cloudflare、GitLab、GitHub、Apple签名和Windows签名配置均由新机独立授权。不要把私有配置加入commit。业务源码迁移完成、验收通过、获得对应发布授权前，不修改生产。

## 7. Windows 特别说明

线上r3与已交接Rust源码不是已证明一一对应的构建。需要Windows同事提供实际源码commit/分支、锁文件和构建记录；原r3 ZIP的SHA256在source-ledger中。仅有ZIP能做升级输入检查，不能补全源码审计或可重建证明。

新电脑若是Windows，可优先完成M01的平台验证及M12安装/服务/重启用例；Mac ARM与Intel仍要在各自系统验收。不要用此前某个旧Windows包的成功记录替代新构建的验收。

## 8. 完成与交付

开发结束时交付源码、锁文件、生成物manifest、每平台真实验收、签名状态、官网兼容MR和回退说明。每个正式包必须能追溯到已合并源码；不能只上传ZIP不记录来源。详细门槛见M15–M17。
