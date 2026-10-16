# 迁移前已发布版本基线

记录时间：2026-10-15。这是此前实际发布的记录摘要，本轮分析未重新部署。

| 范围 | 当前记录 |
| --- | --- |
| Mac | onecrew_lite 0.7.1 r4，Apple Silicon，macOS13.5+，Library0.1.4 |
| Windows | onecrew_lite 0.7.1 r3，用户指定包，完整源码未核实 |
| Pro | 0.1.8，独立产品，迁移不得改变 |
| Access Worker | 92154874-dc68-4fb9-a763-d42c80ac4fd6 |
| Access部署源码 | 3a40933f19c29de413687c4f839699de90bb7d9a |
| Mac源码 | 8cedccf2a29433728e9c81f052c9d485df39a1ad，已合入公司main26eefcab |

## 已完成的修复和验收

Mac r4包括Library登录和账号一致性预检、明确搜索补词错误、Apple iPhone整体语义、网站及包内下载安装/授权说明。Mac原生构建与12项隔离安装、MCP、插件、重复安装、停止服务检查通过。真实Codex语义及真实Library未登录检查通过；未执行同事电脑全部故障复现。

两平台生产中英文私人指南、HEAD、完整GET均HTTP200，ZIP大小和SHA256通过。后台返回Lite双平台，Pro保持三个平台。Windows r3未在本机执行EXE，不宣称包含本轮Mac新修复。

Mac ZIP：86,122,281字节，SHA256 `e3965841c90f201bafd5db9ba967138bfcc58e81808522ccc815f72d7432c09a`。

Windows ZIP：85,563,372字节，SHA256 `48df24ba9ae3932139b4f852ba8a582edea217849b78cd97770cabe61923d28a`。

## 公司记录

- [Access MR !5](https://git.digifinex.org/onecrew/onecrew-access/-/merge_requests/5)：网站安装提示词、双平台汇总、修订包支持；已合并部署，描述已同步实际发布结果。
- [客户端 MR !1](https://git.digifinex.org/onecrew_lite/onecrew_lite/-/merge_requests/1)：0.7.1及Mac修复源码；已合并，描述已同步r4发布结果。
- [Access MR !6](https://git.digifinex.org/onecrew/onecrew-access/-/merge_requests/6)：r4清单、来源、验收与回退记录，已合入lite（ff6da0c）。

Access实际部署同时保留公司Pro基线；不要只部署尚未包含整合内容的分支。新电脑需要重新检查远程最新提交与生产版本。

## 保留的限制

Mac Developer ID签名与公证、Windows签名分发未完成。原发布记录未重跑全部历史测试；构建存在已记录的非失败警告。Windows r3源码来源仍待核实。统一程序开发及Windows隔离候选验收见[开发执行记录](09-implementation-status.md)，未执行生产安装或部署。

## 旧版本回退资料

旧Mac对象保留：`onecrew_lite/releases/0.7.1/onecrew_lite-0.7.1-aarch64-apple-darwin.zip`。旧Mac清单SHA256 `e085fcc2be24e53546a2cb5b5e630fe694a1297d12a1a66e8911c635a1738128`。发布机保存旧清单与ETag，在 `outputs/onecrew_lite-0.7.1-distribution-verification/mac-r4-production/`。Windows旧清单位于同级r3-production目录。

这些本机备份和私人指南不在本GitHub源码中。发布前必须建立新机自己的受保护备份并验证，不能假设旧机绝对路径可用。恢复旧清单使用条件写入，保留新旧ZIP；数据库回退必须处理新版写入，不能无条件覆盖。Worker旧版本参考c689f22d-74d9-4eb4-b48b-876fb0d08a2d，执行前先确认是否已有后续公司部署。
