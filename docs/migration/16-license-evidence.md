# 候选包许可全文证据

更新日期：2026-10-19。适用于当前 TypeScript candidate-87 / R8.1；许可全文和声明分别记录，不将声明计为全文。

## 已补齐的三项

| 依赖 | 原始全文 | 核验方式 |
| --- | --- | --- |
| `isarray@1.0.0` | npm 发布包的完整 `README.md`，含 MIT 全文 | 发布版本精确匹配，完整文件 SHA-256 为 `ff138e683771b187f3629c383db72ee7d632009010a36d08e18e8d2a34222ec7` |
| `@wreq-js/binding-darwin-arm64@3.2.0` | 同版本 `wreq-js` 父包的 `LICENSE` | 父包以精确版本声明该可选依赖，双方仓库和 MIT 声明一致；全文 SHA-256 为 `f5e211eaa1c732f23cae866f00c7a0d9f458cbb6e37051170a3f7bb45c2e5d8e` |
| `saxes@5.0.1` | 官方 v5.0.1 对应提交的完整 `LICENSE` | 提交为 `6ef6a275b20f19cb67808daf0d8f64e06aaf5b0e`；全文 SHA-256 为 `0fac2374380621b22e6b50451057721a9c52935b02d16d106a9f04897f061d0e`，保留三段历史许可及原始换行 |

来源分别为 [isarray 发布包](https://registry.npmjs.org/isarray/-/isarray-1.0.0.tgz)、[wreq-js 发布包](https://registry.npmjs.org/wreq-js/-/wreq-js-3.2.0.tgz)和 [saxes 固定提交全文](https://raw.githubusercontent.com/lddubeau/saxes/6ef6a275b20f19cb67808daf0d8f64e06aaf5b0e/LICENSE)。

[supplemental-license.ts](../../onecrew_lite-app/src/packaging/supplemental-license.ts) 只匹配已核验的包名和版本，读取前校验文件类型及全文哈希。wreq 规则覆盖对应版本的 Mac ARM、Mac Intel 和 Windows x64 平台子包；当前版本实际验证在 Windows 完成，Mac 双架构原生验收仍未完成。

构建将全文复制到组件的 `LICENSE.verified`，在 `runtime-manifest.json` 的 `licenseProvenance` 保存包内路径、固定来源和 SHA-256。清单校验要求来源证据指向该组件的实际许可文件，并与包内文件哈希一致。历史候选清单可以没有此可选字段；已提供的错误证据会明确失败。

boolbase@1.0.0 与 buffers@0.1.1 的对应许可材料也已补齐并纳入打包哈希校验，来源规则及固定哈希保存在 supplemental-license.ts；当前缺口以 candidate.json 为准。

## 保留的十一项缺口

| 类别 | 依赖 | 尚缺材料 |
| --- | --- | --- |
| 第三方 | `@hono/zod-validator@0.7.6`、`kysely-pglite-dialect@1.3.1` | 已核对的发布包及对应历史源码中尚未取得许可全文 |
| 第三方 | `binary@0.3.0`、`chainsaw@0.1.0` | 发布目录缺少全文，原官方仓库无法取得全文 |
| 第三方 | `react-remove-scroll-bar@2.3.8` | 尚未取得与该发布版本对应的完整许可文件 |
| 第三方 | `snowball-stemmers@0.6.0` | 内部 Snowball COPYING 的覆盖范围未能证明包括整个 npm 包；当前主分支与 npm 声明不能相互替代 |
| 内部包 | `@onecrew/contracts@0.1.1`、`@onecrew/runtime-contracts@0.7.1`、`onecrew-web@0.7.1`、`onecrew_lite-app@0.0.0`、`onecrew_lite_tables@0.1.4` | 对应包的发行许可全文或权利方明确授权材料；第三方通知不能替代本包许可 |

以上继续标为 `declared_only`。当前候选仍为 `productionReady: false`，不使用其他版本的许可或自行编写的文本消除缺口。

## 真实验证

当前 Windows candidate-87 的 candidate.json 明确列出上述 11 项缺口，productionReady 为 false。相关许可补充测试、真实 ZIP 清单校验、原生运行和 Codex 插件检查已执行；R8.1 完整包的 525 个文件逐一校验通过。当前 Windows 的实际范围见 [配套验证记录](28-sync-and-table-loading-windows.md)，不沿用历史 Mac 候选的结论作为当前平台验收。
