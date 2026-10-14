#!/usr/bin/env python3
"""Read-only contact provenance audit. Never infer contacts from completed reads."""
import argparse
import hashlib
import json
import sqlite3
from collections import defaultdict
from pathlib import Path
from urllib.parse import quote


def score_passes(row):
    e = json.loads(row['evidence_json'])
    assessment, match = e.get('assessment', {}), e.get('reviewed_match', {})
    level = assessment.get('match_rating', {}).get('level')
    return (row['verdict'] == 'qualified' and assessment.get('reasons') == []
            and match.get('verified') is True and match.get('native_author_id') == row['platform_account_id']
            and match.get('outcome') == 'supported' and level in ('excellent', 'good', 'medium')
            and match.get('level') == level)


def report(data_dir, campaign):
    db = sqlite3.connect('file:' + quote(str(data_dir / 'crm.db')) + '?mode=ro', uri=True)
    db.row_factory = sqlite3.Row
    with db:
        revision = db.execute('SELECT current_revision FROM discovery_campaigns WHERE id=?', (campaign,)).fetchone()
        if revision is None:
            raise ValueError('campaign_not_found')
        revision = revision[0]
        candidates = {r['id']: r for r in db.execute('SELECT * FROM discovery_candidates WHERE discovery_campaign_id=? AND criteria_revision=?', (campaign, revision))}
        lookups = [json.loads(r[0]) for r in db.execute('SELECT receipt_json FROM discovery_contact_lookups WHERE discovery_campaign_id=? AND criteria_revision=?', (campaign, revision))]
        pages = [dict(r) for r in db.execute('SELECT p.* FROM discovery_public_page_reads p JOIN discovery_candidates c ON c.id=p.candidate_id WHERE c.discovery_campaign_id=? AND c.criteria_revision=?', (campaign, revision))]
        contacts = [dict(r) for r in db.execute('SELECT * FROM discovery_contact_observations WHERE discovery_campaign_id=? AND criteria_revision=?', (campaign, revision))]
        policy = db.execute('SELECT policy FROM discovery_recommendation_policies WHERE discovery_campaign_id=? AND criteria_revision=?', (campaign, revision)).fetchone()
    db.close()
    groups = defaultdict(lambda: {'lookup_calls': 0, 'page_reads': 0, 'failed_pages': 0, 'distinct_contacts': set(), 'hidden_email_opportunities': set(), 'other_platform_accounts': set(), 'first_contact_recommendation_authors': set()})
    roots = {(r['candidate_id'], url): r['method'] for r in lookups if r['status'] == 'completed' for url in r['links']}
    for r in lookups:
        groups[r['method']]['lookup_calls'] += 1
    for p in pages:
        c = candidates[p['candidate_id']]
        native = p['requested_url'] in (c['profile_url'], 'https://www.youtube.com/channel/' + c['platform_account_id'])
        p['origin'] = roots.get((p['candidate_id'], p['requested_url']), 'direct_profile' if native else 'associated_page')
        groups[p['origin']]['page_reads'] += 1
        groups[p['origin']]['failed_pages'] += p['status'] != 'completed'
    # Links and successful reads do not prove contact discovery. Count only
    # persisted source-bound contact observations; same contact is deduplicated.
    first = {}
    for c in contacts:
        origin = 'native' if c['attribution'] == 'author_published' else 'unattributed'
        if origin == 'unattributed':
            saved = [p for p in pages if p['candidate_id'] == c['candidate_id'] and p['status'] == 'completed' and p['stop_code'] is None and c['source_url'] in (p['document_url'], p['requested_url'])]
            if len({p['origin'] for p in saved}) == 1:
                origin = saved[0]['origin']
        if c['kind'] in ('hidden_email', 'social_profile'):
            field = 'hidden_email_opportunities' if c['kind'] == 'hidden_email' else 'other_platform_accounts'
            groups[origin][field].add((c['candidate_id'], c['normalized_value']))
            continue
        groups[origin]['distinct_contacts'].add((c['candidate_id'], c['kind'], c['normalized_value']))
        first.setdefault(c['candidate_id'], []).append((c['captured_at'], origin))
    for candidate, observations in first.items():
        if policy and policy[0] == 'score_and_contact_v1' and score_passes(candidates[candidate]):
            earliest = min(t for t, _ in observations)
            origins = {origin for t, origin in observations if t == earliest}
            origin = next(iter(origins)) if len(origins) == 1 else 'ambiguous_first_contact'
            groups[origin]['first_contact_recommendation_authors'].add(candidate)
    for metrics in groups.values():
        for field in ('distinct_contacts', 'hidden_email_opportunities', 'other_platform_accounts', 'first_contact_recommendation_authors'):
            metrics[field] = len(metrics[field])
    telemetry = defaultdict(lambda: {'measured_calls': 0, 'provider_elapsed_ms': 0, 'raw_results': 0, 'kept_links': 0})
    path = data_dir / 'contact-enrichment-metrics' / (hashlib.sha256(campaign.encode()).hexdigest() + '.jsonl')
    if path.exists():
        for line in path.read_text().splitlines():
            try:
                e = json.loads(line)
            except json.JSONDecodeError:
                continue  # A diagnostic interrupted at append is not a saved result.
            key = e.get('method') if e['kind'] == 'lookup' else 'page'
            m = telemetry[key]
            m['measured_calls'] += 1
            m['provider_elapsed_ms'] += e['elapsed_ms']
            m['raw_results'] += e.get('raw_results', 0)
            m['kept_links'] += e.get('kept_links', 0)
    return {'campaign_id': campaign, 'revision': revision, 'saved_provenance': dict(groups), 'measured_provider_calls': dict(telemetry),
            'notes': ['Telemetry covers only calls after this instrumentation; missing timing is unknown, not zero.',
                      'Counts use saved contacts, never links or completed status. First-contact authors are distinct by source.',
                      'Saved association is conservative; chained links without exact lookup provenance stay associated_page.',
                      'Provider timings exclude model scoring, database commit and Library synchronization.']}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--data-dir', required=True, type=Path)
    parser.add_argument('--campaign', required=True)
    args = parser.parse_args()
    print(json.dumps(report(args.data_dir, args.campaign), ensure_ascii=False, indent=2))
