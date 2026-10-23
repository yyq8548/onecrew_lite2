原 GitHub 0.7.1 CreatorRuntime `discovery_content::tests::export_discovery_content_hydration_node_oracle` 复用原 Bilibili `setup/success` 与 X `setup_x/x_success`，调用原生产仓储，生成读取前、付费领取后、完成后的 `VACUUM INTO` 快照。仅含原公开测试材料，无用户数据、凭据或浏览器状态。六个 SQLite 产物直接复制，未修改或裁剪。

生成：在 `onecrew_lite-rust` 执行 `cmd /c ..\.scratch\discovery-content-hydration-oracle.cmd`。核心命令 `cargo +1.98.0-x86_64-pc-windows-msvc test --locked -p onecrew-creator-runtime export_discovery_content_hydration_node_oracle --lib`，环境变量 `ONECREW_CONTENT_HYDRATION_ORACLE_DIRECTORY` 指向独立工作区目录。

SHA256：

- bilibili-before.sqlite: `d5a25698f3ce58bd64c0d1b39505ca575635cab01f722e8a9c322fd7ca936ae1`
- bilibili-claimed.sqlite: `0dc91f923154413382f71cfa296730425507309ff6b7d512c8c5d6cedb1aea50`
- bilibili-after.sqlite: `ca5c85bb12a555ac72229a9ef72b57ace0d293af57a12f3d993a4a99de30e02f`
- twitter-before.sqlite: `fe8cf727eaf810cafc34a254657d23f11fc2e68649a1fd81a9ebbc8fc945468d`
- twitter-claimed.sqlite: `a39ac4ed84888e0c18cecac35dfcf9e2267df40585041e2954c3ed5b223d695a`
- twitter-after.sqlite: `2fcf5085ff1be4af247ba6f21cddf1745bdcbc8a54c8a71b4e6f6bfcecc8e999`
