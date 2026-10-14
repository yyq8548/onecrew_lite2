import { readFile } from "node:fs/promises";
import { isAbsolute, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const repositoryRoot = fileURLToPath(new URL("../", import.meta.url));
export const defaultWranglerConfig = fileURLToPath(
  new URL("../wrangler.toml", import.meta.url),
);

const ACCESS_DATABASE_ID = "c373bd25-4aef-4b00-96e0-b88d95133f9f";
const ACCESS_RATE_LIMIT_NAMESPACE = "93010001";
const ACCESS_RELEASE_PROFILE = "onecrew";

export class WranglerTargetError extends Error {
  constructor(message) {
    super(message);
    this.name = "WranglerTargetError";
  }
}

function normalizeArgs(rawArgs) {
  return rawArgs.filter((argument) => argument !== "--");
}

function optionValue(args, index, longName, shortName) {
  const argument = args[index];
  if (argument === longName || argument === shortName) {
    const value = args[index + 1];
    if (!value || value.startsWith("-")) {
      throw new WranglerTargetError(`${longName} requires a value.`);
    }
    return { consumed: 2, value };
  }
  if (argument.startsWith(`${longName}=`)) {
    const value = argument.slice(longName.length + 1);
    if (!value) throw new WranglerTargetError(`${longName} requires a value.`);
    return { consumed: 1, value };
  }
  return null;
}

export function parseWranglerTargetArgs(
  rawArgs,
  { allowPassthrough = false, cwd = process.cwd() } = {},
) {
  const args = normalizeArgs(rawArgs);
  let configPath = defaultWranglerConfig;
  let configSeen = false;
  let profile = ACCESS_RELEASE_PROFILE;
  let profileSeen = false;
  const passthrough = [];

  for (let index = 0; index < args.length;) {
    const configOption = optionValue(args, index, "--config", "-c");
    if (configOption) {
      if (configSeen)
        throw new WranglerTargetError("--config may be provided only once.");
      configSeen = true;
      configPath = isAbsolute(configOption.value)
        ? configOption.value
        : resolve(cwd, configOption.value);
      index += configOption.consumed;
      continue;
    }
    const profileOption = optionValue(args, index, "--profile");
    if (profileOption) {
      if (profileSeen) {
        throw new WranglerTargetError("--profile may be provided only once.");
      }
      if (profileOption.value !== ACCESS_RELEASE_PROFILE) {
        throw new WranglerTargetError(
          `Access release operations require --profile ${ACCESS_RELEASE_PROFILE}.`,
        );
      }
      profileSeen = true;
      index += profileOption.consumed;
      continue;
    }
    if (!allowPassthrough) {
      throw new WranglerTargetError(`Unsupported argument: ${args[index]}`);
    }
    passthrough.push(args[index]);
    index += 1;
  }

  return { configPath, passthrough, profile };
}

export function wranglerTargetArgs(target) {
  const relativeConfig = relative(repositoryRoot, target.configPath);
  const configArgument =
    relativeConfig.length > 0 &&
    relativeConfig !== ".." &&
    !relativeConfig.startsWith(
      `..${process.platform === "win32" ? "\\" : "/"}`,
    ) &&
    !isAbsolute(relativeConfig)
      ? relativeConfig
      : target.configPath;
  return ["--config", configArgument, "--profile", target.profile];
}

function parseTomlSections(source) {
  const sections = [{ array: false, body: [], name: "" }];
  let current = sections[0];
  for (const line of source.split(/\r?\n/)) {
    const header = /^\s*(\[\[?)([^\]]+)(\]\]?)\s*(?:#.*)?$/.exec(line);
    if (header) {
      const array = header[1] === "[[" && header[3] === "]]";
      current = { array, body: [], name: header[2].trim() };
      sections.push(current);
      continue;
    }
    current.body.push(line);
  }
  return sections;
}

function values(section, key) {
  const escaped = key.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const pattern = new RegExp(`^\\s*${escaped}\\s*=\\s*(.+?)\\s*(?:#.*)?$`);
  return section.body.flatMap((line) => {
    const match = pattern.exec(line);
    return match?.[1] ? [match[1]] : [];
  });
}

function requiredSection(sections, name, array = false) {
  const matches = sections.filter(
    (section) => section.name === name && section.array === array,
  );
  if (matches.length !== 1) {
    throw new WranglerTargetError(
      `Expected exactly one ${array ? "[[" : "["}${name}${array ? "]]" : "]"} section.`,
    );
  }
  return matches[0];
}

function requiredValue(section, key, expected) {
  const found = values(section, key);
  if (found.length !== 1 || found[0] !== expected) {
    throw new WranglerTargetError(
      `Wrangler config must set ${section.name || "top-level"}.${key} to ${expected}.`,
    );
  }
}

function unquote(value, label) {
  const match = /^"([^"\\]*)"$/.exec(value);
  if (!match)
    throw new WranglerTargetError(`${label} must be a plain TOML string.`);
  return match[1];
}

