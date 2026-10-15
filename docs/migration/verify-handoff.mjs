import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFile, access } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));
const ledger = JSON.parse(await readFile(new URL('source-ledger.json', import.meta.url), 'utf8'));
const git = args => execFileSync('git', args, { cwd: root, encoding: 'utf8' }).trim();
const baseline = ledger.lite.company_main;

// 固定源码身份，防止克隆默认旧分支后误认为已经接手最新版。
assert.equal(git(['rev-parse', `${baseline}^{tree}`]), ledger.lite.source_tree);
assert.equal(git(['rev-parse', `${ledger.lite.mac_build_source}^{tree}`]), ledger.lite.source_tree);
git(['merge-base', '--is-ancestor', baseline, 'HEAD']);
const paths = execFileSync('git', ['ls-tree', '-r', '--name-only', '-z', baseline], { cwd: root, encoding: 'utf8' }).split('\0').filter(Boolean);
assert.equal(paths.length, ledger.lite.tracked_files_before_handoff_docs);
for (const [directory, count] of Object.entries(ledger.lite.directory_files)) {
  assert.equal(paths.filter(value => value.startsWith(`${directory}/`)).length, count, directory);
  assert.equal(git(['diff', '--name-only', baseline, '--', directory]), '', `${directory} 与原始交接基线不同`);
  assert.equal(git(['rev-parse', `HEAD:${directory}`]), git(['rev-parse', `${baseline}:${directory}`]), `${directory} 提交内容不同`);
}

const documents = [
  'README.md', '01-lite-codebase-analysis.md', '02-pro-reference-analysis.md',
  '03-detailed-migration-plan.md', '04-acceptance-and-release.md',
  '05-new-computer-handoff.md', '06-agent-handoff-prompt.md', '07-published-baseline.md',
];
for (const document of documents) {
  const text = await readFile(new URL(document, import.meta.url), 'utf8');
  assert.ok(text.length > 100, `${document} 内容缺失`);
}
for (const path of [
  'onecrew_lite-rust/Cargo.lock', 'onecrew_lite-rust/rust-toolchain.toml',
  'onecrew_lite-rust/apps/web/package-lock.json', 'onecrew_lite-rust/apps/mcp-ui/package-lock.json',
  'onecrew_lite-library/pnpm-lock.yaml', '.github/workflows/workspace-candidates.yml',
  'onecrew_lite-rust/development/NATIVE_RELEASE.md',
]) await access(new URL(`../../${path}`, import.meta.url));

assert.equal(ledger.published_windows.source_verified, false);
assert.equal(ledger.access.embedded_snapshot_is_current_production, false);
assert.equal(ledger.pro_reference.implementation_copied, false);
assert.equal(ledger.migration_status, 'analysis_and_plan_only');
console.log(JSON.stringify({
  verified: true,
  head: git(['rev-parse', 'HEAD']),
  baseline,
  sourceTree: ledger.lite.source_tree,
  baselineTrackedFiles: paths.length,
  sourceDirectoriesUnchanged: Object.keys(ledger.lite.directory_files),
  documentsChecked: documents.length,
  windowsR3SourceVerified: false,
  productionAccessIncluded: false,
}, null, 2));
