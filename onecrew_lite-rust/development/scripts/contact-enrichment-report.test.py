import importlib.util
import json
import sqlite3
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location('audit', Path(__file__).with_name('contact-enrichment-report.py'))
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)


class AuditTest(unittest.TestCase):
    def test_completed_leads_are_not_contacts_and_saved_contact_duplicates_do_not_inflate_contribution(self):
        with tempfile.TemporaryDirectory() as temp:
            folder = Path(temp)
            db = sqlite3.connect(folder / 'crm.db')
            db.executescript('''
              CREATE TABLE discovery_campaigns(id TEXT,current_revision INTEGER);
              CREATE TABLE discovery_candidates(id TEXT,discovery_campaign_id TEXT,criteria_revision INTEGER,verdict TEXT,evidence_json TEXT,platform_account_id TEXT,profile_url TEXT);
              CREATE TABLE discovery_contact_lookups(discovery_campaign_id TEXT,criteria_revision INTEGER,receipt_json TEXT);
              CREATE TABLE discovery_public_page_reads(candidate_id TEXT,requested_url TEXT,document_url TEXT,status TEXT,stop_code TEXT);
              CREATE TABLE discovery_contact_observations(discovery_campaign_id TEXT,criteria_revision INTEGER,candidate_id TEXT,kind TEXT,normalized_value TEXT,source_url TEXT,attribution TEXT,captured_at INTEGER);
              CREATE TABLE discovery_recommendation_policies(discovery_campaign_id TEXT,criteria_revision INTEGER,policy TEXT);
              INSERT INTO discovery_campaigns VALUES('campaign',1);
              INSERT INTO discovery_recommendation_policies VALUES('campaign',1,'score_and_contact_v1');
            ''')
            evidence = {'assessment': {'reasons': [], 'match_rating': {'level': 'good'}}, 'reviewed_match': {'verified': True, 'native_author_id': 'author', 'outcome': 'supported', 'level': 'good'}}
            db.execute('INSERT INTO discovery_candidates VALUES(?,?,?,?,?,?,?)', ('candidate', 'campaign', 1, 'qualified', json.dumps(evidence), 'author', 'https://creator.example'))
            lookup = {'candidate_id': 'candidate', 'method': 'search', 'status': 'completed', 'links': ['https://agency.example/contact']}
            db.execute('INSERT INTO discovery_contact_lookups VALUES(?,?,?)', ('campaign', 1, json.dumps(lookup)))
            db.execute('INSERT INTO discovery_public_page_reads VALUES(?,?,?,?,?)', ('candidate', 'https://agency.example/contact', 'https://agency.example/contact', 'completed', None))
            db.commit()
            empty = audit.report(folder, 'campaign')
            self.assertEqual(empty['saved_provenance']['search']['distinct_contacts'], 0)
            self.assertEqual(empty['saved_provenance']['search']['first_contact_recommendation_authors'], 0)
            self.assertEqual(empty['measured_provider_calls'], {})
            for _ in range(2):
                db.execute('INSERT INTO discovery_contact_observations VALUES(?,?,?,?,?,?,?,?)', ('campaign', 1, 'candidate', 'email', 'synthetic@example.com', 'https://agency.example/contact', 'unconfirmed', 10))
            db.commit()
            saved = audit.report(folder, 'campaign')['saved_provenance']['search']
            self.assertEqual(saved['distinct_contacts'], 1)
            self.assertEqual(saved['first_contact_recommendation_authors'], 1)
            db.close()


if __name__ == '__main__':
    unittest.main()
