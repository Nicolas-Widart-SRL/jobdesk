/// The folder of the checkout that holds a folder for each portal, and the manifest each portal folder holds.
pub const SKILLS: &str = ".agents/skills";
pub const MANIFEST: &str = "SKILL.md";

/// The portals `ai-job-search` shipped before 2.0.0, which now live in the community list, by folder, with the country
/// each one searches.
const FORMER_BUNDLED: [(&str, &str); 6] = [
    ("linkedin-search", "Worldwide"),
    ("freehire-search", "Worldwide"),
    ("jobindex-search", "Denmark"),
    ("jobbank-search", "Denmark"),
    ("jobdanmark-search", "Denmark"),
    ("jobnet-search", "Denmark"),
];

/// Where an installed portal came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Installed from the community list, or one of the portals `ai-job-search` shipped before 2.0.0.
    Community,
    /// Made with `/add-portal`, or anything else Jobdesk does not know.
    MadeByYou,
}

/// Where the portal in `folder` came from, and its country when Jobdesk knows it. `community` holds the community
/// list's folders, each with its country. The list is read only when Browse community opens, so a portal
/// `ai-job-search` shipped before 2.0.0 reads as Community without it.
pub fn origin(folder: &str, community: &[(String, String)]) -> (Origin, Option<String>) {
    if let Some((_, country)) = community.iter().find(|(name, _)| name == folder) {
        return (Origin::Community, Some(country.clone()));
    }
    match FORMER_BUNDLED.iter().find(|(name, _)| *name == folder) {
        Some((_, country)) => (Origin::Community, Some((*country).to_owned())),
        None => (Origin::MadeByYou, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_is_listed_then_formerly_bundled_then_made_by_you() {
        let community = [("northfield-search".to_owned(), "Belgium".to_owned()), ("linkedin-search".to_owned(), "Elsewhere".to_owned())];
        assert_eq!(origin("linkedin-search", &community), (Origin::Community, Some("Elsewhere".into())), "the list wins");
        assert_eq!(origin("jobnet-search", &[]), (Origin::Community, Some("Denmark".into())));
        assert_eq!(origin("northfield-search", &community), (Origin::Community, Some("Belgium".into())));
        assert_eq!(origin("northfield-search", &[]), (Origin::MadeByYou, None));
        assert_eq!(origin("LinkedIn-search", &[]), (Origin::MadeByYou, None), "folder names match exactly");
    }
}
