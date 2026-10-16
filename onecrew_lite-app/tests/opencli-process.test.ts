import assert from "node:assert/strict";
import { test } from "node:test";
import { runTrustedOpenCliProcess, classifyOpenCliFailure } from "../src/platforms/opencli/process.ts";
import { OpenCliError } from "../src/platforms/opencli/errors.ts";

test("真实Node进程读取版本，无shell且关闭stdin", async () => {
  const result = await runTrustedOpenCliProcess({ executable: process.execPath }, ["--version"], { deadlineMs: 15000 });
  assert.equal(result.stdout.trim(), process.version);
  assert.equal(result.stderr.length, 0);
  const stdin = await runTrustedOpenCliProcess({ executable: process.execPath }, ["--eval", "process.stdin.on('end',()=>process.stdout.write('closed'));process.stdin.resume()"], { deadlineMs: 15000 });
  assert.equal(stdin.stdout, "closed");
});

test("真实子进程超时和取消结束运行，预取消不启动进程", async () => {
  await assert.rejects(runTrustedOpenCliProcess({ executable: process.execPath }, ["--eval", "setInterval(()=>{},1000)"], { deadlineMs: 30 }), { code: "opencli_timeout" });
  const controller = new AbortController();
  const running = runTrustedOpenCliProcess({ executable: process.execPath }, ["--eval", "setInterval(()=>{},1000)"], { deadlineMs: 15000, signal: controller.signal });
  controller.abort();
  await assert.rejects(running, { code: "opencli_cancelled" });
  await assert.rejects(runTrustedOpenCliProcess({ executable: process.execPath }, ["--version"], { deadlineMs: 15000, signal: controller.signal }), { code: "opencli_cancelled" });
});

test("真实进程输出限制、严格UTF8和失败不暴露stderr", async () => {
  for (const script of ["process.stdout.write(Buffer.alloc(4*1024*1024+1))", "process.stderr.write(Buffer.alloc(32*1024+1))"]) await assert.rejects(runTrustedOpenCliProcess({ executable: process.execPath }, ["--eval", script], { deadlineMs: 15000 }), { code: "opencli_output_too_large" });
  await assert.rejects(runTrustedOpenCliProcess({ executable: process.execPath }, ["--eval", "process.stdout.write(Buffer.from([255]))"], { deadlineMs: 15000 }), { code: "opencli_invalid_output" });
  await assert.rejects(runTrustedOpenCliProcess({ executable: process.execPath }, ["--eval", "process.stderr.write('private provider detail');process.exit(7)"], { deadlineMs: 15000 }), error => error instanceof OpenCliError && error.exitCode === 7 && !error.message.includes("private"));
});

test("原退出分类保持身份、验证、数据损坏及SESSION_BUSY区别", () => {
  const classify = (code: number, body: string) => classifyOpenCliFailure(code, Buffer.from(body), Buffer.alloc(0)).code;
  assert.equal(classify(77, ""), "opencli_auth_required");
  assert.equal(classify(1, "captcha"), "opencli_risk_control");
  assert.equal(classify(1, "onecrew_weibo_id_conflict"), "opencli_invalid_output");
  assert.equal(classify(75, "code: timeout"), "opencli_timeout");
  assert.equal(classify(75, "code: SESSION_BUSY"), "opencli_command_failed");
});

test("原生读取失败保留阶段，授权与风控优先于接口错误且不泄露原始内容", () => {
  const classify = (body: string) => classifyOpenCliFailure(1, Buffer.from(body), Buffer.alloc(0));
  for (const [marker, code] of [
    ["onecrew_instagram_search_read_http_404", "provider_endpoint_not_found"],
    ["onecrew_instagram_search_read_http_500", "provider_response_http_error"],
    ["onecrew_instagram_search_read_json", "provider_response_invalid_json"],
    ["onecrew_reddit_profile_read_redirect", "provider_response_redirected"],
    ["onecrew_reddit_profile_inaccessible", "provider_source_unavailable"],
    ["onecrew_pinterest_read_resource", "provider_response_shape_invalid"],
    ["onecrew_youtube_layout_unsupported", "youtube_watch_layout_unsupported"],
    ["onecrew_youtube_description_unavailable", "youtube_description_unavailable"],
  ]) {
    const error = classify(marker!);
    assert.equal(error.code, code);
    assert.equal(error.message, code);
  }
  assert.equal(classify("onecrew_instagram_search_risk onecrew_instagram_search_read_http_404").code, "opencli_risk_control");
  assert.equal(classify("onecrew_instagram_search_auth onecrew_instagram_search_read_json").code, "opencli_auth_required");
  assert.equal(classify("captcha onecrew_youtube_video_unavailable").code, "opencli_risk_control");
});
