# 搜索引擎原生来源对照数据

基线为 GitHub OneCrew Lite 0.7.1 `efc3c6eb69507b9809bb30279d9af26156d075e3`。导出测试位于 `creator-runtime/src/discovery_web_sources/tests.rs::export_web_source_node_oracle`，复用原文件的 `setup`、`material`、`twitter_material` 和 `douyin_material`，实际调用原 CreatorRuntime 创建、批准、搜索完成、原生领取及完成事务。没有平台调用。

Google 和 Yahoo 分别对照 YouTube、X、抖音三个原生目标，共六组；每组保存实际 SQLite 的 before、claimed、finished 三阶段以及原实际 claim/material/result JSON。文件哈希见 `sha256.json`。Node 测试默认执行所有冻结组，不依赖现场 Rust 编译，不跳过。

设置 `ONECREW_WEB_SOURCE_ORACLE_DIRECTORY` 到工作区专用目录后，执行 `cargo test -p onecrew-creator-runtime discovery_web_sources::tests::export_web_source_node_oracle -- --exact --nocapture`。原测试 1 项通过，导出六组真实 SQLite，使用 `VACUUM INTO` 保存一致视图。

Node 对照保留来源引擎、目标、1/3/2 次原生调用计费、完整原文、字段和永久作者绑定。独立执行的系统时间及新生成的 candidate UUID 按各自真实值核验；其他收据、候选自然键和证据逐字段比较。原 SQLite 文件不得为适应新实现而修改。额外测试检查预算、陈旧批准、未知领取不重复、原生风险隔离以及数据库触发器造成的整事务回滚。
