import { randomUUID } from "node:crypto";
import { z } from "zod";

const principalSchema = z.object({
  user: z.object({ id: z.string().min(1), email: z.email() }),
  tenantId: z.string().min(1),
  baseId: z.string().min(1),
  deviceId: z.string().min(1),
  source: z.enum(["access", "migration"]),
}).strict();

export type VerifiedPrincipal = z.infer<typeof principalSchema>;
export interface AccountContext {
  readonly principal: Readonly<Omit<VerifiedPrincipal, "user"> & {
    user: Readonly<VerifiedPrincipal["user"]>;
  }>;
  readonly epoch: string;
  readonly signal: AbortSignal;
}

export class AccountChangedError extends Error {
  readonly code = "access.account_changed";
  readonly status = 409;
  constructor() {
    super("The selected OneCrew account changed; reload the page.");
  }
}

// 调用方先完成 Access 凭据验证与 tenant 归属解析，再激活身份。
export class AccountContextService {
  #current: AccountContext | undefined;
  #controller: AbortController | undefined;
  #issued = new WeakSet<AccountContext>();

  activate(verified: VerifiedPrincipal): AccountContext {
    const parsed = principalSchema.parse(verified);
    const principal = Object.freeze({ ...parsed, user: Object.freeze(parsed.user) });
    this.#controller?.abort(new AccountChangedError());
    const controller = new AbortController();
    const context = Object.freeze({ principal, epoch: randomUUID(), signal: controller.signal });
    this.#controller = controller;
    this.#current = context;
    this.#issued.add(context);
    return context;
  }

  logout(): void {
    this.#controller?.abort(new AccountChangedError());
    this.#controller = undefined;
    this.#current = undefined;
  }

  current(): AccountContext {
    if (!this.#current) throw new Error("A verified user principal is required");
    return this.#current;
  }

  currentIfActive(): AccountContext | undefined {
    return this.#current;
  }

  assertCurrent(context: AccountContext): void {
    if (!this.#issued.has(context) || context !== this.#current || context.signal.aborted) {
      throw new AccountChangedError();
    }
  }

  forSession(accountId: string, epoch: string): AccountContext {
    const context = this.#current;
    if (!context) throw new AccountChangedError();
    if (context.principal.user.id !== accountId || context.epoch !== epoch) throw new AccountChangedError();
    return context;
  }

  // 发送前冻结归属；已开始发送的回执仍写入原账号，不受页面切换取消信号影响。
  receiptOwner(context: AccountContext): Readonly<{ accountId: string; tenantId: string; epoch: string }> {
    this.assertCurrent(context);
    return Object.freeze({
      accountId: context.principal.user.id,
      tenantId: context.principal.tenantId,
      epoch: context.epoch,
    });
  }
}
