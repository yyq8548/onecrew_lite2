原 GitHub 0.7.1 CreatorRuntime `discovery_facebook_identity::tests::export_discovery_facebook_content_node_oracle` 复用原 `originals_ready` 和两条既有公开原文测试来源，通过原生产仓储生成 `VACUUM INTO` 快照。没有用户数据、凭据或浏览器状态，五个 SQLite 文件直接复制，未修改或裁剪。

生成：在 `onecrew_lite-rust` 执行 `cmd /c ..\.scratch\discovery-facebook-content-oracle.cmd`。核心命令 `cargo +1.98.0-x86_64-pc-windows-msvc test --locked -p onecrew-creator-runtime export_discovery_facebook_content_node_oracle --lib`，环境变量 `ONECREW_FACEBOOK_CONTENT_ORACLE_DIRECTORY` 指向独立工作区目录。

SHA256：

- before.sqlite: `390bd8e675a028338f0eb302603481b658a6dab3f4e986ede6760e84744e097c`
- claimed.sqlite: `f72364d7fa38f8dd0a3daa3971563598fff01e1e6ba4ce6b81fe412591dc7867`
- failed.sqlite: `478d38be4391cc88624ab24bc72b484a145cfd413f4cc3fad83a64c0f58929b2`
- second-claimed.sqlite: `f6032809f21977e66bf4aa721fe378d943fb58da264c579ff93a06f572e0fe0b`
- after.sqlite: `7b39105fc586abd3dc2f9816effb5530de39df1e2dbe8f5ea642d5e1c615f2d0`
