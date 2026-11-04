use std::ffi::OsString;
use std::path::Path;

/// The arguments the new copy starts with: every original one in order, minus each `--workspace <value>` pair,
/// then `--workspace <open folder>` when a folder is open. `original` holds the arguments after the program name.
pub fn relaunch_args(original: &[OsString], workspace: Option<&Path>) -> Vec<OsString> {
    let mut args = Vec::with_capacity(original.len() + 2);
    let mut rest = original.iter();
    while let Some(arg) = rest.next() {
        if arg == "--workspace" {
            rest.next();
        } else {
            args.push(arg.clone());
        }
    }
    if let Some(folder) = workspace {
        args.push(OsString::from("--workspace"));
        args.push(folder.as_os_str().to_owned());
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn no_arguments_gain_the_open_folder() {
        assert_eq!(relaunch_args(&[], Some(Path::new("/jobs"))), os(&["--workspace", "/jobs"]));
    }

    #[test]
    fn a_stale_workspace_is_replaced() {
        assert_eq!(relaunch_args(&os(&["--workspace", "/old"]), Some(Path::new("/new"))), os(&["--workspace", "/new"]));
        assert_eq!(relaunch_args(&os(&["--workspace", "/a", "--workspace", "/b"]), Some(Path::new("/c"))), os(&["--workspace", "/c"]));
    }

    #[test]
    fn other_arguments_are_kept_in_order() {
        let original = os(&["--verbose", "--workspace", "/old", "--theme", "dark"]);
        assert_eq!(relaunch_args(&original, Some(Path::new("/new"))), os(&["--verbose", "--theme", "dark", "--workspace", "/new"]));
    }

    #[test]
    fn with_no_folder_open_no_workspace_is_passed() {
        assert_eq!(relaunch_args(&os(&["--workspace", "/old", "--verbose"]), None), os(&["--verbose"]));
        assert_eq!(relaunch_args(&os(&["--workspace"]), None), Vec::<OsString>::new());
        assert_eq!(relaunch_args(&[], None), Vec::<OsString>::new());
    }
}
