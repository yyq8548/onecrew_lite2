use serde::Deserialize;

pub(super) fn deserialize_unique_filter<'de, D, T>(
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
