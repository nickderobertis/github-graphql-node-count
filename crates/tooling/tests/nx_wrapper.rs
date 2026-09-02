//! `scripts/nx.sh`, the one entry point to the workspace's orchestrator.
//!
//! Every gate recipe runs through it, so its output *is* the gate's output: a
//! green run owes a line rather than Nx's whole task log, and a failing one owes
//! that log. It is driven with a stand-in `nx` on the path it looks for, so the
//! wrapper's own behaviour is what is under test rather than Nx's.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A throwaway workspace holding the wrapper and a stand-in orchestrator.
struct Workspace {
    root: PathBuf,
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Workspace {
    /// Lay down the wrapper, and an `nx` shim that exits with `exit_code` after
    /// printing `output` — where a real Nx would print its task log.
    fn with_orchestrator(label: &str, exit_code: i32, output: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "nx-wrapper-{label}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(root.join("scripts")).expect("create the workspace");
        std::fs::create_dir_all(root.join("node_modules/.bin")).expect("create node_modules");
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/nx.sh")
            .canonicalize()
            .expect("the script under test exists");
        std::fs::copy(&source, root.join("scripts/nx.sh")).expect("copy the script");

        let shim = root.join("node_modules/.bin/nx");
        std::fs::write(
            &shim,
            format!(
                "#!/usr/bin/env bash\nprintf '%s\\n' {output:?} \"args: $*\"\nexit {exit_code}\n"
            ),
        )
        .expect("write the orchestrator shim");
        make_executable(&shim);
        Self { root }
    }

    fn run(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        // An absolute interpreter, so a test can empty `PATH` to hide a tool from
        // the script without also hiding the shell that runs it.
        let mut command = Command::new(bash());
        command
            .arg("scripts/nx.sh")
            .args(args)
            .current_dir(&self.root);
        for (key, value) in env {
            command.env(key, value);
        }
        command.output().expect("run nx.sh")
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.root.join(".logs/nx.log")).expect("the preserved log")
    }
}

/// The shell to run the script under test with, by absolute path.
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
        .expect("stat the shim")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).expect("make the shim executable");
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("utf-8")
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("utf-8")
}

#[test]
fn a_successful_run_says_one_line_and_preserves_the_log() {
    let workspace = Workspace::with_orchestrator("ok", 0, "Successfully ran target lint");

    let output = workspace.run(&["run-many", "-t", "lint"], &[]);
    assert!(output.status.success(), "{}", stderr_of(&output));

    let stdout = stdout_of(&output);
    assert_eq!(
        stdout.lines().count(),
        1,
        "a green run owes one line, got: {stdout}"
    );
    assert!(stdout.contains("requested targets succeeded"), "{stdout}");
    assert!(
        !stdout.contains("Successfully ran target lint"),
        "the task log stays in the file"
    );

    let log = workspace.log();
    assert!(log.contains("Successfully ran target lint"), "{log}");
    assert!(
        log.contains("args: run-many -t lint"),
        "the arguments reach the orchestrator: {log}"
    );
}

#[test]
fn a_failing_run_shows_the_log_and_names_the_next_step() {
    let workspace = Workspace::with_orchestrator("fail", 1, "lint failed in crate-under-test");

    let output = workspace.run(&["run-many", "-t", "lint"], &[]);
    assert!(
        !output.status.success(),
        "a failing orchestrator must fail the wrapper"
    );

    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("lint failed in crate-under-test"),
        "the log is shown: {stderr}"
    );
    assert!(stderr.contains("rerun the same 'just' recipe"), "{stderr}");
}

#[test]
fn show_output_mode_hands_the_orchestrators_stdout_through_untouched() {
    // The callers that parse Nx's stdout must get exactly it — not even a summary
    // line may be added.
    let workspace = Workspace::with_orchestrator("stream", 0, "[\"a\",\"b\"]");

    let output = workspace.run(&["show", "projects"], &[("NODE_COUNT_NX_SHOW_OUTPUT", "1")]);
    assert!(output.status.success(), "{}", stderr_of(&output));
    let stdout = stdout_of(&output);
    assert!(stdout.starts_with("[\"a\",\"b\"]"), "{stdout}");
    assert!(!stdout.contains("requested targets succeeded"), "{stdout}");
}