function requiredPlainString(section, key) {
  const found = values(section, key);
  if (found.length !== 1) {
    throw new WranglerTargetError(
      `Wrangler config must set ${section.name || "top-level"}.${key} exactly once.`,
    );
  }
  return unquote(found[0], `${section.name || "top-level"}.${key}`);
}

export async function validateAccessWranglerConfig(configPath) {
  let source;
  try {
    source = await readFile(configPath, "utf8");
  } catch {
    throw new WranglerTargetError(
      `Unable to read Wrangler config: ${configPath}`,
    );
  }
  const sections = parseTomlSections(source);
  const topLevel = requiredSection(sections, "");
  const route = requiredSection(sections, "routes", true);
  const rateLimit = requiredSection(sections, "ratelimits", true);
  const assets = requiredSection(sections, "assets");
  const database = requiredSection(sections, "d1_databases", true);
  const services = sections.filter(
    (section) => section.name === "services" && section.array,
  );
  if (services.length !== 2) {
    throw new WranglerTargetError(
      "Expected exactly the revocation and credential-verifier service bindings.",
    );
  }
  const serviceBinding = (binding) => {
    const found = services.filter((section) =>
      values(section, "binding").includes(`"${binding}"`),
    );
    if (found.length !== 1)
      throw new WranglerTargetError(`Expected one ${binding} service binding.`);
    requiredValue(found[0], "binding", `"${binding}"`);
    requiredValue(found[0], "service", '"sync"');
    return found[0];
  };
  const service = serviceBinding("CREATOR_SYNC_DEVICE_REVOCATION");
  const credentialVerifier = serviceBinding("SYNC_CREDENTIAL_VERIFIER");
  const vars = requiredSection(sections, "vars");

  requiredValue(topLevel, "name", '"access"');
  requiredValue(topLevel, "main", '"src/index.ts"');
  requiredValue(topLevel, "workers_dev", "false");
  requiredValue(topLevel, "preview_urls", "false");
  requiredValue(topLevel, "compatibility_flags", '["enable_request_signal"]');
  requiredValue(route, "custom_domain", "true");
  requiredValue(rateLimit, "name", '"EDGE_RATE_LIMITER"');
  requiredValue(rateLimit, "namespace_id", `"${ACCESS_RATE_LIMIT_NAMESPACE}"`);
  requiredValue(rateLimit, "simple", "{ limit = 120, period = 60 }");
  requiredValue(assets, "directory", '"./web/dist/access"');
  requiredValue(assets, "binding", '"ASSETS"');
  requiredValue(assets, "not_found_handling", '"none"');
  requiredValue(assets, "run_worker_first", "true");
  requiredValue(database, "binding", '"DB"');
  requiredValue(database, "database_name", '"access"');
  requiredValue(database, "database_id", `"${ACCESS_DATABASE_ID}"`);
  requiredValue(database, "migrations_dir", '"migrations"');
  requiredValue(service, "binding", '"CREATOR_SYNC_DEVICE_REVOCATION"');
  requiredValue(service, "service", '"sync"');
  requiredValue(service, "entrypoint", '"CreatorSyncDeviceRevocation"');
  if (values(credentialVerifier, "entrypoint").length !== 0) {
    throw new WranglerTargetError(
      "SYNC_CREDENTIAL_VERIFIER must use Sync's default Fetcher entrypoint.",
    );
  }

  const routeHost = requiredPlainString(route, "pattern");
  const accessOrigin = requiredPlainString(vars, "ACCESS_ORIGIN");
  requiredValue(vars, "BETTER_AUTH_URL", `"${accessOrigin}"`);
  requiredValue(vars, "WEB_ORIGIN", `"${accessOrigin}"`);
  const credentialOrigin = requiredPlainString(vars, "CREATOR_SYNC_API_ORIGIN");
  let credentialUrl;
  try {
    credentialUrl = new URL(credentialOrigin);
  } catch {
    throw new WranglerTargetError(
      "CREATOR_SYNC_API_ORIGIN must be an HTTPS origin.",
    );
  }
  if (
    credentialUrl.protocol !== "https:" ||
    credentialUrl.origin !== credentialOrigin ||
    credentialUrl.port ||
    credentialUrl.username ||
    credentialUrl.password
  ) {
    throw new WranglerTargetError(
      "CREATOR_SYNC_API_ORIGIN must be an HTTPS origin without path or credentials.",
    );
  }

  let origin;
  try {
    origin = new URL(accessOrigin);
  } catch {
    throw new WranglerTargetError("ACCESS_ORIGIN must be an absolute URL.");
  }
  if (
    origin.protocol !== "https:" ||
    origin.origin !== accessOrigin ||
    origin.port ||
    origin.host !== routeHost ||
    origin.pathname !== "/" ||
    origin.search ||
    origin.hash ||
    origin.username ||
    origin.password
  ) {
    throw new WranglerTargetError(
      "ACCESS_ORIGIN must be the root HTTPS origin for the configured custom-domain route.",
    );
  }

  const accessLabels = origin.hostname.split(".");
  const expectedApiOrigin = `https://${["api", ...accessLabels.slice(1)].join(".")}`;
  if (accessLabels.length < 3 || accessLabels[0] !== "access" || credentialOrigin !== expectedApiOrigin) {
    throw new WranglerTargetError("CREATOR_SYNC_API_ORIGIN must be the api sibling of the configured Access domain.");
  }

  return {
    accessOrigin,
    creatorSyncApiOrigin: credentialOrigin,
    configPath,
    databaseId: ACCESS_DATABASE_ID,
    routeHost,
  };
}

