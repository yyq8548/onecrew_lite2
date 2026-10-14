use std::collections::BTreeSet;

use tantivy::{
    TantivyDocument, Term,
    collector::TopDocs,
    query::{BooleanQuery, BoostQuery, ConstScoreQuery, Occur, Query, TermQuery},
    schema::{Field, IndexRecordOption, Value},
};

use super::{
    contract::english_terms_with,
    index::{CorpusIndex, CorpusKind, SearchIndex},
};
use crate::operation_registry::descriptor::EntryId;

#[derive(Clone, Debug)]
pub(super) struct LexicalHit {
    pub(super) id: EntryId,
    pub(super) score: f32,
}

impl SearchIndex {
    pub(super) fn operation_recall(
        &self,
        query: &str,
        allowed_ids: &BTreeSet<EntryId>,
    ) -> Vec<LexicalHit> {
        self.operations.recall(query, allowed_ids)
    }

    pub(super) fn playbook_recall(
        &self,
        query: &str,
        allowed_ids: &BTreeSet<EntryId>,
    ) -> Vec<LexicalHit> {
        self.playbooks.recall(query, allowed_ids)
    }
}

impl CorpusIndex {
    fn recall(&self, query: &str, allowed_ids: &BTreeSet<EntryId>) -> Vec<LexicalHit> {
        if self.document_count == 0 || allowed_ids.is_empty() {
            return Vec::new();
        }
        let terms = english_terms_with(&self.analyzer, query);
        let core_clauses = term_clauses(&terms, &self.core_fields());
        if core_clauses.is_empty() {
            return Vec::new();
        }
        let minimum_term_matches = match self.kind {
            CorpusKind::Operations => (terms.len() * 2).div_ceil(5).max(1),
            CorpusKind::Playbooks if terms.len() <= 4 => terms.len(),
            CorpusKind::Playbooks => (terms.len() * 3).div_ceil(4),
        };
        let core_query =
            BooleanQuery::with_minimum_required_clauses(core_clauses, minimum_term_matches);
        let mut scoring_clauses = vec![(Occur::Must, Box::new(core_query) as Box<dyn Query>)];
        let bonus_clauses = term_clauses(&terms, &self.bonus_fields());
        if !bonus_clauses.is_empty() {
            scoring_clauses.push((Occur::Should, Box::new(BooleanQuery::new(bonus_clauses))));
        }
        let lexical_query = BooleanQuery::new(scoring_clauses);
        let allowed_query = BooleanQuery::new_multiterms_query(
            allowed_ids
                .iter()
                .map(|id| Term::from_field_text(self.fields.id, id.as_str()))
                .collect(),
        );
        let filtered_query = BooleanQuery::new(vec![
            (Occur::Must, Box::new(lexical_query)),
            (
                Occur::Must,
                Box::new(ConstScoreQuery::new(Box::new(allowed_query), 0.0)),
            ),
        ]);
        let searcher = self.reader.searcher();
        let top_docs = searcher
            .search(
                &filtered_query,
                &TopDocs::with_limit(allowed_ids.len()).order_by_score(),
            )
            .expect("immutable in-memory registry search must remain readable");
        top_docs
            .into_iter()
            .map(|(score, address)| {
                let document = searcher
                    .doc::<TantivyDocument>(address)
                    .expect("indexed registry document must remain readable");
                let id = document
                    .get_first(self.fields.id)
                    .and_then(|value| value.as_str())
                    .expect("indexed registry document must retain its ID");
                LexicalHit {
                    id: EntryId::parse(id.to_owned())
                        .expect("indexed registry ID was validated at construction"),
                    score,
                }
            })
            .collect()
    }

    fn core_fields(&self) -> Vec<(Field, f32)> {
        match self.kind {
            CorpusKind::Operations => vec![
                (self.fields.title, 8.0),
                (self.fields.aliases, 4.0),
                (self.fields.summary, 3.0),
            ],
            CorpusKind::Playbooks => vec![
                (self.fields.use_case, 9.0),
                (self.fields.title, 8.0),
                (self.fields.aliases, 4.0),
                (self.fields.summary, 4.0),
            ],
        }
    }

    fn bonus_fields(&self) -> Vec<(Field, f32)> {
        match self.kind {
            CorpusKind::Operations => Vec::new(),
            CorpusKind::Playbooks => vec![
                (self.fields.linked_titles, 3.0),
                (self.fields.steps, 1.0),
                (self.fields.pitfalls, 0.5),
            ],
        }
    }
}

fn term_clauses(terms: &[String], fields: &[(Field, f32)]) -> Vec<(Occur, Box<dyn Query>)> {
    if fields.is_empty() {
        return Vec::new();
    }
    terms
        .iter()
        .map(|term| {
            let field_clauses = fields
                .iter()
                .map(|(field, boost)| {
                    let term_query = TermQuery::new(
                        Term::from_field_text(*field, term),
                        IndexRecordOption::WithFreqs,
                    );
                    (
                        Occur::Should,
                        Box::new(BoostQuery::new(Box::new(term_query), *boost)) as Box<dyn Query>,
                    )
                })
                .collect();
            (
                Occur::Should,
                Box::new(BooleanQuery::new(field_clauses)) as Box<dyn Query>,
            )
        })
        .collect()
}
