import assert from 'node:assert/strict';
import { randomBytes } from 'node:crypto';
import { once } from 'node:events';
import { spawn } from 'node:child_process';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import * as XLSX from 'xlsx';
import { serve } from '@hono/node-server';
import { Hono } from 'hono';
import { createApp } from '../../src/app.js';
import { createDiscoverySyncRoutes } from '../../src/modules/tables/discovery-sync-http.js';
import { TestAccess } from '../helpers/test-access.js';

const binary = process.env.ONECREW_TEST_RUNTIME_BINARY;
assert(
  binary && path.isAbsolute(binary),
  'Build the Rust runtime test binary and provide its absolute path',
);
const root = await mkdtemp(path.join(os.tmpdir(), 'onecrew-rust-library-'));
const liveTikTok = process.env.ONECREW_TEST_LIVE_TIKTOK === 'true';
const liveYouTube = process.env.ONECREW_TEST_LIVE_YOUTUBE === 'true';
const liveTwitter = process.env.ONECREW_TEST_LIVE_TWITTER === 'true';
const liveReddit = process.env.ONECREW_TEST_LIVE_REDDIT === 'true';
const liveFacebook = process.env.ONECREW_TEST_LIVE_FACEBOOK === 'true';
const nativeEntrance = process.env.ONECREW_TEST_NATIVE_ENTRANCE;
const nativeOutcome = process.env.ONECREW_TEST_NATIVE_OUTCOME ?? 'contact';
assert(['contact', 'no_contact', 'hidden_email', 'social_account'].includes(nativeOutcome));
const noContactFixture = process.env.ONECREW_TEST_LIBRARY_NO_CONTACT === 'true';
assert(
  !nativeEntrance ||
    [
      'youtube',
      'tiktok',
      'twitter',
      'bilibili',
      'reddit',
      'weibo',
      'zhihu',
      'facebook',
      'pinterest',
      'linkedin',
      'douyin',
      'google',
      'yahoo',
    ].includes(nativeEntrance),
);
const youtubeAudience = process.env.ONECREW_TEST_YOUTUBE_AUDIENCE === 'true';
const twitterAudience = process.env.ONECREW_TEST_TWITTER_AUDIENCE === 'true';
const redditAudience = process.env.ONECREW_TEST_REDDIT_AUDIENCE === 'true';
assert(!redditAudience || liveReddit, 'Audience acceptance requires the Reddit live case');
assert(!youtubeAudience || liveYouTube, 'Audience acceptance requires the YouTube live case');
assert(!twitterAudience || liveTwitter, 'Audience acceptance requires the X live case');
assert(
  [liveTikTok, liveYouTube, liveTwitter, liveReddit, liveFacebook, !!nativeEntrance].filter(Boolean)
    .length <= 1,
  'Select one live platform per bounded run',
);
const live =
  liveTikTok || liveYouTube || liveTwitter || liveReddit || liveFacebook || !!nativeEntrance;
assert(!noContactFixture || !live, 'Synthetic no-contact fixture cannot run live provider reads');
const webOrigin = process.env.ONECREW_TEST_WEB_ORIGIN;
assert(!webOrigin || ['google', 'yahoo'].includes(webOrigin));
assert(!webOrigin || !live, 'Engine projection fixture cannot run live provider reads');
const exportPath = path.join(root, 'engine-creators.xlsx');
const expectationPath = path.join(root, 'native-expectations.json');
const runtime = await createApp(
  root,
  new TestAccess({
    id: live ? 'fixture-operator' : 'rust-library-acceptance',
    email: 'rust@example.test',
  }),
);
const token = randomBytes(32).toString('hex');
const producer = createDiscoverySyncRoutes(runtime.data, runtime.resolveTenant, token);
const app = new Hono()
  .all('/_local/discovery/*', (c) => producer.fetch(c.req.raw))
  .all('*', (c) => runtime.app.fetch(c.req.raw));
