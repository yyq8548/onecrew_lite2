# Discovery 原生合同测试资料

来源为 GitHub `yyq8548/onecrew_lite` 0.7.1，基准提交 `efc3c6eb69507b9809bb30279d9af26156d075e3`。以下资料由原 Rust 生产纯函数或原真实 SQLite 测试仓储导出，只包含公开测试输入，不包含用户账号、凭据或浏览器状态。JSON 排版经过调整；已使用成熟 JSON 解析库逐值核对导出文件与冻结文件完全一致。

在 `onecrew_lite-rust` 下使用锁定依赖运行 `cargo test --locked -p <package> <test> --lib`。对应测试读取环境变量指定的工作区输出文件；导出源码保留在原测试模块中。常规 Node 测试直接读取本目录，无须重新运行 Rust。

| 冻结文件 | package / test | 输出环境变量 | 冻结文件 SHA256 |
| --- | --- | --- | --- |
| discovery-search-plan-rust.json | onecrew-local-runtime / export_discovery_search_plan_node_oracle | ONECREW_DISCOVERY_SEARCH_PLAN_ORACLE | 22075945dacabf50fb8deb90f4dd0d183d8691c3244054a714dc01583f8d1858 |
| discovery-native-search-rust.json | onecrew-mcp-adapter / export_native_search_node_contracts | ONECREW_NATIVE_SEARCH_NODE_CONTRACTS | f239f6a037f3bab96fcad8d59ad61b78abcc8e89d95373b8cc99a8f5ee3cabb8 |
| discovery-library-rust.json | onecrew-creator-runtime / export_discovery_library_node_oracle | ONECREW_DISCOVERY_LIBRARY_NODE_ORACLE | f03db29825297b8f4562d6c28daa2a4616c6aefc186f00b9d90afea0978086db |
| discovery-review-rust.json | onecrew-creator-runtime / export_discovery_review_node_oracle | ONECREW_DISCOVERY_REVIEW_NODE_ORACLE | bf6310cd27d8f2d944d10719d908f0db66709b1f72c1491c1221d6366d735ac3 |
| discovery-qualification-rust.json | onecrew-creator-runtime / export_discovery_qualification_node_oracle | ONECREW_DISCOVERY_QUALIFICATION_NODE_ORACLE | d7be954ce4e0e755d895d2b1ea3a4f9e2746f6476f96ad8488e0d58604c2f98f |
| discovery-saved-profiles-rust.json | onecrew-creator-runtime / export_discovery_saved_profiles_node_oracle | ONECREW_DISCOVERY_SAVED_PROFILES_NODE_ORACLE | f5544ed59faa3e186a092b7c9836bae32f14a79091c5fd9ecfb47647758990d4 |

SQLite 身份审核和联系人资料的原生生成步骤及二进制哈希分别见 `discovery-identity` 和 `discovery-contacts` 目录内的来源记录。

`discovery-match-rust.json` 来自 `onecrew-local-runtime::discovery_match::export_discovery_match_node_oracle`，输出环境变量为 `ONECREW_DISCOVERY_MATCH_NODE_ORACLE`。原测试实际构建已认证的公开测试资料，再调用生产 prepare 分别导出自动评分、重新评分的请求键及完整提示。冻结资料保留完整原材料和两个键，将重复的完整提示替换为其 SHA256，并保留实际目标 `Shoe reviewers`。完整导出 SHA256 为 `db80c3ac26e2f701c39d94e4819f12c356b6d73876bc1b4086400ca9aacc0dd1`，冻结 JSON SHA256 为 `bab2046e4c042856ba0c9458751c16e35bf266001fb21abc05d01c3ce82d8a04`。测试重建完整生产提示并对比哈希，无模型替身或网络读取。
