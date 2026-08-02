use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

#[test]
fn desktop_binary_launch_smoke() {
    // Tauri/GTK needs a display. Linux CI must wrap cargo test with xvfb-run.
    if cfg!(target_os = "linux")
        && std::env::var_os("DISPLAY").is_none()
        && std::env::var_os("WAYLAND_DISPLAY").is_none()
    {
        eprintln!(
            "skipping desktop_binary_launch_smoke: no DISPLAY/WAYLAND_DISPLAY \
             (on GitHub Actions Linux, run tests under xvfb-run -a)"
        );
        return;
    }

    let binary = resolve_binary_path();

    if !binary.exists() {
        panic!(
            "desktop binary not found at {}; run `cargo build -p tiamat` first",
            binary.display()
        );
    }

    let pid = std::process::id();
    let tmp = std::env::temp_dir();
    let config_home = tmp.join(format!("tiamat-smoke-config-{pid}"));
    let data_home = tmp.join(format!("tiamat-smoke-data-{pid}"));
    let runtime_dir = tmp.join(format!("tiamat-smoke-runtime-{pid}"));
    for dir in [&config_home, &data_home, &runtime_dir] {
        std::fs::create_dir_all(dir).expect("create smoke XDG dirs");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&runtime_dir, std::fs::Permissions::from_mode(0o700));
    }

    let mut child = Command::new(&binary)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Isolate app data so CI / local smokes do not collide with a real profile.
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_DATA_HOME", &data_home)
        .env("XDG_RUNTIME_DIR", &runtime_dir)
        .env("WEBKIT_DISABLE_COMPOSITING_MODE", "1")
        .spawn()
        .expect("spawn desktop binary");

    std::thread::sleep(Duration::from_millis(2000));

    match child.try_wait().expect("poll child") {
        None => {
            let _ = child.kill();
            let _ = child.wait();
        }
        Some(status) => {
            let stdout = read_pipe(child.stdout.take());
            let stderr = read_pipe(child.stderr.take());
            let _ = child.wait();
            panic!(
                "desktop binary exited early ({status})\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
            );
        }
    }

    let _ = std::fs::remove_dir_all(&config_home);
    let _ = std::fs::remove_dir_all(&data_home);
    let _ = std::fs::remove_dir_all(&runtime_dir);
}

fn read_pipe(pipe: Option<impl Read>) -> String {
    let Some(mut pipe) = pipe else {
        return String::new();
    };
    let mut buf = String::new();
    let _ = pipe.read_to_string(&mut buf);
    buf
}

fn resolve_binary_path() -> PathBuf {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_tiamat") {
        return PathBuf::from(path);
    }

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    if cfg!(windows) {
        manifest_dir.join("../target/debug/tiamat.exe")
    } else {
        manifest_dir.join("../target/debug/tiamat")
    }
}
