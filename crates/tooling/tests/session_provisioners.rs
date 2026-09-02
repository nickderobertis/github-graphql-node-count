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

/// Write an executable stand-in for `name` under `bin`, with `body` as its script.
///
/// An absolute shebang, because the tests below run the provisioners with a `PATH`
/// that deliberately omits the directories the real `just`, `uv` and `llmlint`
/// live in — a stand-in reached through `/usr/bin/env` would not start.
fn stand_in(bin: &Path, name: &str, body: &str) {
    std::fs::create_dir_all(bin).expect("create the stand-in bin directory");
    let path = bin.join(name);
    std::fs::write(&path, format!("#!{}\n{body}\n", bash().display())).expect("write the stand-in");
    make_executable(&path);
}

/// A `PATH` carrying the stand-ins and the system utilities the scripts call
/// (`dirname`, `command`, ...), and nothing else — in particular not the
/// directories the real `just`, `uv` and `llmlint` are installed in, so a test
/// asking for one of them absent really gets it.
fn path_with(bin: &Path) -> String {
    format!("{}:/usr/bin:/bin", bin.display())
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

    let output = session.run("setup-llmlint.sh", &path_with(&bin), &[]);
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
        &path_with(&bin),
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

#[test]
fn a_session_that_already_has_just_installs_nothing() {
    // The common case: the image shipped `just`, so the hook reports it and moves
    // on rather than reinstalling on every session start.
    let session = Session::new("just-present");
    let bin = session.root.join("fake-bin");
    stand_in(&bin, "just", "echo 'just 1.51.0'");
    stand_in(&bin, "uv", "echo 'uv must not be called' >&2; exit 17");

    let output = session.run("session-setup.sh", &path_with(&bin), &[]);
    assert!(output.status.success(), "{}", stderr_of(&output));
    let stderr = stderr_of(&output);
    assert!(stderr.contains("just present (just 1.51.0)"), "{stderr}");
    assert!(
        !stderr.contains("installing rust-just"),
        "it reinstalls nothing: {stderr}"
    );
}

#[test]
fn a_session_without_just_installs_it_from_pypi() {
    // The reason the hook exists: a cloud image often ships the language runtime
    // but not `just`, and there is no version manager here to read a pin from.
    let session = Session::new("just-absent");
    let bin = session.root.join("fake-bin");
    stand_in(&bin, "uv", "echo \"uv $*\" >&2; exit 0");

    let output = session.run("session-setup.sh", &path_with(&bin), &[]);
    assert!(output.status.success(), "{}", stderr_of(&output));
    let stderr = stderr_of(&output);
    assert!(stderr.contains("installing rust-just"), "{stderr}");
    assert!(
        stderr.contains("uv tool install --upgrade rust-just>="),
        "it installs the floor: {stderr}"
    );
}

#[test]
fn a_failed_just_install_is_logged_and_the_session_still_starts() {
    let session = Session::new("just-install-fails");
    let bin = session.root.join("fake-bin");
    stand_in(&bin, "uv", "echo 'network unreachable' >&2; exit 1");

    let output = session.run("session-setup.sh", &path_with(&bin), &[]);
    assert!(
        output.status.success(),
        "a failed install must not abort the session"
    );
    assert!(
        stderr_of(&output).contains("rust-just install failed (continuing)"),
        "{}",
        stderr_of(&output)
    );
}

#[test]
fn the_provisioner_hands_off_through_the_documented_command_surface() {
    // With `just` resolvable, the handoff goes through `just setup-llmlint` rather
    // than reaching past it into the script the recipe wraps.
    let session = Session::new("handoff");
    let bin = session.root.join("fake-bin");
    stand_in(&bin, "just", "echo \"just $*\" >&2");
    stand_in(&bin, "uv", "exit 0");

    let output = session.run("session-setup.sh", &path_with(&bin), &[]);
    assert!(output.status.success(), "{}", stderr_of(&output));
    let stderr = stderr_of(&output);
    assert!(stderr.contains("running just setup-llmlint"), "{stderr}");
    assert!(
        stderr.contains("just setup-llmlint"),
        "the recipe is what runs: {stderr}"
    );
}

#[test]
fn a_failing_handoff_is_logged_and_the_session_still_starts() {
    let session = Session::new("handoff-fails");
    let bin = session.root.join("fake-bin");
    stand_in(&bin, "just", "exit 1");
    stand_in(&bin, "uv", "exit 0");

    let output = session.run("session-setup.sh", &path_with(&bin), &[]);
    assert!(
        output.status.success(),
        "a failed handoff must not abort the session"
    );
    assert!(
        stderr_of(&output).contains("just setup-llmlint reported an issue (continuing)"),
        "{}",
        stderr_of(&output),
    );
}

#[test]
fn the_provisioner_falls_back_to_the_script_when_just_is_unavailable() {
    // The case the command surface cannot serve: the hook exists precisely
    // because a fresh session may have no `just`, and a failed provision must not
    // turn into a skipped llmlint setup.
    let session = Session::new("no-just");
    let bin = session.root.join("fake-bin");
    stand_in(&bin, "uv", "exit 0");

    let output = session.run("session-setup.sh", &path_with(&bin), &[]);
    assert!(output.status.success(), "{}", stderr_of(&output));
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("just unavailable; running setup-llmlint.sh directly"),
        "{stderr}"
    );
    // And it really did run: the installer's own log line is there.
    assert!(stderr.contains("setup-llmlint:"), "{stderr}");
}

#[test]
fn a_ready_llmlint_is_reported_with_its_version_and_doctor_verdict() {
    let session = Session::new("llmlint-ready");
    let bin = session.root.join("fake-bin");
    stand_in(&bin, "uv", "exit 0");
    stand_in(
        &bin,
        "llmlint",
        "case \"$1\" in --version) echo 'llmlint 0.4.0' ;; doctor) echo 'oneharness ok' ;; esac",
    );

    let output = session.run("setup-llmlint.sh", &path_with(&bin), &[]);
    assert!(output.status.success(), "{}", stderr_of(&output));
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("ready (llmlint: llmlint 0.4.0)"),
        "{stderr}"
    );
    assert!(!stderr.contains("doctor reported an issue"), "{stderr}");
}

#[test]
fn a_complaining_doctor_is_surfaced_without_failing_the_hook() {
    let session = Session::new("doctor-fails");
    let bin = session.root.join("fake-bin");
    stand_in(&bin, "uv", "exit 0");
    stand_in(
        &bin,
        "llmlint",
        "case \"$1\" in --version) echo 'llmlint 0.4.0' ;; doctor) echo 'no harness' >&2; exit 1 ;; esac",
    );

    let output = session.run("setup-llmlint.sh", &path_with(&bin), &[]);
    assert!(
        output.status.success(),
        "a complaining doctor must not abort the session"
    );
    assert!(
        stderr_of(&output).contains("doctor reported an issue"),
        "{}",
        stderr_of(&output)
    );
}

#[test]
fn an_install_that_produced_no_binary_says_so_rather_than_claiming_success() {
    // `uv` exited 0 but nothing landed on PATH — the outcome that would otherwise
    // read as a successful setup and fail later, in `just lint-llm`.
    let session = Session::new("llmlint-missing");
    let bin = session.root.join("fake-bin");
    stand_in(&bin, "uv", "exit 0");

    let output = session.run("setup-llmlint.sh", &path_with(&bin), &[]);
    assert!(output.status.success(), "the hook still returns");
    let stderr = stderr_of(&output);
    assert!(stderr.contains("llmlint not installed"), "{stderr}");
    assert!(!stderr.contains("ready ("), "{stderr}");
}
