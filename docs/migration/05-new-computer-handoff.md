# 新电脑环境与接手流程

当前源码已包含 TypeScript 统一程序。先阅读 [当前状态、完整验证和下一步计划](14-handoff-status-and-next-plan.md)，不要从原始迁移基线重新实施。源码交付后按用户要求暂停；后续实现需用户明确恢复。

## 获取源码

本仓库私有，在新电脑使用自己的 GitHub 登录或 SSH 授权。不要复制旧电脑的 token、Cookie、Chrome 身份或客户数据库。选择新的工作目录：

```powershell
git clone --branch codex/typescript-unified-gmail https://github.com/yyq8548/onecrew_lite2.git
cd onecrew_lite2
git status --short
git log -1 --oneline
git fsck --full --no-dangling
git merge-base --is-ancestor efc3c6eb69507b9809bb30279d9af26156d075e3 HEAD
```

首次克隆应无本地修改，核对远端实际提交。`verify-handoff.mjs` 仅校验原始未修改基线，不能用于判定当前迁移实现正确，也不能修改其期望值掩盖差异。

## 环境与构建

| 工具或材料 | 要求 |
| --- | --- |
| Node.js | 26.7.0，与 SEA 模板和原生依赖一致 |
| pnpm | 10.32.1，使用根 workspace 与锁文件 |
| Windows C++ 工具 | 匹配原生依赖的 Visual Studio Build Tools / SDK |
| Rust | 重新生成来源对照或构建旧程序时，使用原项目固定工具链 |
| OpenCLI / Chrome | 实际安装 OpenCLI 1.8.8，官方扩展和用户批准的专用 Chrome |
| Codex | 实际官方 CLI、宿主和独立授权的账号 |
| Mac | ARM 与 Intel 各需真实原生环境；尚未验收 |

依次执行 [当前构建与测试命令](14-handoff-status-and-next-plan.md#新电脑构建与验证)。主程序、MCP UI、工作台和 Library 必须同一来源配套构建，Library 使用 `VITE_ONECREW_UNIFIED=1`。每次构建写入新的忽略目录，不覆盖已有验收产物。

## 本机验收

1. 记录实际 Git 提交、工作区修改、工具版本、目标平台、程序和 payload 哈希。不要依赖旧电脑端口或版本号判断一致性。
2. 执行类型检查、主程序完整套件、工作台和适配器测试；真实原生测试显式提供候选、旧版、Rust 对照和实际安装组件。逐项记录失败及跳过。
3. 使用隔离的私有目录检查冷暖启动、权限、中文长路径、父进程退出、并发、缓存损坏和旧版替换。不得借测试控制其他账号的服务或任务。
4. 官方安装配套 Codex 插件并在需要时完整重启宿主。核对实际加载程序哈希，再使用正式 live MCP 回读。
5. 复用 Codex 内置浏览器检查工作台与当前 Library 表格，检查专用 Chrome 真实绑定。状态卡、工作台和同计划表格须联合验收；缺失或无法观察保持未完成。
6. 只在用户批准的计划和读取预算内执行平台搜索。未获具体发送授权，不执行 Gmail 发送；不接受远端同步协议代替用户同意。

## 数据和发布边界

保持账号、Creator SQLite 与 Library PGlite 原归属；旧账号不得自动合并。当前 `onecrew_lite-access` 是旧快照，未核实公司当前源码和发布权限前不部署。16 项许可全文、Mac 原生验收、状态卡视觉和正式发布条件见当前交接记录。

交接者完成本机可验证事项后更新实际结果和下一步，未经用户恢复授权不启动后续开发、平台搜索、生产迁移或发布。开发清理仅限确认不用的旧构建及缓存，保留当前、上一版和正在运行、验证或必要回退的产物。
