/// The `Subfolder naming` rule from `documents/README.md`.
pub fn archive_folder(company: &str, role: &str) -> Option<String> {
    let lowered = format!("{company}_{role}").to_lowercase().replace(' ', "_");
    let kept: String = lowered.chars().filter(|c| c.is_alphanumeric() || *c == '_').collect();
    let name = kept.split('_').filter(|part| !part.is_empty()).collect::<Vec<_>>().join("_");
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::archive_folder;

    #[test]
    fn follows_the_documented_examples() {
        assert_eq!(archive_folder("Novo Nordisk A/S", "ML Engineer").as_deref(), Some("novo_nordisk_as_ml_engineer"));
        assert_eq!(archive_folder("  Acme__Co ", "../Lead").as_deref(), Some("acme_co_lead"));
        assert_eq!(archive_folder("//", "!!"), None);
    }
}
