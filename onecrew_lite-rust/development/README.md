# 构建与上传

正式配套交付统一使用 [Windows / Mac 原生构建入口](NATIVE_RELEASE.md)：同一提交分别构建，汇总三种架构的主程序、内嵌插件和 Library；下方单组件命令保留用于开发与诊断。

交付物为内嵌网页和插件资源的原生 CLI。版本统一在 `release/version.json`。
运行 Python 辅助脚本统一使用 **uv**。

## 三个目标的构建

需要仓库指定的 Rust toolchain、Node/npm、uv。macOS 目标需要 Xcode
Command Line Tools；从 Mac 交叉构建 Windows 还需要 cargo-xwin、Clang/LLD
以及其 Windows SDK/CRT 依赖。TLS 原生依赖还需要 CMake、Ninja 和 NASM；
将 LLVM 的 `clang-cl`、`lld-link`、`llvm-lib` 与这些工具加入当前 shell 的 PATH。
绑定生成使用 LLVM 的 libclang，必要时设置 `LIBCLANG_PATH` 为 LLVM 的 `lib` 目录。
构建入口在交叉编译前检查这些命令，避免编译到原生依赖时才发现缺失。

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin x86_64-pc-windows-msvc
cargo install cargo-xwin --locked

# 自动读取 release/version.json，三个目标顺序构建，前端只构建一次
# 创建 dist/onecrew/<版本>/、三份二进制及 manifest、汇总 cli-manifest.json，并执行离线 R2 预演
bash development/scripts/build.sh --all

# 单独构建
bash development/scripts/build.sh --target aarch64-apple-darwin
bash development/scripts/build.sh --target x86_64-apple-darwin
bash development/scripts/build.sh --target x86_64-pc-windows-msvc

# 查看命令，不安装依赖、不编译
bash development/scripts/build.sh --all --dry-run
```

也可以在 Windows MSVC 开发环境中单独构建 Windows 目标。
本机使用 GNU 工具链时，可运行 `bash development/scripts/build.sh --target x86_64-pc-windows-gnu`；此目标仅用于匹配本机的开发安装，不加入 `--all` 发布清单。
输出为 `dist/onecrew/<版本>/<target>/onecrew`，Windows 文件名为 `onecrew.exe`。
同目录的 `manifest.json` 记录目标、版本、字节数和 SHA-256；生成时会检查
二进制头中的架构。`--debug` 仅用于开发，输出到独立的 `dist/onecrew/debug/`。

## 替换本机 CLI

构建完成后，用 manifest 替换本机 `~/.local/bin/onecrew`：

```sh
bash development/scripts/install-local-build.sh --library-dir <配套Library产物目录> # 先校验两份产物，再安装和复核
bash development/scripts/install-local-build.sh --library-dir <配套Library产物目录> --dry-run # 只校验并打印命令
bash development/scripts/install-local-build.sh --library-dir <配套Library产物目录> --plugin-install # 同步更新插件
```

### 安装、启动的一致性要求

Windows 共享服务由当前用户的按需计划任务启动，客户端不再用自己的临时 Job
托管服务。安装器先通过隔离任务实际执行候选程序的版本命令，再替换已有安装；
显式 `service host install` 注册宿主，`service host remove` 仅移除注册，均不自动
停止正在运行的服务。`status` 只读取现有服务，不触发启动。

打包的桌面宿主可能重定向 App Local Data。注册任务必须传递已解析的物理程序
路径和原数据路径；不能将终端中能访问的逻辑路径直接当作外部进程可见的安装位置。
独立宿主保持原安装环境，避免 Library/OpenCLI 使用另一套空目录。

本地安装脚本用 `.onecrew-upgrade.lock` 串行化安装，并在
`.onecrew-upgrade.json` 保留版本化的阶段、CLI/插件预期哈希、Library manifest
身份及失败历史。成功安装文件只标记 `installed_pending_live_verification`，不等于
live MCP 已加载。该锁不是旧版本客户端共同遵守的服务 owner lease，也不构成
跨进程、跨组件的原子事务；中断锁需先核对进程与日志后恢复，禁止盲删或自动降级数据库。

CLI、Codex 插件和 Library 必须来自本次配套构建。CLI 与 Library 是独立版本号，不要求数字相同；必须同时满足产物哈希、目标平台与同步接口能力一致。相同版本号不能替代构建哈希检查。

禁止仅更新 CLI 后沿用未经验证的旧 Library。安装器在替换 CLI 前验证配套 Library 的 manifest、文件大小、SHA-256 和必要能力；缺少配套产物或能力时立即失败。安装过程中任一组件失败，不得报告整个工作区更新成功。

启动同步前通过 Library 健康握手再次检查 `discovery-contacts-v2` 和 `recommendation-capture-v1`；旧 Library 应返回明确的升级要求，不能把“进程在线”当作“支持同步”。安装后必须确认 live MCP 的主程序哈希、插件安装版本、Library 实际接口，以及同一计划的结果回读。

搜索适配器也是运行验收的一部分。构建内嵌适配器发生变化时，已有 `~/.opencli/clis` 文件会被识别为冲突，启动不会自动覆盖。通过本次已安装 CLI 的 `opencli repair` 正式修复入口备份并更新配套适配器，再用 live `onecrew.discovery.sources.get` 核对预期平台及实际命令，并确认工作台冲突消失。不能仅核对 CLI、插件和 Library 哈希就宣称所有搜索入口已更新。

运行验收必须同时检查 Codex 状态卡、内置浏览器中的工作台和当前 Campaign 动态表格；确认连接、状态更新与结果一致。页面打开排队或仅有 URL 不算验收成功。卡片受宿主控制；宿主关闭或连接失效时明确报错并通过 `onecrew_web` 重新挂载，不声称能够阻止宿主关闭。

脚本会检查：

- manifest 的产品、版本、目标与 `release/version.json` 和本机 target 一致；
- 产物的 SHA-256 和字节数与 manifest 一致。

检查通过后，脚本才会用 manifest 中的版本、哈希和大小调用 `__bootstrap-update`。安装完成后，再核对已安装 binary 的 `--version`、哈希和大小。

可选参数：

- `--install-path` 指定其他安装位置；
- `--debug` 使用 `dist/onecrew/debug/` 下的产物。

插件安装后需要完全退出并重启 Codex。

升版后同步各包和锁文件：

```sh
node development/scripts/sync-release-version.mjs
```

## R2 上传

默认预演会验证文件及 manifest，只输出对象 key，不调用 R2：

```sh
uv run development/scripts/upload-r2.py \
  --release
