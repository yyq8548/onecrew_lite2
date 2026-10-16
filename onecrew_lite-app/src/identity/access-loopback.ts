import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import type { AddressInfo } from "node:net";

export type AccessLoopbackOutcome = "succeeded" | "rejected" | "cancelled" | "expired";
export class AccessLoopback {
  readonly port: number;
  readonly finished: Promise<AccessLoopbackOutcome>;
  readonly #server: Server;
  readonly #finish: (outcome: AccessLoopbackOutcome) => void;
  readonly #timer: ReturnType<typeof setTimeout>;
  #consumed = false;
  #completing: Promise<void> | undefined;
  #closed = false;
  private constructor(server: Server, port: number, callback: (code: string, state: string) => Promise<void>) {
    this.#server = server; this.port = port;
    const result = Promise.withResolvers<AccessLoopbackOutcome>();
    this.finished = result.promise; this.#finish = result.resolve;
    this.#timer = setTimeout(() => { this.#stop("expired"); }, 5 * 60_000);
    server.on("request", (request, response) => this.#receive(request, response, callback));
    server.on("clientError", (_error, socket) => { socket.end("HTTP/1.1 400 Bad Request\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"); this.#stop("rejected"); });
    server.on("error", () => this.#stop("rejected"));
    server.on("connection", socket => socket.setTimeout(3000, () => { socket.destroy(); if (!this.#consumed) this.#stop("rejected"); }));
  }
  static async start(callback: (code: string, state: string) => Promise<void>): Promise<AccessLoopback> {
    const server = createServer({ maxHeaderSize: 8192, headersTimeout: 3000, requestTimeout: 3000, connectionsCheckingInterval: 1000, keepAliveTimeout: 1 });
    server.maxConnections = 1; server.maxRequestsPerSocket = 1;
    await new Promise<void>((resolve, reject) => { server.once("error", reject); server.listen(0, "127.0.0.1", () => { server.off("error", reject); resolve(); }); });
    const address = server.address() as AddressInfo;
    if (address.address !== "127.0.0.1" || address.port < 1024) { server.close(); throw new Error("access.callback_listener_unavailable"); }
    return new AccessLoopback(server, address.port, callback);
  }
  #stop(outcome: AccessLoopbackOutcome) {
    if (this.#closed) return;
    this.#closed = true; clearTimeout(this.#timer);
    this.#server.close(); this.#server.closeAllConnections();
    this.#finish(outcome);
  }
  #receive(request: IncomingMessage, response: ServerResponse, callback: (code: string, state: string) => Promise<void>) {
    response.setHeader("cache-control", "no-store"); response.setHeader("referrer-policy", "no-referrer"); response.setHeader("connection", "close");
    response.setHeader("content-security-policy", "default-src 'none'; frame-ancestors 'none'");
    const target = request.url ?? "", host = request.headers.host;
    const raw = request.rawHeaders;
    let valid = !this.#consumed && !this.#closed && request.method === "GET" && request.httpVersion === "1.1" &&
      Buffer.byteLength(target) + 13 <= 2048 && raw.length <= 64 && host === `127.0.0.1:${this.port}` &&
      request.headers["transfer-encoding"] === undefined && (request.headers["content-length"] === undefined || request.headers["content-length"] === "0") &&
      target.startsWith("/creator-sync/pairing-callback?") && !/[%#]/u.test(target) &&
      raw.filter((value, index) => index % 2 === 0 && value.toLowerCase() === "host").length === 1 &&
      raw.filter((value, index) => index % 2 === 0 && value.toLowerCase() === "content-length").length <= 1;
    for (let index = 0; index < raw.length; index += 2) if (Buffer.byteLength(`${raw[index]}: ${raw[index + 1]}`) > 1024) valid = false;
    if (!valid) { response.writeHead(400); response.end("Authorization callback rejected.", () => this.#stop("rejected")); return; }
    const url = new URL(target, `http://127.0.0.1:${this.port}`), params = [...url.searchParams];
    const code = url.searchParams.get("code") ?? "", state = url.searchParams.get("state") ?? "";
    valid &&= url.pathname === "/creator-sync/pairing-callback" && Buffer.byteLength(url.search) - 1 <= 1024 && params.length === 2 &&
      params.filter(([key]) => key === "code").length === 1 && params.filter(([key]) => key === "state").length === 1 &&
      /^reacher_pairing_v1\.[A-Za-z0-9_-]{43}$/u.test(code) && /^[A-Za-z0-9_-]{43}$/u.test(state) &&
      Buffer.from(state, "base64url").toString("base64url") === state;
    if (!valid) { response.writeHead(400); response.end("Authorization callback rejected.", () => this.#stop("rejected")); return; }
    this.#consumed = true;
    request.socket.setTimeout(0);
    this.#completing = callback(code, state).then(() => {
      response.writeHead(200); response.end("Authorization complete. Return to OneCrew.", () => this.#stop("succeeded"));
    }, () => {
      response.writeHead(400); response.end("Authorization failed. Return to OneCrew.", () => this.#stop("rejected"));
    });
  }
  async cancel(): Promise<void> {
    // 注册开始后只关闭浏览器连接，等待真实结果，不中断或重复提交一次性凭据。
    this.#server.closeAllConnections();
    await this.#completing;
    this.#stop("cancelled");
    await this.finished;
  }
}
