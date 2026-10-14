CREATE TABLE task_attention (
  sequence INTEGER PRIMARY KEY AUTOINCREMENT,
  id TEXT NOT NULL UNIQUE,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  kind TEXT NOT NULL CHECK(kind IN ('run_failed','dispatch_blocked')),
  task_run_id TEXT,
  occurrence_key TEXT NOT NULL UNIQUE
    CHECK(length(occurrence_key) BETWEEN 1 AND 192),
  reason_code TEXT CHECK(reason_code IS NULL OR (
    length(reason_code) BETWEEN 1 AND 64
    AND reason_code NOT GLOB '*[^a-z0-9_]*'
    AND substr(reason_code, 1, 1) BETWEEN 'a' AND 'z'
  )),
  message TEXT CHECK(message IS NULL OR (
    length(trim(message)) BETWEEN 1 AND 512
  )),
  occurred_at INTEGER NOT NULL CHECK(occurred_at >= 0),
  resolved_at INTEGER CHECK(resolved_at IS NULL OR resolved_at >= occurred_at),
  acknowledged_at INTEGER CHECK(acknowledged_at IS NULL OR acknowledged_at >= occurred_at),
  FOREIGN KEY(task_id, task_run_id) REFERENCES task_runs(task_id, id) ON DELETE CASCADE,
  CHECK((kind = 'run_failed' AND task_run_id IS NOT NULL AND reason_code IS NULL AND message IS NULL)
     OR (kind = 'dispatch_blocked' AND task_run_id IS NULL AND reason_code IS NOT NULL AND message IS NOT NULL
         AND ((reason_code = 'codex_cli_missing' AND message = '未找到可用的 Codex CLI')
           OR (reason_code = 'plugin_install_failed' AND message = 'Codex 插件安装或更新失败')
           OR (reason_code = 'mcp_unavailable' AND message = '本地 MCP 未就绪')
           OR (reason_code = 'session_start_failed' AND message = '任务会话启动失败'))))
)
