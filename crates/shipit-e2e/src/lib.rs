//! End-to-end harness for the shipit CLI, ported from
//! `tests/test_e2e.py` (pytest). The case table lives in [`cases`];
//! `tests/e2e.rs` materializes one `#[test]` per (case, build-mode) pair
//! named `<suite>__<mode>__<example>` so nextest filter expressions can
//! slice per suite (`test(/^php__/)`) and per mode (`test(/__wasmer__/)`).
//!
//! The pytest harness's asyncio usage is incidental: this port uses
//! `std::process` plus reader threads. Flaky handling is nextest's job
//! (retries live in `.config/nextest.toml`); there is no rerun logic here.

pub mod cases;

pub use cases::{Case, HttpRequest, RunCommand, Suite, CASES};

use std::fs;
use std::io::{BufReader, Read};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{bail, ensure, Context, Result};
use regex::Regex;

pub const BUILD_PHRASE: &str = "Build complete ✅";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BuildMode {
    Local,
    Wasmer,
    WasmerAndDocker,
}

impl BuildMode {
    pub const ALL: [BuildMode; 3] = [
        BuildMode::Local,
        BuildMode::Wasmer,
        BuildMode::WasmerAndDocker,
    ];

    /// Slug used in generated test names (`__wasmer_and_docker__` etc.).
    pub fn slug(self) -> &'static str {
        match self {
            BuildMode::Local => "local",
            BuildMode::Wasmer => "wasmer",
            BuildMode::WasmerAndDocker => "wasmer_and_docker",
        }
    }
}

