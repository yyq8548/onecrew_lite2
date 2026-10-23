# OneCrew Lite 0.9.1 源码同步

公司基线为 `main` 的 `5d3f495efa2861ed2751bb1b386d34fe88edbd8d`，导入来源为 GitHub `yyq8548/onecrew_lite2` 的 `1ac72b654eaf823df6c14ce512e34c6c43ddd8f2`。

TypeScript 项目位于仓库根目录，作为 Lite 唯一现行实现。按用户要求移除旧 `onecrew_lite-rust/`，以新版配套 `onecrew_lite-library/` 替换旧 Library，更新 Lite 构建入口与 CI。公司独立 `onecrew_lite-access/` 和其他服务配置保持原样。安装 ZIP、校验侧文件及本地客户数据不导入公司源码仓库。源码中的相对构建目录、锁文件及资源结构保持原样。

## 成品来源与验证

- Mac：`OneCrew-Lite-0.9.1-Mac-arm64-R32.zip`，61,806,735 字节，SHA-256 `61de9bf244c735d629e2f88888af69cf40241dec711b087ab2460e0fa49cb686`；实际构建提交 `ef229efc4d0d5a78c2a15dd27df541f8be7d5377`，程序 SHA-256 `0d2f311935b39cd27c991d67e05305e125711fb971d5566381a1a8cffb7c897f`。
- Windows：`OneCrew-Lite-0.9.1-Windows-x64.zip`，57,327,269 字节，SHA-256 `c6db2eeb7c2cf62db457c363b48be7d8db012fe5d9d1d9278139340fafb7feae`；包内记录的实际构建提交为 `ba8fe47be22f8cae7d0e525c52ded19cb5c4f658`，程序 SHA-256 `e119eb53bb66923a1b9e8fd989b9395d1c33ec52fded63a3389744b65f7f71cb`。

两个包均保留 `sourceDirty=true`、`productionReady=false` 的实际构建标志，未改写原成品身份。Mac 原生启动、MCP、服务停止后的修复、主密钥保留与网页资源校验已在发布电脑执行。Windows 原生安装、升级、数据保留和 MCP 验收采用提供方随包记录；其原客户端仍有重启加载插件的待完成项，不宣称已在 Mac 运行 Windows 原生检查。

主程序、当前对话模型规划和评分、身份与证据校验、成功资料复用、Instagram OpenCLI DOM 读取及一键修复源码均已保存。完整 0.9.1 程序构建所需的源文件、依赖锁、网页和插件资源与导入来源逐文件核对。

网站分发清单与安装指引在独立的 `onecrew/onecrew-access` MR 中维护。按公司流程完成合并后，再上传成品、完整回读校验和切换下载入口。用户账号、搜索及已有结果保持原状态。
