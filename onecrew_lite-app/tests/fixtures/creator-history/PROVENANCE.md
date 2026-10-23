原 GitHub 0.7.1 CreatorRuntime schema90 的真实仓储测试产物，无用户数据或凭据。

源：`crates/creator-runtime/src/repository/sqlite/tests/reply_triage.rs::export_creator_history_node_oracle`。沿原 `dm_fixture` 初始化真实数据库，再执行原会话列表、待分类债务、最后联系账户和分类仓储方法。

生成：MSVC Rust 1.98，`cargo test --locked -p onecrew-creator-runtime export_creator_history_node_oracle --lib`；设置 `ONECREW_CREATOR_HISTORY_ORACLE_DIRECTORY` 到工作区专用目录。分类时间固定为 123，编号保留真实随机原产物。文件按原字节复制。

SHA256：

- baseline.sqlite：`427a9c988709a30af9fca64fde64a0448787e13b669551e994af0116de6234e3`
- expected.json：`4705d754c8405d3a4c9c8b31df5c73a7660c81bbd6156867db572e45ec444c24`
