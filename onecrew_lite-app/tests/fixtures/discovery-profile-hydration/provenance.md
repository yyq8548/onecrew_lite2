# 原生 profile 领取与完成测试资料

来源为 GitHub `yyq8548/onecrew_lite` 0.7.1，基准 `efc3c6eb69507b9809bb30279d9af26156d075e3`。仅包含原测试中公开的 Pinterest 作者、键盘话题和 example.org 联系资料，无用户数据、凭据或浏览器状态。

生成入口：`onecrew_lite-rust/crates/creator-runtime/src/discovery_profiles/tests.rs::export_discovery_profile_hydration_node_oracle`。从 Rust 工作区执行 `cargo test --locked -p onecrew-creator-runtime export_discovery_profile_hydration_node_oracle --lib`，并将 `ONECREW_PROFILE_HYDRATION_ORACLE_DIRECTORY` 设为专用工作区目录。三个数据库均由真实原 CreatorRuntime 操作及 SQLite `VACUUM INTO` 导出，直接复制冻结，未重写数据库内容。

| 文件 | 阶段 | SHA256 |
| --- | --- | --- |
| before.sqlite | 原搜索已提交，尚未领取 profile | 8304e9f3f666056200842fdb417b3fcd51fabe4f6294cb93f78f53c134e143c6 |
| claimed.sqlite | 原 profile 已原子计费并持久领取 | 4418221dc2945cb3528d0aa49fd305ffba7d659c06da85fab7a4fa7173d19763 |
| after.sqlite | 原 profile、候选和资格证据已共同提交 | 024ddc00891b7e73e58530fb86c9b409b42a17a63e869cd034836bf6ae591230 |

Node 测试使用真实 SQLite 副本执行实际新仓储，无网络替身。跨语言比较仅规范化运行时真实时钟产生的时间；候选 UUID 同样由各实现独立生成。原始作者、领取、计费、资料、资格结果和证据逐字段比较。
