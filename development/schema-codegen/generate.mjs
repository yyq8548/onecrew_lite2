import {
  access,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rename,
  rm,
  writeFile,
} from "node:fs/promises";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import Ajv2020 from "ajv/dist/2020.js";
import standaloneCode from "ajv/dist/standalone/index.js";
import { build } from "esbuild";
import { compile } from "json-schema-to-typescript";
import ts from "typescript";

const toolDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = resolve(toolDirectory, "../..");
export async function runCodegen(argv = process.argv.slice(2)) {
  const args = parseArgs(argv);
  const mode = requiredArg(args, "mode");
  if (mode !== "generate" && mode !== "check") {
    throw new Error("--mode must be generate or check");
  }
  const configPath = resolve(requiredArg(args, "config"));
  const configDirectory = dirname(configPath);
  const config = validateConfig(JSON.parse(await readFile(configPath, "utf8")));
  const outputDirectory = resolve(configDirectory, config.output);
  const outputParent = dirname(outputDirectory);
  await mkdir(outputParent, { recursive: true });
  const stagingDirectory = await mkdtemp(join(outputParent, ".schema-codegen-"));
  const manifestPath = join(stagingDirectory, "owner-manifest.json");

  try {
    await exportManifest(config, manifestPath, configDirectory);
    const manifest = validateManifest(
      JSON.parse(await readFile(manifestPath, "utf8")),
      config.target,
    );
    await generateArtifacts(manifest, stagingDirectory);
    await rm(manifestPath);
    await verifyGeneratedArtifacts(manifest, stagingDirectory);

    if (mode === "check") {
      const differences = await compareDirectories(outputDirectory, stagingDirectory);
      if (differences.length !== 0) {
        throw new Error(
          `Generated contracts are stale:\n${differences.map((path) => `  ${path}`).join("\n")}`,
        );
      }
      console.log(`Contract artifacts are current for ${config.target}`);
    } else {
      await replaceDirectory(outputDirectory, stagingDirectory);
      console.log(`Generated contract artifacts for ${config.target}`);
    }
  } finally {
    await rm(stagingDirectory, { recursive: true, force: true });
  }
}

const invokedPath = process.argv[1] ? resolve(process.argv[1]) : null;
if (invokedPath === fileURLToPath(import.meta.url)) {
  await runCodegen();
}

async function exportManifest(config, manifestPath, configDirectory) {
  const source = await readFile(resolve(configDirectory, config.manifest), "utf8");
  validateManifest(JSON.parse(source), config.target);
  await writeFile(manifestPath, source);
}

async function generateArtifacts(manifest, stagingDirectory) {
  const typesDirectory = join(stagingDirectory, "types");
  const validatorsDirectory = join(stagingDirectory, "validators");
  await mkdir(typesDirectory);
  await mkdir(validatorsDirectory);

  const typeModules = [];
  for (const root of manifest.roots) {
    const declaration = await compile(root.schema, root.name, {
      bannerComment: "/* Generated. Do not edit. */",
      declareExternallyReferenced: true,
      ignoreMinAndMaxItems: true,
      unknownAny: true,
    });
    if (containsUnsafeAnyType(declaration)) {
      throw new Error(`${root.name} generated an unsafe any type`);
    }
    await writeFile(join(typesDirectory, `${root.name}.ts`), declaration);
    await generateValidator(root, validatorsDirectory);
    typeModules.push(root.name);
  }

  await writeFile(
    join(stagingDirectory, "index.ts"),
    `${typeModules.map((name) => `export type { ${name} } from "./types/${name}";`).join("\n")}\n`,
  );
  await writeFile(
    join(stagingDirectory, "manifest.json"),
    `${JSON.stringify(manifest, null, 2)}\n`,
  );
}

