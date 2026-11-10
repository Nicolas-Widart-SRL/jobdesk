/// The portals that ship with `ai-job-search`, by folder, with the country each one searches.
pub const BUILT_IN: [(&str, &str); 6] = [
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
    /// Shipped with `ai-job-search`.
    BuiltIn,
    /// Installed from the community list.
    Community,
    /// Made with `/add-portal`, or anything else Jobdesk does not know.
    MadeByYou,
}

/// Where the portal in `folder` came from, and its country when Jobdesk knows it. `community` holds the community
/// list's folders, each with its country.
pub fn origin(folder: &str, community: &[(String, String)]) -> (Origin, Option<String>) {
    if let Some((_, country)) = BUILT_IN.iter().find(|(name, _)| *name == folder) {
        return (Origin::BuiltIn, Some((*country).to_owned()));
    }
    match community.iter().find(|(name, _)| name == folder) {
        Some((_, country)) => (Origin::Community, Some(country.clone())),
        None => (Origin::MadeByYou, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_is_built_in_then_community_then_made_by_you() {
        let community = [("northfield-search".to_owned(), "Belgium".to_owned()), ("linkedin-search".to_owned(), "Elsewhere".to_owned())];
        assert_eq!(origin("linkedin-search", &community), (Origin::BuiltIn, Some("Worldwide".into())), "a built-in name wins");
        assert_eq!(origin("jobnet-search", &[]), (Origin::BuiltIn, Some("Denmark".into())));
        assert_eq!(origin("northfield-search", &community), (Origin::Community, Some("Belgium".into())));
        assert_eq!(origin("northfield-search", &[]), (Origin::MadeByYou, None));
        assert_eq!(origin("LinkedIn-search", &[]), (Origin::MadeByYou, None), "folder names match exactly");
    }
}
