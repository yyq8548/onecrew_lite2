import { OpenCliError } from './errors.ts';
import type { ReviewedOpenCliRead } from './read-plan.ts';
import { validateHistoricalDiscoveryContent, validateHistoricalDiscoveryProfile } from '../../discovery/historical-native-material.ts';

export function zhihuReadPlan(kind: 'profile' | 'content', author: string, source: string): ReviewedOpenCliRead<Record<string, unknown>> {
  const profile = /^https:\/\/www\.zhihu\.com\/people\/[A-Za-z0-9_-]{1,128}$/u;
  const content = /^https:\/\/(?:www\.zhihu\.com\/question\/[1-9][0-9]{0,19}\/answer\/[1-9][0-9]{0,19}|zhuanlan\.zhihu\.com\/p\/[1-9][0-9]{0,19})$/u;
  if (!/^[a-f0-9]{32}$/u.test(author) || !(kind === 'profile' ? profile : content).test(source)) throw new OpenCliError('opencli_invalid_input');
  return { site: 'onecrew-zhihu', name: kind, args: ['--author', author, '-f', 'json', '--', source], addons: ['onecrew-zhihu', 'onecrew-browser-search'], maximumBytes: 24000,
    decode(value: unknown) {
      if (!Array.isArray(value) || value.length !== 1) throw new OpenCliError('opencli_invalid_output');
      try { return kind === 'profile' ? validateHistoricalDiscoveryProfile('zhihu', author, source, value[0]) : validateHistoricalDiscoveryContent('zhihu', author, source, value[0]); }
      catch (error) { throw new OpenCliError('opencli_invalid_output', null, { stage: `zhihu_${kind}_identity_validation`, exit_code: 0, stdout: '', stderr: error instanceof Error ? error.message : String(error) }); }
    } };
}
