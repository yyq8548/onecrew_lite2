import { randomBytes } from 'node:crypto';
import {
  browserOpenInputSchema,
  browserSnapshotResultSchema,
  browserTabsResultSchema,
  extensionIdSchema,
  resultSchema,
} from '@onecrew/contracts/browser';
import {
  getPlatformOperation,
  platformOperationSchema,
  supportedPlatformUrlSchema,
} from '../platforms/index.js';
import { z } from 'zod';
import type { TenantContext } from '../tenancy/service.js';

const operationSchema = z.union([
  z.enum(['browser.open', 'browser.tabs', 'browser.snapshot']),
  platformOperationSchema,
]);

export class BrowserBridgeError extends Error {
  constructor(
    message: string,
    readonly status = 400,
    readonly code = 'browser.error',
  ) {
    super(message);
  }
}

type Session = {
  token: string;
  extensionId: string;
  origin: string;
  lastSeen: number;
};
type Command = { id: string; operation: string; input: unknown };
type Pending = {
  command: Command;
  resolve: (value: unknown) => void;
  reject: (error: unknown) => void;
  timer: NodeJS.Timeout;
};
const EXT_ORIGIN = /^chrome-extension:\/\/[a-p]{32}$/;

export const extensionOrigin = (id: string) => `chrome-extension://${id}`;
export const isLocalOrigin = (origin: string | undefined, host: string) =>
  /^(?:localhost|127\.0\.0\.1):\d+$/.test(host) && (!origin || origin === `http://${host}`);

export class BrowserBridgeService {
  constructor(private readonly commandTimeoutMs?: number) {}
  private session?: Session;
  private queue: Command[] = [];
  private pending = new Map<string, Pending>();
  private pollWaiter?: {
    resolve: (command: Command | null) => void;
    timer: NodeJS.Timeout;
    signal?: AbortSignal;
    abort?: () => void;
  };

  connect(extensionId: string, origin?: string) {
    extensionIdSchema.parse(extensionId);
    if (origin && (!EXT_ORIGIN.test(origin) || origin !== extensionOrigin(extensionId)))
      throw new BrowserBridgeError('extension origin does not match id', 403, 'browser.origin');
    if (this.session && this.session.extensionId !== extensionId)
      throw new BrowserBridgeError(
        'another browser extension is already connected',
        409,
        'browser.connected',
      );
    if (this.session) {
      this.session.lastSeen = Date.now();
      return { token: this.session.token };
    }
    const token = randomBytes(32).toString('base64url');
    this.session = {
      token,
      extensionId,
      origin: extensionOrigin(extensionId),
      lastSeen: Date.now(),
    };
    return { token };
  }

  status() {
    return {
      connected: Boolean(this.session && Date.now() - this.session.lastSeen < 70_000),
      last_seen: this.session ? new Date(this.session.lastSeen).toISOString() : null,
      paired: Boolean(this.session),
      extension_id: this.session?.extensionId ?? null,
    };
  }

  private clearPoll(value: Command | null) {
    const waiter = this.pollWaiter;
    if (!waiter) return;
    this.pollWaiter = undefined;
    clearTimeout(waiter.timer);
    if (waiter.signal && waiter.abort) waiter.signal.removeEventListener('abort', waiter.abort);
    waiter.resolve(value);
  }
  private rejectAll(error: unknown) {
    for (const item of this.pending.values()) {
      clearTimeout(item.timer);
      item.reject(error);
    }
    this.pending.clear();
    this.queue = [];
    this.clearPoll(null);
  }
  revoke() {
    this.rejectAll(new BrowserBridgeError('browser connection revoked', 401, 'browser.revoked'));
    this.session = undefined;
  }
  close() {
    this.revoke();
  }

  authenticate(token: string | undefined, extensionId: string | undefined, origin?: string) {
    const session = this.session;
    if (!token || !session || token !== session.token || extensionId !== session.extensionId)
      throw new BrowserBridgeError('browser extension is not authenticated', 401, 'browser.auth');
    if (origin && origin !== session.origin)
      throw new BrowserBridgeError(
        'browser extension origin is not allowed',
        403,
        'browser.origin',
      );
    session.lastSeen = Date.now();
    return session;
  }

  heartbeat(token: string | undefined, extensionId: string | undefined, origin?: string) {
    const session = this.authenticate(token, extensionId, origin);
    return { ok: true, last_seen: new Date(session.lastSeen).toISOString() };
  }

