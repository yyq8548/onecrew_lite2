# 语言证据原始实现对照数据

来源为 GitHub OneCrew Lite 0.7.1 基线 `efc3c6eb69507b9809bb30279d9af26156d075e3` 的 `creator-runtime/src/discovery_review/language_tests.rs`。测试构造原生 CreatorRuntime、实际 SQLite、批准搜索、原始搜索和 profile 回执及语言条件审核，再执行原语言领取和完成事务。没有调用真实平台。

导出入口仅在 `ONECREW_LANGUAGE_ORACLE_DIRECTORY` 指向专用工作区目录时，将该原测试的实际数据库通过 SQLite `VACUUM INTO` 保存；普通原测试不会导出。命令为 `cargo test -p onecrew-creator-runtime discovery_review::language_tests::tiktok_language_reads_only_matched_post_once_without_commenter_ids -- --exact --nocapture`，执行结果 1 项通过。Node 测试不要求现场编译 Rust，不跳过这些冻结样本。

| 文件 | SHA256 |
| --- | --- |
| tiktok-before.sqlite | be34e800e5642b98b4bf4ab14e7ebb07d7951173483c98b8588e794305bf8dcd |
| tiktok-claimed.sqlite | dc1a1b8a460537a5c25a17ad233975c69027a7000f18661ebedfe92b54f81045 |
| tiktok-finished.sqlite | 7951fa9c970aeafcd3e72411eca33b6f2faa9727e346ec5d6b0ed9bf19b65ff6 |
| tiktok.json | c0857d2027d0412944ec627c81d3878d99cad054daece695d77275b0d04213b5 |

三份数据库分别为批准且已审核的语言缺证据状态、原生计费领取后的状态、原完成收据提交后的状态。JSON 保存原实际领取的全部字段和测试使用的原样本。跨语言比较允许独立执行的实际系统时间不同，保留数据库其他字段和 JSON 原文；不得修改固定数据库来适应新实现。
