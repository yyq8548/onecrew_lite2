# Development operations

TikTok Business/TikTok One configuration and callback operations are documented
in [integrations/README.md](integrations/README.md).

This directory contains Cloudflare deployment, D1 migration and generated
binding-type helpers. Product code and tests stay under `src/` and `test/`;
the immutable migration guard also lives here because it protects deployment
operations rather than runtime behavior.

From the repository root:

```sh
pnpm run db:migrate:local -- --config wrangler.toml --profile onecrew
pnpm run db:migrate:remote -- --config wrangler.toml --profile onecrew
pnpm run types -- --config wrangler.toml --profile onecrew
pnpm run deploy -- --config wrangler.toml --profile onecrew
```

The profile is optional for local work and selects an existing Wrangler auth
profile; it is not a secret. Every helper passes an explicit config path and
validates the Access Worker name, custom-domain route, asset boundary, rate-limit
namespace, service target and exact `DB` binding/ID before invoking Wrangler.
The D1 migration helper addresses the `DB` binding, never a historical database
name. Named Wrangler environments and target-changing deploy flags are rejected
because they could bypass the validated target.

Remote migration and deployment are explicit production actions. Configure the
optional Resend secret against the same config/profile without putting it in
source control:

```sh
pnpm exec wrangler secret put RESEND_API_KEY --config wrangler.toml --profile onecrew
pnpm run deploy -- --config wrangler.toml --profile onecrew --var 'RESEND_FROM_EMAIL:OneCrew <auth@verified-domain.example>'
```

The sender domain must already be verified in Resend. Never pass the API key as
a command-line argument or commit it to this repository.

The deploy helper rejects known secret binding names and origin/target overrides
in forwarded options, verifies migrations `0001` through `0006`, builds the web
artifacts, and requires `web/dist/access/index.html` before it can reach the
Wrangler deploy command.
## OneCrew 官网部署

官网使用独立的 `site/wrangler.toml`、`site/build/client` 和 Custom Domain，不能使用
Access Worker 的部署命令。正式发布运行：

```bash
pnpm run deploy:site
```

脚本固定使用 `site/wrangler.toml` 与 `onecrew` auth profile，先检查域名字面量
边界、用 React Router Framework Mode 预渲染 46 个默认英文与 `/zh` 中文路径，并确认
中英文首页、`robots.txt`、`sitemap.xml`、`llms.txt` 均已生成后才调用
Wrangler。只验证不发布时运行：

```bash
pnpm run deploy:site -- --dry-run
```
