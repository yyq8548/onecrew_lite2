import ExcelJS from "exceljs";
import JSZip from "jszip";
import { channel, confidence, integer, locatorKind, parseJson, platform, sourceKind, stage, tags, trim } from "./contracts.ts";
import { normalizeContact } from "./normalization.ts";
import { buildBundle, compareText, validateBundle, type TransferBundle, type TransferCreator, type TransferContact } from "./transfer-contracts.ts";
import { preflightWorkbook } from "./workbook-security.ts";

export const creatorHeaders = ["creator_ref", "Creator", "初次收录", "建联方式", "X 粉丝", "Instagram 粉丝", "Telegram 粉丝", "YouTube 粉丝", "评分", "阶段", "置顶", "最近触达", "更新时间", "头像 URL", "分组", "TikTok 粉丝", "标签 1", "标签 2", "标签 3", "备注"];
export const accountHeaders = ["creator_ref", "Creator", "platform", "target_kind", "external_id", "handle", "profile display name", "profile URL", "avatar URL", "link state", "confidence", "source kind", "capture order"];
export const contactHeaders = ["creator_ref", "Creator", "save order", "channel", "locator kind", "display value", "normalized value", "confidence", "source kinds"];
export interface WorkbookDisplay { last_touch_at: number | null; updated_at: number; group_name: string | null }
class Row extends Map<string, ExcelJS.CellValue> { readonly rowNumber: number; constructor(rowNumber: number) { super(); this.rowNumber = rowNumber; } }
function value(cell: ExcelJS.Cell): ExcelJS.CellValue {
  const v = cell.value;
  if (v !== null && typeof v === "object" && !(v instanceof Date)) {
    if ("formula" in v || "sharedFormula" in v || "error" in v) throw new Error("Creator XLSX formulas and error cells are unsupported");
    if ("richText" in v) return v.richText.map(part => part.text).join("");
    if ("hyperlink" in v) return v.text;
    throw new Error("Creator XLSX unsupported cell type");
  }
  if (typeof v === "number" && !Number.isFinite(v)) throw new Error("Creator XLSX non-finite number");
  return v;
}
function sheetRows(sheet: ExcelJS.Worksheet, expected: string[]): Row[] {
  const headers = new Map<number, string>();
  sheet.getRow(1).eachCell((cell, column) => { const header = value(cell); if (typeof header !== "string" || !expected.includes(header) || [...headers.values()].includes(header)) throw new Error(`Creator XLSX ${sheet.name} has unknown or duplicate header`); headers.set(column, header); });
  if (headers.size !== expected.length) throw new Error(`Creator XLSX ${sheet.name} missing headers`);
  const rows: Row[] = [];
  sheet.eachRow((row, index) => { if (index === 1) return; const result = new Row(index); row.eachCell((cell, column) => { const v = value(cell); if (v === null || v === undefined) return; const header = headers.get(column); if (!header) throw new Error(`Creator XLSX ${sheet.name} has data outside known columns`); result.set(header, v); }); if (result.size) rows.push(result); });
  return rows;
}
function optionalText(row: Row, key: string): string | null { const v = row.get(key); if (v === undefined || v === null) return null; if (typeof v !== "string") throw new Error(`Creator XLSX requires text in ${key}`); return v; }
function requiredText(row: Row, key: string): string { const v = optionalText(row, key); if (v === null || v.length === 0) throw new Error(`Creator XLSX requires text in ${key}`); return v; }
function optionalNumber(row: Row, key: string): number | null { const v = row.get(key); return v === undefined || v === null ? null : integer.parse(v); }
function order(row: Row, key: string): number { const v = optionalNumber(row, key); if (v === null || v > 4294967295) throw new Error(`Creator XLSX invalid ${key}`); return v; }
function note(cell: ExcelJS.Cell): string { return typeof cell.note === "string" ? cell.note : cell.note?.texts?.map(part => part.text).join("") ?? ""; }
function recommendation(sheet: ExcelJS.Worksheet): TransferBundle {
  const rows = sheetRows(sheet, ["名字", "类别", "平台", "评分", "联系方式"]), creators: TransferCreator[] = [];
  for (const row of rows) {
    const name = requiredText(row, "名字"), source = sheet.getRow(row.rowNumber), profile = source.getCell(1).hyperlink;
    if (!profile) throw new Error(`推荐第 ${row.rowNumber} 行缺少主页链接，不能仅按名字创建身份`);
    const url = new URL(profile); if (!["http:", "https:"].includes(url.protocol) || !url.hostname || url.username || url.password) throw new Error("Library profile URL invalid");
    const routes: TransferContact[] = [];
    const add = (value: string, kind: Parameters<typeof normalizeContact>[0], display: string) => { const normalized = normalizeContact(kind, value); if (!routes.some(r => r.channel === normalized.channel && r.normalized_value === normalized.normalized_value)) routes.push({ save_order: routes.length, channel: normalized.channel, locator_kind: normalized.kind, normalized_value: normalized.normalized_value, display_value: display, confidence: "asserted", source_kinds: ["creator_import"] }); };
    add(profile, "website", profile);
    const contacts = optionalText(row, "联系方式") ?? "";
    for (const line of contacts.split(/\r?\n/).map(trim).filter(Boolean)) {
      const wechat = /^(?:微信：|微信:|WeChat: )(.*)$/.exec(line);
      if (wechat) { const handle = wechat[1]!.split(/[（( ]/)[0]!; add(handle, "wechat", handle); }
      else if (line.includes("https://") || line.includes("http://")) { const start = line.indexOf("https://") >= 0 ? line.indexOf("https://") : line.indexOf("http://"); add(line.slice(start), "website", line.slice(start)); }
      else if (line.includes("@")) add(line, "email", line);
      else throw new Error(`推荐第 ${row.rowNumber} 行包含无法识别的联系方式，请核对：${line}`);
    }
    const accounts: TransferCreator["accounts"] = [], identityNote = note(source.getCell(1));
    const identityLine = identityNote.split("\n").find(line => line.startsWith("Identity: "));
    if (identityLine) {
      const identity = parseJson(identityLine.slice(10)); if (!identity || typeof identity !== "object" || Array.isArray(identity) || typeof identity.platform !== "string" || typeof identity.platform_id !== "string" || !identity.platform_id) throw new Error("Library identity comment invalid");
      const p = identity.platform === "twitter" ? "x" : identity.platform;
      if (["x", "instagram", "telegram", "youtube", "tiktok"].includes(p) && (p !== "tiktok" || identity.platform_id.startsWith("tto:"))) accounts.push({ capture_order: 0, platform: platform.parse(p), target_kind: p === "youtube" ? "channel" : "user", external_id: identity.platform_id, link_state: "claimed", confidence: "asserted", source_kind: "creator_import", latest_profile: { handle: null, display_name: name, profile_url: profile, avatar_url: null } });
    }
    creators.push({ semantic_sha256: "", record_key: "", display_name: name, tags: [], notes: `Library 推荐导入\n名字：${name}\n平台：${optionalText(row, "平台") ?? ""}\n类别：${optionalText(row, "类别") ?? ""}\n评分：${optionalText(row, "评分") ?? ""}\n评分说明：${note(source.getCell(4))}\n联系方式：${contacts}\n${identityNote}`, score: null, follower_counts: {}, pinned: false, relationship: { stage: "prospect" }, accounts, contact_routes: routes });
  }
  const parents = creators.map((_, index) => index), root = (index: number): number => { while (parents[index] !== index) index = parents[index]!; return index; }, primary = new Map<string, number>();
  for (const [i, c] of creators.entries()) { const key = c.contact_routes[0]!.normalized_value, previous = primary.get(key); primary.set(key, i); if (previous !== undefined) parents[root(i)] = root(previous); }
  for (const [i, c] of creators.entries()) for (const route of c.contact_routes.filter(r => r.channel === "website")) { const j = primary.get(route.normalized_value); if (j !== undefined) parents[root(j)] = root(i); }
  const merged = new Map<number, TransferCreator>();
  for (const [i, c] of creators.entries()) { const key = root(i), existing = merged.get(key); if (!existing) merged.set(key, c); else { existing.notes = `${existing.notes ?? ""}\n\n${c.notes ?? ""}`; for (const account of c.accounts) if (!existing.accounts.some(a => a.platform === account.platform && a.external_id === account.external_id)) existing.accounts.push({ ...account, capture_order: existing.accounts.length }); for (const route of c.contact_routes) if (!existing.contact_routes.some(r => r.channel === route.channel && r.normalized_value === route.normalized_value)) existing.contact_routes.push({ ...route, save_order: existing.contact_routes.length }); } }
  let capture = 0; const result = [...merged.entries()].sort((a, b) => a[0] - b[0]).map(([, c]) => c); for (const c of result) for (const account of c.accounts) account.capture_order = capture++;
  return buildBundle(result);
}
export async function decodeWorkbook(bytes: Buffer): Promise<TransferBundle> {
  const { sheetNames } = await preflightWorkbook(bytes), book = new ExcelJS.Workbook();
  await book.xlsx.load(bytes as unknown as ExcelJS.Buffer);
  if (JSON.stringify(sheetNames) === JSON.stringify(["推荐"])) return recommendation(book.worksheets[0]!);
  if (JSON.stringify(sheetNames) !== JSON.stringify(["Creators", "Accounts", "Contacts"])) throw new Error("Creator XLSX must contain exactly Creators, Accounts, Contacts in that order");
  const head = book.worksheets[0]!.getRow(1), values: string[] = []; head.eachCell(cell => { if (typeof value(cell) === "string") values.push(value(cell) as string); });
  const size = values.includes("备注") ? 20 : values.includes("标签 1") ? 19 : values.includes("TikTok 粉丝") ? 16 : 15;
  const creators = new Map<string, TransferCreator>();
  for (const row of sheetRows(book.worksheets[0]!, creatorHeaders.slice(0, size))) {
    const ref = requiredText(row, "creator_ref"); if (!/^[A-Za-z0-9_-]{1,64}$/.test(ref) || creators.has(ref)) throw new Error("Creator XLSX invalid or duplicate creator_ref");
    for (const name of ["初次收录", "建联方式", "头像 URL", "分组"]) optionalText(row, name);
    for (const name of ["最近触达", "更新时间"]) { const v = row.get(name); if (v != null && !(v instanceof Date)) throw new Error(`Creator XLSX requires date cell in ${name}`); }
    const counts: TransferCreator["follower_counts"] = {}; for (const [i, p] of ["x", "instagram", "telegram", "youtube"].entries()) { const count = optionalNumber(row, creatorHeaders[4 + i]!); if (count !== null) counts[platform.parse(p)] = count; }
    if (size >= 16) { const count = optionalNumber(row, "TikTok 粉丝"); if (count !== null) counts.tiktok = count; }
    const score = optionalNumber(row, "评分"); if (score !== null && score > 100) throw new Error("Creator XLSX score must be 0-100");
    const pinned = row.get("置顶"); if (typeof pinned !== "boolean") throw new Error("Creator XLSX requires boolean in 置顶");
    creators.set(ref, { semantic_sha256: "", record_key: "", display_name: requiredText(row, "Creator"), tags: size >= 19 ? tags.parse(["标签 1", "标签 2", "标签 3"].map(key => optionalText(row, key)).filter(v => v !== null)) : [], notes: size === 20 ? optionalText(row, "备注") : null, score, follower_counts: counts, pinned, relationship: { stage: stage.parse(requiredText(row, "阶段")) }, accounts: [], contact_routes: [] });
  }
  for (const row of sheetRows(book.worksheets[1]!, accountHeaders)) {
    const c = creators.get(requiredText(row, "creator_ref")); if (!c) throw new Error("Creator XLSX Accounts row has orphan creator_ref"); requiredText(row, "Creator");
    const capture_order = order(row, "capture order"); if (c.accounts.some(a => a.capture_order === capture_order)) throw new Error("Creator XLSX duplicate account capture order");
    const profile = { handle: optionalText(row, "handle"), display_name: optionalText(row, "profile display name"), profile_url: optionalText(row, "profile URL"), avatar_url: optionalText(row, "avatar URL") };
    c.accounts.push({ capture_order, platform: platform.parse(requiredText(row, "platform")), target_kind: requiredText(row, "target_kind") as TransferCreator["accounts"][number]["target_kind"], external_id: requiredText(row, "external_id"), link_state: requiredText(row, "link state") as "claimed", confidence: confidence.parse(requiredText(row, "confidence")), source_kind: sourceKind.parse(requiredText(row, "source kind")), latest_profile: Object.values(profile).some(v => v !== null) ? profile : null });
  }
  for (const row of sheetRows(book.worksheets[2]!, contactHeaders)) {
    const c = creators.get(requiredText(row, "creator_ref")); if (!c) throw new Error("Creator XLSX Contacts row has orphan creator_ref"); requiredText(row, "Creator");
    const save_order = order(row, "save order"); if (c.contact_routes.some(r => r.save_order === save_order)) throw new Error("Creator XLSX duplicate contact save order");
    const sources = optionalText(row, "source kinds"); if (sources !== null && (sources.includes("\r") || sources.split("\n").some(line => !line || trim(line) !== line))) throw new Error("Creator XLSX source kinds require unpadded LF-separated values");
    c.contact_routes.push({ save_order, channel: channel.parse(requiredText(row, "channel")), locator_kind: locatorKind.parse(requiredText(row, "locator kind")), display_value: requiredText(row, "display value"), normalized_value: requiredText(row, "normalized value"), confidence: confidence.parse(requiredText(row, "confidence")), source_kinds: sources === null ? [] : sources.split("\n").map(v => sourceKind.parse(v)) });
  }
  const records = [...creators.entries()].sort((a, b) => compareText(a[0], b[0])).map(([, c]) => c);
  if (size === 15 && records.some(c => c.follower_counts.tiktok !== undefined || c.accounts.some(a => a.platform === "tiktok") || c.contact_routes.some(r => r.channel === "tiktok"))) throw new Error("Creator workbook v1 cannot contain TikTok");
  return buildBundle(records);
}
export async function encodeWorkbook(input: TransferBundle, display = new Map<string, WorkbookDisplay>()): Promise<Buffer> {
  const bundle = validateBundle(input), book = new ExcelJS.Workbook(); book.creator = "Reacher"; book.title = "Reacher Creators"; book.created = new Date("2000-01-01T00:00:00.000Z"); book.modified = book.created;
  const sheets = [creatorHeaders, accountHeaders, contactHeaders].map((headers, index) => { const sheet = book.addWorksheet(["Creators", "Accounts", "Contacts"][index]!); sheet.addRow(headers); sheet.views = [{ state: "frozen", xSplit: 2, ySplit: 1 }]; sheet.getColumn(1).hidden = true; sheet.getRow(1).height = 26; sheet.getRow(1).eachCell(cell => { cell.font = { bold: true, color: { argb: "FFFFFFFF" } }; cell.fill = { type: "pattern", pattern: "solid", fgColor: { argb: "FF1F2937" } }; }); sheet.columns.forEach((column, i) => { column.width = i === 1 ? 30 : 24; }); return sheet; });
  for (const [index, c] of bundle.creators.entries()) {
    const ref = `creator_${String(index).padStart(6, "0")}`, info = display.get(c.record_key), first = c.accounts.filter(a => a.link_state !== "rejected").sort((a, b) => a.capture_order - b.capture_order)[0], avatar = c.accounts.filter(a => a.link_state !== "rejected" && a.latest_profile?.avatar_url).sort((a, b) => a.capture_order - b.capture_order)[0]?.latest_profile?.avatar_url;
    const row = sheets[0]!.addRow([ref, c.display_name, first ? `${first.platform} · ${first.latest_profile?.handle ?? first.latest_profile?.display_name ?? first.external_id}` : null, [...new Set(c.contact_routes.map(r => r.channel))].join("\n") || null, c.follower_counts.x ?? null, c.follower_counts.instagram ?? null, c.follower_counts.telegram ?? null, c.follower_counts.youtube ?? null, c.score, c.relationship.stage, c.pinned, info?.last_touch_at == null ? null : new Date(info.last_touch_at * 1000), info ? new Date(info.updated_at * 1000) : null, avatar ? { text: avatar, hyperlink: avatar } : null, info?.group_name ?? null, c.follower_counts.tiktok ?? null, c.tags[0] ?? null, c.tags[1] ?? null, c.tags[2] ?? null, c.notes ?? null]);
    row.eachCell({ includeEmpty: true }, (cell, col) => { const readonly = [1, 3, 4, 12, 13, 14, 15].includes(col); cell.protection = { locked: readonly }; if (readonly) { cell.fill = { type: "pattern", pattern: "solid", fgColor: { argb: "FFF3F4F6" } }; cell.font = { color: { argb: "FF6B7280" } }; } if ([12, 13].includes(col)) cell.numFmt = "yyyy-mm-dd hh:mm"; });
    row.getCell(9).dataValidation = { type: "whole", operator: "between", formulae: [0, 100] }; row.getCell(10).dataValidation = { type: "list", formulae: ['"prospect,engaged,collaborating,archived"'] };
    for (const a of c.accounts) sheets[1]!.addRow([ref, c.display_name, a.platform, a.target_kind, a.external_id, a.latest_profile?.handle ?? null, a.latest_profile?.display_name ?? null, a.latest_profile?.profile_url ?? null, a.latest_profile?.avatar_url ?? null, a.link_state, a.confidence, a.source_kind, a.capture_order]);
    for (const r of c.contact_routes) sheets[2]!.addRow([ref, c.display_name, r.save_order, r.channel, r.locator_kind, r.display_value, r.normalized_value, r.confidence, r.source_kinds.join("\n") || null]);
  }
  let cells = 0;
  for (const sheet of sheets) { cells += sheet.rowCount * sheet.columnCount; if (sheet.rowCount > 100000 || sheet.rowCount * sheet.columnCount > 1000000 || cells > 1000000) throw new Error("Creator XLSX tabular shape exceeds limits"); sheet.autoFilter = { from: { row: 1, column: 1 }, to: { row: sheet.rowCount, column: sheet.columnCount } }; if (sheet !== sheets[0]) sheet.eachRow((row, i) => { if (i > 1) row.eachCell({ includeEmpty: true }, (cell, col) => { cell.protection = { locked: col <= 2 }; }); }); await sheet.protect("", { insertRows: true, deleteRows: true, sort: true, autoFilter: true, spinCount: 0 }); }
  return canonicalWorkbookZip(Buffer.from(await book.xlsx.writeBuffer()));
}
export async function canonicalWorkbookZip(bytes: Buffer): Promise<Buffer> {
  const input = await JSZip.loadAsync(bytes), output = new JSZip();
  for (const name of Object.keys(input.files).sort(compareText)) { const file = input.files[name]!; if (!file.dir) output.file(name, await file.async("nodebuffer"), { createFolders: false, date: new Date("2000-01-01T00:00:00.000Z") }); }
  const result = await output.generateAsync({ type: "nodebuffer", compression: "DEFLATE" }); if (result.length > 32 * 1048576) throw new Error("Creator XLSX exceeds 32 MiB"); return result;
}
