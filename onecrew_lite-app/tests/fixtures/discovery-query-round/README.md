这组文件由 GitHub 0.7.1 原 Rust 测试 `discovery_query_rounds::tests::query_round_preserves_filters_and_resumes_after_exhaustion` 导出。导出只通过环境变量增加测试产物，不改变原业务执行。

`before.sqlite` 为搜索耗尽状态；`after.sqlite` 为原实现追加查询后的状态。`input.json` 保存原批准范围、版本与新增查询。Node 测试校验全部文件 SHA-256，再比较原始数据库的查询轮次、计划、批准版本和平台运行字段；仅排除独立运行产生的时间。