  private validateInput(operation: string, input: unknown) {
    const platformOperation = getPlatformOperation(operation);
    if (platformOperation) return platformOperation.inputSchema.parse(input);
    if (operation === 'browser.open') {
      const parsed = browserOpenInputSchema.parse(input);
      return { url: supportedPlatformUrlSchema.parse(parsed.url) };
    }
    if (operation === 'browser.tabs') return {};
    if (operation === 'browser.snapshot') {
      const tabId = Number((input as { tab_id?: unknown })?.tab_id);
      if (!Number.isInteger(tabId) || tabId < 0) throw new BrowserBridgeError('tab_id is invalid');
      return { tab_id: tabId };
    }
    return input;
  }

  dispatch(operation: string, input: unknown) {
    operationSchema.parse(operation);
    if (!this.session || !this.status().connected)
      throw new BrowserBridgeError('browser extension is offline', 409, 'browser.offline');
    if (this.pending.size + this.queue.length >= 20)
      throw new BrowserBridgeError('browser command queue is busy', 429, 'browser.busy');
    const command: Command = {
      id: randomBytes(12).toString('hex'),
      operation,
      input: this.validateInput(operation, input),
    };
    return new Promise((resolve, reject) => {
      const configuredTimeout = getPlatformOperation(operation)?.timeoutMs;
      const timeoutMs = this.commandTimeoutMs ?? configuredTimeout ?? 60_000;
      const timer = setTimeout(() => {
        const item = this.pending.get(command.id);
        if (item) {
          this.pending.delete(command.id);
          this.queue = this.queue.filter((queued) => queued.id !== command.id);
          reject(new BrowserBridgeError('browser command timed out', 504, 'browser.timeout'));
        }
      }, timeoutMs);
      this.pending.set(command.id, { command, resolve, reject, timer });
      this.queue.push(command);
      if (this.pollWaiter) this.clearPoll(this.queue.shift()!);
    });
  }

  async nextCommand(session: Session, signal?: AbortSignal) {
    if (!this.session || session.token !== this.session.token)
      throw new BrowserBridgeError('browser extension is not authenticated', 401, 'browser.auth');
    if (this.pollWaiter)
      throw new BrowserBridgeError('another command poll is active', 409, 'browser.poll');
    const command = this.queue.shift();
    if (command) return command;
    return await new Promise<Command | null>((resolve) => {
      const abort = () => this.clearPoll(null);
      const timer = setTimeout(() => this.clearPoll(null), 20_000);
      this.pollWaiter = { resolve, timer, signal, abort };
      signal?.addEventListener('abort', abort, { once: true });
    });
  }

  submitResult(payload: unknown) {
    const parsed = resultSchema.parse(payload);
    const item = this.pending.get(parsed.id);
    if (!item) return { accepted: false };
    try {
      if (parsed.error)
        item.reject(new BrowserBridgeError(parsed.error.message, 502, 'browser.extension_error'));
      else if (getPlatformOperation(item.command.operation))
        item.resolve(
          getPlatformOperation(item.command.operation)!.resultSchema.parse(parsed.result),
        );
      else if (item.command.operation === 'browser.tabs')
        item.resolve(browserTabsResultSchema.parse(parsed.result));
      else if (item.command.operation === 'browser.snapshot')
        item.resolve(browserSnapshotResultSchema.parse(parsed.result));
      else item.resolve(parsed.result);
    } catch (error) {
      item.reject(error);
    } finally {
      clearTimeout(item.timer);
      this.pending.delete(parsed.id);
    }
    return { accepted: true };
  }
}

export class BrowserBridgeRegistry {
  private readonly services = new Map<string, { userId: string; service: BrowserBridgeService }>();

  constructor(
    private readonly createBridge: () => BrowserBridgeService = () => new BrowserBridgeService(),
  ) {}

  get(context: TenantContext): BrowserBridgeService {
    const current = this.services.get(context.tenantId);
    if (current) {
      if (current.userId !== context.user.id)
        throw new BrowserBridgeError(
          'tenant owner does not match request user',
          404,
          'browser.tenant',
        );
      return current.service;
    }
    const service = this.createBridge();
    this.services.set(context.tenantId, { userId: context.user.id, service });
    return service;
  }

  closeForUser(userId: string): void {
    for (const [tenantId, entry] of this.services) {
      if (entry.userId !== userId) continue;
      entry.service.close();
      this.services.delete(tenantId);
    }
  }

  closeAll(): void {
    for (const entry of this.services.values()) entry.service.close();
    this.services.clear();
  }
}
