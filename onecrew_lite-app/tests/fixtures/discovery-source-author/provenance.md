原 GitHub 0.7.1 CreatorRuntime `discovery_douyin::tests::export_discovery_source_author_node_oracle` 复用原 `setup/profile/content`，通过原生产仓储完成两条匹配视频的同一原生作者观察。第一条联合保存资料、内容及待审核候选；第二条仅永久保存重复作者关系。均为 `VACUUM INTO` 快照，五个文件直接复制，未修改或裁剪。没有用户数据、凭据或浏览器状态。

生成：在 `onecrew_lite-rust` 执行 `cmd /c ..\.scratch\discovery-source-author-oracle.cmd`；核心命令 `cargo +1.98.0-x86_64-pc-windows-msvc test --locked -p onecrew-creator-runtime export_discovery_source_author_node_oracle --lib`，环境变量 `ONECREW_SOURCE_AUTHOR_ORACLE_DIRECTORY` 指向独立工作区目录。

SHA256：

- before.sqlite: `eff7d50af07e1040fb1bc8d39f2038f706fd901055b1891c4fc2fd5ea71a98d5`
- claimed.sqlite: `7bbefc01c25678f07abfebe8e7159c87a3b9f9af15140be1d9e6792a6ca11c7d`
- completed.sqlite: `5d4c848d129c3f7e50b8eb9c98cb00e8f0fa9013683f9afc422312e0b0c93c89`
- duplicate-claimed.sqlite: `368dff0e60cb46a37aab9992be8691a442285243834a7f9ba874f757627e79ff`
- duplicate.sqlite: `8b9a45458ba5c3cab98ab9fd9ea6d7fb065cd9ba072c25bd1ea75f4a52e7cec9`
