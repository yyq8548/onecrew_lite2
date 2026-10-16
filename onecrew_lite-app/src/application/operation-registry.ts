import type { z } from "zod";
import { assertOperationAvailable } from "../contracts/retired-operations.ts";
import { AccountContextService, type AccountContext } from "../identity/account-context.ts";

interface RegisteredOperation {
  validate(input: unknown): void;
  execute(input: unknown, context: AccountContext): Promise<unknown>;
}

// HTTP、MCP 与兼容命令共用此入口；注册和执行均检查退役名单。
export class OperationRegistry {
  #accounts: AccountContextService;
  #operations = new Map<string, RegisteredOperation>();
  readonly #verifyIdentity: (() => Promise<void>) | undefined;

  constructor(accounts: AccountContextService, verifyIdentity?: () => Promise<void>) {
    this.#accounts = accounts;
    this.#verifyIdentity = verifyIdentity;
  }

  register<Input, Output>(
    name: string,
    schema: z.ZodType<Input>,
    handler: (input: Input, context: AccountContext) => Promise<Output>,
  ): void {
    assertOperationAvailable(name);
    if (!name || this.#operations.has(name)) throw new Error("Operation name must be nonempty and unique");
    this.#operations.set(name, {
      validate: input => { schema.parse(input); },
      execute: async (input, context) => {
        const parsed = schema.parse(input);
        this.#accounts.assertCurrent(context);
        return handler(parsed, context);
      },
    });
  }

  names(): readonly string[] {
    return Object.freeze([...this.#operations.keys()].sort());
  }

  validate(name: string, input: unknown): void {
    assertOperationAvailable(name);
    const operation = this.#operations.get(name);
    if (!operation) throw Object.assign(new Error("Operation is not registered"), { code: "operation.not_found", status: 404 });
    operation.validate(input);
  }

  async execute(name: string, input: unknown, accountId: string, epoch: string): Promise<unknown> {
    await this.#verifyIdentity?.();
    const context = this.#accounts.forSession(accountId, epoch);
    assertOperationAvailable(name);
    const operation = this.#operations.get(name);
    if (!operation) throw Object.assign(new Error("Operation is not registered"), { code: "operation.not_found", status: 404 });
    return operation.execute(input, context);
  }
}
