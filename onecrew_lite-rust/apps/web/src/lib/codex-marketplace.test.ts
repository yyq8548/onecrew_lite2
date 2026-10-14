import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { basename, join } from "node:path";
import { describe, expect, it } from "vitest";

// The bundled Codex plugin marketplace is a static contract: these tests are
// the Linux-runnable guard for its shape and secret boundary (the desktop
// crate itself only compiles on the packaging hosts).
const REPO_ROOT = join(__dirname, "..", "..", "..", "..");
const MARKETPLACE_ROOT = join(REPO_ROOT, "integrations", "codex-marketplace");
const PLUGIN_ROOT = join(MARKETPLACE_ROOT, "plugins", "onecrew");

describe("codex marketplace contract", () => {
  it("keeps CLI, web, service and plugin release versions synchronized", () => {
    const releaseVersion = JSON.parse(readFileSync(join(REPO_ROOT, "development/release/version.json"), "utf8")).cliPluginBase;
    const versions = [
      JSON.parse(readFileSync(join(REPO_ROOT, "apps/web/package.json"), "utf8")).version,
      JSON.parse(readFileSync(join(REPO_ROOT, "apps/web/package-lock.json"), "utf8")).version,
      JSON.parse(readFileSync(join(PLUGIN_ROOT, ".codex-plugin/plugin.json"), "utf8")).version,
      ...["onecrew", "local-service"].map((name) => /^version = "([^"]+)"$/m.exec(readFileSync(join(REPO_ROOT, "crates", name, "Cargo.toml"), "utf8"))?.[1]),
    ];
    expect(new Set(versions)).toEqual(new Set([releaseVersion]));
    const cargoLock = readFileSync(join(REPO_ROOT, "Cargo.lock"), "utf8");
    expect(cargoLock).not.toMatch(/name = "tauri(?:-|"|_)/);
  });

  it("declares one local plugin with complete policy metadata", () => {
    const marketplace = JSON.parse(
      readFileSync(join(MARKETPLACE_ROOT, ".agents", "plugins", "marketplace.json"), "utf8"),
    );
    expect(marketplace.name).toBe("onecrew_lite-cli");
    expect(marketplace.interface.displayName).toBe("onecrew_lite Local Plugins");
    expect(JSON.stringify(marketplace.interface)).not.toContain("Reacher");
    expect(marketplace.plugins).toHaveLength(1);
    const entry = marketplace.plugins[0];
    expect(entry.name).toBe("onecrew_lite");
    expect(entry.source).toEqual({ source: "local", path: "./plugins/onecrew" });
    expect(entry.policy.installation).toBe("AVAILABLE");
    expect(entry.policy.authentication).toBeTruthy();
    expect(entry.category).toBeTruthy();
  });

  it("ships a secret-free stdio connection and one Router without startup hooks", () => {
    const manifest = JSON.parse(
      readFileSync(join(PLUGIN_ROOT, ".codex-plugin", "plugin.json"), "utf8"),
    );
    const appVersion = JSON.parse(
      readFileSync(join(REPO_ROOT, "apps", "web", "package.json"), "utf8"),
    ).version;
    expect(manifest.name).toBe("onecrew_lite");
    // The source directory remains stable for existing packaging scripts.
    expect(basename(PLUGIN_ROOT)).toBe("onecrew");
    expect(manifest.version).toBe(appVersion);
    expect(manifest.version).toMatch(/^\d+\.\d+\.\d+$/);
    expect(manifest.description?.length ?? 0).toBeGreaterThan(20);
    expect(manifest.homepage).toBe("https://onecrewai.io");
    expect(manifest.author?.name).toBe("onecrew_lite");
    expect(manifest.skills).toBe("./skills/");
    expect(existsSync(join(PLUGIN_ROOT, "skills"))).toBe(true);
    const retiredCanonicalDir = join(REPO_ROOT, "crates", "mcp-adapter", "skills");
    expect(!existsSync(retiredCanonicalDir) || readdirSync(retiredCanonicalDir).length === 0).toBe(true);

    expect(manifest.mcpServers).toBe("./.mcp.json");
    const servers = JSON.parse(readFileSync(join(PLUGIN_ROOT, ".mcp.json"), "utf8")).mcpServers;
    expect(Object.keys(servers)).toEqual(["onecrew_lite"]);
    const connection = servers.onecrew_lite;
    expect(connection.command).toBe("./bin/onecrew_lite");
    expect(connection.cwd).toBe(".");
    expect(connection.args).toEqual(["mcp", "--stdio"]);
    expect(connection.url).toBeUndefined();
    expect(connection.http_headers).toBeUndefined();
    expect(manifest.apps).toBeUndefined();
    expect(manifest.hooks).toBeUndefined();
    expect(existsSync(join(PLUGIN_ROOT, "hooks.json"))).toBe(false);

    const iface = manifest.interface;
    expect(iface.displayName).toBe("onecrew_lite");
    expect(iface.shortDescription).toContain("onecrew_lite");
    expect(iface.longDescription?.length ?? 0).toBeGreaterThan(20);
    expect(iface.longDescription).toContain("onecrew_lite");
    expect(iface.developerName).toBe("onecrew_lite");
    expect(iface.websiteURL).toBe("https://onecrewai.io");
    expect(iface.composerIcon).toBe("./assets/onecrew-favicon.svg");
    expect(iface.logo).toBe("./assets/onecrew-favicon.svg");
    expect(iface.category).toBeTruthy();
    expect(Array.isArray(iface.capabilities)).toBe(true);
    expect(iface.capabilities.length).toBeGreaterThan(0);
    for (const capability of iface.capabilities) expect(capability).toBeTypeOf("string");
    expect(Array.isArray(iface.defaultPrompt)).toBe(true);
    expect(iface.defaultPrompt.length).toBeGreaterThanOrEqual(1);
    expect(iface.defaultPrompt.length).toBeLessThanOrEqual(3);
    for (const prompt of iface.defaultPrompt) expect(prompt).toBeTypeOf("string");
    expect(iface.defaultPrompt.every((prompt: string) => prompt.includes("onecrew_lite"))).toBe(true);

    const visibleMetadata = JSON.stringify({
      description: manifest.description,
      author: manifest.author,
      interface: manifest.interface,
    });
    expect(visibleMetadata).not.toContain("Reacher");
    expect(visibleMetadata).not.toContain("Agent Platform");

    const iconPath = join(PLUGIN_ROOT, "assets", "onecrew-favicon.svg");
    expect(existsSync(iconPath)).toBe(true);
    const icon = readFileSync(iconPath, "utf8");
    expect(createHash("sha256").update(icon).digest("hex")).toBe(
      "10a284fa719ae64bcd174740ff17b3b9b0808d8e6c385ee2542fafd84d52fae5",
    );
    expect(icon).toContain('viewBox="0 0 161 161"');
    expect(icon).toContain('fill="#17191C"');
    expect(icon.match(/fill="#FFDE65"/g)).toHaveLength(3);
  });

  it("keeps the public website separate from operational service origins", () => {
    const manifest = JSON.parse(
      readFileSync(join(PLUGIN_ROOT, ".codex-plugin", "plugin.json"), "utf8"),
    );
    expect(manifest.homepage).toBe("https://onecrewai.io");
    expect(manifest.interface.websiteURL).toBe("https://onecrewai.io");

    const creatorSyncConfig = readFileSync(
      join(REPO_ROOT, "crates", "creator-sync", "src", "config.rs"),
      "utf8",
    );
    expect(creatorSyncConfig).toContain("access_origin:");
    expect(creatorSyncConfig).toContain("sync_origin:");
    const origins = [...creatorSyncConfig.matchAll(/(?:access_origin|sync_origin):\s*"(https:\/\/[^\"]+)"/g)]
      .map((match) => new URL(match[1]).origin);
    const publisherHost = new URL(manifest.homepage).hostname;
    expect(origins).toEqual(["access", "api"].map((service) => `https://${service}.${publisherHost}`));
    expect(new Set(origins).size).toBe(2);
    expect(creatorSyncConfig).not.toContain(".workers.dev");
    expect(creatorSyncConfig).not.toMatch(
      /(?:access_origin|sync_origin):\s*"https:\/\/onecrewai\.io"/,
    );
  });

  // This pins the rules the Router must carry, not one phrasing of them. Each
  // entry names the load-bearing tokens of one rule, so rewording a step keeps
  // it passing while dropping the rule fails. The structural assertions stay
  // exact: the file is trigger-first, its description carries the negative
  // triggers and no platform inventory, and it names no second operation set.
  it("ships exactly one V2 Router with resolvable policy and scoring references", () => {
    const skillsRoot = join(PLUGIN_ROOT, "skills");
    expect(readdirSync(skillsRoot).sort()).toEqual(["onecrew"]);
    const routerRoot = join(skillsRoot, "onecrew");
    const router = readFileSync(join(routerRoot, "SKILL.md"), "utf8");
    expect(router).toMatch(/^---\nname: onecrew\ndescription: /);
    expect(router).toContain("# OneCrew");
    // Trigger-first structure: all "when to use" lives in the description, so
    // the body carries no when-to-use section to drift from it.
    expect(router).toContain("## Route");
    expect(router).toContain("## Never");
    expect(router).toContain("## References");
    expect(router).not.toMatch(/^##+ When (?:NOT )?to [Uu]se/m);
    const description = router.split("\n")[2] ?? "";
    expect(description).toMatch(/^description: /);
    expect(description).toMatch(/\bNot for\b/);
    // The routing trigger outlives any provider inventory: naming platforms
    // here would need a new staged digest for every platform OneCrew adds.
    for (const platform of ["X,", "Instagram", "Telegram", "YouTube", "TikTok"]) {
      expect(description).not.toContain(platform);
    }

    // Rules are matched against whitespace-normalized prose: rewrapping a
    // paragraph must not fail a pin whose rule is still there.
    const prose = router.replace(/\s+/g, " ");
    expect(prose).not.toContain("user_requested_direct");
    const rules: [string, RegExp][] = [
      ["answer in the user's language", /(?:Answer|answer)[^.]*in the language of the user's current request/],
      ["report an available update once, then wait for authorization", /`cli_update\.state`[\s\S]{0,200}?`current_version[\s\S]{0,160}?wait for explicit update authorization/],
      ["status is not authorization", /status is not authorization/],
      ["a cached memory_cache read is last-checked evidence", /`?source:\s*memory_cache`?[\s\S]{0,80}?`checked_at`/],
      ["never call a non-available state an update", /`current`, `unavailable` or `not_checked` an available update/],
      ["a cached plugin status is not proof of the install", /cached plugin status is evidence, not proof/],
      ["keep one cross-platform outcome together", /one cross-platform business outcome together/],
      ["name each outcome's owner from the live directory, not memory", /connection, discovery, read, write, presentation[\s\S]{0,80}?live directory and exact contracts, never from memory/],
      ["search only for an unnamed capability", /Search only for a capability you cannot already name/],
      ["one outcome per query in English", /one outcome per query in concise English/],
      ["an explicit toolkit filter per named provider", /explicit toolkit filter for every provider the user named/],
      ["execution values stay out of search", /keywords, handles, IDs, counts[\s\S]{0,80}?out of it[\s\S]{0,80}?`arguments`/],
      ["do not infer effect filters from business wording", /Do not infer an effect filter from business wording/],
      ["native searches are reads without Creator persistence", /Current native creator and content searches are `read_open_world`; they create no Creator records or snapshots/],
      ["effect filters require an explicit policy restriction", /Use an effect filter only for an explicit policy restriction/],
      ['"do not save to OneCrew" is not a policy restriction', /"do not save to OneCrew" is not one/],
      ["call search once", /Call it once/],
      ["consume the ranked operations and every represented guide", /ranked `operations`, every represented `toolkit_guides` entry with its ordering and pitfalls/],
      ["no second guide lookup", /no second guide lookup/],
      ["a hint is not a contract", /a hint is guidance, never a contract/],
      ["an exact ID goes straight to Get, 1-8 unique", /`onecrew_tool_get`, 1–8 unique IDs per call/],
      ["never re-search or re-fetch a contract", /Never search for an ID you already have[\s\S]{0,120}?never fetch one contract twice/],
      ["every returned contract field is authoritative, output schema included", /input schema, `schema_revision`[\s\S]{0,140}?requested output schema is authoritative/],
      ["output schema stays opt-in", /`include_output_schema` false for plan preparation and for any operation's first execute/],
      ["the prepare ID is fixed and rides the same Get", /`onecrew\.search\.plan\.prepare` in the same Get as the selected search operation; its ID is fixed, so never search for it/],
      ["a plan states acceptance criteria and editable keywords", /`description` stating the acceptance criteria, editable `keywords`/],
      ["one scope per search call, bound to that call's contract", /one scope per search call carrying that call's `operation_id`, `schema_revision`, tagged connection and `arguments`/],
      ["scope arguments exclude the execute-only fields", /search criteria only, without the execute-only `search_plan_policy`, `confirmed_plan_ref` or `cursor`/],
      ["nothing is persisted and no provider is called before confirmation", /nothing is persisted and no provider is called before the user confirms/],
      ["execute the confirmed scope unchanged and add no criterion", /confirmed scope's criteria unchanged, passing its `confirmed_plan_ref`, and add no criterion the scope left out/],
      ["a revision patches description and keywords only", /revision patches `description` and `keywords` only/],
      ["changed provider criteria need a new plan", /changed provider criteria need a new plan/],
      ["direct search needs an explicit user request", /Only when the user explicitly asks in the current request[\s\S]{0,100}?skip the search plan[\s\S]{0,80}?search directly/],
      ["direct search is never autonomous", /never choose it autonomously/],
      ["Execute takes 1-8 unique calls with exact identity", /`onecrew_tool_execute` takes 1–8 unique calls, each with its own `call_id`, the exact `operation_id` and `schema_revision`/],
      ["direct authority", /`authority=\{"kind":"direct"\}`/],
      ["arguments are an object, never a string", /`arguments` object — never a string/],
      ["batch only independent calls, prerequisites first", /Batch only independent calls[\s\S]{0,120}?run the prerequisite round/],
      ["all three presentation modes", /`presentation="never"`[\s\S]{0,180}?`"if_required"`[\s\S]{0,120}?`"requested"`/],
      ["a required confirmation or choice can still mint a card", /confirmation or choice can mint a card either way/],
      ["a server stop ends the round", /server stop outcome ends that round/],
      ["X exposes its native continuation cursor", /X pages expose `next_cursor`/],
      ["YouTube exposes its native page token", /YouTube search pages expose `next_page_token`/],
      ["continue a scope with the actual provider value", /Pass the actual provider value as `cursor` to continue the same scope[\s\S]{0,80}?without one return its first page/],
      ["Instagram and Telegram searches have native arrays without cursors", /Instagram user search and Telegram community search return native arrays and have no cursor/],
      ["present only an execute-minted reference, once, with a locale", /`result_ref` that `onecrew_tool_execute` returned to `onecrew_result_present` once, with `locale`/],
      ["never restate the card in prose", /Do not restate the card's fields, options or result in prose/],
      ["customer decisions use the workbench", /Customer-only decisions[\s\S]{0,180}corresponding workbench flow/],
      ["status cards do not replace workbench decisions", /Do not create `onecrew\.ask_user_question` cards for these decisions/],
      ["decisions require a persisted owner outcome", /Wait for its persisted owner outcome before continuing/],
      ["stop on a non-completed outcome and follow its recovery", /Stop on input-required, failed, unknown-effect, skipped, missing authority, missing connection or revision mismatch, and follow the recovery contract/],
      ["acceptance is not completion; one key per intent; read state back", /Request acceptance is not completion: hold one idempotency key per intent, and read persisted OneCrew or provider state back where supported before reporting success/],
      ["network_unreachable is diagnosed once before any retry", /`network_unreachable`, run the recovery's `connection\.check` operation once, report its `status`[\s\S]{0,120}?do not retry until that check returns `ok`/],
      ["an Agent-edit continuation seals the old card", /Agent-edit continuation seals the old card/],
      ["keep matching separate from recommendation grade", /Match determines whether a candidate counts toward the user's target; grade expresses recommendation/],
      ["retrieve the canonical grade rubric", /Retrieve the shared grade rubric from the live find-batch workflow guide[\s\S]{0,100}?instead of inventing a standard/],
      ["write an individual evidence-based assessment", /Each person's assessment names their own evidence[\s\S]{0,160}?relevant limitation/i],
      ["apply one cross-platform discovery flow", /Content, video and account discovery use one cross-platform flow[\s\S]{0,160}?stable platform identity/],
      ["keep unknown separate from unmatched", /matched, unmatched and unverified[\s\S]{0,120}?unknown is not unmatched or low/],
      ["make one targeted read for a decision gap", /one decision-changing evidence gap[\s\S]{0,120}?one targeted read/],
      ["stop when the matching target is reached", /Stop new search and unnecessary checks when the matching target is reached/],
      ["target completion still prepares and presents the review", /Reaching the matching target stops search only; it does not skip candidate preparation and presentation/],
      ["use the actual continuation cursor", /continue only with the actual cursor returned by that query/],
      ["default discovery delivery uses both result views", /Finding creators\s+is not complete at a Markdown list[\s\S]{0,250}?results in the workbench and matching Library table/],
      ["honor an explicit pure-text or no-card request", /Unless the user explicitly asks for pure text\/no card[\s\S]{0,180}?default delivery/],
      ["save review is requested and confirmed in the workbench", /Prepare a save review only when requested and its owner-backed workbench\s+confirmation is available/],
      ["a plan reference is not a candidate review reference", /search-plan `result_ref` a candidate-review ref[\s\S]{0,160}?claim the default discovery complete/],
      ["report candidate preparation failure honestly", /If preparation capability fails[\s\S]{0,160}?[Dd]o not call the search-plan `result_ref` a candidate-review ref/],
      ["default candidate preparation to add_new", /candidate preparation defaults to explicit `add_new`[\s\S]{0,120}?`save_or_update` only when requested/],
      ["prepare obtains fresh data without a mechanical prefetch", /preparation obtains fresh platform data[\s\S]{0,160}?not mechanically fetch a profile or channel first/i],
      ["preparation does not write or auto-confirm", /Preparation writes no Creator[\s\S]{0,150}?only the user's workbench confirmation can save/],
      ["save each chosen score and assessment", /use the fixed representative score specified there[\s\S]{0,160}?each person's assessment/i],
      ["never substitute another route for the connection", /Never substitute a browser, a direct provider call, model memory, a guessed operation ID, a cached schema, an alias or a fallback route/],
      ["never translate an identifier", /Never translate or rewrite a schema field name, enum value, operation ID, schema revision, connection tag or identifier value/],
      ["neither the Agent nor the App authors a binding or continuation", /author or replay a result binding, revision or continuation payload — neither the Agent nor the App does any of these/],
      ["never echo a secret", /Never echo a pairing secret or any other secret/],
      ["installation belongs to the explicit CLI, and startup performs neither", /Never install or refresh the plugin from here[\s\S]{0,160}?MCP startup performs neither/],
      ["read at most one reference, only when the step is about it", /Read at most one of these, and only when the current step is about it/],
      ["live guidance overrides this cached text", /never let this cached text override the live tool surface/],
      ["provider capability guidance stays live", /comes from the live search toolkit guides and exact contracts, never from this text/],
    ];
    for (const [rule, pattern] of rules) {
      expect(prose, `router dropped the rule: ${rule}`).toMatch(pattern);
    }

    expect(router).not.toContain("input schema, output schema");
    for (const tool of [
      "onecrew_connection_list",
      "onecrew_tool_search",
      "onecrew_tool_get",
      "onecrew_tool_execute",
      "onecrew_result_present",
    ]) {
      expect(router).toContain(tool);
    }
    for (const retired of [
      "onecrew_catalog_search", "onecrew_catalog_get", "onecrew_catalog_present", "onecrew_operation_execute",
      "entry_form", "presentation_ref", "replaces_presentation_ref",
    ]) expect(router).not.toContain(retired);
    // Every exact operation ID the Router may name, and no second inventory:
    // the plan ID is named once, in the step that prepares it.
    expect(router.match(/onecrew\.search\.plan\.prepare/g)).toHaveLength(1);
    const exactOperationIds = new Set(router.match(/`onecrew\.[a-z][a-z0-9_.]*`/g) ?? []);
    expect([...exactOperationIds].sort()).toEqual([
      "`onecrew.ask_user_question`",
      "`onecrew.search.plan.prepare`",
      "`onecrew.workflow.guide`",
    ]);
    const references = readdirSync(join(routerRoot, "references")).sort();
    expect(references).toEqual(["campaign-discovery.md", "match-evaluation.md", "onecrew-writes.md", "toolkit-connections.md"]);
    const campaign = readFileSync(join(routerRoot, "references", "campaign-discovery.md"), "utf8");
    expect(campaign).toContain("onecrew.discovery.target.increase");
    expect(campaign).toContain("decision_id");
    expect(campaign).toContain("same request");
    expect(campaign).toContain("criteria");
    expect(router).toContain("`onecrew.workflow.guide`");
    expect(router).toContain("grants no");
    expect(router).toContain("business-write or sending authority");
    for (const reference of references) {
      if (reference === "match-evaluation.md") expect(campaign).toContain("[match-evaluation.md](match-evaluation.md)");
      else expect(router).toContain(`references/${reference}`);
      expect(readFileSync(join(routerRoot, "references", reference), "utf8").trim()).not.toBe("");
    }
  });

  // These pin the rules the Router must carry, not one phrasing of them: the
  // shortest route per verb, one dependency round per Get, independent calls
  // only per Execute, one presentation then silence. Rewording a step keeps
  // them passing; dropping a rule does not.
  it("routes dependency rounds and result presentation without duplicate narration", () => {
    const router = readFileSync(
      join(PLUGIN_ROOT, "skills", "onecrew", "SKILL.md"),
      "utf8",
    ).replace(/\s+/g, " ");
    expect(router).toMatch(/connection status or selection straight to `onecrew_connection_list`/i);
    expect(router).toMatch(/Search only for a capability you cannot already name/);
    expect(router).toMatch(/exact contracts for one dependency round/);
    expect(router).toMatch(/`onecrew_tool_get`, 1–8 unique IDs per call/);
    expect(router).toMatch(/`onecrew_tool_execute` takes 1–8 unique calls/);
    expect(router).toMatch(/`onecrew_result_present` once, with `locale`/);
    expect(router).toMatch(/never omitted[\s\S]{0,250}Do not restate the card's fields, options or result in prose/);
    expect(router).toMatch(/Customer-only decisions[\s\S]{0,200}workbench flow/);
    expect(router).toMatch(/Wait for its persisted owner outcome before continuing/);
  });

  it("retains no per-use-case Skill aliases or second operation inventory", () => {
    const routerRoot = join(PLUGIN_ROOT, "skills", "onecrew");
    const texts = [
      readFileSync(join(routerRoot, "SKILL.md"), "utf8"),
      ...readdirSync(join(routerRoot, "references")).map((name) =>
        readFileSync(join(routerRoot, "references", name), "utf8"),
      ),
    ].join("\n");
    for (const retired of [
      "reacher-account-connection", "reacher-ad-hoc-lookup", "reacher-campaign-setup",
      "reacher-connect-instagram", "reacher-connect-resend", "reacher-connect-telegram",
      "reacher-connect-x", "reacher-connect-youtube", "reacher-direct-message",
      "reacher-email-outreach", "reacher-inbox-triage", "reacher-task-orchestration",
    ]) {
      expect(texts).not.toContain(retired);
      expect(existsSync(join(PLUGIN_ROOT, "skills", retired))).toBe(false);
    }
    for (const duplicatedField of [
      "inputSchema", "attention_ids", "include_acknowledged", "include_resolved",
    ]) {
      expect(texts).not.toContain(duplicatedField);
    }
    // Retired scheduler ids, paths and wire names from the old Campaign /
    // CRM / direct-X / TikTok One flows must not survive in the Router or its
    // references. The TikTok One and cloud-authorization entries are
    // case-insensitive substrings, mirroring the Rust `RetiredForm::Substring`
    // concepts; plain `tiktok` stays live as a Creator platform.
    for (const retiredToken of [
      /campaign\.setup_form/, /campaign\.claim/, /campaign\.start/, /\bcrm\./i, /onecrew\.creator\.discover/, /onecrew\.creator\.search/,
      /x\.users\.search/, /\/mcp\/domains\//, /SearchPlanDraft/, /\bplaybook\b/i,
      /task\.choice\.request/, /tiktok_searches/, /advanced_x/,
      /onecrew\.outreach\.conversation\.sync/,
      /onecrew\.creator\.contact\.enrich/, /onecrew\.creator\.link_hub\.enrich/, /onecrew\.creator\.contact\.confirm/,
      /onecrew\.creator\.candidate\.(?:batch\.)?save\.confirm/,
      /onecrew\.creator\.community_administrators\.discover/,
      /onecrew\.creator\.shortlist\.(?:add|list|remove)/,
      /\bselection_ref\b/,
      /tiktok_one/i, /tiktok one/i, /tiktok-one/i, /cloud_oauth/i, /cloud authorization/i,
      /plan_budget/, /plan_novelty/, /search_plan_page_budget_exhausted/, /search_plan_candidate_budget_reached/,
    ]) {
      expect(texts, `Router retains retired token ${retiredToken}`).not.toMatch(retiredToken);
    }
    const allowedMetaTools = new Set([
      "onecrew_status", "onecrew_login", "onecrew_login_cancel", "onecrew_logout", "onecrew_web",
      "onecrew_connection_list", "onecrew_tool_search", "onecrew_tool_get",
      "onecrew_tool_execute", "onecrew_result_present",
    ]);
    for (const token of texts.match(/\bonecrew_[a-z][a-z0-9_]*\b/g) ?? []) {
      expect(allowedMetaTools.has(token), `Router retains non-meta tool ${token}`).toBe(true);
    }
  });

  it("preserves stable connection and OneCrew-write semantics", () => {
    const referencesRoot = join(PLUGIN_ROOT, "skills", "onecrew", "references");
    const required: Record<string, string[]> = {
      "toolkit-connections.md": ["tagged IDs", "same proxy", "transient secrets", "human flows", "expired", "idempotency"],
      "onecrew-writes.md": ["confirmation", "idempotency", "unknown", "Read back", "discovery_id", "candidate_id", "target", "connection", "provenance", "candidates", "row_id", "up to 50", "eight-call MCP envelope", "writes no Creator rows", "without refetching", "confirmed Creator link", "frozen body", "Each review authorizes only its current call", "same `result_ref` can advance", "record_pending", "creator_recorded", "without sending again", "save-only prefetch", "missing quality or other evidence gap", "explicit per-candidate `intent`", "eligible_new", "existing_skipped", "duplicate_skipped", "save_or_update", "final batch prepare call", "reuse that same task's current batch-save schema", "schema_revision", "missing, stale or invalid, or changed", "`quality.score`", "`quality.assessment`", "typed `provenance`", "actual operation/source IDs/URLs", "never fabricate an ID, URL or other source value", "evidence gap would change the current decision"],
    };
    for (const [reference, phrases] of Object.entries(required)) {
      const body = readFileSync(join(referencesRoot, reference), "utf8").replace(/\s+/g, " ");
      for (const phrase of phrases) expect(body, `${reference} misses ${phrase}`).toContain(phrase);
    }
  });

  it("requires the live batch-save contract before final candidate preparation", () => {
    const body = readFileSync(
      join(PLUGIN_ROOT, "skills", "onecrew", "references", "onecrew-writes.md"),
      "utf8",
    ).replace(/\s+/g, " ");
    const currentContract = body.indexOf("Fetch the current operation contract before constructing either input");
    const finalBatchPrepare = body.indexOf("For the final batch prepare call");
    const reusedSchema = body.indexOf("reuse that same task's current batch-save schema");
    const router = readFileSync(join(PLUGIN_ROOT, "skills", "onecrew", "SKILL.md"), "utf8").replace(/\s+/g, " ");
    expect(router).toContain("Candidate-review preparation after discovery");
    expect(body).toContain("candidate-review preparation after discovery");
    expect(currentContract).toBeGreaterThanOrEqual(0);
    expect(finalBatchPrepare).toBeGreaterThan(currentContract);
    expect(reusedSchema).toBeGreaterThan(finalBatchPrepare);
    expect(reusedSchema).toBeLessThan(finalBatchPrepare + 240);
  });

  it("contains only approved publisher URLs and no connection material", () => {
    const files: string[] = [];
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = join(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else files.push(path);
      }
    };
    walk(MARKETPLACE_ROOT);
    expect(files.length).toBeGreaterThan(2);
    const credentialMaterial = /\bauthorization["']?\s*[:=]|\bbearer\s+[a-z0-9._~+/-]{12,}/i;
    expect("Authorization: Bearer synthetic-credential-value").toMatch(credentialMaterial);
    expect('"authorization": "synthetic-header"').toMatch(credentialMaterial);
    expect("authorization state is read back after a disconnect").not.toMatch(credentialMaterial);
    const urls: string[] = [];
    for (const file of files) {
      const lowered = readFileSync(file, "utf8").toLocaleLowerCase();
      for (const forbidden of [
        "127.0.0.1", "localhost", "39393",
        "access-token", "config.toml",
      ]) {
        expect(lowered, `${file} must not contain ${forbidden}`).not.toContain(forbidden);
      }
      expect(lowered, `${file} must not contain credential headers or bearer values`).not.toMatch(credentialMaterial);
      urls.push(...(lowered.match(/https?:\/\/[^"'\s<>]+/g) ?? []));
    }
    expect(urls.sort()).toEqual([
      "http://www.w3.org/2000/svg",
      "https://github.com/onecrewai/onecrew-plugins/blob/0cebf1220c15f4252533e6b4a6bddb079cfdc101/plugins/onecrew/skills/onecrew/references/match-evaluation.md).",
      "https://onecrewai.io",
      "https://onecrewai.io",
      "https://support.reddithelp.com/hc/en-us/articles/19696541895316-available-search-features).",
    ]);
  });
});
