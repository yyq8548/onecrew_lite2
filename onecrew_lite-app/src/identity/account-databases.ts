import { createHash } from "node:crypto";
import { lstatSync, readFileSync, realpathSync, type BigIntStats } from "node:fs";
import { lstat, open, readFile } from "node:fs/promises";
import { basename, dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { CreatorRepository } from "../creators/repository.ts";
import { parseStoredJson } from "../contracts/stored-json.ts";
import { GmailBulkRepository } from "../mail/gmail-bulk-repository.ts";
import { GmailMailboxRepository } from "../mail/gmail-mailbox-repository.ts";
import { MailHistoryRepository } from "../mail/mail-history.ts";
import { MailThreadMaintenanceRepository } from "../mail/mail-thread-maintenance.ts";
import { GmailReplyRepository } from "../mail/gmail-reply-repository.ts";
import type { GmailBulkRepositoryOwner } from "../mail/gmail-bulk-account-binding.ts";
import type { VerifiedMailIdentityProvider } from "../mail/gmail-access.ts";
import type { InstanceLease } from "../service/instance-lock.ts";
import { initializeNewCreatorDatabase, validateCreatorSchema90 } from "../storage/creator-schema.ts";
import { createPrivateDirectory, verifyPrivateDirectory, verifyPrivateFile } from "../storage/private-backup.ts";
import type { AccountContext, AccountContextService } from "./account-context.ts";
import { VerifiedMigrationCredential } from "./access-verification.ts";
import { backupLegacyDatabase } from "../storage/legacy-sqlite.ts";
import { fingerprintSqlite } from "../migration/sqlite-fingerprint.ts";
import { isDeepStrictEqual } from "node:util";
import { DiscoveryRepository } from "../discovery/repository.ts";
import { DiscoveryIdentityRepository } from "../discovery/identity-repository.ts";
import { CreatorHistoryRepository } from "../creators/history-repository.ts";

const manifestSchema = z.object({
  format: z.literal(1), accountId: z.string().min(1), tenantId: z.string().min(1),
  baseId: z.string().min(1), database: z.literal("creator.sqlite"),
}).strict();

const adoptionIssuer = Symbol("prepared-creator-adoption");
export class PreparedCreatorAdoption {
  readonly directory: string;
  readonly databasePath: string;
  readonly owner: Readonly<GmailBulkRepositoryOwner>;
  readonly #lease: InstanceLease;
  readonly #identity: BigIntStats;
  constructor(issuer: symbol, lease: InstanceLease, directory: string, owner: GmailBulkRepositoryOwner) {
    if (issuer !== adoptionIssuer) throw new Error("Creator adoption requires full staging validation");
    this.#lease = lease; this.directory = directory; this.databasePath = join(directory, "creator.sqlite"); this.owner = Object.freeze({ ...owner });
    this.#identity = lstatSync(this.databasePath, { bigint: true });
    Object.freeze(this);
  }
  assertPrepared(lease: InstanceLease): void {
    this.#lease.assertOwned();
    if (lease !== this.#lease) throw new Error("Creator adoption lease changed");
    const current = lstatSync(this.databasePath, { bigint: true }), expected = this.#identity;
    if (current.isSymbolicLink() || !current.isFile() || current.nlink !== 1n || current.dev !== expected.dev || current.ino !== expected.ino ||
        current.size !== expected.size || current.mtimeNs !== expected.mtimeNs || current.ctimeNs !== expected.ctimeNs ||
        relative(resolve(this.directory), realpathSync(this.directory)) !== "" || lstatSync(this.directory).isSymbolicLink()) throw new Error("Validated Creator staging changed");
  }
  toJSON(): never { throw new Error("Prepared Creator adoption authority cannot be serialized"); }
}
type Repositories = Readonly<{
  owner: GmailBulkRepositoryOwner;
  bulk: GmailBulkRepository;
  mailbox: GmailMailboxRepository;
  history: MailHistoryRepository;
  maintenance: MailThreadMaintenanceRepository;
  creators: CreatorRepository;
  replies: GmailReplyRepository;
  discovery: DiscoveryRepository;
  identity: DiscoveryIdentityRepository;
  creatorHistory: CreatorHistoryRepository;
}>;
function ownerOf(context: AccountContext): GmailBulkRepositoryOwner {
  return Object.freeze({ accountId: context.principal.user.id, tenantId: context.principal.tenantId, baseId: context.principal.baseId });
}
function accountKey(owner: GmailBulkRepositoryOwner): string {
  return createHash("sha256").update(JSON.stringify([owner.accountId, owner.tenantId, owner.baseId])).digest("hex");
}

// 数据库只在持有服务锁时打开；切换账号后保留原连接，供已经开始的发送写入原账号回执。
export class AccountDatabases {
  readonly #lease: InstanceLease;
  readonly #accounts: AccountContextService;
  readonly #rootDirectory: string;
  readonly #opened = new Map<string, { database: DatabaseSync; repositories: Repositories; dev: number; ino: number; manifestDev: number; manifestIno: number; manifestSha256: string }>();
  readonly #initializing = new Map<string, Promise<Repositories>>();
  #closed = false;
  constructor(lease: InstanceLease, accounts: AccountContextService, generationRoot?: string) {
    lease.assertOwned();
    this.#lease = lease;
    this.#accounts = accounts;
    this.#rootDirectory = resolve(generationRoot ?? lease.directory);
    const child = relative(lease.directory, this.#rootDirectory);
    if (child === ".." || child.startsWith(`..${sep}`) || isAbsolute(child)) throw new Error("Account database root must belong to the current lease");
  }
  #assertOpen(): void {
    this.#lease.assertOwned();
    if (this.#closed) throw new Error("Account databases are closed");
  }
  #directory(owner: GmailBulkRepositoryOwner): string { return join(this.#rootDirectory, `account-${accountKey(owner)}`); }

  async ensureNewAccount(context: AccountContext): Promise<Repositories> {
    this.#assertOpen(); this.#accounts.assertCurrent(context);
    const owner = ownerOf(context), key = accountKey(owner);
    const existing = this.#initializing.get(key);
    if (existing) { const value = await existing; this.#accounts.assertCurrent(context); return value; }
    const operation = (async () => {
      if (!lstatSync(this.#directory(owner), { throwIfNoEntry: false })) await this.createNewAccount(context);
      return this.resolve(context);
    })();
    this.#initializing.set(key, operation);
    try { const value = await operation; this.#accounts.assertCurrent(context); return value; }
    finally { if (this.#initializing.get(key) === operation) this.#initializing.delete(key); }
  }
  async platformStore(context: AccountContext) {
    await this.resolve(context); const owner = ownerOf(context), value = this.#opened.get(accountKey(owner));
    if (!value) throw new Error("Platform Creator connection is unavailable");
    const directory = await this.accountDirectory(context);
    return Object.freeze({ database: value.database, owner: value.repositories.owner, assertOwned: () => { this.#accounts.assertCurrent(context); directory.assertOwned(); } });
  }

  // 全量读取必须在取得短期在线证明之前完成；此步骤仍不写入账号归属。
  static async prepareValidatedAdoption(lease: InstanceLease, generationRoot: string,
    owner: GmailBulkRepositoryOwner, input: { sourcePath: string; expectedFingerprint: ReturnType<typeof fingerprintSqlite>; existing?: boolean }) {
    lease.assertOwned();
    const root = await verifyPrivateDirectory(generationRoot);
    if (relative(resolve(lease.directory), dirname(root)) !== "" || !/^generation-[0-9a-f-]{36}$/u.test(basename(root))) throw new Error("Migration adoption requires a fresh generation directory");
    manifestSchema.parse({ format: 1, ...owner, database: "creator.sqlite" });
    await verifyPrivateFile(input.sourcePath);
    if (!isDeepStrictEqual(fingerprintSqlite(input.sourcePath), input.expectedFingerprint)) throw new Error("Creator staging changed before adoption");
    const directory = join(root, `account-${accountKey(owner)}`), databasePath = join(directory, "creator.sqlite");
    if (input.existing) {
      if (relative(resolve(databasePath), resolve(input.sourcePath)) !== "") throw new Error("Creator staging layout differs from its owner namespace");
      await verifyPrivateDirectory(directory);
    } else await backupLegacyDatabase(input.sourcePath, databasePath, "creator");
    if (!isDeepStrictEqual(fingerprintSqlite(databasePath), input.expectedFingerprint) || !isDeepStrictEqual(fingerprintSqlite(input.sourcePath), input.expectedFingerprint)) throw new Error("Creator migration copy changed during adoption");
    if (lstatSync(join(directory, "account.json"), { throwIfNoEntry: false })) throw new Error("Creator staging already has an ownership manifest");
    lease.assertOwned();
    return new PreparedCreatorAdoption(adoptionIssuer, lease, directory, owner);
  }
  static async adoptPrepared(lease: InstanceLease, proof: VerifiedMigrationCredential, prepared: PreparedCreatorAdoption) {
    if (!(proof instanceof VerifiedMigrationCredential)) throw new Error("Verified migration credential required");
    if (!(prepared instanceof PreparedCreatorAdoption)) throw new Error("Prepared Creator adoption authority required");
    proof.assertFresh(); prepared.assertPrepared(lease);
    const owner = prepared.owner;
    if (owner.accountId !== proof.user.id || owner.accountId !== proof.metadata.access_user_id) throw new Error("Creator migration owner differs from the verified identity");
    const manifestPath = join(prepared.directory, "account.json");
    await using handle = await open(manifestPath, "wx", 0o600);
    await verifyPrivateFile(manifestPath);
    await handle.writeFile(JSON.stringify({ format: 1, ...owner, database: "creator.sqlite" })); await handle.sync();
    proof.assertFresh(); prepared.assertPrepared(lease);
    return { directory: prepared.directory, databasePath: prepared.databasePath };
  }

  // 仅用于已经确认的新账号初始化。旧数据须通过独立迁移流程验证归属，不能自动创建空库替代。
  async createNewAccount(context: AccountContext): Promise<void> {
    this.#assertOpen();
    this.#accounts.assertCurrent(context);
    const owner = ownerOf(context);
    const directory = await createPrivateDirectory(this.#directory(owner));
    this.#accounts.assertCurrent(context);
    await initializeNewCreatorDatabase(join(directory, "creator.sqlite"));
    this.#accounts.assertCurrent(context);
    const handle = await open(join(directory, "account.json"), "wx", 0o600);
    try {
      await handle.writeFile(JSON.stringify({ format: 1, ...owner, database: "creator.sqlite" }));
      await handle.sync();
    } finally { await handle.close(); }
    await verifyPrivateFile(join(directory, "account.json"));
    this.#accounts.assertCurrent(context);
  }

  async resolve(context: AccountContext): Promise<Repositories> {
    this.#assertOpen();
    this.#accounts.assertCurrent(context);
    const owner = ownerOf(context);
    const directory = await verifyPrivateDirectory(this.#directory(owner));
    const manifestPath = join(directory, "account.json");
    await verifyPrivateFile(manifestPath);
    const manifestBytes = await readFile(manifestPath);
    const manifest = manifestSchema.parse(parseStoredJson(new TextDecoder("utf-8", { fatal: true }).decode(manifestBytes), true));
    const manifestInfo = await lstat(manifestPath), manifestSha256 = createHash("sha256").update(manifestBytes).digest("hex");
    for (const field of ["accountId", "tenantId", "baseId"] as const) {
      if (manifest[field] !== owner[field]) throw new Error("Account database ownership does not match the verified principal");
    }
    const path = join(directory, manifest.database);
    await verifyPrivateFile(path);
    const info = await lstat(path);
    this.#accounts.assertCurrent(context);
    this.#assertOpen();
    const key = accountKey(owner);
    const existing = this.#opened.get(key);
    if (existing) {
      if (existing.dev !== info.dev || existing.ino !== info.ino || existing.manifestDev !== manifestInfo.dev || existing.manifestIno !== manifestInfo.ino || existing.manifestSha256 !== manifestSha256) throw new Error("Account database file identity changed");
      return existing.repositories;
    }
    const database = new DatabaseSync(path);
    let ready = false;
    try {
      validateCreatorSchema90(database);
      database.exec("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA busy_timeout=5000");
      const after = await lstat(path);
      if (after.dev !== info.dev || after.ino !== info.ino || after.isSymbolicLink()) throw new Error("Account database path changed during opening");
      this.#accounts.assertCurrent(context);
      this.#assertOpen();
      const concurrent = this.#opened.get(key);
      if (concurrent) {
        if (concurrent.dev !== info.dev || concurrent.ino !== info.ino || concurrent.manifestDev !== manifestInfo.dev || concurrent.manifestIno !== manifestInfo.ino || concurrent.manifestSha256 !== manifestSha256) throw new Error("Account database file identity changed");
        return concurrent.repositories;
      }
      const repositories = Object.freeze({ owner, bulk: new GmailBulkRepository(database), mailbox: new GmailMailboxRepository(database), history: new MailHistoryRepository(database, owner), maintenance: new MailThreadMaintenanceRepository(database, owner), creators: new CreatorRepository(database), replies: new GmailReplyRepository(database, owner), discovery: new DiscoveryRepository(database), identity: new DiscoveryIdentityRepository(database), creatorHistory: new CreatorHistoryRepository(database) });
      this.#opened.set(key, { database, repositories, dev: info.dev, ino: info.ino, manifestDev: manifestInfo.dev, manifestIno: manifestInfo.ino, manifestSha256 });
      ready = true;
      return repositories;
    } finally { if (!ready) database.close(); }
  }

  bulkResolver() { return { resolve: async (context: AccountContext) => { const value = await this.resolve(context); return { owner: value.owner, repository: value.bulk }; } }; }
  async accountDirectory(context: AccountContext) {
    const value = await this.resolve(context);
    const owned = await this.discoveryResolver().resolve(context);
    const directory = await verifyPrivateDirectory(this.#directory(value.owner));
    this.#accounts.assertCurrent(context);
    this.#assertOpen();
    return Object.freeze({ directory, owner: value.owner, assertOwned: () => { owned.assertOwned(); this.#accounts.assertCurrent(context); } });
  }
  historyResolver() { return { resolve: async (context: AccountContext) => { const value = await this.resolve(context); return { owner: value.owner, repository: value.history }; } }; }
  maintenanceResolver() { return { resolve: async (context: AccountContext) => { const value = await this.resolve(context); return { owner: value.owner, repository: value.maintenance }; } }; }
  creatorsResolver() { return { resolve: async (context: AccountContext) => { const value = await this.resolve(context); return { owner: value.owner, repository: value.creators }; } }; }
  identityResolver() { return { resolve: async (context: AccountContext) => { const value = await this.resolve(context); return { owner: value.owner, repository: value.identity }; } }; }
  creatorHistoryResolver() { return { resolve: async (context: AccountContext) => { const value = await this.resolve(context); return { owner: value.owner, repository: value.creatorHistory }; } }; }
  discoveryResolver() { return { resolve: async (context: AccountContext) => {
    const value = await this.resolve(context), key = accountKey(value.owner), directory = this.#directory(value.owner);
    return { owner: value.owner, repository: value.discovery, assertOwned: () => {
      this.#assertOpen();
      const opened = this.#opened.get(key), database = lstatSync(join(directory, "creator.sqlite")), manifestPath = join(directory, "account.json"), manifest = lstatSync(manifestPath);
      if (!opened || relative(resolve(directory), realpathSync(directory)) !== "" || database.isSymbolicLink() || !database.isFile() || database.nlink !== 1 || database.dev !== opened.dev || database.ino !== opened.ino ||
          manifest.isSymbolicLink() || !manifest.isFile() || manifest.nlink !== 1 || manifest.dev !== opened.manifestDev || manifest.ino !== opened.manifestIno || createHash("sha256").update(readFileSync(manifestPath)).digest("hex") !== opened.manifestSha256) throw new Error("Account database ownership changed");
    } };
  } }; }
  repliesResolver() { return { resolve: async (context: AccountContext) => { const value = await this.resolve(context); return { owner: value.owner, repository: value.replies, history: value.history }; } }; }
  mailboxResolver(identities: VerifiedMailIdentityProvider) {
    return { resolve: async (identity: Parameters<VerifiedMailIdentityProvider["assertCurrent"]>[0]) => {
      await identities.assertCurrent(identity);
      const context = this.#accounts.current();
      const owner = ownerOf(context);
      for (const field of ["accountId", "tenantId", "baseId"] as const) if (identity.owner[field] !== owner[field]) throw new Error("Mailbox identity belongs to another account");
      const value = await this.resolve(context);
      await identities.assertCurrent(identity);
      return { owner: value.owner, repository: value.mailbox };
    } };
  }
  // 调用方先停止任务并等待回执写入完成，再关闭连接和释放服务锁。
  close(): void {
    this.#assertOpen();
    for (const value of this.#opened.values()) value.database.close();
    this.#opened.clear();
    this.#closed = true;
  }
}
