use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

#[test]
fn desktop_binary_launch_smoke() {
    let binary = resolve_binary_path();

    if !binary.exists() {
        panic!(
            "desktop binary not found at {}; run `cargo build -p tiamat` first",
            binary.display()
        );
    }

    let mut child = Command::new(&binary)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn desktop binary");

    std::thread::sleep(Duration::from_millis(1500));

    let still_running = child.try_wait().expect("poll child").is_none();

    assert!(
        still_running,
        "desktop binary should stay alive briefly after launch"
    );

    child.kill().expect("kill smoke child");
    child.wait().expect("wait for child");
}

fn resolve_binary_path() -> PathBuf {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_tiamat") {
        return PathBuf::from(path);
    }

    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    if cfg!(windows) {
        manifest_dir.join("../target/debug/tiamat.exe")
    } else {
        manifest_dir.join("../target/debug/tiamat")
    }
}
