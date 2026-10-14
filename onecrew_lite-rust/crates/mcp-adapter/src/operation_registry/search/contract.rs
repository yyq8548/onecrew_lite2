use onecrew_local_runtime::ToolkitId;
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use tantivy::tokenizer::{
    Language, LowerCaser, RemoveLongFilter, SimpleTokenizer, Stemmer, StopWordFilter, TextAnalyzer,
    TokenStream,
};

use crate::operation_registry::descriptor::{
    DescriptorError, DomainId, EffectClass, SearchLimit, SearchText,
};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SearchQuery {
    pub(crate) text: SearchText,
    #[serde(default, deserialize_with = "deserialize_toolkits")]
    pub(crate) toolkits: Vec<ToolkitId>,
    pub(crate) domain: Option<DomainId>,
    #[serde(default, deserialize_with = "deserialize_effects")]
    pub(crate) effects: Vec<EffectClass>,
    pub(crate) limit: Option<SearchLimit>,
}

pub(crate) fn validate_query_text(value: &str) -> Result<(), DescriptorError> {
    if value
        .chars()
        .any(|character| !character.is_ascii() && character.is_alphanumeric())
    {
        return Err(DescriptorError::SearchTextNotEnglish);
    }
    if english_terms(value).is_empty() {
        return Err(DescriptorError::SearchTextHasNoMeaningfulTerms);
    }
    Ok(())
}

pub(super) fn english_analyzer() -> TextAnalyzer {
    TextAnalyzer::builder(SimpleTokenizer::default())
        .filter(RemoveLongFilter::limit(40))
        .filter(LowerCaser)
        .filter(
            StopWordFilter::new(Language::English)
                .expect("Tantivy's English stop-word list is enabled"),
        )
        .filter(Stemmer::new(Language::English))
        .build()
}

pub(super) fn english_terms(value: &str) -> Vec<String> {
    english_terms_with(&english_analyzer(), value)
}

pub(super) fn english_terms_with(analyzer: &TextAnalyzer, value: &str) -> Vec<String> {
    let mut analyzer = analyzer.clone();
    let mut stream = analyzer.token_stream(value);
    let mut terms = BTreeSet::new();
    while stream.advance() {
        terms.insert(stream.token().text.clone());
    }
    terms.into_iter().collect()
}

fn deserialize_toolkits<'de, D>(deserializer: D) -> Result<Vec<ToolkitId>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserialize_unique(deserializer, 8, "toolkit")
}

fn deserialize_effects<'de, D>(deserializer: D) -> Result<Vec<EffectClass>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserialize_unique(deserializer, 7, "effect")
}

fn deserialize_unique<'de, D, T>(
    deserializer: D,
    maximum: usize,
    label: &str,
) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + PartialEq,
{
    let values = Vec::<T>::deserialize(deserializer)?;
    if values.len() > maximum
        || values
            .iter()
            .enumerate()
            .any(|(index, value)| values[..index].contains(value))
    {
        return Err(serde::de::Error::custom(format!(
            "{label} filters must be unique and contain at most {maximum} values"
        )));
    }
    Ok(values)
}