const server = serve({ fetch: app.fetch, hostname: '127.0.0.1', port: 0 });
try {
  if (!server.listening) await once(server, 'listening');
  const address = server.address();
  assert(address && typeof address !== 'string');
  const child = spawn(
    binary,
    [
      noContactFixture
        ? 'library_sync::client::tests::isolated_library_receiver_excludes_scored_sources_without_contacts'
        : nativeEntrance
          ? 'discovery_match::native_acceptance::actual_native_source_contact_recommendation_and_restart'
          : liveFacebook
            ? 'discovery_match::live_tests::facebook_language_screening_stops_and_publishes'
            : liveReddit
              ? redditAudience
                ? 'discovery_match::live_tests::reddit_audience_language_scores_and_publishes'
                : 'discovery_match::live_tests::reddit_observed_author_scores_and_publishes'
              : liveTwitter
                ? twitterAudience
                  ? 'discovery_match::live_tests::twitter_audience_language_screening_stops_and_publishes'
                  : 'discovery_match::live_tests::twitter_language_screening_stops_and_publishes'
                : liveYouTube
                  ? youtubeAudience
                    ? 'discovery_match::live_tests::youtube_audience_language_screening_stops_and_publishes'
                    : 'discovery_match::live_tests::youtube_language_screening_stops_and_publishes'
                  : liveTikTok
                    ? process.env.ONECREW_TEST_TIKTOK_LANGUAGE === 'true'
                      ? 'discovery_match::live_tests::tiktok_language_screening_stops_and_publishes'
                      : 'discovery_match::live_tests::tiktok_observed_profile_scores_and_publishes_without_unnecessary_comments'
                    : 'library_sync::client::tests::isolated_library_receiver_preserves_score_and_manual_override',
      '--ignored',
      '--exact',
      '--nocapture',
    ],
    {
      windowsHide: true,
      stdio: ['ignore', 'pipe', 'pipe'],
      env: {
        ...process.env,
        ONECREW_TEST_LIBRARY_URL: `http://127.0.0.1:${address.port}`,
        ONECREW_TEST_LIBRARY_TOKEN: token,
        ONECREW_TEST_LIBRARY_EXPORT: exportPath,
        ONECREW_TEST_NATIVE_EXPECTATIONS: expectationPath,
      },
    },
  );
  child.stdout.pipe(process.stdout);
  child.stderr.pipe(process.stderr);
  const timer = setTimeout(
    () => child.kill(),
    nativeEntrance
      ? 240000
      : youtubeAudience || twitterAudience || redditAudience
        ? 240000
        : live
          ? 120000
          : 60000,
  );
  try {
    const [code] = await once(child, 'exit');
    assert.equal(code, 0);
    if (noContactFixture) {
      const workbook = XLSX.read(await readFile(exportPath), { type: 'buffer' });
      assert.deepEqual(workbook.SheetNames, ['推荐']);
      const rows = XLSX.utils.sheet_to_json<unknown[]>(workbook.Sheets['推荐'], { header: 1 });
      assert.deepEqual(rows, [['名字', '类别', '平台', '评分', '联系方式', '粉丝量']]);
      console.log(
        'Synthetic no-contact fixture: HTTP export contains no recommendation PASS',
      );
    }
    if (nativeEntrance) {
      const workbook = XLSX.read(await readFile(exportPath), { type: 'buffer' });
      assert.deepEqual(workbook.SheetNames, ['推荐']);
      const rows = XLSX.utils.sheet_to_json<unknown[]>(workbook.Sheets['推荐'], {
        header: 1,
        defval: '',
      });
      assert.deepEqual(rows[0], ['名字', '类别', '平台', '评分', '联系方式', '粉丝量']);
      const expected = JSON.parse(await readFile(expectationPath, 'utf8'));
      const sources = expected.view.sources.filter(
        (source: { verdict: string }) => source.verdict === 'qualified',
      );
      assert.equal(sources.length, nativeOutcome === 'contact' ? 1 : 0);
      assert.equal(rows.length, sources.length + 1, 'Only actual qualified creators are exported');
      if (nativeOutcome !== 'contact') {
        assert.equal(expected.view.sources.length, 1);
        assert.equal(expected.view.sources[0].verdict, 'pending');
        assert.deepEqual(expected.view.sources[0].reasons, ['contact_unavailable']);
        assert.deepEqual(expected.view.sources[0].contacts, []);
        if (nativeOutcome === 'hidden_email' || nativeOutcome === 'social_account') {
          const binding = expected.view.projection.source_rows.find(
            (source: { search_hit_id: string }) =>
              source.search_hit_id === expected.view.sources[0].search_hit_id,
          );
          assert(binding?.candidate_id, 'Source identity projection binds the candidate');
          const candidate = expected.displayed_candidates.find(
            (candidate: { id: string }) => candidate.id === binding.candidate_id,
          );
          assert(candidate, 'Fallback display belongs to the actual candidate');
          assert.equal(candidate.platform_account_id, binding.native_author_id);
          if (nativeOutcome === 'hidden_email') {
            assert.equal(candidate.evidence.contact_status.manual_email_required, true);
            assert.equal(candidate.evidence.contact_status.has_contact, false);
            assert.equal(
              candidate.profile_url,
              `https://www.youtube.com/channel/${candidate.platform_account_id}`,
            );
          } else {
            assert(candidate.evidence.associated_social_accounts.length > 0);
            for (const account of candidate.evidence.associated_social_accounts) {
              assert.equal(account.kind, 'social_profile');
              assert.equal(account.native_author_id, candidate.platform_account_id);
              assert(account.source_url.startsWith('https://'));
            }
          }
        }
        console.log(
          `${nativeEntrance}: real source and score, ${nativeOutcome} branch and empty recommendation five-column HTTP export PASS`,
        );
      } else {
        assert.equal(rows[1].length, 6);
        const member = expected.view.projection.groups
          .flatMap((group: { members: Array<{ source_hit_ids: string[] }> }) => group.members)
          .find((value: { source_hit_ids: string[] }) =>
            value.source_hit_ids.includes(sources[0].search_hit_id),
          );
        assert(member);
        assert.equal(rows[1][0], member.display_name || member.native_author_id);
        assert.equal(rows[1][2], member.platform === 'twitter' ? 'X' : member.platform);
        const scoreColumn = expected.table.columns.find(
          (column: { name: string }) => column.name === 'Match evaluation',
        );
        const categoryColumn = expected.table.columns.find(
          (column: { name: string }) => column.name === 'Content category',
        );
        assert.equal(
          rows[1][1],
          categoryColumn ? (sources[0].row.values[categoryColumn.id] ?? '') : '',
        );
        const labels: Record<string, string> = { excellent: '优秀', good: '良好', medium: '一般' };
        assert.equal(rows[1][3], labels[sources[0].row.values[scoreColumn.id].level]);
        assert(
          sources[0].contacts.some((contact: { normalized_value: string }) =>
            String(rows[1][4]).includes(contact.normalized_value),
          ),
          'Export contact comes from the actual saved source receipt',
        );
        console.log(
          `${nativeEntrance}: real source, native model, recommended contacts and five-column HTTP export PASS`,
        );
      }
    }
    if (webOrigin) {
      const workbook = XLSX.read(await readFile(exportPath), { type: 'buffer' });
      assert.deepEqual(workbook.SheetNames, ['推荐']);
      const rows = XLSX.utils.sheet_to_json(workbook.Sheets['推荐'], { header: 1 });
      assert.deepEqual(rows[0], ['名字', '类别', '平台', '评分', '联系方式', '粉丝量']);
      assert.equal(rows.length, 2);
      assert.deepEqual((rows[1] as unknown[]).slice(0, 5), ['Creator', '', 'youtube', '良好', 'business@example.test']);
      console.log(`${webOrigin} source: native platform, email and five-column HTTP export PASS`);
    }
    if (live && process.env.ONECREW_TEST_LIVE_CONTACT_UI === 'true') {
      console.log(
        'Isolated result available for 90 seconds for read-only UI acceptance; cleanup follows.',
      );
      await new Promise((resolve) => setTimeout(resolve, 90000));
    }
  } finally {
    clearTimeout(timer);
  }
} finally {
  await new Promise<void>((resolve, reject) => {
    server.close((error) => (error ? reject(error) : resolve()));
    if ('closeAllConnections' in server) server.closeAllConnections();
  });
  await runtime.closeMcp();
  runtime.browsers.closeAll();
  await runtime.data.close();
  await rm(root, { recursive: true, force: true });
}
