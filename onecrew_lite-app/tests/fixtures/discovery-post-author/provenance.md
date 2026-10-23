原 GitHub 0.7.1 CreatorRuntime `discovery_identity::tests::export_discovery_post_author_node_oracle` 复用原 `setup(10)` 与 `success`，通过原生产仓储生成三阶段 `VACUUM INTO` 快照。仅含原公开测试材料，不含用户数据、凭据或浏览器状态。三个 SQLite 文件直接复制，未修改或裁剪。

生成：在 `onecrew_lite-rust` 执行 `cmd /c ..\.scratch\discovery-post-author-oracle.cmd`。核心命令 `cargo +1.98.0-x86_64-pc-windows-msvc test --locked -p onecrew-creator-runtime export_discovery_post_author_node_oracle --lib`；环境变量 `ONECREW_POST_AUTHOR_ORACLE_DIRECTORY` 指向独立工作区目录。

SHA256：

- before.sqlite: `193eacd4ef9f6007f756a072da2e95f669ff11f7aaa0497c169acd7ba0882156`
- claimed.sqlite: `254e65fe5cf025da50589bbd6f372c44a13b513911d147f972c873536145f812`
- after.sqlite: `b7ba525cb9ac70ffb75ec6e50f91819c26cb3dd03a1610d409209a09d8fba6ce`
