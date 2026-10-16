// 所有结果种类共用原进程上限，不能为 Choice 和业务确认分别增加一份额度。
export class ResultStoreBudget {
  readonly #maxRows: number;
  readonly #maxBytes: number;
  #rows = 0;
  #bytes = 0;
  constructor(maxRows = 10000, maxBytes = 64 * 1024 * 1024) {
    if (![maxRows, maxBytes].every(value => Number.isSafeInteger(value) && value >= 0)) throw new Error("result_store_limits");
    this.#maxRows = maxRows; this.#maxBytes = maxBytes;
  }
  permits(bytes: number, rows = 0): boolean { return this.#bytes + bytes <= this.#maxBytes && this.#rows + rows <= this.#maxRows; }
  check(bytes: number, rows = 0): void {
    if (![bytes, rows].every(Number.isSafeInteger) || this.#bytes + bytes < 0 || this.#rows + rows < 0) throw new Error("result_budget_accounting");
    if (this.#rows + rows > this.#maxRows) throw new Error("result_row_limit");
    if (this.#bytes + bytes > this.#maxBytes) throw new Error("result_payload_limit");
  }
  change(bytes: number, rows = 0): void { this.check(bytes, rows); this.#bytes += bytes; this.#rows += rows; }
}
