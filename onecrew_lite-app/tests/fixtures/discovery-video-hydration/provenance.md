# 原生 YouTube 视频读取测试资料

来源为 GitHub `yyq8548/onecrew_lite` 0.7.1，基准 `efc3c6eb69507b9809bb30279d9af26156d075e3`。仅包含原审核测试使用的鞋子话题、公开 YouTube 定位符与原测试 channel ID，不含用户账号、令牌或浏览器状态。

原测试入口：`creator-runtime/src/discovery_profiles/tests.rs::export_discovery_video_hydration_node_oracle`。在 Rust 工作区设置 `ONECREW_VIDEO_HYDRATION_ORACLE_DIRECTORY` 为专用工作区目录，再运行 `cargo test --locked -p onecrew-creator-runtime export_discovery_video_hydration_node_oracle --lib`。原 CreatorRuntime 执行真实搜索入库、付费视频领取、候选评估、完成回执和身份绑定事务；三个阶段由 SQLite `VACUUM INTO` 导出，二进制直接复制冻结。

| 文件 | 阶段 | SHA256 |
| --- | --- | --- |
| before.sqlite | 搜索已完成，尚未领取视频 | 4f868c2eb2f6b9562b5c52748dfa1e93130aeb96434bf85fd304f5b0f06409a0 |
| claimed.sqlite | 视频读取已付费并领取 | db1a2bc9e6a90a55f83b57a9e587806ae79731085aaf4f79756857b415f467c0 |
| after.sqlite | 候选、完成和身份绑定共同提交 | 6a77ce0578ee0e7c0a21d945c680fa34d17a96b5b92c5c7897114d449883c075 |

Node 测试用 after 数据库恢复原候选输入，移除仅由仓储计算的 assessment，再在 before 副本执行实际新仓储。比较原身份、资料和评估证据；候选 UUID、创建与更新时间由各运行独立生成。没有提供方替身或真实平台读取。