```

实际上传时，在环境或权限为 `600` 的本地 `.env` 中配置 `R2_ENDPOINT_URL`、
`R2_ACCESS_KEY_ID` 和 `R2_SECRET_ACCESS_KEY`，再加 `--upload`。文件按数据解析，
不会作为 shell 执行，也不会打印密钥：

```sh
uv run development/scripts/upload-r2.py --release --upload \
  --env-file development/scripts/.env
```

默认 bucket 为 `onecrew-releases`，可通过 `--bucket` 修改。Endpoint 使用
R2 的官方 HTTPS S3 地址。对象名包含版本和目标，例如
`releases/0.5.21/onecrew-0.5.21-aarch64-apple-darwin`。
上传采用条件写入，已有对象必须与本地产物一致；每个对象完整 GET 回读并
校验大小和 SHA-256 后才报告成功。`--release` 要求三个目标齐全；三个二进制
全部回读通过后，最后上传 `releases/<版本>/cli-manifest.json`，缺少目标或任一
回读失败均不会发布总清单。上传器只处理 R2，不调用 Access，不创建 git tag。
Access 根据该私有清单签发短期安装入口，详见 [安装器模板](installers/README.md)。

## 相关文件

- `scripts/build.sh`：三个目标的构建入口。
- `scripts/binary-manifest.py`：目标架构与文件身份校验。
- `scripts/sync-release-version.mjs`：统一版本同步。
- `scripts/upload-r2.py`：预演、条件上传及回读。
- `schema-codegen/`：两个前端的 Rust 契约生成工具。

lint 和 Skill 配套目录及其检查入口保持保留。
跨平台编译不等于原生系统验证；每个目标仍需在对应系统上检查启动、登录、
网页、MCP、插件安装与正常停止。