export function containsUnsafeAnyType(declaration) {
  const source = ts.createSourceFile("contract.ts", declaration, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  let found = false;
  function visit(node) {
    if (node.kind === ts.SyntaxKind.AnyKeyword) found = true;
    if (!found) ts.forEachChild(node, visit);
  }
  visit(source);
  return found;
}

export async function generateValidator(root, validatorsDirectory) {
  const ajv = new Ajv2020({
    allErrors: false,
    code: { esm: true, source: true },
    strict: true,
  });
  ajv.addFormat("uint64", true);
  ajv.addFormat("int64", true);
  // Rust integer-width formats are annotations; the emitted JSON Schema owns
  // integer/type/range validation. Floating widths likewise annotate numbers;
  // Ajv's number validation still rejects non-finite values.
  for (const format of ["uint", "int", "uint8", "int8", "uint16", "int16", "uint32", "int32", "float", "double"]) {
    ajv.addFormat(format, true);
  }
  ajv.addSchema(root.schema, root.id);
  const standalone = standaloneCode(ajv, {
    [`validate${root.name}`]: root.id,
  });
  const bundled = await build({
    bundle: true,
    format: "esm",
    minify: true,
    platform: "browser",
    stdin: {
      contents: standalone,
      resolveDir: toolDirectory,
      sourcefile: `${root.name}.raw.mjs`,
    },
    write: false,
  });
  const validatorJavaScript = bundled.outputFiles?.[0]?.text;
  if (!validatorJavaScript) {
    throw new Error(`Ajv validator bundling produced no output for ${root.name}`);
  }
  await writeFile(
    join(validatorsDirectory, `${root.name}.js`),
    validatorJavaScript,
  );
  await writeFile(
    join(validatorsDirectory, `${root.name}.d.ts`),
    `import type { ${root.name} } from "../types/${root.name}";\nexport type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };\nexport const validate${root.name}: ContractValidator<${root.name}>;\n`,
  );
}

async function verifyGeneratedArtifacts(manifest, directory) {
  for (const root of manifest.roots) {
    const moduleUrl = new URL(`file://${join(directory, "validators", `${root.name}.js`)}`).href;
    const validators = await import(`${moduleUrl}?revision=${root.revision}`);
    if (typeof validators[`validate${root.name}`] !== "function") {
      throw new Error(`Missing validator export validate${root.name}`);
    }
  }
}

export async function replaceDirectory(
  outputDirectory,
  stagingDirectory,
  operations = { rename, rm },
) {
  const move = operations.rename;
  const remove = operations.rm;
  const backup = `${outputDirectory}.backup`;
  await remove(backup, { recursive: true, force: true });
  const hadOutput = await exists(outputDirectory);
  if (hadOutput) {
    await move(outputDirectory, backup);
  }
  try {
    await move(stagingDirectory, outputDirectory);
    await remove(backup, { recursive: true, force: true });
  } catch (error) {
    if (hadOutput && !(await exists(outputDirectory))) {
      await move(backup, outputDirectory);
    }
    throw error;
  }
}

export async function compareDirectories(expectedDirectory, actualDirectory) {
  if (!(await exists(expectedDirectory))) {
    return [relative(repositoryRoot, expectedDirectory) || expectedDirectory];
  }
  const expectedFiles = await filesIn(expectedDirectory);
  const actualFiles = await filesIn(actualDirectory);
  const names = [...new Set([...expectedFiles, ...actualFiles])].sort();
  const differences = [];
  for (const name of names) {
    if (!expectedFiles.includes(name) || !actualFiles.includes(name)) {
      differences.push(name);
      continue;
    }
    const [expected, actual] = await Promise.all([
      readFile(join(expectedDirectory, name)),
      readFile(join(actualDirectory, name)),
    ]);
    if (!expected.equals(actual)) differences.push(name);
  }
  return differences;
}

async function filesIn(directory, prefix = "") {
  const entries = await readdir(join(directory, prefix), { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const name = join(prefix, entry.name);
    if (entry.isDirectory()) files.push(...await filesIn(directory, name));
    else if (entry.isFile()) files.push(name);
    else throw new Error(`Generated output contains a non-file entry: ${name}`);
  }
  return files.sort();
}

async function exists(path) {
  try {
    await access(path);
    return true;
  } catch {
    return false;
  }
}

function validateConfig(value) {
  for (const key of [
    "target",
    "manifest",
    "output",
  ]) {
    if (typeof value?.[key] !== "string" || value[key].length === 0) {
      throw new Error(`Invalid contract config field ${key}`);
    }
  }
  return value;
}

export function validateManifest(value, target) {
  if (
    value?.schema_version !== 1 ||
    value?.target !== target ||
    typeof value?.manifest_revision !== "string" ||
    !Array.isArray(value?.roots) ||
    value.roots.length === 0
  ) {
    throw new Error(`Invalid schema manifest for ${target}`);
  }
  const names = new Set();
  for (const root of value.roots) {
    if (
      typeof root?.name !== "string" ||
      names.has(root.name) ||
      !["serialize", "deserialize"].includes(root?.direction) ||
      typeof root?.id !== "string" ||
      typeof root?.revision !== "string" ||
      root?.schema?.$id !== root.id
    ) {
      throw new Error(`Invalid schema root ${root?.name ?? "<unknown>"}`);
    }
    names.add(root.name);
  }
  return value;
}

function parseArgs(values) {
  const parsed = new Map();
  for (let index = 0; index < values.length; index += 2) {
    const key = values[index];
    const value = values[index + 1];
    if (!key?.startsWith("--") || value === undefined) {
      throw new Error(`Invalid argument sequence near ${key ?? "<end>"}`);
    }
    parsed.set(key.slice(2), value);
  }
  return parsed;
}

function requiredArg(args, name) {
  const value = args.get(name);
  if (!value) throw new Error(`Missing --${name}`);
  return value;
}
