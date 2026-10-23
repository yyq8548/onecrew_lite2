import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { z } from "zod";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { canonicalCandidate } from "../src/creators/candidate-contracts.ts";
import { candidateObservations, discoverCandidateContacts } from "../src/creators/candidate-extraction.ts";
import { parseCreatorSaveCandidates } from "../src/creators/candidate-input.ts";
import { prepareCandidateRows } from "../src/creators/candidate-prepare.ts";
import { candidatePreparedPayload, candidateSelectedPayload, selectCandidateRoutes } from "../src/creators/candidate-selection.ts";
import { CreatorRepository } from "../src/creators/repository.ts";
import { AccountContextService } from "../src/identity/account-context.ts";
import { OwnerReviewContext } from "../src/results/owner-context.ts";
import { OwnerConfirmationStore } from "../src/results/grants.ts";
import { projectCreatorCandidateReview } from "../src/creators/candidate-projection.ts";
import { CandidateWarnings } from "../src/creators/candidate-materialize.ts";
import { PreparedCreatorSaveAction, executeSubmittedCreatorSave } from "../src/creators/candidate-review.ts";
import { completeCreatorCandidateSave } from "../src/creators/candidate-outcome.ts";
import { parseStoredJson } from "../src/contracts/stored-json.ts";
import { candidateJson } from "../src/creators/candidate-contracts.ts";
import { decodeCandidateReceipt } from "../src/creators/candidate-receipt.ts";
import { CandidateCommitUnknownError } from "../src/creators/candidate-execute.ts";

function database() { const db = new DatabaseSync(":memory:"); db.exec("PRAGMA foreign_keys=ON"); for (const file of ["creator-schema90.sql", "creator-indexes90.sql", "creator-triggers90.sql"]) db.exec(readFileSync(new URL(`../src/storage/${file}`, import.meta.url), "utf8")); db.exec("PRAGMA user_version=90"); return db; }
function candidate(id = "row-1") { return canonicalCandidate.parse({ row_id: id, account: { platform: "youtube", external_id: "UCabcdefghijklmnopqrstuv", target_kind: "channel", display_name: "Public channel", profile_url: "https://www.youtube.com/channel/UCabcdefghijklmnopqrstuv" }, profile: { description: "Business: creator@example.com", follower_count: 18446744073709551615n }, profile_observed_at: 100, provenance: { kind: "direct_selection", source_ref: "public-selection", source_url: "https://youtube.com/@public" }, contact_evidence: [{ kind: "profile_field", source: "channel_description", source_url: "https://youtube.com/@public", text: "creator@example.com https://linktr.ee/public" }, { kind: "held_hub", source_url: "https://linktr.ee/public", text: "hub@example.com", links: [] }], quality: {} }); }

test("候选公共输入在读取前拒绝混合intent、身份、账号和来源错配", () => {
  const valid = { intent: "add_new", target: { platform: "youtube", channel_id: "UCabcdefghijklmnopqrstuv" }, connection: { platform: "youtube", service_credential_ref_id: "11111111-1111-4111-8111-111111111111" }, provenance: { kind: "direct_selection", source_url: "https://youtube.com/@public" }, quality: {} };
  assert.equal(parseCreatorSaveCandidates([valid]).length, 1);
  assert.throws(() => parseCreatorSaveCandidates([valid, { ...valid, intent: "save_or_update" }]), /mix intents/);
  assert.throws(() => parseCreatorSaveCandidates([{ ...valid, connection: { platform: "x", account_ref_id: "11111111-1111-4111-8111-111111111111" } }]), /connection/);
  assert.throws(() => parseCreatorSaveCandidates([{ ...valid, target: { platform: "youtube", channel_id: "@public" } }]), /target/);
  assert.throws(() => parseCreatorSaveCandidates([{ ...valid, provenance: { kind: "direct_selection", source_url: "https://youtube.com/@public#" } }]), /source/);
});