#[test]
fn a_clean_clone_heals_itself_by_installing_the_pinned_orchestrator() {
    // The clean-clone path: `node_modules` does not exist yet, so the wrapper runs
    // a locked install before doing anything. That is what lets `just check` work
    // from a fresh checkout with no separate "install the orchestrator" step.
    let workspace = Workspace::with_orchestrator("heal", 0, "Successfully ran target lint");
    let orchestrator = std::fs::read(workspace.root.join("node_modules/.bin/nx"))
        .expect("read the orchestrator shim before hiding it");
    std::fs::remove_dir_all(workspace.root.join("node_modules")).expect("remove node_modules");

    // An `npm` that does what `npm ci` does here: put the orchestrator in place.
    let bin = workspace.root.join("fake-bin");
    std::fs::create_dir_all(&bin).expect("create the stand-in bin directory");
    let npm = bin.join("npm");
    std::fs::write(
        &npm,
        "#!/usr/bin/env bash\nset -eu\nmkdir -p node_modules/.bin\ncp installed-nx node_modules/.bin/nx\nchmod +x node_modules/.bin/nx\necho \"npm $*\" >&2\n",
    )
    .expect("write the npm stand-in");
    make_executable(&npm);
    std::fs::write(workspace.root.join("installed-nx"), &orchestrator).expect("stage the shim");

    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let output = workspace.run(&["run-many", "-t", "lint"], &[("PATH", &path)]);
    assert!(output.status.success(), "{}", stderr_of(&output));
    assert!(
        workspace.root.join("node_modules/.bin/nx").is_file(),
        "the wrapper installed the orchestrator it needed",
    );
    assert!(
        stdout_of(&output).contains("requested targets succeeded"),
        "{}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("npm ci"),
        "it heals with a locked install: {}",
        stderr_of(&output)
    );
}

#[test]
fn a_failing_install_says_what_to_check_rather_than_command_not_found() {
    let workspace = Workspace::with_orchestrator("heal-fails", 0, "unused");
    std::fs::remove_dir_all(workspace.root.join("node_modules")).expect("remove node_modules");
    let bin = workspace.root.join("fake-bin");
    std::fs::create_dir_all(&bin).expect("create the stand-in bin directory");
    let npm = bin.join("npm");
    std::fs::write(
        &npm,
        "#!/usr/bin/env bash\necho 'ENOTFOUND registry.npmjs.org' >&2\nexit 1\n",
    )
    .expect("write the failing npm stand-in");
    make_executable(&npm);

    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let output = workspace.run(&["run-many", "-t", "lint"], &[("PATH", &path)]);
    assert!(!output.status.success());
    let stderr = stderr_of(&output);
    assert!(stderr.contains("'npm ci' failed"), "{stderr}");
    assert!(stderr.contains("ACTION:"), "{stderr}");
}

#[test]
fn a_workspace_without_the_orchestrator_and_without_npm_says_what_to_install() {
    // The clean-clone path, with the heal impossible: it must name the missing
    // tool rather than failing with `nx: command not found`.
    let workspace = Workspace::with_orchestrator("no-npm", 0, "unused");
    std::fs::remove_dir_all(workspace.root.join("node_modules")).expect("remove node_modules");

    // A `PATH` with nothing on it, so `npm` cannot be found. The interpreter is
    // reached by absolute path, so the shell itself is still there.
    let output = workspace.run(&["run-many", "-t", "lint"], &[("PATH", "")]);
    assert!(!output.status.success());
    let stderr = stderr_of(&output);
    assert!(stderr.contains("npm not found"), "{stderr}");
    assert!(stderr.contains("ACTION:"), "{stderr}");
}
