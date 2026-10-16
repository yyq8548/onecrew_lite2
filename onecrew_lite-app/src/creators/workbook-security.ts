import yauzl from "yauzl";
import sax from "sax";
import type { Readable } from "node:stream";

export interface XmlElement { name: string; attributes: Record<string, string>; text: string }
export function xmlElements(bytes: Buffer, visit: (element: XmlElement) => void): void {
  const xml = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  const parser = sax.parser(true, { xmlns: true });
  parser.ENTITIES = { amp: "&", lt: "<", gt: ">", apos: "'", quot: '"' };
  const elements: XmlElement[] = [];
  let attributeNames = new Set<string>();
  parser.onopentagstart = () => { attributeNames = new Set(); };
  parser.onattribute = attribute => { if (attributeNames.has(attribute.name)) throw new Error("Creator XLSX duplicate XML attribute"); attributeNames.add(attribute.name); };
  parser.ondoctype = () => { throw new Error("Creator XLSX DTD content is unsupported"); };
  parser.onprocessinginstruction = instruction => { if (instruction.name !== "xml" || /encoding\s*=\s*["'](?!(?:utf-8|us-ascii|ascii)["'])/i.test(instruction.body)) throw new Error("Creator XLSX processing instructions or XML encoding unsupported"); };
  parser.onopentag = tag => {
    if (elements.length >= 64 || Object.keys(tag.attributes).length > 64) throw new Error("Creator XLSX XML structure exceeds limits");
    const attributes: Record<string, string> = {};
    for (const [key, attribute] of Object.entries(tag.attributes)) { const value = typeof attribute === "string" ? attribute : attribute.value; if (Buffer.byteLength(value) > 1048576) throw new Error("Creator XLSX XML attribute exceeds limit"); attributes[key] = value; }
    elements.push({ name: tag.name.split(":").at(-1)!, attributes, text: "" });
  };
  const onText = (value: string) => { if (Buffer.byteLength(value) > 1048576) throw new Error("Creator XLSX XML text exceeds limit"); for (const element of elements) { if (!["definedName", "si", "c", "t", "comment"].includes(element.name)) continue; element.text += value; if (element.name === "si" || element.name === "c") { if ([...element.text].length > 32767) throw new Error("Creator XLSX cell string exceeds limit"); } } };
  parser.ontext = onText; parser.oncdata = onText;
  parser.onclosetag = () => { visit(elements.pop()!); };
  parser.write(xml).close();
}
const fail = (message: string): never => { throw new Error(`Creator XLSX ${message}`); };
function localTarget(base: string, value: string): string {
  if (!value || /[\\?#%]/.test(value)) return fail("relationship target is unsafe");
  const segments = value.replace(/^\/+/, "").split("/");
  if (segments.some(s => !s || s === "." || s === ".." || !/^[A-Za-z0-9_.\[\]-]+$/.test(s))) return fail("relationship target is unsafe");
  return value.startsWith("/") || !base ? segments.join("/") : `${base}/${segments.join("/")}`;
}
function relationships(bytes: Buffer): XmlElement[] {
  const result: XmlElement[] = [], ids = new Set<string>();
  xmlElements(bytes, element => {
    if (element.name !== "Relationship") return;
    const { Id, Type, Target, TargetMode } = element.attributes;
    if (!Id || !Type || !Target || ids.has(Id)) fail("relationship attributes are missing or duplicated");
    ids.add(Id!);
    if (/\/(oleobject|package|control|activexcontrol|activexcontrolbinary|ctrlprop|vbaproject|externallink|externallinkpath|attachedtemplate)$/i.test(Type!) || Type!.toLowerCase().includes("/customui")) fail("contains active relationships");
    if (TargetMode?.toLowerCase() === "external") { const url = new URL(Target!); if (!Type!.endsWith("/hyperlink") || !["http:", "https:"].includes(url.protocol) || !url.hostname) fail("external relationships are unsupported"); }
    result.push(element);
  });
  return result;
}
function cellPosition(value: string): [number, number] {
  const match = /^\$?([A-Za-z]+)\$?([0-9]+)$/.exec(value);
  if (!match) return fail("cell reference is invalid");
  const row = Number(match[2]); let col = 0;
  for (const letter of match[1]!.toUpperCase()) col = col * 26 + letter.charCodeAt(0) - 64;
  if (!Number.isSafeInteger(row) || row < 1 || row > 100000 || col < 1 || col > 256) return fail("worksheet dimensions exceed limits");
  return [row, col];
}
export async function preflightWorkbook(bytes: Buffer): Promise<{ parts: Map<string, Buffer>; sheetNames: string[] }> {
  if (bytes.length > 32 * 1048576 || bytes.subarray(0, 4).toString("hex") !== "504b0304") fail("is not a bounded XLSX ZIP package");
  const zip = await new Promise<yauzl.ZipFile>((resolve, reject) => yauzl.fromBuffer(bytes, { lazyEntries: true, validateEntrySizes: true, strictFileNames: true }, (error, result) => error ? reject(error) : resolve(result!)));
  if (zip.entryCount > 512) { zip.close(); fail("contains too many ZIP entries"); }
  const parts = new Map<string, Buffer>(), names = new Set<string>(), folded = new Set<string>(); let total = 0;
  try {
    await new Promise<void>((resolve, reject) => {
      zip.on("error", reject); zip.on("end", resolve);
      zip.on("entry", (entry: yauzl.Entry) => {
        void (async () => {
          const name = entry.fileName, lower = name.toLowerCase(), mode = entry.externalFileAttributes >>> 16 & 0o170000;
          if (!name || /[\0\\]/.test(name) || name.startsWith("/") || name.split("/").some(s => !s || s === "." || s === "..") || names.has(name) || folded.has(lower)) fail("contains unsafe or duplicate ZIP entry names");
          names.add(name); folded.add(lower);
          if (entry.isEncrypted() || ![0, 8].includes(entry.compressionMethod) || mode !== 0 && mode !== 0o100000) fail("ZIP entry is encrypted or not a supported regular file");
          if (/vbaproject|externallinks\/|embeddings\/|activex\/|customui\/|connections\.xml|querytables\/|pivottables\/|pivotcache\/|charts\/|media\/|macrosheets\/|intlmacrosheets\/|\.(bin|xlsb|xlsm)$/i.test(name)) fail("contains active or external content");
          const limit = name.startsWith("xl/worksheets/") || name === "xl/sharedStrings.xml" ? 64 * 1048576 : 8 * 1048576;
          if (entry.uncompressedSize > limit || entry.uncompressedSize > entry.compressedSize * 100) fail("ZIP part exceeds size or expansion limit");
          const stream = await new Promise<Readable>((done, error) => zip.openReadStream(entry, (err, value) => err ? error(err) : done(value!)));
          const buffers: Buffer[] = []; let size = 0;
          for await (const value of stream) { const buffer = Buffer.from(value); size += buffer.length; if (size > limit || size > entry.compressedSize * 100 || total + size > 256 * 1048576) { stream.destroy(); fail("decompressed size exceeds limits"); } buffers.push(buffer); }
          total += size;
          if (name.endsWith(".xml") || name.endsWith(".rels")) { const content = Buffer.concat(buffers); xmlElements(content, () => {}); parts.set(name, content); }
          zip.readEntry();
        })().then(undefined, reject);
      });
      zip.readEntry();
    });
  } finally { zip.close(); }
  const required = (name: string) => parts.get(name) ?? fail(`is missing ${name}`);
  const root = relationships(required("_rels/.rels")).filter(r => ["http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument", "http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument"].includes(r.attributes.Type!));
  if (root.length !== 1 || root[0]!.attributes.TargetMode?.toLowerCase() === "external" || localTarget("", root[0]!.attributes.Target!) !== "xl/workbook.xml") fail("must reference exactly xl/workbook.xml");
  const overrides = new Set<string>(), defaults = new Set<string>(); let workbooks = 0;
  xmlElements(required("[Content_Types].xml"), e => {
    const { ContentType, PartName, Extension } = e.attributes;
    if (!["Override", "Default"].includes(e.name)) return;
    if (!ContentType) fail("content type missing");
    if (e.name === "Override") { if (!PartName?.startsWith("/")) fail("content type path must be absolute"); const part = localTarget("", PartName!); if (!names.has(part) || overrides.has(part)) fail("content type target is missing or duplicated"); overrides.add(part); if (ContentType!.toLowerCase() === "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml") { if (part !== "xl/workbook.xml") fail("alternate workbook root unsupported"); workbooks++; } else if (ContentType!.toLowerCase().includes("spreadsheetml.sheet.main")) fail("unsupported workbook content type"); }
    else { if (!Extension || !/^[A-Za-z0-9]+$/.test(Extension) || defaults.has(Extension.toLowerCase())) fail("invalid duplicate default content type"); defaults.add(Extension!.toLowerCase()); if (![...names].some(n => n.toLowerCase().endsWith(`.${Extension!.toLowerCase()}`))) return; }
    if (/macroenabled|vnd\.ms-office\.(vbaproject|activex)|officedocument\.(oleobject|package)|spreadsheetml\.externallink|vnd\.ms-excel\.controlproperties\+xml/i.test(ContentType!)) fail("contains active content type");
  });
  if (workbooks !== 1) fail("must declare exactly one canonical workbook");
  for (const [name, content] of parts) if (name.endsWith(".rels")) relationships(content);
  const workbookRels = relationships(required("xl/_rels/workbook.xml.rels")), sheetNames: string[] = [], sheetParts = new Set<string>();
  const shared = workbookRels.filter(r => r.attributes.Type!.endsWith("/sharedStrings"));
  if (shared.length > 1) fail("sharedStrings relationship duplicated");
  if (shared.length) { let count = 0; xmlElements(required(localTarget("xl", shared[0]!.attributes.Target!)), e => { if (e.name === "si" && ++count > 500000 || e.name === "sst" && e.attributes.uniqueCount !== undefined && (!/^[0-9]+$/.test(e.attributes.uniqueCount) || Number(e.attributes.uniqueCount) > 500000)) fail("has too many shared strings"); }); }
  xmlElements(required("xl/workbook.xml"), e => {
    if (e.name === "definedName") { const id = Number(e.attributes.localSheetId), sheet = ["Creators", "Accounts", "Contacts"][id]; const prefix = `${sheet}!$A$1:$`; if (e.attributes.name !== "_xlnm._FilterDatabase" || !["1", "true"].includes(e.attributes.hidden!) || !Number.isInteger(id) || !sheet || !e.text.startsWith(prefix)) fail("defined-name formulas unsupported"); cellPosition(e.text.slice(prefix.length).replace("$", "")); }
    if (e.name !== "sheet") return;
    const name = e.attributes.name, id = e.attributes["r:id"] ?? e.attributes.id;
    const rel = workbookRels.find(r => r.attributes.Id === id && r.attributes.Type!.endsWith("/worksheet"));
    if (!name || !rel) fail("sheet relationship unresolved");
    const part = localTarget("xl", rel!.attributes.Target!); required(part);
    if (sheetParts.has(part) || sheetNames.some(n => n.toLowerCase() === name!.toLowerCase())) fail("duplicate sheet");
    sheetNames.push(name!); sheetParts.add(part);
  });
  if (!sheetNames.length || sheetNames.length > 32) fail("unsupported sheet count");
  let allCells = 0;
  for (const part of sheetParts) {
    const rows = new Set<string>(), cells = new Set<string>(); let maxRow = 0, maxCol = 0;
    xmlElements(required(part), e => {
      if (e.name === "f") fail("formulas unsupported");
      if (e.name === "dimension") { if (e.attributes.ref) { const [r, c] = cellPosition(e.attributes.ref.split(":").at(-1)!); if (r * c > 1000000) fail("declared area exceeds limit"); } }
      if (e.name === "row") { const ref = e.attributes.r; if (!ref || !/^[0-9]+$/.test(ref) || Number(ref) < 1 || Number(ref) > 100000 || rows.has(String(Number(ref)))) fail("invalid or duplicate worksheet row"); rows.add(String(Number(ref))); }
      if (e.name === "c") { const [r, c] = cellPosition(e.attributes.r ?? ""); const key = `${r}:${c}`; if (cells.has(key) || ++allCells > 1000000) fail("duplicate or excessive cells"); cells.add(key); maxRow = Math.max(r, maxRow); maxCol = Math.max(c, maxCol); }
    });
    if (maxRow * maxCol > 1000000) fail("populated area exceeds limit");
  }
  return { parts, sheetNames };
}
