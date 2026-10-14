pub(super) fn validate_prefixed_uuid(value: &str, prefix: &str) -> Result<(), &'static str> {
    let Some(hex) = value.strip_prefix(prefix) else {
        return Err("invalid result identifier prefix");
    };
    if hex.len() != 32
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("invalid result identifier");
    }
    Ok(())
}
