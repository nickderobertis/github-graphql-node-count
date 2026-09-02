//! `scripts/session-setup.sh` and `scripts/setup-llmlint.sh`, the two hooks that
//! run before anybody asks for them.
//!
//! They provision a session's toolchain from a `SessionStart` hook, so their one
//! non-negotiable contract is that they never abort the thing that invoked them:
//! whatever goes wrong — no `uv`, a failed install, a `doctor` that complains —
//! they log it and exit 0. That is exactly the behaviour a green run hides, so it
//! is driven here rather than assumed.
//!
//! They are run in a throwaway `HOME` with a stripped `PATH`, so nothing they do
//! reaches the developer's real toolchain, and the environment they persist can
//! be read back from the file the harness would source.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A throwaway home and script directory, removed when the test ends.
struct Session {
    root: PathBuf,
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Session {
    /// Copy both provisioners into a scratch tree with its own `HOME`.
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "session-{label}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(root.join("scripts")).expect("create the scratch tree");
        std::fs::create_dir_all(root.join("home")).expect("create the scratch home");
        for script in ["session-setup.sh", "setup-llmlint.sh"] {
            let source = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../scripts")
                .join(script)
                .canonicalize()
                .expect("the script under test exists");
            let destination = root.join("scripts").join(script);
            std::fs::copy(&source, &destination).expect("copy the script");
            make_executable(&destination);
        }
        Self { root }
    }

    /// The file a Claude Code session sources after the hook returns.
    fn env_file(&self) -> PathBuf {
        self.root.join("session-env.sh")
    }

    /// Run `script` with a scratch `HOME`, an empty `PATH` unless `path` says
    /// otherwise, and the given extra environment.
    fn run(&self, script: &str, path: &str, env: &[(&str, &str)]) -> Output {
        let mut command = Command::new(bash());
        command
            .arg(format!("scripts/{script}"))
            .current_dir(&self.root)
            .env("HOME", self.root.join("home"))
            .env("PATH", path)
            .env_remove("CI")
            .env_remove("CLAUDE_ENV_FILE");
        for (key, value) in env {
            command.env(key, value);
        }
        command.output().expect("run the provisioner")
    }
}

/// The shell to run the scripts with, by absolute path, so a test can empty
/// `PATH` without also hiding the interpreter.
fn bash() -> PathBuf {
    ["/bin/bash", "/usr/bin/bash"]
        .into_iter()
        .map(PathBuf::from)
        .find(|candidate| candidate.is_file())
        .expect("a bash interpreter")
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)
        .expect("stat the script")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).expect("make the script executable");
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("utf-8")
}

#[test]
fn a_session_provisioner_never_aborts_the_session_that_started_it() {
    // Nothing on PATH at all: no `uv`, no `just`, no `command` to install with.
    // A startup hook must still return successfully.
    let session = Session::new("hostile");

    for script in ["session-setup.sh", "setup-llmlint.sh"] {
        let output = session.run(script, "", &[]);
        assert!(
            output.status.success(),
            "{script} must exit 0 even with nothing available; stderr: {}",
            stderr_of(&output),
        );
    }
}

#[test]
fn a_missing_installer_is_reported_with_where_to_get_it() {
    let session = Session::new("no-uv");

    let output = session.run("session-setup.sh", "", &[]);
    let stderr = stderr_of(&output);
    assert!(stderr.contains("uv not found"), "{stderr}");
    assert!(
        stderr.contains("https://docs.astral.sh/uv/"),
        "it says where to get it: {stderr}"
    );
}

#[test]
fn a_failing_install_is_logged_and_the_hook_still_returns() {
    // A `uv` that fails the way a flaky network does. The hook must log it and
    // carry on rather than taking the session down with it.
    let session = Session::new("uv-fails");
    let bin = session.root.join("fake-bin");
    std::fs::create_dir_all(&bin).expect("create the stand-in bin directory");
    let uv = bin.join("uv");
    std::fs::write(
        &uv,
        "#!/usr/bin/env bash\necho 'network unreachable' >&2\nexit 1\n",
    )
    .expect("write the failing uv");
    make_executable(&uv);

    let output = session.run("setup-llmlint.sh", &bin.display().to_string(), &[]);
    assert!(
        output.status.success(),
        "a failed install must not abort the session"
    );
    let stderr = stderr_of(&output);
    assert!(stderr.contains("install failed (continuing)"), "{stderr}");
}

#[test]
fn a_session_gets_the_freshly_installed_binaries_on_its_path() {
    // The reason the hook exists: what it installs must resolve in every later
    // command, which it arranges by appending to the file the harness sources.
    let session = Session::new("env");
    let bin = session.root.join("fake-bin");
    std::fs::create_dir_all(&bin).expect("create the stand-in bin directory");
    let uv = bin.join("uv");
    std::fs::write(&uv, "#!/usr/bin/env bash\nexit 0\n").expect("write the no-op uv");
    make_executable(&uv);

    let env_file = session.env_file();
    let output = session.run(
        "setup-llmlint.sh",
        &bin.display().to_string(),
        &[("CLAUDE_ENV_FILE", &env_file.display().to_string())],
    );
    assert!(output.status.success(), "{}", stderr_of(&output));

    let persisted = std::fs::read_to_string(&env_file).expect("the session env file");
    assert!(persisted.contains("export PATH="), "{persisted}");
    assert!(
        persisted.contains(&format!(
            "{}/.local/bin",
            session.root.join("home").display()
        )),
        "the install directory is what gets prepended: {persisted}",
    );
}

#[test]
fn ci_provisions_itself_so_the_hook_stands_aside() {
    // CI installs its own toolchain in the workflow; the hook racing it would be
    // a second, slower answer to a question already settled.
    let session = Session::new("ci");

    let output = session.run("session-setup.sh", "", &[("CI", "true")]);
    assert!(output.status.success());
    let stderr = stderr_of(&output);
    assert!(stderr.contains("CI detected; skipping"), "{stderr}");
    assert!(
        !stderr.contains("installing"),
        "it installs nothing in CI: {stderr}"
    );
}
