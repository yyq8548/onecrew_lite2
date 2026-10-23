原 GitHub 0.7.1 CreatorRuntime 测试 `discovery_transcripts::tests::export_discovery_transcript_hydration_node_oracle` 复用原 `setup` 与 `completed`，通过原生产仓储生成三个 `VACUUM INTO` 快照。仅含原公开测试材料，不含用户数据、凭据或浏览器状态。

生成命令：在 `onecrew_lite-rust` 执行 `cmd /c ..\.scratch\discovery-transcript-hydration-oracle.cmd`。核心命令为 `cargo +1.98.0-x86_64-pc-windows-msvc test --locked -p onecrew-creator-runtime export_discovery_transcript_hydration_node_oracle --lib`，设置 `ONECREW_TRANSCRIPT_HYDRATION_ORACLE_DIRECTORY` 为独立工作区目录。三个 SQLite 产物直接复制，未修改或裁剪。

SHA256：

- before.sqlite: `dda19e09202ed3bd394ecbee84e7d53199be35fba39d9049a4ef3c98c158e194`
- claimed.sqlite: `149cb116037aa72fe273cd36891c0f071969222ab546454b77b6dcc40eaa104d`
- after.sqlite: `958bc7e83e83c4fdf7b6fa97d9089aae4b202b2c97f4e966555534cd565a46fc`