/// Entry point used by every generated test in `tests/e2e.rs`.
/// Port of `test_end_to_end`.
pub fn run_case(test_id: &str, build_mode: BuildMode) -> Result<()> {
    let case = CASES
        .iter()
        .find(|case| case.test_id == test_id)
        .with_context(|| format!("unknown e2e case id: {test_id:?}"))?;
    ensure!(
        case.structural_modes().contains(&build_mode),
        "case {test_id:?} is not structurally enabled for {build_mode:?}; \
         the generated test list is out of sync with the case table"
    );

    let repo_root = workspace_root();
    let project_path = materialize_case(case, &repo_root)?;

    let port = if case.use_random_port {
        get_free_port()
    } else {
        8080 // This is the default port if not specified.
    };

    // The spawned commands inherit the full process environment; these
    // pairs are layered on top (mirrors `os.environ.copy()` + updates).
    let mut envs: Vec<(String, String)> = Vec::new();
    for (key, value) in case.env {
        envs.push((key.to_string(), value.to_string()));
    }
    for (key, value) in case.extra_env {
        envs.push((key.to_string(), value.to_string()));
    }

    let mut created_db_name: Option<String> = None;
    let mut wp_content_volume_dir: Option<PathBuf> = None;
    let result = run_case_inner(
        case,
        build_mode,
        &repo_root,
        &project_path,
        port,
        &mut envs,
        &mut created_db_name,
        &mut wp_content_volume_dir,
    );

    // Teardown, mirroring the pytest `finally` block: always attempt the
    // DB drop and volume cleanup; a failed drop only fails the test when
    // the run itself succeeded.
    let mut drop_error: Option<String> = None;
    if let Some(name) = created_db_name {
        let drop_result = drop_mysql_database(&envs, &repo_root, &name);
        match drop_result {
            Ok(completed) if completed.returncode == Some(0) => {}
            Ok(completed) => {
                let message = format!(
                    "Failed to drop temporary MySQL database.\n\
                     database={name}\n\n\
                     --- Captured output start ---\n{}\n\
                     --- Captured output end ---",
                    completed.output()
                );
                if result.is_ok() {
                    drop_error = Some(message);
                } else {
                    println!("{message}");
                }
            }
            Err(err) => {
                let message =
                    format!("Failed to run mysql to drop database {name}: {err:#}");
                if result.is_ok() {
                    drop_error = Some(message);
                } else {
                    println!("{message}");
                }
            }
        }
    }
    if let Some(dir) = wp_content_volume_dir {
        let _ = fs::remove_dir_all(dir);
    }

    result?;
    if let Some(message) = drop_error {
        bail!(message);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_case_inner(
    case: &Case,
    build_mode: BuildMode,
    repo_root: &Path,
    project_path: &Path,
    port: u16,
    envs: &mut Vec<(String, String)>,
    created_db_name: &mut Option<String>,
    wp_content_volume_dir: &mut Option<PathBuf>,
) -> Result<()> {
    if case.create_db {
        let name = create_mysql_database(envs, repo_root)?;
        envs.push(("DB_NAME".to_string(), name.clone()));
        *created_db_name = Some(name);
    }
    let mut volume_specs: Vec<String> = Vec::new();
    if case.create_wp_content_volume {
        let host_dir = create_wp_content_volume(project_path)?;
        *wp_content_volume_dir = Some(host_dir);
        volume_specs.push("wp-content:/app/wp-content".to_string());
    }

    if case.download.is_some() || !case.commands.is_empty() {
        // Build first, then serve via `shipit run --start`, then execute
        // each RunCommand via `shipit run --command=...`.
        let build_cmd = shipit_build_command(repo_root, project_path, build_mode, port)?;
        let build_result = run_completed_command(
            &build_cmd,
            repo_root,
            envs,
            Duration::from_secs(180),
        )?;
        let build_output = build_result.output();
        if build_result.returncode != Some(0) || !build_output.contains(BUILD_PHRASE) {
            bail!(
                "End-to-end build command failed.\n\
                 command={}\n\
                 returncode={:?}\n\n\
                 --- Captured output start ---\n{build_output}\n\
                 --- Captured output end ---",
                shell_join(&build_cmd),
                build_result.returncode,
            );
        }

        let run_cmd = shipit_run_command(
            repo_root,
            project_path,
            build_mode,
            case.run_after_deploy,
            true,
            None,
            &volume_specs,
        )?;
        run_server_and_check(case, &run_cmd, repo_root, envs, project_path, port, false)?;

        for command in case.commands {
            let cmd = shipit_run_command(
                repo_root,
                project_path,
                build_mode,
                false,
                false,
                Some(command.command),
                &volume_specs,
            )?;
            let result =
                run_completed_command(&cmd, repo_root, envs, Duration::from_secs(180))?;
            print_run_command_output(command, &cmd, &result);
            assert_run_command(command, &cmd, &result)?;
        }
        return Ok(());
    }

    let cmd = shipit_auto_command(
        repo_root,
        project_path,
        build_mode,
        port,
        case.run_after_deploy,
    )?;
    run_server_and_check(case, &cmd, repo_root, envs, project_path, port, true)
}

// ---------------------------------------------------------------------------
// Serve harness (port of `_run_server_and_check`)
// ---------------------------------------------------------------------------

fn run_server_and_check(
    case: &Case,
    cmd: &[String],
    cwd: &Path,
    envs: &[(String, String)],
    project_path: &Path,
    port: u16,
    expect_build: bool,
) -> Result<()> {
    let serve_re = Arc::new(
        Regex::new(case.serve_pattern)
            .with_context(|| format!("invalid serve_pattern for {}", case.test_id))?,
    );

    let mut child = spawn_group(cmd, cwd, envs)
        .with_context(|| format!("failed to spawn: {}", shell_join(cmd)))?;
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");
    // Guard kills the whole process group (SIGINT, then SIGKILL after a 2s
    // grace) on scope exit — including panics and early `?` returns.
    let mut guard = ProcessGroupGuard::new(child);

    let output = Arc::new(Mutex::new(String::new()));
    let found_build = Arc::new(AtomicBool::new(!expect_build));
    let found_serve = Arc::new(AtomicBool::new(false));
    let matched_serve_output = Arc::new(AtomicBool::new(false));

    let reader_out = spawn_line_reader(
        "stdout",
        stdout,
        Arc::clone(&output),
        Arc::clone(&found_build),
        Arc::clone(&found_serve),
        Arc::clone(&matched_serve_output),
        Arc::clone(&serve_re),
    );
    let reader_err = spawn_line_reader(
        "stderr",
        stderr,
        Arc::clone(&output),
        Arc::clone(&found_build),
        Arc::clone(&found_serve),
        Arc::clone(&matched_serve_output),
        Arc::clone(&serve_re),
    );

    // Wait until both events are seen, the process exits, or timeout
    // elapses (180s, as in pytest).
    let mut verified_http_ready = false;
    let deadline = Instant::now() + Duration::from_secs(180);
    while Instant::now() < deadline {
        if found_build.load(Ordering::SeqCst) && found_serve.load(Ordering::SeqCst) {
            break;
        }
        if found_build.load(Ordering::SeqCst)
            && !found_serve.load(Ordering::SeqCst)
            && !case.http.is_empty()
        {
            // Some servers print their banner before the harness attaches
            // or in a format we don't match; fall back to a quick HTTP
            // readiness probe once the build finished.
            let readiness = case.http[0].readiness();
            if wait_for_http_response("localhost", port, &readiness, 0.5) {
                verified_http_ready = true;
                found_serve.store(true, Ordering::SeqCst);
                break;
            }
        }
        if guard.poll_exit().is_some() {
            // Process ended early; stop waiting.
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }

    // Exercise assertions while the server is still up; report failures
    // only after teardown (pytest.fail inside `try` still runs `finally`).
    let mut check_result: Result<()> = Ok(());
    if found_serve.load(Ordering::SeqCst) {
        check_result = serve_time_checks(case, port, project_path, &output);
    }

    let exit_status = guard.shutdown();
    join_reader(reader_out, Duration::from_secs(5));
    join_reader(reader_err, Duration::from_secs(5));

    check_result?;

    let full_output = output.lock().unwrap().clone();
    if !(found_build.load(Ordering::SeqCst) && found_serve.load(Ordering::SeqCst)) {
        bail!(
            "End-to-end run did not reach expected state.\n\
             command={}\n\
             returncode={}\n\
             Saw build={} serve={}\n\n\
             --- Captured output start ---\n{full_output}\n\
             --- Captured output end ---",
            shell_join(cmd),
            exit_status
                .map(|status| status.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            found_build.load(Ordering::SeqCst),
            found_serve.load(Ordering::SeqCst),
        );
    }
    if expect_build {
        ensure!(
            full_output.contains(BUILD_PHRASE),
            "build phrase missing from output despite build event"
        );
    }
    ensure!(
        matched_serve_output.load(Ordering::SeqCst) || verified_http_ready,
        "Serve banner regex not found in output and HTTP readiness did not pass"
    );
    Ok(())
}

fn serve_time_checks(
    case: &Case,
    port: u16,
    project_path: &Path,
    output: &Arc<Mutex<String>>,
) -> Result<()> {
    if case.expected_memory_limit.is_some() || case.expect_no_memory_limit {
        let app_yaml_path = project_path.join(".shipit").join("wasmer").join("app.yaml");
        if !app_yaml_path.is_file() {
            bail!(
                "Expected generated app.yaml for Wasmer run, but it was not \
                 found.\n\nPath: {}\n\n\
                 --- Captured output start ---\n{}\n\
                 --- Captured output end ---",
                app_yaml_path.display(),
                output.lock().unwrap(),
            );
        }
        let app_yaml = fs::read_to_string(&app_yaml_path)?;
        let limit = extract_phpix_memory_limit(&app_yaml);
        if let Some(expected) = case.expected_memory_limit {
            if limit.as_deref() != Some(expected) {
                bail!(
                    "Generated app.yaml has wrong phpix memory limit.\n\n\
                     Expected: {expected}\nActual: {limit:?}\nPath: {}\n\n\
                     --- Captured output start ---\n{}\n\
                     --- Captured output end ---",
                    app_yaml_path.display(),
                    output.lock().unwrap(),
                );
            }
        }
        if case.expect_no_memory_limit && limit.is_some() {
            bail!(
                "Generated app.yaml unexpectedly sets phpix memory limit for \
                 non-WordPress app.\n\n\
                 Actual: {limit:?}\nPath: {}\n\n\
                 --- Captured output start ---\n{}\n\
                 --- Captured output end ---",
                app_yaml_path.display(),
                output.lock().unwrap(),
            );
        }
    }
    for req in case.http {
        if !wait_for_http_response("localhost", port, req, 20.0) {
            bail!(
                "Server did not return expected HTTP content.\n\n\
                 Request path: '{}'\n\
                 Expected status: {:?}\n\
                 Expected location regex: {:?}\n\
                 Expected body regex: {:?}\n\n\
                 --- Captured output start ---\n{}\n\
                 --- Captured output end ---",
                req.path,
                req.expected_status,
                req.location_match,
                req.body_match,
                output.lock().unwrap(),
            );
        }
    }
    Ok(())
}

fn spawn_line_reader<R: Read + Send + 'static>(
    label: &'static str,
    stream: R,
    output: Arc<Mutex<String>>,
    found_build: Arc<AtomicBool>,
    found_serve: Arc<AtomicBool>,
    matched_serve_output: Arc<AtomicBool>,
    serve_re: Arc<Regex>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match read_until_newline(&mut reader, &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            let line = String::from_utf8_lossy(&buf);
            let tagged = format!("[{label}] {line}");
            print!("{tagged}");
            output.lock().unwrap().push_str(&tagged);
            if !found_build.load(Ordering::SeqCst) && line.contains(BUILD_PHRASE) {
                found_build.store(true, Ordering::SeqCst);
            }
            if !found_serve.load(Ordering::SeqCst) && serve_re.is_match(&line) {
                matched_serve_output.store(true, Ordering::SeqCst);
                found_serve.store(true, Ordering::SeqCst);
            }
        }
    })
}

fn read_until_newline<R: std::io::BufRead>(
    reader: &mut R,
    buf: &mut Vec<u8>,
) -> std::io::Result<usize> {
    reader.read_until(b'\n', buf)
}

/// Reader threads normally finish when the pipe closes after the process
/// group dies. If a stray descendant keeps the pipe open, give up joining
/// after `timeout`; the leaked thread dies with the (per-test) process.
fn join_reader(handle: JoinHandle<()>, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while !handle.is_finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    if handle.is_finished() {
        let _ = handle.join();
    }
}

// ---------------------------------------------------------------------------
// Process-group management
// ---------------------------------------------------------------------------

fn spawn_group(cmd: &[String], cwd: &Path, envs: &[(String, String)]) -> Result<Child> {
    use std::os::unix::process::CommandExt;
    ensure!(!cmd.is_empty(), "empty command");
    let mut command = Command::new(&cmd[0]);
    command
        .args(&cmd[1..])
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // New process group so SIGINT/SIGKILL reach every descendant
        // (equivalent of pytest's start_new_session for our purposes).
        .process_group(0);
    for (key, value) in envs {
        command.env(key, value);
    }
    Ok(command.spawn()?)
}

/// Owns a child spawned in its own process group and guarantees the group
/// is torn down (SIGINT, 2s grace, then SIGKILL) even if the test panics.
struct ProcessGroupGuard {
    child: Child,
    pgid: i32,
    reaped: Option<ExitStatus>,
    shut_down: bool,
}

impl ProcessGroupGuard {
    fn new(child: Child) -> Self {
        let pgid = child.id() as i32;
        Self {
            child,
            pgid,
            reaped: None,
            shut_down: false,
        }
    }

    fn poll_exit(&mut self) -> Option<ExitStatus> {
        if self.reaped.is_none() {
            if let Ok(Some(status)) = self.child.try_wait() {
                self.reaped = Some(status);
            }
        }
        self.reaped
    }

    fn shutdown(&mut self) -> Option<ExitStatus> {
        self.shut_down = true;
        if self.reaped.is_some() {
            // The leader already exited and was reaped. Mirror pytest,
            // where os.getpgid() on the reaped pid raises and the group
            // kill is skipped; also avoids signalling a recycled pgid.
            return self.reaped;
        }
        // Graceful shutdown first with Ctrl-C (SIGINT), then kill.
        unsafe {
            libc::killpg(self.pgid, libc::SIGINT);
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Ok(Some(status)) = self.child.try_wait() {
                self.reaped = Some(status);
                return self.reaped;
            }
            if Instant::now() >= deadline {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        unsafe {
            libc::killpg(self.pgid, libc::SIGKILL);
        }
        if let Ok(status) = self.child.wait() {
            self.reaped = Some(status);
        }
        self.reaped
    }
}

impl Drop for ProcessGroupGuard {
    fn drop(&mut self) {
        if !self.shut_down {
            let _ = self.shutdown();
        }
    }
}

// ---------------------------------------------------------------------------
// One-shot commands (port of `_run_completed_command`)
// ---------------------------------------------------------------------------

pub struct CompletedCommand {
    pub returncode: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl CompletedCommand {
    pub fn output(&self) -> String {
        format!("[stdout]\n{}\n[stderr]\n{}", self.stdout, self.stderr)
    }
}

fn run_completed_command(
    cmd: &[String],
    cwd: &Path,
    envs: &[(String, String)],
    timeout: Duration,
) -> Result<CompletedCommand> {
    let mut child = spawn_group(cmd, cwd, envs)
        .with_context(|| format!("failed to spawn: {}", shell_join(cmd)))?;
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");
    let mut guard = ProcessGroupGuard::new(child);

    let out_buf = spawn_sink(stdout);
    let err_buf = spawn_sink(stderr);

    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = guard.poll_exit() {
            break Some(status);
        }
        if Instant::now() >= deadline {
            break None;
        }
        thread::sleep(Duration::from_millis(50));
    };
    // On timeout: SIGINT the group, 2s grace, SIGKILL — then collect
    // whatever output was produced (pytest does _stop_process + communicate).
    let status = match status {
        Some(status) => Some(status),
        None => guard.shutdown(),
    };
    guard.shut_down = true;

    let (out_thread, out_bytes) = out_buf;
    let (err_thread, err_bytes) = err_buf;
    join_reader(out_thread, Duration::from_secs(5));
    join_reader(err_thread, Duration::from_secs(5));

    let stdout = String::from_utf8_lossy(&out_bytes.lock().unwrap()).into_owned();
    let stderr = String::from_utf8_lossy(&err_bytes.lock().unwrap()).into_owned();
    Ok(CompletedCommand {
        returncode: status.and_then(|status| status.code()),
        stdout,
        stderr,
    })
}

type Sink = (JoinHandle<()>, Arc<Mutex<Vec<u8>>>);

fn spawn_sink<R: Read + Send + 'static>(mut stream: R) -> Sink {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&buf);
    let handle = thread::spawn(move || {
        let mut chunk = [0u8; 8192];
        loop {
            match stream.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => sink.lock().unwrap().extend_from_slice(&chunk[..n]),
            }
        }
    });
    (handle, buf)
}

fn print_run_command_output(command: &RunCommand, cmd: &[String], result: &CompletedCommand) {
    println!("[run-command] {}", shell_join(cmd));
    println!(
        "[run-command] expected_returncode={} returncode={:?}",
        command.expected_returncode, result.returncode
    );
    if !result.stdout.is_empty() {
        println!("[run-command stdout]");
        print!("{}", result.stdout);
        if !result.stdout.ends_with('\n') {
            println!();
        }
    }
    if !result.stderr.is_empty() {
        println!("[run-command stderr]");
        print!("{}", result.stderr);
        if !result.stderr.ends_with('\n') {
            println!();
        }
    }
}

fn assert_run_command(
    command: &RunCommand,
    cmd: &[String],
    result: &CompletedCommand,
) -> Result<()> {
    if result.returncode != Some(command.expected_returncode) {
        bail!(
            "Run command exited with unexpected status.\n\
             command={}\n\
             expected_returncode={}\n\
             returncode={:?}\n\n\
             --- Captured output start ---\n{}\n\
             --- Captured output end ---",
            shell_join(cmd),
            command.expected_returncode,
            result.returncode,
            result.output(),
        );
    }
    if let Some(pattern) = command.stdout_match {
        let re = Regex::new(pattern).expect("invalid stdout_match regex");
        if !re.is_match(&result.stdout) {
            bail!(
                "Run command stdout did not match expected regex.\n\
                 command={}\n\
                 stdout_match={pattern:?}\n\n\
                 --- Captured output start ---\n{}\n\
                 --- Captured output end ---",
                shell_join(cmd),
                result.output(),
            );
        }
    }
    if let Some(pattern) = command.stderr_match {
        let re = Regex::new(pattern).expect("invalid stderr_match regex");
        if !re.is_match(&result.stderr) {
            bail!(
                "Run command stderr did not match expected regex.\n\
                 command={}\n\
                 stderr_match={pattern:?}\n\n\
                 --- Captured output start ---\n{}\n\
                 --- Captured output end ---",
                shell_join(cmd),
                result.output(),
            );
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// HTTP polling (port of `_wait_for_http_response`)
// ---------------------------------------------------------------------------

fn wait_for_http_response(
    host: &str,
    port: u16,
    request: &HttpRequest,
    timeout_secs: f64,
) -> bool {
    let url = format!("http://{host}:{port}{}", request.path);
    let deadline = Instant::now() + Duration::from_secs_f64(timeout_secs);
    let request_timeout = timeout_secs.clamp(0.2, 5.0);
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs_f64(request_timeout))
        .redirects(if request.follow_redirects { 5 } else { 0 })
        .build();
    let body_re = request
        .body_match
        .map(|pattern| Regex::new(pattern).expect("invalid body_match regex"));
    let location_re = request
        .location_match
        .map(|pattern| Regex::new(pattern).expect("invalid location_match regex"));

    while Instant::now() < deadline {
        let response = match agent.request(request.method, &url).call() {
            Ok(response) => Some(response),
            // 4xx/5xx responses still carry the status/body we assert on.
            Err(ureq::Error::Status(_, response)) => Some(response),
            Err(_) => None, // Not ready yet; retry shortly.
        };
        if let Some(response) = response {
            let status = response.status();
            let location = response.header("Location").unwrap_or("").to_string();
            let mut body_bytes = Vec::new();
            let _ = response
                .into_reader()
                .take(16 * 1024 * 1024)
                .read_to_end(&mut body_bytes);
            let body = String::from_utf8_lossy(&body_bytes);

            let mut ok = true;
            let mut checked_any = false;
            if let Some(expected) = request.expected_status {
                checked_any = true;
                if status != expected {
                    ok = false;
                }
            }
            if let Some(re) = &location_re {
                checked_any = true;
                if !re.is_match(&location) {
                    ok = false;
                }
            }
            if let Some(re) = &body_re {
                checked_any = true;
                if !re.is_match(&body) {
                    ok = false;
                }
            }
            if ok && checked_any {
                return true;
            }
        }
        thread::sleep(Duration::from_millis(200));
    }
    false
}

// ---------------------------------------------------------------------------
// shipit invocation builders (ports of `_shipit_*_command`)
// ---------------------------------------------------------------------------

/// The shipit invocation under test. `SHIPIT_BIN` (split on whitespace)
/// overrides the default workspace debug binary.
fn shipit_base_command(repo_root: &Path) -> Result<Vec<String>> {
    if let Ok(bin) = std::env::var("SHIPIT_BIN") {
        if !bin.trim().is_empty() {
            return Ok(bin.split_whitespace().map(str::to_string).collect());
        }
    }
    let default = repo_root.join("target").join("debug").join("shipit");
    ensure!(
        default.is_file(),
        "shipit binary not found at {}; build it with \
         `cargo build -p shipit-cli` or point SHIPIT_BIN at a binary",
        default.display()
    );
    Ok(vec![default.to_string_lossy().into_owned()])
}

fn shipit_auto_command(
    repo_root: &Path,
    project_path: &Path,
    build_mode: BuildMode,
    port: u16,
    run_after_deploy: bool,
) -> Result<Vec<String>> {
    let mut cmd = shipit_base_command(repo_root)?;
    cmd.push(project_path.to_string_lossy().into_owned());
    cmd.push("--skip-prepare".to_string());
    cmd.push("--start".to_string());
    cmd.push("--regenerate".to_string());
    if run_after_deploy {
        cmd.push("--after-deploy".to_string());
    }
    append_build_mode_flags(&mut cmd, build_mode);
    cmd.push(format!("--serve-port={port}"));
    Ok(cmd)
}

fn shipit_build_command(
    repo_root: &Path,
    project_path: &Path,
    build_mode: BuildMode,
    port: u16,
) -> Result<Vec<String>> {
    let mut cmd = shipit_base_command(repo_root)?;
    cmd.push(project_path.to_string_lossy().into_owned());
    cmd.push("--skip-prepare".to_string());
    cmd.push("--regenerate".to_string());
    append_build_mode_flags(&mut cmd, build_mode);
    cmd.push(format!("--serve-port={port}"));
    Ok(cmd)
}

fn shipit_run_command(
    repo_root: &Path,
    project_path: &Path,
    build_mode: BuildMode,
    run_after_deploy: bool,
    start: bool,
    command: Option<&str>,
    volume_specs: &[String],
) -> Result<Vec<String>> {
    let mut cmd = shipit_base_command(repo_root)?;
    cmd.push("run".to_string());
    cmd.push(project_path.to_string_lossy().into_owned());
    if run_after_deploy {
        cmd.push("--after-deploy".to_string());
    }
    if start {
        cmd.push("--start".to_string());
    }
    if let Some(command) = command {
        cmd.push(format!("--command={command}"));
    }
    for spec in volume_specs {
        cmd.push("--volume".to_string());
        cmd.push(spec.clone());
    }
    // Run-mode flags are identical to build-mode flags (the pytest file
    // keeps two identical helpers; collapsed here).
    append_build_mode_flags(&mut cmd, build_mode);
    Ok(cmd)
}

fn append_build_mode_flags(cmd: &mut Vec<String>, build_mode: BuildMode) {
    match build_mode {
        BuildMode::Wasmer => {
            cmd.push("--wasmer".to_string());
            cmd.push("--wasmer-registry=wasmer.io".to_string());
        }
        BuildMode::WasmerAndDocker => {
            cmd.push("--wasmer".to_string());
            cmd.push("--wasmer-registry=wasmer.io".to_string());
            cmd.push("--docker".to_string());
        }
        BuildMode::Local => {}
    }
}

// ---------------------------------------------------------------------------
// Case materialization (path or downloaded archive)
// ---------------------------------------------------------------------------

fn materialize_case(case: &Case, repo_root: &Path) -> Result<PathBuf> {
    ensure!(
        !(case.path.is_some() && case.download.is_some()),
        "E2ECase can define either path or download, not both"
    );
    if let Some(path) = case.path {
        return Ok(repo_root.join(path));
    }
    let Some(url) = case.download else {
        bail!("E2ECase requires either path or download");
    };
    let tmp = std::env::temp_dir().join(format!(
        "shipit-e2e-{}-{:016x}",
        case.test_id,
        rand_u64()
    ));
    fs::create_dir_all(&tmp)?;
    download_and_extract_archive(url, &tmp)
}

fn download_and_extract_archive(url: &str, tmp_path: &Path) -> Result<PathBuf> {
    let download_dir = tmp_path.join("download");
    fs::create_dir_all(&download_dir)?;
    let archive_name = url_file_name(url).unwrap_or_else(|| "download.zip".to_string());
    let archive_path = download_dir.join(&archive_name);

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(120))
        .timeout_read(Duration::from_secs(120))
        .redirects(10)
        .build();
    let response = agent
        .get(url)
        .call()
        .with_context(|| format!("failed to download {url}"))?;
    let mut reader = response.into_reader();
    let mut file = fs::File::create(&archive_path)?;
    std::io::copy(&mut reader, &mut file)
        .with_context(|| format!("failed to write {}", archive_path.display()))?;
    drop(file);

    let extract_dir = tmp_path.join("src");
    fs::create_dir_all(&extract_dir)?;
    ensure!(
        archive_name.ends_with(".zip"),
        "only .zip archives are supported by the Rust e2e harness (got {archive_name})"
    );
    let status = Command::new("unzip")
        .arg("-q")
        .arg(&archive_path)
        .arg("-d")
        .arg(&extract_dir)
        .status()
        .context("failed to run `unzip`")?;
    ensure!(
        status.success(),
        "unzip failed for {} ({status})",
        archive_path.display()
    );

    let mut children: Vec<PathBuf> = fs::read_dir(&extract_dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.file_name().map_or(true, |name| name != "__MACOSX"))
        .collect();
    if children.len() == 1 && children[0].is_dir() {
        return Ok(children.remove(0));
    }
    Ok(extract_dir)
}

fn url_file_name(url: &str) -> Option<String> {
    let after_scheme = url.split_once("://").map_or(url, |(_, rest)| rest);
    let path = after_scheme.split_once('/').map(|(_, path)| path)?;
    let path = path.split(['?', '#']).next().unwrap_or(path);
    let name = path.rsplit('/').next()?;
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

// ---------------------------------------------------------------------------
// MySQL helpers (ports of `_create_mysql_database` etc.)
// ---------------------------------------------------------------------------

fn create_mysql_database(envs: &[(String, String)], repo_root: &Path) -> Result<String> {
    let name = format!("shipit_e2e_{:016x}{:016x}", rand_u64(), rand_u64());
    let sql = format!(
        "CREATE DATABASE {} CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci",
        quote_mysql_identifier(&name)?
    );
    let result = run_mysql_sql(envs, repo_root, &sql)?;
    if result.returncode != Some(0) {
        bail!(
            "Failed to create temporary MySQL database.\n\
             database={name}\n\n\
             --- Captured output start ---\n{}\n\
             --- Captured output end ---",
            result.output()
        );
    }
    Ok(name)
}

fn drop_mysql_database(
    envs: &[(String, String)],
    repo_root: &Path,
    name: &str,
) -> Result<CompletedCommand> {
    let sql = format!(
        "DROP DATABASE IF EXISTS {}",
        quote_mysql_identifier(name)?
    );
    run_mysql_sql(envs, repo_root, &sql)
}

fn run_mysql_sql(
    envs: &[(String, String)],
    repo_root: &Path,
    sql: &str,
) -> Result<CompletedCommand> {
    let cmd = mysql_command(envs, sql)?;
    run_completed_command(&cmd, repo_root, envs, Duration::from_secs(30))
}

fn mysql_command(envs: &[(String, String)], sql: &str) -> Result<Vec<String>> {
    let mysql = which("mysql").context(
        "`mysql` client is not available; it is required for \
         E2ECase(create_db=True).",
    )?;
    let mut cmd = vec![
        mysql.to_string_lossy().into_owned(),
        "--protocol=TCP".to_string(),
        "--batch".to_string(),
        "--skip-column-names".to_string(),
        "--host".to_string(),
        lookup_env(envs, "DB_HOST").unwrap_or_else(|| "127.0.0.1".to_string()),
        "--port".to_string(),
        lookup_env(envs, "DB_PORT").unwrap_or_else(|| "3306".to_string()),
        "--user".to_string(),
        lookup_env(envs, "DB_USERNAME").unwrap_or_else(|| "root".to_string()),
    ];
    if let Some(password) = lookup_env(envs, "DB_PASSWORD") {
        cmd.push(format!("--password={password}"));
    }
    cmd.push("--execute".to_string());
    cmd.push(sql.to_string());
    Ok(cmd)
}

fn quote_mysql_identifier(name: &str) -> Result<String> {
    ensure!(
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "Invalid MySQL identifier: {name:?}"
    );
    Ok(format!("`{name}`"))
}

/// Layered env lookup: case env pairs first (later wins), then the real
/// process environment — mirrors pytest's merged `env` dict.
fn lookup_env(envs: &[(String, String)], key: &str) -> Option<String> {
    envs.iter()
        .rev()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .or_else(|| std::env::var(key).ok())
}

// ---------------------------------------------------------------------------
// wp-content volume (port of `_create_wp_content_volume`)
// ---------------------------------------------------------------------------

fn create_wp_content_volume(project_path: &Path) -> Result<PathBuf> {
    let host_dir = PathBuf::from(format!(
        "/tmp/shipit-e2e-wp-content-{:016x}",
        rand_u64()
    ));
    fs::create_dir_all(&host_dir)?;
    let volume_path = project_path
        .join(".shipit")
        .join("volumes")
        .join("wp-content");
    if let Some(parent) = volume_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let meta = fs::symlink_metadata(&volume_path);
    if let Ok(meta) = meta {
        if meta.file_type().is_symlink() || meta.is_file() {
            fs::remove_file(&volume_path)?;
        } else if meta.is_dir() {
            fs::remove_dir_all(&volume_path)?;
        }
    }
    std::os::unix::fs::symlink(&host_dir, &volume_path)?;
    Ok(host_dir)
}

// ---------------------------------------------------------------------------
// app.yaml phpix memory-limit extraction
// ---------------------------------------------------------------------------

/// Minimal indentation-aware scan for `capabilities: -> memory: -> limit:`
/// in the generated app.yaml (avoids a YAML dependency; the file is
/// machine-generated so the shape is stable).
fn extract_phpix_memory_limit(yaml: &str) -> Option<String> {
    let mut cap_indent: Option<usize> = None;
    let mut mem_indent: Option<usize> = None;
    for raw in yaml.lines() {
        let stripped_start = raw.trim_start();
        if stripped_start.is_empty() || stripped_start.starts_with('#') {
            continue;
        }
        let indent = raw.len() - stripped_start.len();
        let line = stripped_start.trim_end();

        if let Some(mi) = mem_indent {
            if indent <= mi {
                mem_indent = None;
            }
        }
        if let Some(ci) = cap_indent {
            if indent <= ci {
                cap_indent = None;
            }
        }

        if mem_indent.is_some() {
            if let Some(value) = yaml_key_value(line, "limit") {
                return Some(value);
            }
        } else if cap_indent.is_some() {
            if yaml_is_bare_key(line, "memory") {
                mem_indent = Some(indent);
            }
        } else if yaml_is_bare_key(line, "capabilities") {
            cap_indent = Some(indent);
        }
    }
    None
}

fn yaml_is_bare_key(line: &str, key: &str) -> bool {
    line.strip_prefix(key)
        .and_then(|rest| rest.strip_prefix(':'))
        .is_some_and(|rest| rest.trim().is_empty())
}

fn yaml_key_value(line: &str, key: &str) -> Option<String> {
    let rest = line.strip_prefix(key)?.strip_prefix(':')?;
    let value = rest.trim();
    if value.is_empty() {
        return None;
    }
    let value = value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
        .unwrap_or(value);
    Some(value.to_string())
}

// ---------------------------------------------------------------------------
// Misc utilities
// ---------------------------------------------------------------------------

fn workspace_root() -> PathBuf {
    // crates/shipit-e2e -> crates -> workspace root
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate lives two levels below the workspace root")
        .to_path_buf()
}

/// Port of `get_free_port`: random candidate in [1024, 65535], accepted
/// when it binds. Each test process picks its own port, which keeps
/// nextest's process-per-test parallelism safe.
fn get_free_port() -> u16 {
    loop {
        let port = 1024 + (rand_u64() % (65535 - 1024 + 1)) as u16;
        if TcpListener::bind(("0.0.0.0", port)).is_ok() {
            return port;
        }
    }
}

fn rand_u64() -> u64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
    );
    hasher.write_u32(std::process::id());
    hasher.finish()
}

/// Roughly `shlex.join`, for error messages only.
fn shell_join(cmd: &[String]) -> String {
    cmd.iter()
        .map(|arg| {
            let needs_quotes = arg.is_empty()
                || arg.chars().any(|c| {
                    c.is_whitespace() || "'\"\\$&|;<>()`!*?[]{}~#".contains(c)
                });
            if needs_quotes {
                format!("'{}'", arg.replace('\'', "'\\''"))
            } else {
                arg.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn which(binary: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(binary);
        if candidate.is_file() {
            if let Ok(meta) = fs::metadata(&candidate) {
                if meta.permissions().mode() & 0o111 != 0 {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Structural parity: called by a non-ignored test in tests/e2e.rs
// ---------------------------------------------------------------------------

/// Asserts that the generated `#[test]` list matches the case table
/// exactly: one test per (case, structurally-enabled build mode), named
/// `<suite>__<mode>__<test_id>`.
pub fn verify_test_list(generated: &[(&str, &str, BuildMode)]) {
    use std::collections::BTreeSet;

    let mut expected: BTreeSet<String> = BTreeSet::new();
    for case in CASES {
        for mode in case.structural_modes() {
            expected.insert(format!(
                "{}__{}__{}",
                case.suite.slug(),
                mode.slug(),
                case.test_id
            ));
        }
    }

    let mut actual: BTreeSet<String> = BTreeSet::new();
    for (name, test_id, mode) in generated {
        let case = CASES
            .iter()
            .find(|case| case.test_id == *test_id)
            .unwrap_or_else(|| panic!("test {name} references unknown case {test_id:?}"));
        assert!(
            case.structural_modes().contains(mode),
            "test {name}: case {test_id:?} is not structurally enabled for {mode:?}"
        );
        let expected_name = format!(
            "{}__{}__{}",
            case.suite.slug(),
            mode.slug(),
            case.test_id
        );
        assert_eq!(
            *name, expected_name,
            "test fn name must be <suite>__<mode>__<example> for case {test_id:?}"
        );
        assert!(
            actual.insert(expected_name.clone()),
            "duplicate test for {expected_name}"
        );
    }

    let missing: Vec<&String> = expected.difference(&actual).collect();
    let extra: Vec<&String> = actual.difference(&expected).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "generated e2e tests out of sync with case table.\nmissing: {missing:?}\nextra: {extra:?}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_test_ids_are_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for case in CASES {
            assert!(
                seen.insert(case.test_id),
                "duplicate test_id {:?}",
                case.test_id
            );
        }
    }

    #[test]
    fn case_regexes_compile() {
        for case in CASES {
            Regex::new(case.serve_pattern).unwrap_or_else(|err| {
                panic!("bad serve_pattern for {}: {err}", case.test_id)
            });
            for req in case.http {
                if let Some(pattern) = req.body_match {
                    Regex::new(pattern).unwrap_or_else(|err| {
                        panic!("bad body_match for {}: {err}", case.test_id)
                    });
                }
                if let Some(pattern) = req.location_match {
                    Regex::new(pattern).unwrap_or_else(|err| {
                        panic!("bad location_match for {}: {err}", case.test_id)
                    });
                }
            }
            for command in case.commands {
                if let Some(pattern) = command.stdout_match {
                    Regex::new(pattern).unwrap();
                }
                if let Some(pattern) = command.stderr_match {
                    Regex::new(pattern).unwrap();
                }
            }
        }
    }

    /// Port of `_technology_for_case` — proves the hardcoded `suite`
    /// fields match the pytest marker-assignment logic.
    #[test]
    fn suites_match_pytest_classification() {
        const STATIC_E2E_PATHS: &[&str] = &[
            "examples/cdn",
            "examples/hugo",
            "examples/static-htmlwithjs",
            "examples/static-nobuild",
            "examples/staticfile",
            "examples/staticfile-redirects",
        ];
        const STATIC_PYTHON_E2E_PATHS: &[&str] =
            &["examples/mkdocs", "examples/mkdocs-with-plugins"];
        const STATIC_NODE_1_E2E_PATHS: &[&str] = &[
            "examples/nodestatic-angular",
            "examples/nodestatic-assemble",
            "examples/nodestatic-astro",
            "examples/nodestatic-brunch",
            "examples/nodestatic-docusaurus",
            "examples/nodestatic-eleventy",
            "examples/nodestatic-harp",
            "examples/nodestatic-hexo",
            "examples/nodestatic-metalsmith",
            "examples/nodestatic-next",
            "examples/nodestatic-nuxt",
            "examples/nodestatic-remix",
            "examples/nodestatic-svelte",
            "examples/nodestatic-sveltekit",
            "examples/nodestatic-vitepress",
            "examples/nodestatic-vuepress",
        ];

        fn classify(case: &Case) -> Suite {
            let identifier = case.path.or(case.name).unwrap_or("");
            if let Some(url) = case.download {
                let stem = url_file_name(url)
                    .map(|name| {
                        name.rsplit_once('.')
                            .map(|(stem, _)| stem.to_string())
                            .unwrap_or(name)
                    })
                    .unwrap_or_default();
                if stem.contains("wordpress") {
                    return Suite::Php;
                }
            }
            if identifier.starts_with("wordpress")
                || identifier.starts_with("examples/php-")
            {
                return Suite::Php;
            }
            if identifier.starts_with("examples/python-") {
                return Suite::Python;
            }
            if identifier == "examples/node" || identifier.starts_with("examples/node-")
            {
                return Suite::Node;
            }
            if STATIC_E2E_PATHS.contains(&identifier) {
                return Suite::Static;
            }
            if STATIC_PYTHON_E2E_PATHS.contains(&identifier) {
                return Suite::StaticPython;
            }
            if STATIC_NODE_1_E2E_PATHS.contains(&identifier) {
                return Suite::StaticNode1;
            }
            if identifier.starts_with("examples/nodestatic-") {
                return Suite::StaticNode2;
            }
            panic!("could not classify case {:?}", case.test_id);
        }

        for case in CASES {
            assert_eq!(
                case.suite,
                classify(case),
                "suite mismatch for {}",
                case.test_id
            );
        }
    }

    /// Recomputes the pytest case ids (str(case) plus pytest's duplicate
    /// 0/1/2 suffixing), sanitizes them the way the test names do, and
    /// checks they equal the hardcoded `test_id`s.
    #[test]
    fn test_ids_match_pytest_ids() {
        let base_ids: Vec<String> = CASES
            .iter()
            .map(|case| {
                if let Some(name) = case.name {
                    name.to_string()
                } else if let Some(path) = case.path {
                    path.to_string()
                } else {
                    let url = case.download.expect("case needs path/name/download");
                    let name = url_file_name(url).unwrap();
                    name.rsplit_once('.')
                        .map(|(stem, _)| stem.to_string())
                        .unwrap_or(name)
                }
            })
            .collect();

        for (index, case) in CASES.iter().enumerate() {
            let base = &base_ids[index];
            let duplicates = base_ids.iter().filter(|id| *id == base).count();
            let mut pytest_id = base.clone();
            if duplicates > 1 {
                let occurrence = base_ids[..index]
                    .iter()
                    .filter(|id| *id == base)
                    .count();
                pytest_id = format!("{base}{occurrence}");
            }
            let sanitized: String = pytest_id
                .trim_start_matches("examples/")
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect();
            assert_eq!(
                case.test_id, sanitized,
                "test_id mismatch at table index {index}"
            );
        }
    }

    #[test]
    fn memory_limit_extraction() {
        let yaml = "kind: wasmer.io/App.v0\n\
                    capabilities:\n  memory:\n    limit: 2Gb\n\
                    name: something\n";
        assert_eq!(
            extract_phpix_memory_limit(yaml).as_deref(),
            Some("2Gb")
        );
        let quoted = "capabilities:\n  memory:\n    limit: \"2Gb\"\n";
        assert_eq!(
            extract_phpix_memory_limit(quoted).as_deref(),
            Some("2Gb")
        );
        let none = "capabilities:\n  instaboot:\n    enabled: true\n";
        assert_eq!(extract_phpix_memory_limit(none), None);
        let other_limit = "scaling:\n  memory:\n    limit: 9Gb\ncapabilities:\n  memory: {}\n";
        assert_eq!(extract_phpix_memory_limit(other_limit), None);
    }
}