test("公开证据提取保留真实来源，过滤短链接/图像/自有平台并约束YouTube视频", () => {
  const rows = discoverCandidateContacts("youtube", [{ kind: "profile_field", source: "video_description", source_url: "https://youtube.com/watch?v=one", text: "business@example.com https://instagram.com/contact telegram @public_handle logo.png" }], []);
  assert.deepEqual(rows.map(row => [row.channel, row.value]), [["email", "business@example.com"]]);
  const result = candidateObservations(candidate()); assert.equal(result.ok, true);
  assert.equal(result.value.filter(row => row.source === "initial_platform_account").length, 1);
  assert.ok(result.value.some(row => row.value === "hub@example.com" && row.source_url === "https://linktr.ee/public"));
  assert.deepEqual(discoverCandidateContacts("x", [{ kind: "profile_field", source: "profile_bio", source_url: null, text: "https://t.co/secret https://pbs.twimg.com/a.png https://x.com/self Telegram @valid_name" }], []).map(row => row.value), ["valid_name"]);
});

test("真实schema90候选准备只读、无损u64、重复冲突及LinkHub选项依赖", () => {
  const db = database();
  try {
    const before = db.prepare("SELECT COUNT(*) AS n FROM creators").get()!.n;
    const prepared = prepareCandidateRows(db, [candidate(), candidate("row-2")], "add_new");
    assert.equal(prepared.rows[0]!.preview.quality.follower_counts.youtube, 18446744073709551615n);
    assert.deepEqual(prepared.rows.map(row => row.preview.status), ["eligible_new", "duplicate_skipped"]);
    const row = prepared.rows[0]!, required = row.preview.route_options.find(route => route.required)!, hubChild = row.routes.find(([, route]) => route.observations.some(observation => observation.source === "link_hub"))!;
    assert.throws(() => selectCandidateRoutes(prepared.rows, "add_new", [{ row_id: row.input.row_id, confirmed_route_ids: [hubChild[0]] }]), /required/);
    assert.throws(() => selectCandidateRoutes(prepared.rows, "add_new", [{ row_id: row.input.row_id, confirmed_route_ids: [required.route_id, hubChild[0]] }]), /parent LinkHub/);
    const selected = selectCandidateRoutes(prepared.rows, "add_new", [{ row_id: row.input.row_id, confirmed_route_ids: row.preview.route_options.map(route => route.route_id) }]);
    assert.equal(selected.rows.length, 1); assert.equal(selected.rows[0]!.confirmations.find(route => route.confidence === "exact")!.preview.locator.channel, "youtube");
    assert.equal(db.prepare("SELECT COUNT(*) AS n FROM creators").get()!.n, before);
    const invalidQuality = candidate("invalid-quality"); invalidQuality.quality.score = 201;
    const partial = prepareCandidateRows(db, [candidate(), invalidQuality], "add_new"); assert.equal(partial.rows.length, 1); assert.equal(partial.failures[0]!.kind, "invalid_fields");
    const conflict = candidate("row-3"); conflict.notes = "Conflicting selected update";
    assert.deepEqual(prepareCandidateRows(db, [candidate(), conflict], "save_or_update").rows.map(row => row.preview.reason), ["conflicting_update", "conflicting_update"]);
    const repository = new CreatorRepository(db), existing = repository.create({ display_name: "Contact owner" });
    repository.contactAdd({ creator_id: existing.creator_id, channel: "youtube", value: candidate().account.profile_url });
    assert.equal(prepareCandidateRows(db, [candidate()], "add_new").rows[0]!.preview.status, "contact_conflict");
  } finally { db.close(); }
});

test("候选执行保存完整来源/历史/父子依赖，精确收据重放保持旧结果与u64", () => {
  const db = database();
  try {
    const repository = new CreatorRepository(db), prepared = repository.candidatePrepare([candidate()], "add_new"), row = prepared.rows[0]!;
    const selected = selectCandidateRoutes(prepared.rows, prepared.intent, [{ row_id: row.input.row_id, confirmed_route_ids: row.preview.route_options.map(route => route.route_id) }]);
    const first = repository.candidateExecute(selected, "candidate-save-one"); assert.equal(first.changed, true);
    const saved = first.result.outcomes[0]!; assert.equal(saved.status, "saved"); if (saved.status !== "saved") throw new Error("candidate save did not complete");
    assert.equal(saved.creator.creator.follower_counts.youtube, 18446744073709551615n);
    assert.equal(saved.creator.account_links[0]!.state, "confirmed"); assert.equal(saved.creator.contact_routes.length, 4);
    assert.equal(db.prepare("SELECT COUNT(*) AS n FROM creator_contact_route_dependencies WHERE kind='link_hub'").get()!.n, 1);
    assert.equal(saved.creator.account_observations.length, 1);
    assert.ok(saved.creator.observation_sources.every(source => source.external_ref?.includes("creator-save:v1:")));
    repository.update({ creator_id: saved.creator.creator.id, display_name: "Later local name" });
    const replay = repository.candidateExecute(selected, "candidate-save-one");
    assert.equal(replay.changed, false); assert.deepEqual(replay.result, first.result);
    assert.equal(repository.get(saved.creator.creator.id).creator.display_name, "Later local name");
    const altered = structuredClone(selected); altered.rows[0]!.input.notes = "changed";
    assert.throws(() => repository.candidateExecute(altered, "candidate-save-one"), /conflicts/);
    const page = repository.list({ limit: 1, sorting: [{ field: "followers", direction: "desc" }] });
    assert.equal(page.items[0]!.follower_counts.youtube, 18446744073709551615n);
  } finally { db.close(); }
});

