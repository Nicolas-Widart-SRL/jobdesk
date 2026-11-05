use std::path::Path;
use std::process::Command;

/// Names a data folder in place of the usual one, so a copy that is tried out never touches the user's own.
pub const DATA_DIR_VARIABLE: &str = "JOBDESK_DATA_DIR";

/// Gives the copy `command` starts the folder `moved`, the data folder `DATA_DIR_VARIABLE` chose. With none, the
/// variable is removed, so a value this copy ignored never reaches the next one.
pub fn pass_on_data_folder(command: &mut Command, moved: Option<&Path>) {
    match moved {
        Some(folder) => command.env(DATA_DIR_VARIABLE, folder),
        None => command.env_remove(DATA_DIR_VARIABLE),
    };
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    #[test]
    fn a_moved_folder_is_passed_on_and_no_folder_removes_the_variable() {
        let mut command = Command::new("jobdesk");
        pass_on_data_folder(&mut command, Some(Path::new("/scratch/jobdesk-data")));
        assert_eq!(command.get_envs().collect::<Vec<_>>(), [(OsStr::new(DATA_DIR_VARIABLE), Some(OsStr::new("/scratch/jobdesk-data")))]);
        let mut command = Command::new("jobdesk");
        pass_on_data_folder(&mut command, None);
        assert_eq!(command.get_envs().collect::<Vec<_>>(), [(OsStr::new(DATA_DIR_VARIABLE), None)]);
    }
}
