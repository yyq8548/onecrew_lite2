import { createRequire, findPackageJSON } from "node:module";
import { execFile } from "node:child_process";
import { promisify, parseArgs } from "node:util";
import { chmod, copyFile, lstat, mkdir, readFile, readdir, realpath, writeFile } from "node:fs/promises";
import { basename, dirname, join, relative, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { createPayloadManifest, payloadDigest, payloadBaseSchema } from "./payload-manifest.ts";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { readDevelopmentBuildIdentity } from "../service/build-identity.ts";
import { buildCurrentMcpRegistry, parseCurrentMcpRegistry } from "../service/mcp-catalogue.ts";
import JSZip from "jszip";
import { load } from "cheerio";
import { OPENCLI_ADDON_FILES } from "../platforms/opencli/addons.ts";
import { supplementalLicense } from "./supplemental-license.ts";

const execute = promisify(execFile);
const root = fileURLToPath(new URL("../../../", import.meta.url));
const libraryRequire = createRequire(join(root, "onecrew_lite-library", "package.json"));
const appPackage = pathToFileURL(join(root, "onecrew_lite-app", "package.json")).href;
const exists = async (path: string) => lstat(path).then(() => true, error => { if (error.code === "ENOENT") return false; throw error; });
const json = async (path: string) => parseApplicationJson(await readFile(path)) as Record<string, unknown>;

export async function buildCandidate(options: { output: string; productVersion: string; target: "win32-x64" | "darwin-arm64" | "darwin-x64"; nodePath: string; webAssets: string; libraryWebAssets: string; windowsRuntime?: string }): Promise<void> {
  if (process.version !== "v26.7.0") throw new Error("package.node_version_required");
  if (options.target === "win32-x64" && !options.windowsRuntime) throw new Error("package.windows_runtime_required");
  const output = resolve(options.output);
  if (await exists(output)) throw new Error("package.output_already_exists");
  for (const [directory, prefix] of [[options.webAssets, "/assets/"], [options.libraryWebAssets, "/library/assets/"]] as const) {
    const document = load(await readFile(join(directory, "index.html"), "utf8"));
    const scripts = document("script[src]").map((_, script) => document(script).attr("src")!).get();
    if (!scripts.length || scripts.some(source => !source.startsWith(prefix))) throw new Error("package.frontend_build_mode_mismatch");
  }
  const before = await readDevelopmentBuildIdentity();
  const nodeSha256 = payloadDigest(await readFile(options.nodePath));
  if (options.target === `${process.platform}-${process.arch}`) {
    const version = await execute(resolve(options.nodePath), ["--version"], { timeout: 15_000, windowsHide: true });
    if (version.stdout.trim() !== process.version) throw new Error("package.target_node_version_mismatch");
  }
  const stage = join(output, "payload");
  await mkdir(output); await mkdir(stage);
  async function copyTree(source: string, destination: string): Promise<void> {
    const info = await lstat(source);
    if (info.isSymbolicLink()) throw new Error(`package.input_symlink:${source}`);
    if (info.isDirectory()) {
      await mkdir(destination, { recursive: true });
      for (const child of await readdir(source)) if (child !== "node_modules" && (basename(source) !== "prebuilds" || child === options.target)) await copyTree(join(source, child), join(destination, child));
    } else if (info.isFile()) {
      if (source.endsWith(".ps1")) {
        const script = await readFile(source);
        if (script.some(byte => byte > 127) && !script.subarray(0, 3).equals(Buffer.from([0xef, 0xbb, 0xbf]))) throw new Error(`package.powershell_utf8_bom_required:${source}`);
      }
      await mkdir(dirname(destination), { recursive: true }); await copyFile(source, destination);
    } else throw new Error(`package.input_not_regular:${source}`);
  }
  const components: { name: string; version: string; license: string; licenseFiles: string[]; licenseEvidence?: "full_text" | "declared_only"; licenseProvenance?: { file: string; source: string; sha256: string }[] }[] = [];
  const licensed = new Set<string>();
  async function licensePackage(packageRoot: string): Promise<void> {
    if (licensed.has(packageRoot)) return; licensed.add(packageRoot);
    const metadata = await json(join(packageRoot, "package.json"));
    const identity = `${String(metadata.name)}@${String(metadata.version)}`, destination = `licenses/${payloadDigest(identity).slice(0, 24)}`;
    const licenseFiles: string[] = [];
    for (const name of await readdir(packageRoot)) if (/^(?:licen[sc]e|copying|notice)(?:[.-].*)?$/iu.test(name) && (await lstat(join(packageRoot, name))).isFile()) {
      await copyTree(join(packageRoot, name), join(stage, destination, name)); licenseFiles.push(`${destination}/${name}`);
    }
    const teableRoot = join(root, "onecrew_lite-library", "vendor", "teable");
    if (!licenseFiles.length && relative(teableRoot, packageRoot).startsWith("packages")) {
      const inherited = metadata.license === "MIT" ? join(teableRoot, "packages", "core", "LICENSE") : join(teableRoot, "LICENSE");
      await copyTree(inherited, join(stage, destination, "LICENSE")); licenseFiles.push(`${destination}/LICENSE`);
    }
    const supplemental = !licenseFiles.length ? await supplementalLicense(packageRoot) : null;
    const licenseProvenance = supplemental ? [{ file: `${destination}/LICENSE.verified`, source: supplemental.source, sha256: supplemental.sha256 }] : undefined;
    if (supplemental) {
      await copyTree(supplemental.path, join(stage, destination, "LICENSE.verified"));
      licenseFiles.push(`${destination}/LICENSE.verified`);
    }
    const licenseEvidence = licenseFiles.length ? "full_text" : "declared_only";
    if (!licenseFiles.length) {
      await copyTree(join(packageRoot, "package.json"), join(stage, destination, "package.json")); licenseFiles.push(`${destination}/package.json`);
    }
    const declaration = supplemental?.license ?? (typeof metadata.license === "string" ? metadata.license : metadata.license || metadata.licenses ? JSON.stringify(metadata.license ?? metadata.licenses) : metadata.private === true ? "UNLICENSED" : licenseEvidence === "full_text" ? "SEE_LICENSE_FILES" : "UNKNOWN");
    components.push({ name: String(metadata.name), version: String(metadata.version), license: declaration, licenseFiles, licenseEvidence, ...(licenseProvenance ? { licenseProvenance } : {}) });
  }
  const packageRoots = new Set<string>();
  const hoisted = new Map<string, string>();
  async function stagePackage(name: string, from: string, destination: string, ancestors: ReadonlyMap<string, string>): Promise<void> {
    const packageFile = findPackageJSON(name, from);
    if (!packageFile) throw new Error(`package.dependency_missing:${name}`);
    const source = await realpath(dirname(packageFile));
    if (ancestors.get(name) === source) return;
    if (!ancestors.has(name)) {
      if (hoisted.get(name) === source) return;
      if (!hoisted.has(name)) { hoisted.set(name, source); destination = join(stage, "node_modules", ...name.split("/")); }
    }
    const metadata = await json(join(source, "package.json"));
    await copyTree(source, destination); await licensePackage(source); packageRoots.add(source);
    const next = new Map(ancestors); next.set(name, source);
    const dependencies = metadata.dependencies as Record<string, string> | undefined;
    for (const dependency of Object.keys(dependencies ?? {})) await stagePackage(dependency, pathToFileURL(join(source, "package.json")).href, join(destination, "node_modules", ...dependency.split("/")), next);
    const optional = metadata.optionalDependencies as Record<string, string> | undefined;
    for (const dependency of Object.keys(optional ?? {})) {
      if (name === "wreq-js" && dependency !== `@wreq-js/binding-${options.target}${options.target.startsWith("win32-") ? "-msvc" : ""}`) continue;
      const location = findPackageJSON(dependency, pathToFileURL(join(source, "package.json")).href);
      if (!location) throw new Error(`package.optional_runtime_not_installed:${dependency}`);
      const dependencyMetadata = await json(location);
      const os = dependencyMetadata.os as string[] | undefined, cpu = dependencyMetadata.cpu as string[] | undefined;
      const [platform, architecture] = options.target.split("-");
      if (os && !os.includes(platform!) || cpu && !cpu.includes(architecture!)) continue;
      await stagePackage(dependency, pathToFileURL(join(source, "package.json")).href, join(destination, "node_modules", ...dependency.split("/")), next);
    }
  }
  const esbuild = libraryRequire("esbuild") as typeof import("esbuild");
  const parserPackage = findPackageJSON("jsonc-parser", pathToFileURL(join(root, "onecrew_lite-contracts", "package.json")).href);
  if (!parserPackage) throw new Error("package.json_parser_missing");
  const alias = { "jsonc-parser": join(dirname(parserPackage), "lib", "esm", "main.js") };
  const external = ["@electric-sql/pglite", "@electric-sql/pglite/*", "fs-native-extensions", "wreq-js", "@primno/dpapi", "@mtcute/wasm"];
  const built = await esbuild.build({ entryPoints: [join(root, "onecrew_lite-app", "src", "cli.ts")], outfile: join(stage, "app", "cli.mjs"), bundle: true, format: "esm", platform: "node", target: "node26", external, alias, metafile: true,
    banner: { js: "import {createRequire as __createRequire} from 'node:module'; const require=__createRequire(import.meta.url);" },
    plugins: [{ name: "reject-unlisted-native-addon", setup(build) { build.onResolve({ filter: /\.node$/ }, args => { throw new Error(`package.unlisted_native_addon:${args.path}`); }); } }],
    tsconfig: join(root, "onecrew_lite-library", "tsconfig.build.json"), absWorkingDir: root, logLevel: "info" });
  await copyTree(join(root, "onecrew_lite-app", "src", "packaging", "runtime-loader.cjs"), join(stage, "app", "runtime-loader.cjs"));
  for (const input of Object.keys(built.metafile!.inputs)) {
    let directory = dirname(resolve(root, input));
    while (directory !== dirname(directory)) {
      if (await exists(join(directory, "package.json"))) { const metadata = await json(join(directory, "package.json")); if (typeof metadata.name === "string" && typeof metadata.version === "string") { await licensePackage(directory); break; } }
      directory = dirname(directory);
    }
  }
  for (const name of ["@electric-sql/pglite", "fs-native-extensions", "wreq-js", ...(options.target.startsWith("win32-") ? ["@primno/dpapi"] : [])]) await stagePackage(name, name === "@electric-sql/pglite" ? pathToFileURL(join(root, "onecrew_lite-library", "package.json")).href : appPackage, join(stage, "node_modules", ...name.split("/")), new Map());
  const telegramPackage = findPackageJSON("@mtcute/node", appPackage); if (!telegramPackage) throw new Error("package.telegram_runtime_missing");
  await stagePackage("@mtcute/wasm", pathToFileURL(await realpath(telegramPackage)).href, join(stage, "node_modules", "@mtcute", "wasm"), new Map());
  for (const directory of ["storage", "migration", "discovery"]) for (const name of await readdir(join(root, "onecrew_lite-app", "src", directory))) if (/\.(?:sql|ps1|json)$/u.test(name) || name === "assets") await copyTree(join(root, "onecrew_lite-app", "src", directory, name), join(stage, "resources", directory, name));
  const currentRegistry = Buffer.from(stringifyApplicationJson(await buildCurrentMcpRegistry()));
  parseCurrentMcpRegistry(currentRegistry);
  await writeFile(join(stage, "resources", "mcp-current-registry.json"), currentRegistry, { flag: "wx" });
  await copyTree(join(root, "onecrew_lite-app", "src", "service", "workflow-guides.json"), join(stage, "resources", "workflow-guides.json"));
  await copyTree(join(root, "onecrew_lite-app", "src", "service", "windows-host.ps1"), join(stage, "resources", "service", "windows-host.ps1"));
  await copyTree(join(root, "onecrew_lite-mcp-ui", "dist"), join(stage, "resources", "mcp"));
  for (const [id, names] of Object.entries(OPENCLI_ADDON_FILES)) for (const name of names) await copyTree(join(root, "onecrew_lite-opencli", id, name), join(stage, "resources", "opencli-addons", id, name));
  for (const name of await readdir(join(root, "onecrew_lite-app", "src", "platforms", "browser"))) if (name.endsWith(".ps1")) await copyTree(join(root, "onecrew_lite-app", "src", "platforms", "browser", name), join(stage, "resources", "browser", name));
  await copyTree(join(root, "onecrew_lite-web", "public", "browser-binding.html"), join(stage, "resources", "browser", "browser-binding.html"));
  await copyTree(join(root, "onecrew_lite-app", "src", "client-adapters", "plugin-template"), join(stage, "resources", "client-plugin"));
  await copyTree(options.webAssets, join(stage, "resources", "web"));
  await copyTree(options.libraryWebAssets, join(stage, "resources", "library-web"));
  const frontendPackages = new Set<string>();
  async function frontendLicenses(packageFile: string): Promise<void> {
    const packageRoot = await realpath(dirname(packageFile));
    if (frontendPackages.has(packageRoot)) return; frontendPackages.add(packageRoot);
    const metadata = await json(packageFile);
    if (typeof metadata.version === "string") await licensePackage(packageRoot);
    for (const name of Object.keys((metadata.dependencies ?? {}) as Record<string, unknown>)) {
      const dependency = findPackageJSON(name, pathToFileURL(join(packageRoot, "package.json")).href);
      if (!dependency) throw new Error(`package.frontend_dependency_missing:${name}`);
      await frontendLicenses(dependency);
    }
  }
  await frontendLicenses(join(root, "onecrew_lite-web", "package.json"));
  await frontendLicenses(join(root, "onecrew_lite-library", "web", "package.json"));
  await copyTree(join(root, "pnpm-lock.yaml"), join(stage, "build", "pnpm-lock.yaml"));
  const nodeLicense = options.target.startsWith("darwin-") ? join(dirname(options.nodePath), "..", "LICENSE") : join(dirname(options.nodePath), "LICENSE");
  await copyTree(nodeLicense, join(stage, "licenses", "node.txt")); components.push({ name: "node", version: process.version.slice(1), license: "MIT", licenseFiles: ["licenses/node.txt"] });
  if (options.target === "win32-x64") {
    const sourcePath = join(root, "development", "windows-runtime.json"), source = await json(sourcePath);
    const dll = join(options.windowsRuntime!, "vcruntime140.dll");
    if (payloadDigest(await readFile(dll)) !== source.dllSha256) throw new Error("package.windows_runtime_hash_mismatch");
    await copyTree(dll, join(stage, "runtime", "vcruntime140.dll"));
    await copyTree(sourcePath, join(stage, "runtime", "source.json"));
    await copyTree(join(root, "development", "windows-runtime-notice.txt"), join(stage, "licenses", "Visual-C++-Runtime.txt"));
    components.push({ name: "Microsoft Visual C++ Runtime", version: String(source.version), license: "Microsoft-Visual-C++-Runtime", licenseFiles: ["licenses/Visual-C++-Runtime.txt"], licenseEvidence: "declared_only" });
  }
  const files: { path: string; size: number; sha256: string; executable: boolean }[] = [];
  async function inventory(directory: string): Promise<void> {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) await inventory(path);
      else if (entry.isFile()) { const bytes = await readFile(path); files.push({ path: relative(stage, path).replaceAll("\\", "/"), size: bytes.length, sha256: payloadDigest(bytes), executable: false }); }
      else throw new Error("package.staged_special_file");
    }
  }
  await inventory(stage); files.sort((a, b) => Buffer.compare(Buffer.from(a.path), Buffer.from(b.path)));
  components.sort((a, b) => Buffer.compare(Buffer.from(a.name + a.version), Buffer.from(b.name + b.version)));
  const checkoutCommit = (await execute("git", ["rev-parse", "HEAD"], { cwd: root })).stdout.trim();
  const dirty = (await execute("git", ["status", "--porcelain"], { cwd: root, maxBuffer: 16 * 1024 * 1024 })).stdout.length > 0;
  const manifest = createPayloadManifest({ format: 1, product: "onecrew_lite", state: "candidate", productVersion: options.productVersion, builtAt: Math.floor(Date.now() / 1000), target: options.target, nodeVersion: process.version,
    source: { baselineRepository: "https://github.com/yyq8548/onecrew_lite.git", baselineCommit: "efc3c6eb69507b9809bb30279d9af26156d075e3", checkoutCommit, sourceTreeSha256: before.source_tree_sha256, dirty }, lockfileSha256: before.dependency_lock_sha256, components, files });
  const after = await readDevelopmentBuildIdentity();
  if (JSON.stringify(before) !== JSON.stringify(after)) throw new Error("package.source_changed_during_build");
  const manifestPath = join(output, "runtime-manifest.json"); await writeFile(manifestPath, JSON.stringify(manifest), { flag: "wx" });
  const launcher = join(output, "bootstrap.mjs");
  await esbuild.build({ entryPoints: [join(root, "onecrew_lite-app", "src", "packaging", "sea-bootstrap.ts")], outfile: launcher, bundle: true, format: "esm", platform: "node", target: "node26", logLevel: "info", alias, banner: { js: "import {createRequire as __bootstrapRequire} from 'node:module'; const require=__bootstrapRequire(process.execPath);" } });
  if (JSON.stringify(before) !== JSON.stringify(await readDevelopmentBuildIdentity())) throw new Error("package.source_changed_during_bootstrap_build");
  const assets = Object.fromEntries(files.map(file => [file.path, join(stage, ...file.path.split("/"))])); assets["runtime-manifest.json"] = manifestPath;
  const executable = join(output, options.target.startsWith("win32-") ? "onecrew_lite.exe" : "onecrew_lite");
  const config = join(output, "sea-config.json");
  await writeFile(config, JSON.stringify({ main: launcher, mainFormat: "module", executable: resolve(options.nodePath), output: executable, disableExperimentalSEAWarning: true, useSnapshot: false, useCodeCache: false, execArgvExtension: "none", assets }), { flag: "wx" });
  if (options.target === "darwin-x64") {
    const blob = join(output, "sea.blob"), blobConfig = join(output, "sea-blob-config.json");
    await writeFile(blobConfig, JSON.stringify({ main: launcher, mainFormat: "module", output: blob, disableExperimentalSEAWarning: true, useSnapshot: false, useCodeCache: false, execArgvExtension: "none", assets }), { flag: "wx" });
    await execute(process.execPath, [`--experimental-sea-config=${blobConfig}`], { cwd: output, timeout: 240_000, maxBuffer: 4 * 1024 * 1024 });
    await copyFile(options.nodePath, executable);
    if (process.platform === "darwin") await execute("/usr/bin/codesign", ["--remove-signature", executable]);
    const postject = libraryRequire("postject") as { inject(path: string, name: string, data: Uint8Array, options: { sentinelFuse: string; machoSegmentName: string }): Promise<void> };
    await postject.inject(executable, "NODE_SEA_BLOB", await readFile(blob), { sentinelFuse: "NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2", machoSegmentName: "NODE_SEA" });
  } else await execute(process.execPath, [`--build-sea=${config}`], { cwd: output, timeout: 240_000, maxBuffer: 4 * 1024 * 1024 });
  if (payloadDigest(await readFile(options.nodePath)) !== nodeSha256) throw new Error("package.target_node_changed");
  if (options.target.startsWith("darwin-") && process.platform === "darwin") await execute("/usr/bin/codesign", ["--force", "--sign", "-", executable]);
  await chmod(executable, 0o755);
  const executableBytes = await readFile(executable);
  const receipt = { productVersion: options.productVersion, target: options.target, nodeSha256, payloadSha256: manifest.payloadSha256, executableSha256: payloadDigest(executableBytes), executableBytes: executableBytes.length, productionReady: false, licenseTextMissing: manifest.components.filter(component => component.licenseEvidence === "declared_only").map(component => `${component.name}@${component.version}`) };
  await writeFile(join(output, "candidate.json"), JSON.stringify(receipt), { flag: "wx" });
  const zip = new JSZip();
  zip.file(basename(executable), executableBytes, { unixPermissions: 0o100755 });
  zip.file("runtime-manifest.json", JSON.stringify(manifest)); zip.file("candidate.json", JSON.stringify(receipt));
  for (const file of files.filter(file => file.path.startsWith("licenses/"))) zip.file(file.path, await readFile(join(stage, ...file.path.split("/"))));
  const archive = await zip.generateAsync({ type: "nodebuffer", compression: "DEFLATE", compressionOptions: { level: 6 }, platform: options.target.startsWith("win32-") ? "DOS" : "UNIX" });
  const archiveName = `OneCrew-Lite-${options.productVersion}-${options.target}-candidate.zip`;
  await writeFile(join(output, archiveName), archive, { flag: "wx" });
  await writeFile(join(output, "archive.json"), JSON.stringify({ path: archiveName, bytes: archive.length, sha256: payloadDigest(archive), productionReady: false }), { flag: "wx" });
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const { values } = parseArgs({ options: { output: { type: "string" }, "product-version": { type: "string" }, target: { type: "string" }, node: { type: "string" }, "web-assets": { type: "string" }, "library-web-assets": { type: "string" }, "windows-runtime": { type: "string" } }, strict: true });
  if (!values.output || !values["product-version"] || !values.target || !values.node || !values["web-assets"] || !values["library-web-assets"]) throw new Error("package.explicit_build_inputs_required");
  const target = payloadBaseSchema.shape.target.parse(values.target);
  await buildCandidate({ output: values.output, productVersion: values["product-version"], target, nodePath: values.node, webAssets: values["web-assets"], libraryWebAssets: values["library-web-assets"], ...(values["windows-runtime"] ? { windowsRuntime: values["windows-runtime"] } : {}) });
}
