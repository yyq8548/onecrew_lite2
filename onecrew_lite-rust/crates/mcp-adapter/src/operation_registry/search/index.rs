use std::collections::BTreeMap;

use tantivy::{
    Index, IndexReader, ReloadPolicy, TantivyDocument,
    schema::{Field, IndexRecordOption, STORED, STRING, Schema, TextFieldIndexing, TextOptions},
    tokenizer::TextAnalyzer,
};

use super::contract::english_analyzer;
use crate::operation_registry::descriptor::{EntryId, RegistryEntry};

pub(super) const TOKENIZER_NAME: &str = "reacher_en";
const INDEX_WRITER_MEMORY_BYTES: usize = 15_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SearchIndexBuildError;

pub(crate) struct SearchIndex {
    pub(super) operations: CorpusIndex,
    pub(super) playbooks: CorpusIndex,
}

impl SearchIndex {
    pub(crate) fn build(
        entries: &BTreeMap<EntryId, RegistryEntry>,
    ) -> Result<Self, SearchIndexBuildError> {
        let operation_documents = entries
            .values()
            .filter_map(|entry| match entry {
                RegistryEntry::Operation(operation) => Some(IndexDocument {
                    id: operation.metadata.id.as_str().to_owned(),
                    title: operation.metadata.title.clone(),
                    summary: operation.metadata.summary.clone(),
                    aliases: operation.metadata.aliases.join(" "),
                    use_case: String::new(),
                    linked_titles: String::new(),
                    steps: String::new(),
                    pitfalls: String::new(),
                }),
                RegistryEntry::Playbook(_) | RegistryEntry::ToolkitGuide(_) => None,
            })
            .collect::<Vec<_>>();
        let playbook_documents = entries
            .values()
            .filter_map(|entry| match entry {
                RegistryEntry::Playbook(playbook) => {
                    let linked_titles = playbook
                        .primary_operation_ids
                        .iter()
                        .chain(&playbook.supporting_operation_ids)
                        .filter_map(|id| match entries.get(id) {
                            Some(RegistryEntry::Operation(operation)) => {
                                Some(operation.metadata.title.as_str())
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    Some(IndexDocument {
                        id: playbook.metadata.id.as_str().to_owned(),
                        title: playbook.metadata.title.clone(),
                        summary: playbook.metadata.summary.clone(),
                        aliases: playbook.metadata.aliases.join(" "),
                        use_case: playbook.use_case.clone(),
                        linked_titles,
                        steps: playbook
                            .recommended_steps
                            .iter()
                            .map(|step| step.instruction.as_str())
                            .collect::<Vec<_>>()
                            .join(" "),
                        pitfalls: playbook.known_pitfalls.join(" "),
                    })
                }
                RegistryEntry::Operation(_) | RegistryEntry::ToolkitGuide(_) => None,
            })
            .collect::<Vec<_>>();

        Ok(Self {
            operations: CorpusIndex::build(operation_documents, CorpusKind::Operations)?,
            playbooks: CorpusIndex::build(playbook_documents, CorpusKind::Playbooks)?,
        })
    }
}

#[derive(Clone, Copy)]
pub(super) enum CorpusKind {
    Operations,
    Playbooks,
}

pub(super) struct CorpusIndex {
    pub(super) reader: IndexReader,
    pub(super) fields: SearchFields,
    pub(super) analyzer: TextAnalyzer,
    pub(super) kind: CorpusKind,
    pub(super) document_count: usize,
}

impl CorpusIndex {
    fn build(
        documents: Vec<IndexDocument>,
        kind: CorpusKind,
    ) -> Result<Self, SearchIndexBuildError> {
        let (schema, fields) = search_schema();
        let index = Index::create_in_ram(schema);
        let analyzer = english_analyzer();
        index
            .tokenizers()
            .register(TOKENIZER_NAME, analyzer.clone());
        let mut writer = index
            .writer(INDEX_WRITER_MEMORY_BYTES)
            .map_err(|_| SearchIndexBuildError)?;
        let document_count = documents.len();
        for source in documents {
            let mut document = TantivyDocument::default();
            document.add_text(fields.id, source.id);
            document.add_text(fields.title, source.title);
            document.add_text(fields.summary, source.summary);
            document.add_text(fields.aliases, source.aliases);
            document.add_text(fields.use_case, source.use_case);
            document.add_text(fields.linked_titles, source.linked_titles);
            document.add_text(fields.steps, source.steps);
            document.add_text(fields.pitfalls, source.pitfalls);
            writer
                .add_document(document)
                .map_err(|_| SearchIndexBuildError)?;
        }
        writer.commit().map_err(|_| SearchIndexBuildError)?;
        drop(writer);
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()
            .map_err(|_| SearchIndexBuildError)?;
        Ok(Self {
            reader,
            fields,
            analyzer,
            kind,
            document_count,
        })
    }
}

#[derive(Clone, Copy)]
pub(super) struct SearchFields {
    pub(super) id: Field,
    pub(super) title: Field,
    pub(super) summary: Field,
    pub(super) aliases: Field,
    pub(super) use_case: Field,
    pub(super) linked_titles: Field,
    pub(super) steps: Field,
    pub(super) pitfalls: Field,
}

fn search_schema() -> (Schema, SearchFields) {
    let mut builder = Schema::builder();
    let id = builder.add_text_field("id", STRING | STORED);
    let indexing = TextFieldIndexing::default()
        .set_tokenizer(TOKENIZER_NAME)
        .set_index_option(IndexRecordOption::WithFreqsAndPositions);
    let text = TextOptions::default().set_indexing_options(indexing);
    let title = builder.add_text_field("title", text.clone());
    let summary = builder.add_text_field("summary", text.clone());
    let aliases = builder.add_text_field("aliases", text.clone());
    let use_case = builder.add_text_field("use_case", text.clone());
    let linked_titles = builder.add_text_field("linked_titles", text.clone());
    let steps = builder.add_text_field("steps", text.clone());
    let pitfalls = builder.add_text_field("pitfalls", text);
    (
        builder.build(),
        SearchFields {
            id,
            title,
            summary,
            aliases,
            use_case,
            linked_titles,
            steps,
            pitfalls,
        },
    )
}

struct IndexDocument {
    id: String,
    title: String,
    summary: String,
    aliases: String,
    use_case: String,
    linked_titles: String,
    steps: String,
    pitfalls: String,
}