test("真实SQLite行失败回滚所有行效果，其他候选及同事务收据仍完整提交", () => {
  const db = database();
  try {
    const repository = new CreatorRepository(db), second = candidate("row-2"); second.account.external_id = "UCZYXWVUTSRQPONMLKJIHGFE"; second.account.profile_url = "https://youtube.com/channel/UCZYXWVUTSRQPONMLKJIHGFE"; second.contact_evidence = []; second.account.display_name = "Second channel";
    const prepared = repository.candidatePrepare([candidate(), second], "add_new"), selected = selectCandidateRoutes(prepared.rows, prepared.intent, prepared.rows.map(row => ({ row_id: row.input.row_id, confirmed_route_ids: row.preview.route_options.map(route => route.route_id) })));
    db.exec("CREATE TRIGGER reject_first_candidate BEFORE INSERT ON creators WHEN NEW.display_name='Public channel' BEGIN SELECT RAISE(ABORT,'candidate write rejected'); END");
    const result = repository.candidateExecute(selected, "candidate-partial");
    assert.deepEqual(result.result.outcomes.map(row => row.status), ["failed", "saved"]);
    assert.equal(db.prepare("SELECT COUNT(*) AS n FROM creators").get()!.n, 1);
    assert.equal(db.prepare("SELECT COUNT(*) AS n FROM platform_accounts").get()!.n, 1);
    assert.equal(db.prepare("SELECT COUNT(*) AS n FROM idempotency_records WHERE key='candidate-partial' AND state='completed'").get()!.n, 1);
  } finally { db.close(); }
});

