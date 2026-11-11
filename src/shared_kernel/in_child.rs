//! Runs a test's body in a child process of the test binary, for a test that changes something the whole process
//! shares, such as the umask, which the tests running beside it on other threads would see.

use std::process::Command;

/// The variable that tells the child which test it runs.
const CHILD: &str = "JOBDESK_TEST_IN_CHILD";

/// Runs `body` in a child process that runs only the test `name` of `module`, as `module_path!()` gives it, and fails
/// when the child fails or runs no test. In that child, it runs `body` itself.
pub fn in_child(module: &str, name: &str, body: impl FnOnce()) {
    // The test harness names a test without its crate.
    let test = format!("{}::{name}", module.split_once("::").map_or(module, |(_, rest)| rest));
    if std::env::var(CHILD).is_ok_and(|value| value == test) {
        body();
        return;
    }
    let output = Command::new(std::env::current_exe().expect("the test binary"))
        .args([test.as_str(), "--exact", "--test-threads=1"])
        .env(CHILD, &test)
        .output()
        .expect("the child test runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success() && stdout.contains("1 passed"), "{test} in its own process:\n{stdout}\n{}", String::from_utf8_lossy(&output.stderr));
}
