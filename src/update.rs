//! Looks on GitHub for a newer release. Nothing is asked until the person
//! presses "check for updates", and nothing is downloaded by the program:
//! the button opens the release page in the browser.

use std::process::Command;

/// Where releases are published.
pub const RELEASES_PAGE: &str = "https://github.com/foxhands/syslens/releases/latest";
const LATEST_API: &str = "https://api.github.com/repos/foxhands/syslens/releases/latest";

pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum State {
    Idle,
    Checking,
    /// This is the newest version.
    Latest,
    /// A newer version, like "0.5.0".
    Newer(String),
    Failed,
}

/// "v0.4.0" or "0.4.0-rc.1+build" → ([0, 4, 0], whether it is a pre-release).
/// The numbers stop at the first `-` (pre-release) or `+` (build mark).
fn parts(v: &str) -> (Vec<u64>, bool) {
    let v = v.trim().trim_start_matches('v');
    let cut = v.find(['-', '+']).unwrap_or(v.len());
    let numbers = v[..cut]
        .split('.')
        .map(|part| part.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0))
        .collect();
    (numbers, v[cut..].starts_with('-'))
}

/// Whether version `a` is newer than version `b`. A pre-release is older
/// than the release with the same numbers; two pre-releases of one version
/// count as equal.
pub fn newer(a: &str, b: &str) -> bool {
    let ((mut a, a_pre), (mut b, b_pre)) = (parts(a), parts(b));
    let len = a.len().max(b.len());
    a.resize(len, 0);
    b.resize(len, 0);
    (a, !a_pre) > (b, !b_pre)
}

/// The answer of the releases service → what to tell the person.
fn judge(answer: &str) -> State {
    let tag = serde_json::from_str::<serde_json::Value>(answer)
        .ok()
        .and_then(|v| v.get("tag_name").and_then(|t| t.as_str()).map(str::to_string));
    match tag {
        Some(tag) if newer(&tag, CURRENT) => State::Newer(tag.trim_start_matches('v').to_string()),
        Some(_) => State::Latest,
        None => State::Failed,
    }
}

/// Asks GitHub for the latest release. Uses curl, which Windows 10 and later,
/// macOS and nearly every Linux have; sends nothing but the request itself.
pub fn check() -> State {
    let mut cmd = Command::new("curl");
    cmd.args(["-fsSL", "-m", "15", "-A", "SysLens", "-H", "Accept: application/vnd.github+json", LATEST_API]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    match cmd.output() {
        Ok(out) if out.status.success() => judge(&String::from_utf8_lossy(&out.stdout)),
        _ => State::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(newer("v0.5.0", "0.4.0"));
        assert!(newer("0.4.1", "0.4"));
        assert!(newer("1.0", "0.99.9"));
        assert!(newer("0.10.0", "0.9.0"));
        assert!(!newer("0.4.0", "0.4.0"));
        assert!(!newer("v0.3.9", "0.4.0"));
        assert!(!newer("garbage", "0.4.0"));
    }

    #[test]
    fn pre_releases() {
        // the release is newer than its own release candidate
        assert!(newer("0.5.0", "0.5.0-rc.1"));
        assert!(!newer("v0.5.0-rc.1", "0.5.0"));
        assert!(!newer("0.5.0-rc.2", "0.5.0-rc.1"));
        assert!(!newer("0.5.0-beta", "0.5.0-beta"));
        assert!(!newer("0.5-rc.1", "0.5.0"));
        // but a candidate of the next version is newer than this one
        assert!(newer("v0.5.0-rc.1", "0.4.0"));
        assert!(newer("0.5.0-rc.1", "0.4.9"));
        assert!(!newer("0.4.0", "0.5.0-rc.1"));
        assert!(newer("0.5.1", "0.5.0-rc.1"));
        // a build mark is neither a number nor a pre-release
        assert!(!newer("0.5.0+build.7", "0.5.0"));
        assert!(!newer("0.5.0", "0.5.0+build.7"));
        assert!(newer("0.5.0+build.7", "0.5.0-rc.1+build.9"));
        assert!(newer("0.5.0+a-b", "0.5.0-rc.1"));
    }

    #[test]
    fn answers() {
        assert_eq!(judge(r#"{"tag_name":"v99.0.0"}"#), State::Newer("99.0.0".into()));
        assert_eq!(judge(&format!(r#"{{"tag_name":"v{CURRENT}"}}"#)), State::Latest);
        assert_eq!(judge("<html>rate limit</html>"), State::Failed);
        assert_eq!(judge(r#"{"message":"Not Found"}"#), State::Failed);
    }
}
