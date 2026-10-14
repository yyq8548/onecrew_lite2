# Windows / Mac 配套构建与统一交付

一个仓库、同一 Git 提交、Windows 和 Mac 两条原生构建流程。Mac 分 Apple Silicon 与 Intel 两个目标。主程序内嵌网页和 Codex 插件；Library 随同一工作区包交付，不单独选择版本。

## 构建候选包

在对应系统、对应架构的干净 checkout 中，从 `onecrew_lite-rust` 执行：

```sh
node development/scripts/workspace-release.mjs build --output dist/native-candidates
```

要求 Node 26.7.0、pnpm 10.32.1、仓库 Rust toolchain、uv、Git Bash（Windows）或 bash（Mac），以及原有构建脚本要求的原生 C/C++ 工具链。Windows 正式目标为 MSVC x64；现有 GNU 本地开发构建不作为此发布入口的候选。Mac 必须使用原生架构 Node，不用 Rosetta 冒充 Intel 实机。

入口拒绝未提交和未跟踪源码；构建前后核对提交和工作区状态。输出必须是新目录，不能覆盖旧候选包。没有 `--allow-dirty`、复用旧二进制或跳过校验的发布选项。

每个候选包包含：主程序、内嵌插件、Library、许可证、原有安装入口、逐文件哈希 manifest，以及 `release-build.json` 来源收据。此收据是构建追踪信息，不是签名或供应链证明；仅汇总受控构建机或本仓库 CI 的产物。

## 统一汇总入口

收集三台原生构建环境的目录，保留以下布局：

```text
bundles/
  win32-x64/
  darwin-arm64/
  darwin-x64/
```

```sh
node development/scripts/workspace-release.mjs collect --input bundles --output bundles/release-manifest.json
node development/scripts/workspace-release.mjs select --input bundles
```

`collect` 必须收到三个目标。它逐文件回读、核对目标与主程序/Library版本、源码提交、大小、SHA-256 和同步能力，拒绝混包、缺文件、多余文件及符号链接。`select` 按本机系统架构选择并重新校验整套包，只输出路径，不执行安装、不启动浏览器、不下载远端文件。

统一下载网站可使用 `release-manifest.json` 作为后续发布输入；当前入口只产生候选包，不连接现有 R2 上传器、不创建 GitHub Release，也不改客户更新地址。原有 CLI-only 发布脚本不能当作已完成配套发布的证据。

## CI

仓库根目录 `.github/workflows/workspace-candidates.yml` 仅手动触发。Windows 和 Mac job 构建同一个 workflow 提交；汇总 job 只在两个流程全部成功后运行。上传 CI artifact 前先用 tar 保留 Mac 可执行权限；最终 artifact 是待验收候选包，不是对外发布。

首次运行必须查看各平台原生工具链诊断。提交并推送之前，GitHub 不会看到本地 workflow；本地脚本测试通过也不代表 CI 已运行。

Runner 标签依据 [GitHub 官方 runner 说明](https://github.com/actions/runner-images/blob/main/README.md)，产物传输使用 [GitHub artifact actions](https://github.com/actions/upload-artifact)。这些环境会更新，首次及升级后的 CI 都需要实际回读验证。

## 安装、恢复与发布验收

安装前先运行 `select`，只使用其返回目录内的配套安装入口，保留上一份已验证完整安装包。程序、Library、插件任一步失败，整个更新标记未完成；不能只回退一个组件，也不能直接覆盖或回退客户数据库。先停止相关进程，再按已有安装机制恢复配套程序；涉及数据版本变化时沿用已有升级/恢复流程。当前不提供跨三个组件的自动事务回滚。

每个目标都在对应真实系统记录以下验收证据，证据绑定本次提交和清单哈希：

1. 全新安装、覆盖更新和一次失败后的恢复结果；确认账号、计划及表格保留。
2. 完全重启 Codex，回读主程序运行哈希、插件版本、Library 实际能力。
3. 状态卡、Codex 内置工作台和当前计划动态表格同时存在、连接正常；运行检查点和结束时都核对数据新鲜度。
4. 同步已有测试数据并回读，重复同步不增加重复记录。计划、平台登录及搜索授权由用户控制。

汇总清单始终标记 `pending-native-runtime`。通过上述人工验收后才进入另行授权的发布步骤；不得手改标记冒充测试、把编译通过写成发布验收通过。
