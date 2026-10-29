# Lite 0.9.9 发布记录

任务 QB-3448。用户授权同步公司 GitLab 并部署网站。Mac 保留 ZIP 与 npm，Windows 只展示 npm，旧 windows_installer、Pro 和公司其他配置保留；不恢复搜索。

Mac 原生构建提交 `beb75290403b55fc062b1052e3816bf18df3bcbb`，Windows 原生构建提交 `dff061edb3d5f545ae4f22090e2ce081bd27f6f6`，工作区均干净。二者业务源码及锁文件一致，Mac 后续提交只更新分发说明。包和具体校验值见 `mac_installer`、`npm_installer/release.json`、`npm_installer/release-inputs.json`。

Mac 本地 npm 的入口、原生程序、后台服务、两个网页、live MCP 及授权会话检查通过，合计 15.292 秒。Windows 使用 GitHub `Windows npm native acceptance` 工作流检查实际 npm 成品；最终运行链接和结果将在完成后写入本记录。

Access 将 Windows npm 清单与旧 ZIP 清单分开；Windows 网页不再提供 ZIP。Mac ZIP/npm 与旧 Windows 0.9.1 下载兼容路由保留。当前安装使用当前对话模型推进规划与评分，保留身份、证据校验和已保存材料。

发布状态：本记录建立时尚未发布 0.9.9 npm，未切换生产下载入口。待公司 MR 合并、Windows 原生检查、网站内测及分发验证完成后，记录实际合并提交、部署版本、下载入口和公开验收结果。

Mac 使用 ad-hoc 签名且未公证；程序保留 productionReady=false 和许可证证据缺口原始列表。Windows 签名、账号登录与日常浏览器授权边界按实际验证记录，不声明未执行的检查已通过。

恢复时先核对当前线上状态，仅调整本次 Lite 分发与页面，保留同事的新变更、Pro、账号数据、原表格和结果。流程入口为 `AGENTS.md` 与 `docs/release/合并与部署流程.md`。