const SAFE_DEPLOY_BOOLEAN_FLAGS = new Set([
  "--dry-run",
  "--minify",
  "--strict",
  "--upload-source-maps",
]);
const SAFE_DEPLOY_VALUE_FLAGS = new Set([
  "--message",
  "--metafile",
  "--outdir",
  "--tag",
  "--var",
]);

export function assertSafeDeployArgs(args) {
  for (let index = 0; index < args.length;) {
    const argument = args[index];
    const flag = argument.split("=", 1)[0];
    if (SAFE_DEPLOY_BOOLEAN_FLAGS.has(argument)) {
      index += 1;
      continue;
    }
    if (!SAFE_DEPLOY_VALUE_FLAGS.has(flag)) {
      throw new WranglerTargetError(
        `${flag || argument} is not an approved Access deploy option.`,
      );
    }
    const inlineValue = argument.startsWith(`${flag}=`)
      ? argument.slice(flag.length + 1)
      : undefined;
    const value = inlineValue ?? args[index + 1];
    if (!value || (inlineValue === undefined && value.startsWith("-"))) {
      throw new WranglerTargetError(`${flag} requires a value.`);
    }
    if (
      value.includes("ACCESS_ORIGIN") ||
      value.includes("BETTER_AUTH_URL") ||
      value.includes("CREATOR_SYNC_API_ORIGIN") ||
      value.includes("WEB_ORIGIN")
    ) {
      throw new WranglerTargetError(
        "Origin bindings cannot be overridden at deploy time.",
      );
    }
    index += inlineValue === undefined ? 2 : 1;
  }
}
