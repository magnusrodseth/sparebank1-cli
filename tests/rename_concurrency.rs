//! Synthetic subprocess checks for profile renames racing with login writes.
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const OLD_TOKEN: &str = "profile-616c696365-oauth-token.json";
const NEW_TOKEN: &str = "profile-6361726f6c-oauth-token.json";
const TOKEN_REPLY: &str =
    r#"{"access_token":"synthetic-fresh","refresh_token":"synthetic-rotated","expires_in":3600}"#;
static NEXT_SANDBOX_ID: AtomicU64 = AtomicU64::new(0);

struct Sandbox(PathBuf);

impl Sandbox {
    fn new(expired: bool) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let id = NEXT_SANDBOX_ID.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("sb1-rename-{}-{nonce}-{id}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let sandbox = Self(root);
        std::fs::create_dir(sandbox.store()).unwrap();
        std::fs::write(
            sandbox.store().join("profiles.json"),
            r#"{"defaultProfile":"alice","profiles":[{"name":"alice","legacy":false}]}"#,
        )
        .unwrap();
        std::fs::write(
            sandbox
                .store()
                .join("profile-616c696365-client-credentials.json"),
            r#"{"client_id":"synthetic-id","client_secret":"synthetic-secret"}"#,
        )
        .unwrap();
        let expires_at = if expired { 1 } else { 4_102_444_800_i64 };
        std::fs::write(
            sandbox.store().join(OLD_TOKEN),
            serde_json::json!({
                "access_token":"synthetic-original",
                "refresh_token":"synthetic-original-refresh",
                "expires_at":expires_at,
            })
            .to_string(),
        )
        .unwrap();
        sandbox
    }

    fn store(&self) -> PathBuf {
        self.0.join("sparebank1-cli")
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_sb1"));
        command
            .args(args)
            .env("SB1_STORE", "file")
            .env("XDG_CONFIG_HOME", &self.0)
            .env("SB1_TEST_NO_BROWSER", "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn wait(mut child: Child) -> Output {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output().unwrap();
            panic!(
                "subprocess timed out: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn request(listener: &TcpListener) -> TcpStream {
    let deadline = Instant::now() + Duration::from_secs(10);
    let (mut stream, _) = loop {
        match listener.accept() {
            Ok(connection) => break connection,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "no mock bank request");
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("accept failed: {error}"),
        }
    };
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut reader = BufReader::new(&mut stream);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let mut length = 0;
    loop {
        line.clear();
        reader.read_line(&mut line).unwrap();
        if line == "\r\n" {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            if key.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().unwrap();
            }
        }
    }
    reader.read_exact(&mut vec![0; length]).unwrap();
    stream
}

fn respond(mut stream: TcpStream, body: &str) {
    write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
}

struct PausedBank {
    url: String,
    entered: Receiver<()>,
    release: Sender<()>,
    task: thread::JoinHandle<()>,
}

fn paused_bank(hello: bool) -> PausedBank {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (entered_tx, entered) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    let task = thread::spawn(move || {
        let token_request = request(&listener);
        entered_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        respond(token_request, TOKEN_REPLY);
        if hello {
            respond(request(&listener), "\"synthetic hello\"");
        }
    });
    PausedBank {
        url,
        entered,
        release,
        task,
    }
}

fn check_refresh_rename(command: &str) {
    let sandbox = Sandbox::new(true);
    let bank = paused_bank(command == "hello");
    let refresh = sandbox
        .command(&["--profile", "alice", command])
        .env("SB1_TEST_API_BASE_URL", &bank.url)
        .spawn()
        .unwrap();
    bank.entered.recv_timeout(Duration::from_secs(10)).unwrap();

    // The OAuth request is paused, so lock contention proves the token read,
    // remote refresh and token replacement form one operation with rename.
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(sandbox.store().join(".profiles.lock"))
        .unwrap();
    let held = fs2::FileExt::try_lock_exclusive(&lock).is_err();
    if !held {
        fs2::FileExt::unlock(&lock).unwrap();
    }
    let rename = sandbox
        .command(&["profile", "rename", "alice", "carol"])
        .spawn()
        .unwrap();
    bank.release.send(()).unwrap();
    let refreshed = wait(refresh);
    let renamed = wait(rename);
    bank.task.join().unwrap();
    assert!(
        held,
        "refresh must hold the profile lock until its rotated token is stored"
    );
    assert_success(&refreshed);
    assert_success(&renamed);
    assert!(!sandbox.store().join(OLD_TOKEN).exists());
    let token: serde_json::Value =
        serde_json::from_slice(&std::fs::read(sandbox.store().join(NEW_TOKEN)).unwrap()).unwrap();
    assert_eq!(token["access_token"], "synthetic-fresh");
    assert_eq!(token["refresh_token"], "synthetic-rotated");
    assert!(token["expires_at"].as_i64().unwrap() > 1);
    let status = wait(sandbox.command(&["--json", "status"]).spawn().unwrap());
    assert_success(&status);
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["defaultProfile"], "carol");
    assert_eq!(status["profiles"][0]["loggedIn"], true);
}

#[test]
fn rename_waits_for_forced_refresh_and_keeps_the_rotated_token() {
    check_refresh_rename("refresh");
}

#[test]
fn rename_waits_for_automatic_refresh_and_keeps_the_rotated_token() {
    check_refresh_rename("hello");
}

fn assert_no_old_secrets(store: &Path) {
    assert!(!store.join(OLD_TOKEN).exists());
    assert!(!store
        .join("profile-616c696365-client-credentials.json")
        .exists());
}