test("真实owner store只执行密封选中候选，失败选择不消耗授权，完成后不能再次发送保存", () => {
  const db = database(), accounts = new AccountContextService(), account = accounts.activate({ user: { id: "candidate-owner", email: "owner@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" }), store = new OwnerConfirmationStore(accounts);
  try {
    const repository = new CreatorRepository(db), prepared = repository.candidatePrepare([candidate()], "add_new"), projected = projectCreatorCandidateReview(prepared.rows, [], prepared.intent, BigInt(Math.floor(Date.now() / 1000) + 900), new CandidateWarnings()), context = new OwnerReviewContext("mcp", "candidate-call", "onecrew.creator.batch.save");
    const action = PreparedCreatorSaveAction.create(prepared.rows, projected.review, performance.now() + 900000), review = { context, action, review: projected.review, presentation: projected.presentation };
    const ref = store.mint(review, { outcomes: [{ call_id: context.call_id, operation_id: context.operation_id, disposition: { kind: "not_run", reason: "confirmation_required" }, summary: "Prepared fresh Creator details for review; confirmation is required to save.", resource_refs: [] }] }, account), presented = store.present(ref, "mcp", account, "zh-CN");
    assert.throws(() => JSON.stringify(action), /must not be serialized/);
    assert.throws(() => store.submit(presented.binding, account, [{ row_id: prepared.rows[0]!.input.row_id, confirmed_route_ids: ["route-unknown"] }]));
    assert.equal(repository.list({}).items.length, 0);
    const selection = [{ row_id: prepared.rows[0]!.input.row_id, confirmed_route_ids: prepared.rows[0]!.preview.route_options.map(route => route.route_id) }];
    const grant = store.submit(presented.binding, account, selection); prepared.rows[0]!.input.account.display_name = "Substituted name";
    const executed = executeSubmittedCreatorSave(grant, account, repository); assert.equal(executed.changed, true); assert.equal(executed.completion.outcome.resource_refs.length, 1); assert.equal(repository.list({}).items[0]!.display_name, "Public channel");
    assert.equal(executed.completion.presentation.title, "达人保存结果"); assert.ok(executed.completion.outcome.summary.includes("已保存 1 个")); assert.equal(executed.completion.continue_batch, true);
    assert.throws(() => executeSubmittedCreatorSave(grant, account, repository), /consumed/);
    const completed = store.complete(grant.binding(), account, { batch: { outcomes: [executed.completion.outcome] }, presentation: executed.completion.presentation, terminal: executed.completion.terminal }); assert.equal(completed.state.kind, "resolved");
    assert.equal(db.prepare("SELECT key FROM idempotency_records").get()!.key, `creator-save:${ref}:${context.call_id}`);
  } finally { store.close(); db.close(); }
});

test("实际profile观察按同秒插入顺序保留历史，延迟确认不会覆盖较新粉丝数量", () => {
  const db = database();
  try {
    const repository = new CreatorRepository(db);
    const execute = (input: ReturnType<typeof candidate>, key: string) => { const prepared = repository.candidatePrepare([input], "save_or_update"); return repository.candidateExecute(selectCandidateRoutes(prepared.rows, prepared.intent, prepared.rows.map(row => ({ row_id: row.input.row_id, confirmed_route_ids: row.preview.route_options.map(route => route.route_id) }))), key); };
    const initial = candidate(); initial.profile.follower_count = 42;
    const first = execute(initial, "profile-a").result.outcomes[0]!; assert.equal(first.status, "saved"); if (first.status !== "saved") throw new Error("save failed");
    const changed = structuredClone(initial); changed.account.display_name = "Latest public profile"; changed.profile.follower_count = 99;
    execute(changed, "profile-b"); assert.equal(repository.get(first.creator.creator.id).account_observations.length, 2);
    execute(initial, "profile-a-again"); assert.equal(repository.get(first.creator.creator.id).account_observations.length, 3);
    changed.profile_observed_at = 120; execute(changed, "newer-public-profile");
    const delayed = structuredClone(initial); delayed.profile_observed_at = 105; delayed.profile.follower_count = 50; execute(delayed, "delayed-profile");
    const record = repository.get(first.creator.creator.id); assert.equal(record.creator.follower_counts.youtube, 99); assert.equal(record.account_observations[0]!.display_name, "Latest public profile"); assert.equal(record.creator.display_name, "Public channel");
    const before = record.account_observations.length; execute(changed, "identical-profile"); assert.equal(repository.get(first.creator.creator.id).account_observations.length, before);
  } finally { db.close(); }
});

test("只读no-op终态保留准备失败和跳过结果，实际store不能发行保存授权", () => {
  const db = database(), accounts = new AccountContextService(), account = accounts.activate({ user: { id: "noop-owner", email: "owner@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" }), store = new OwnerConfirmationStore(accounts);
  try {
    const repository = new CreatorRepository(db), owner = repository.create({ display_name: "Existing contact owner" }); repository.contactAdd({ creator_id: owner.creator_id, channel: "youtube", value: candidate().account.profile_url });
    const prepared = repository.candidatePrepare([candidate()], "add_new");
    const projected = projectCreatorCandidateReview(prepared.rows, [], "add_new", 2000n, new CandidateWarnings()), context = new OwnerReviewContext("mcp", "noop-call", "onecrew.creator.batch.save"), completion = completeCreatorCandidateSave(projected.review, [], null, context, "en-US", true);
    assert.equal(completion.terminal.kind, "unknown"); assert.equal(completion.outcome.disposition.kind, "unknown");
    const frame = { outcomes: [{ call_id: context.call_id, operation_id: context.operation_id, disposition: { kind: "not_run" as const, reason: "confirmation_required" }, summary: "Prepared details", resource_refs: [] }] };
    const ref = store.mintCompleted(context, completion, { outcomes: [completion.outcome] }, account), snapshot = store.present(ref, "mcp", account);
    assert.equal(snapshot.binding.view.kind, "read_only"); assert.throws(() => store.submit(snapshot.binding, account, [])); assert.equal(repository.list({}).items.length, 1);
    assert.throws(() => store.mint({ context, presentation: projected.presentation, review: projected.review, action: PreparedCreatorSaveAction.create(prepared.rows, projected.review, performance.now() + 900000) }, frame, account));
  } finally { store.close(); db.close(); }
});

test("冻结的原Rust实际u64MAX候选prepared与selected字节摘要", () => {
  const raw = z.array(canonicalCandidate).parse(parseStoredJson(readFileSync(new URL("./fixtures/creator-candidate/inputs.json", import.meta.url), "utf8"))), db = database();
  const hash = (value: string | Buffer) => createHash("sha256").update(value).digest("hex");
  try {
    db.exec("INSERT INTO account_refs(id,platform,kind,label,created_at,updated_at) VALUES('operator-x','x','profile','@operator',1,1)");
    const repository = new CreatorRepository(db), prepared = repository.candidatePrepare(raw, "save_or_update");
    // 原 creator-runtime oracle 产物的原始 UTF-8 字节 SHA256。
    assert.equal(hash(candidatePreparedPayload(prepared.rows, prepared.intent)), "c1bb73aa839144d1cf46d26ee7f623fd10a56ae742daaccc5c525508699c6e98");
    const selected = selectCandidateRoutes(prepared.rows, prepared.intent, [{ row_id: "row-original-x", confirmed_route_ids: ["route-0", "route-1"] }, { row_id: "row-original-youtube", confirmed_route_ids: ["route-0"] }]);
    assert.equal(hash(candidateJson(candidateSelectedPayload(selected))), "7e6eb26b27fa931dbb01684dc3f6e8a3c7eb6d10d7b55fd3b9a9009d5244bed7");
    const saved = repository.candidateExecute(selected, "frozen-rust-golden");
    assert.equal(saved.result.outcomes.filter(row => row.status === "saved").length, 2);
    assert.equal(repository.list({ sorting: [{ field: "followers", direction: "desc" }] }).items[0]!.follower_counts.x, 18446744073709551615n);
    assert.equal(repository.candidateExecute(selected, "frozen-rust-golden").changed, false);
  } finally { db.close(); }
});

test("原Rust实际prepared/selected字节、稳定source及完整receipt跨语言无损重放", async () => {
  const directory = process.env.ONECREW_CREATOR_CANDIDATE_ORACLE_DIRECTORY ?? fileURLToPath(new URL("./fixtures/creator-candidate/", import.meta.url)), raw = z.array(canonicalCandidate).parse(parseStoredJson(readFileSync(join(directory, "inputs.json"), "utf8"))), db = database();
  mkdirSync(fileURLToPath(new URL("../../.scratch/", import.meta.url)), { recursive: true });
  const temporary = mkdtempSync(fileURLToPath(new URL("../../.scratch/creator-candidate-oracle-", import.meta.url))), source = new DatabaseSync(join(directory, "creator.sqlite"), { readOnly: true });
  try { source.prepare("VACUUM INTO ?").run(join(temporary, "creator.sqlite")); } finally { source.close(); }
  const original = new DatabaseSync(join(temporary, "creator.sqlite"));
  try {
    db.exec("INSERT INTO account_refs(id,platform,kind,label,created_at,updated_at) VALUES('operator-x','x','profile','@operator',1,1)");
    const repository = new CreatorRepository(db), prepared = repository.candidatePrepare(raw, "save_or_update");
    assert.equal(candidatePreparedPayload(prepared.rows, prepared.intent).toString(), readFileSync(join(directory, "prepared.json"), "utf8").trimEnd());
    const selections = z.array(z.object({ row_id: z.string(), route_ids: z.array(z.string()) })).parse(parseStoredJson(readFileSync(join(directory, "selections.json"), "utf8"))).map(row => ({ row_id: row.row_id, confirmed_route_ids: row.route_ids }));
    const selected = selectCandidateRoutes(prepared.rows, prepared.intent, selections);
    assert.equal(candidateJson(candidateSelectedPayload(selected)), readFileSync(join(directory, "selected.json"), "utf8").trimEnd());
    const expected = decodeCandidateReceipt(parseStoredJson(readFileSync(join(directory, "result.json"), "utf8"))), old = new CreatorRepository(original);
    const replay = old.candidateExecute(selected, "creator-candidate-rust-oracle"); assert.equal(replay.changed, false); assert.deepEqual(replay.result, expected);
    const firstPage = old.list({ limit: 1, sorting: [{ field: "followers", direction: "desc" }] }); assert.equal(firstPage.items[0]!.follower_counts.x, 18446744073709551615n); assert.ok(firstPage.next_cursor);
    const secondPage = old.list({ limit: 1, cursor: firstPage.next_cursor, sorting: [{ field: "followers", direction: "desc" }] }); assert.equal(secondPage.items.length, 1); assert.notEqual(secondPage.items[0]!.id, firstPage.items[0]!.id);
    const node = repository.candidateExecute(selected, "node-own-save"); assert.equal(node.result.outcomes.filter(row => row.status === "saved").length, 2);
    assert.deepEqual(db.prepare("SELECT external_ref,metadata_json FROM observation_sources ORDER BY external_ref").all(), original.prepare("SELECT external_ref,metadata_json FROM observation_sources ORDER BY external_ref").all());
  } finally { original.close(); db.close(); }
});

test("50项业务上限、超大公开资料和路由预览保持预算，丢弃可选路由不能绕过选择", () => {
  const db = database();
  try {
    const repository = new CreatorRepository(db), inputs = Array.from({ length: 50 }, (_, index) => { const input = candidate(`row-${index}`); input.account.external_id = `UC${String(index + 1).padStart(22, "0")}`; input.account.profile_url = `https://youtube.com/channel/${input.account.external_id}`; input.contact_evidence = []; return input; });
    inputs[0]!.profile.description = "名".repeat(5000);
    inputs[0]!.contact_evidence = [{ kind: "profile_field", source: "channel_description", source_url: inputs[0]!.account.profile_url, text: Array.from({ length: 160 }, (_, i) => `person${i}@example.com`).join(" ") }];
    const prepared = repository.candidatePrepare(inputs, "add_new"), projected = projectCreatorCandidateReview(prepared.rows, [], "add_new", 2000n, new CandidateWarnings());
    assert.equal(projected.review.rows.length, 50); assert.equal(projected.review.rows[0]!.preview.routes.length, 128); assert.ok(projected.review.rows[0]!.preview.routes.some(route => route.required));
    assert.ok(Buffer.byteLength(candidateJson(projected.review.rows[0]!.preview)) <= 65536); assert.ok(Buffer.byteLength(candidateJson(projected.presentation)) <= 4 * 1024 * 1024);
    const description = projected.presentation.sections.find(section => section.kind === "fields"); assert.ok(description && description.kind === "fields" && description.fields.some(field => field.label === "Description" && field.value.endsWith("… (truncated)")));
    const action = PreparedCreatorSaveAction.create(prepared.rows, projected.review, performance.now() + 900000), visible = new Set(projected.review.rows[0]!.preview.routes.map(route => route.route_id)), hidden = prepared.rows[0]!.preview.route_options.find(route => !visible.has(route.route_id))!;
    assert.throws(() => action.select([{ row_id: inputs[0]!.row_id, confirmed_route_ids: [...visible, hidden.route_id] }]), /creator_save_routes/);
    assert.throws(() => PreparedCreatorSaveAction.create(prepared.rows, projected.review, performance.now() - 1).select([{ row_id: inputs[0]!.row_id, confirmed_route_ids: [...visible] }]), /expired/);
    assert.throws(() => repository.candidatePrepare([...inputs, candidate("extra")], "add_new"), /batch fields/);
    assert.equal(repository.list({}).items.length, 0);
  } finally { db.close(); }
});

test("真实SQLite延迟外键导致COMMIT失败时核对收据并保留unknown，明确同key重试才重新执行", () => {
  const db = database();
  try {
    const repository = new CreatorRepository(db), prepared = repository.candidatePrepare([candidate()], "add_new"), selected = selectCandidateRoutes(prepared.rows, prepared.intent, prepared.rows.map(row => ({ row_id: row.input.row_id, confirmed_route_ids: row.preview.route_options.map(route => route.route_id) })));
    db.exec("CREATE TRIGGER reject_candidate_commit AFTER INSERT ON creators BEGIN INSERT INTO creator_account_links(id,creator_id,platform_account_id,state,confidence,created_at,updated_at) VALUES('deferred-invalid-link',NEW.id,'missing-native-account','confirmed','exact',1,1); END");
    db.exec("PRAGMA defer_foreign_keys=ON");
    assert.throws(() => repository.candidateExecute(selected, "commit-unknown"), error => error instanceof CandidateCommitUnknownError && error.errors.length === 1 && error.message.includes("outcome is unknown"));
    assert.equal(db.prepare("SELECT COUNT(*) AS n FROM creators").get()!.n, 0); assert.equal(db.prepare("SELECT COUNT(*) AS n FROM idempotency_records").get()!.n, 0); assert.equal(db.isTransaction, false);
    db.exec("DROP TRIGGER reject_candidate_commit");
    assert.equal(repository.candidateExecute(selected, "commit-unknown").changed, true); assert.equal(repository.candidateExecute(selected, "commit-unknown").changed, false);
    assert.equal(db.prepare("SELECT COUNT(*) AS n FROM creators").get()!.n, 1);
  } finally { db.close(); }
});

test("Telegram管理员只按真实账号来源保存，既有身份合并两个社区证据与选中tags且保留本地资料", () => {
  const db = database();
  try {
    db.exec("INSERT INTO account_refs(id,platform,kind,label,created_at,updated_at) VALUES('operator-tg','telegram','profile','@operator',1,1)");
    const repository = new CreatorRepository(db);
    const administrator = (row: string, community: string) => { const input = candidate(row); input.account = { platform: "telegram", external_id: "123456", target_kind: "user", handle: "public_admin", display_name: "Public administrator", profile_url: "https://t.me/public_admin" }; input.profile.follower_count = null; input.contact_evidence = []; input.provenance = { kind: "administrator_selection", source_ref: `telegram:channel:${community}`, role: "administrator", community: { external_id: community, title: `Community ${community}`, handle: null, target_kind: "channel" }, source: { kind: "operator_account", operator_account_ref_id: "operator-tg" } }; return input; };
    const execute = (inputs: ReturnType<typeof candidate>[], key: string) => { const prepared = repository.candidatePrepare(inputs, "save_or_update"); return repository.candidateExecute(selectCandidateRoutes(prepared.rows, prepared.intent, prepared.rows.map(row => ({ row_id: row.input.row_id, confirmed_route_ids: row.preview.route_options.map(route => route.route_id) }))), key); };
    const first = execute([administrator("initial", "1")], "administrator-initial").result.outcomes[0]!; assert.equal(first.status, "saved"); if (first.status !== "saved") throw new Error("administrator save failed");
    repository.update({ creator_id: first.creator.creator.id, display_name: "Local saved name", notes: "Private local notes" });
    const a = administrator("community-two", "2"), b = administrator("community-three", "3"); a.tags = ["公开"]; b.tags = ["资料"];
    const updates = execute([a, b], "administrator-two-sources"); assert.equal(updates.result.outcomes.length, 2);
    for (const outcome of updates.result.outcomes) { assert.equal(outcome.status, "saved"); if (outcome.status !== "saved") throw new Error("administrator update failed"); assert.equal(outcome.created, false); assert.equal(outcome.outreach_context?.operator_account_ref_id, "operator-tg"); assert.deepEqual(outcome.creator.creator.tags, ["公开", "资料"]); }
    const record = repository.get(first.creator.creator.id); assert.equal(record.creator.display_name, "Local saved name"); assert.equal(record.creator.notes, "Private local notes"); assert.equal(record.platform_accounts.length, 1); assert.equal(record.account_observations.length, 3);
    const missing = administrator("invalid", "4"); missing.account.handle = null; assert.throws(() => repository.candidatePrepare([missing], "add_new"), /telegram_eligibility/);
    const wrongAccount = administrator("wrong-account", "5"); if (wrongAccount.provenance.kind !== "administrator_selection") throw new Error("wrong provenance"); wrongAccount.provenance.source.operator_account_ref_id = "unknown-account"; assert.throws(() => repository.candidatePrepare([wrongAccount], "add_new"), /invalid_source/);
    assert.equal(db.prepare("SELECT COUNT(*) AS n FROM messages").get()!.n, 0);
  } finally { db.close(); }
});
