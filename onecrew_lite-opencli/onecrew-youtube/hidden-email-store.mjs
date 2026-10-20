import { randomUUID } from 'node:crypto';
import { constants, closeSync, lstatSync, openSync } from 'node:fs';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';

export class YoutubeHiddenEmailStore {
  constructor(directory) {
    const root = lstatSync(directory);
    if (!root.isDirectory() || root.isSymbolicLink()) throw new Error('youtube_email_state_directory_invalid');
    const path = join(directory, 'youtube-hidden-email.sqlite');
    const existing = lstatSync(path, { throwIfNoEntry: false });
    if (existing && (!existing.isFile() || existing.isSymbolicLink() || existing.nlink !== 1)) throw new Error('youtube_email_state_file_invalid');
    if (!existing) closeSync(openSync(path, constants.O_CREAT | constants.O_EXCL | constants.O_WRONLY, 0o600));
    this.db = new DatabaseSync(path);
    this.db.exec(`PRAGMA busy_timeout=5000;
      CREATE TABLE IF NOT EXISTS task_reveal_gate(task_id TEXT PRIMARY KEY, limited_at INTEGER, reason TEXT, lease TEXT, lease_until INTEGER);
      CREATE TABLE IF NOT EXISTS observations(channel_id TEXT PRIMARY KEY, status TEXT NOT NULL, email TEXT, captured_at INTEGER NOT NULL, reason TEXT);`);
  }
  close() { this.db.close(); }
  gate(task) { return this.db.prepare('SELECT * FROM task_reveal_gate WHERE task_id=?').get(task) ?? null; }
  observation(channel) { return this.db.prepare('SELECT * FROM observations WHERE channel_id=?').get(channel) ?? null; }
  claim(task, now = Date.now()) {
    if (typeof task !== 'string' || !task.trim() || task.length > 200) throw new Error('youtube_email_task_invalid');
    this.db.exec('BEGIN IMMEDIATE');
    let committed = false;
    try {
      this.db.prepare('INSERT OR IGNORE INTO task_reveal_gate(task_id) VALUES(?)').run(task);
      const gate = this.gate(task);
      const status = gate.limited_at !== null ? 'task_limited' : gate.lease_until > now ? 'busy' : 'ready';
      const lease = status === 'ready' ? randomUUID() : null;
      if (lease) this.db.prepare('UPDATE task_reveal_gate SET lease=?,lease_until=? WHERE task_id=?').run(lease, now + 300000, task);
      this.db.exec('COMMIT'); committed = true;
      return { status, lease, reason: gate.reason };
    } finally { if (!committed) this.db.exec('ROLLBACK'); }
  }
  record(channel, status, email, reason, capturedAt = Date.now()) {
    if (!/^UC[A-Za-z0-9_-]{22}$/.test(channel) || !Number.isSafeInteger(capturedAt) || capturedAt <= 0) throw new Error('youtube_email_observation_invalid');
    this.db.prepare(`INSERT INTO observations VALUES(?,?,?,?,?) ON CONFLICT(channel_id) DO UPDATE SET
      status=excluded.status,email=COALESCE(excluded.email,observations.email),captured_at=excluded.captured_at,reason=excluded.reason`).run(channel, status, email, capturedAt, reason);
  }
  noteDailyLimit(task, capturedAt, reason) {
    if (typeof task !== 'string' || !task.trim() || task.length > 200 || !Number.isSafeInteger(capturedAt) || capturedAt <= 0 || typeof reason !== 'string' || !reason.trim()) throw new Error('youtube_email_limit_invalid');
    this.db.prepare(`INSERT INTO task_reveal_gate(task_id,limited_at,reason) VALUES(?,?,?) ON CONFLICT(task_id)
      DO UPDATE SET limited_at=COALESCE(task_reveal_gate.limited_at,excluded.limited_at),reason=excluded.reason`).run(task, capturedAt, reason);
    return this.gate(task);
  }
  release(lease) {
    if (this.db.prepare('UPDATE task_reveal_gate SET lease=NULL,lease_until=NULL WHERE lease=?').run(lease).changes !== 1) throw new Error('youtube_email_lease_changed');
  }
}