#[derive(Clone, Copy)]
enum ProfileChange {
    Rename,
    Logout,
    ReuseName,
}

fn check_pending_reauthentication(change: ProfileChange) {
    let sandbox = Sandbox::new(false);
    let original_token = std::fs::read(sandbox.store().join(OLD_TOKEN)).unwrap();
    let original_credentials = std::fs::read(
        sandbox
            .store()
            .join("profile-616c696365-client-credentials.json"),
    )
    .unwrap();
    let bank = paused_bank(false);
    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let callback_port = reservation.local_addr().unwrap().port();
    drop(reservation);
    let redirect = format!("http://localhost:{callback_port}/callback");
    let mut login = sandbox
        .command(&[
            "--profile",
            "alice",
            "login",
            "--client-id",
            "synthetic-reauth-id",
            "--client-secret",
            "synthetic-reauth-secret",
            "--redirect-uri",
            &redirect,
        ])
        .env("SB1_TEST_API_BASE_URL", &bank.url)
        .spawn()
        .unwrap();
    let stderr = BufReader::new(login.stderr.take().unwrap());
    let (authorize_tx, authorize_rx) = mpsc::channel();
    let stderr_task = thread::spawn(move || {
        let mut captured = String::new();
        for line in stderr.lines() {
            let line = line.unwrap();
            if line.trim().starts_with("http://") {
                authorize_tx.send(line.trim().to_owned()).unwrap();
            }
            captured.push_str(&line);
            captured.push('\n');
        }
        captured
    });
    let authorize = authorize_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let authorize = url::Url::parse(&authorize).unwrap();
    let state = authorize
        .query_pairs()
        .find(|(key, _)| key == "state")
        .unwrap()
        .1
        .into_owned();

    // Rename must finish while BankID is pending; no registry lock is held
    // while waiting for a person to authenticate.
    match change {
        ProfileChange::Logout => {
            let logged_out = wait(sandbox.command(&["logout", "--all"]).spawn().unwrap());
            assert_success(&logged_out);
        }
        ProfileChange::Rename | ProfileChange::ReuseName => {
            let renamed = wait(
                sandbox
                    .command(&["profile", "rename", "alice", "carol"])
                    .spawn()
                    .unwrap(),
            );
            assert_success(&renamed);
            if matches!(change, ProfileChange::ReuseName) {
                // Represent a newly configured profile reusing the released name.
                let registry_path = sandbox.store().join("profiles.json");
                let mut registry: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
                registry["profiles"]
                    .as_array_mut()
                    .unwrap()
                    .push(serde_json::json!({"name":"alice","legacy":false}));
                std::fs::write(registry_path, registry.to_string()).unwrap();
                std::fs::write(
                    sandbox.store().join(OLD_TOKEN),
                    r#"{"access_token":"synthetic-replacement","expires_at":4102444800}"#,
                )
                .unwrap();
                std::fs::write(sandbox.store().join("profile-616c696365-client-credentials.json"), r#"{"client_id":"synthetic-replacement","client_secret":"synthetic-replacement"}"#).unwrap();
            }
        }
    }
    let mut callback = TcpStream::connect(("127.0.0.1", callback_port)).unwrap();
    callback
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    write!(callback, "GET /callback?code=synthetic-code&state={state} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
    let mut response = Vec::new();
    match callback.read_to_end(&mut response) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => {}
        Err(error) => panic!("callback failed: {error}"),
    }
    bank.entered.recv_timeout(Duration::from_secs(10)).unwrap();
    bank.release.send(()).unwrap();
    let logged_in = wait(login);
    let stderr = stderr_task.join().unwrap();
    bank.task.join().unwrap();
    assert!(!logged_in.status.success());
    if matches!(change, ProfileChange::Rename) {
        assert!(stderr.contains("renamed during login"), "{stderr}");
    } else {
        assert!(stderr.contains("login changed during BankID"), "{stderr}");
    }
    if matches!(change, ProfileChange::ReuseName) {
        let token: serde_json::Value =
            serde_json::from_slice(&std::fs::read(sandbox.store().join(OLD_TOKEN)).unwrap())
                .unwrap();
        assert_eq!(token["access_token"], "synthetic-replacement");
        let credentials: serde_json::Value = serde_json::from_slice(
            &std::fs::read(
                sandbox
                    .store()
                    .join("profile-616c696365-client-credentials.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(credentials["client_id"], "synthetic-replacement");
    } else {
        assert_no_old_secrets(&sandbox.store());
    }
    if matches!(change, ProfileChange::Logout) {
        assert!(!sandbox.store().join(NEW_TOKEN).exists());
        return;
    }
    assert_eq!(
        std::fs::read(sandbox.store().join(NEW_TOKEN)).unwrap(),
        original_token
    );
    assert_eq!(
        std::fs::read(
            sandbox
                .store()
                .join("profile-6361726f6c-client-credentials.json")
        )
        .unwrap(),
        original_credentials
    );
}

#[test]
fn pending_reauthentication_rejects_a_renamed_profile_without_recreating_old_secrets() {
    check_pending_reauthentication(ProfileChange::Rename);
}

#[test]
fn pending_reauthentication_does_not_undo_logout() {
    check_pending_reauthentication(ProfileChange::Logout);
}

#[test]
fn pending_reauthentication_does_not_overwrite_a_profile_reusing_the_old_name() {
    check_pending_reauthentication(ProfileChange::ReuseName);
}
