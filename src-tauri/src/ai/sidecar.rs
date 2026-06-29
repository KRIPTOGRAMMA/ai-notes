use std::net::TcpListener;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tauri_plugin_shell::ShellExt;
use tauri_plugin_shell::process::CommandChild;

pub struct SidecarState {
    pub child: Option<CommandChild>,
    pub port: u16,
    pub ready: bool,
}

impl SidecarState {
    pub fn new() -> Self {
        Self { child: None, port: 0, ready: false }
    }
}

pub type SharedSidecar = Mutex<SidecarState>;

fn pick_free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("failed to bind")
        .local_addr()
        .unwrap()
        .port()
}

pub async fn ensure_running(app: &AppHandle, state: &SharedSidecar) -> Result<u16, String> {
    // If already ready, return port immediately.
    let (already_ready, already_started, existing_port) = {
        let s = state.lock().unwrap();
        (s.ready, s.child.is_some(), s.port)
    };

    if already_ready {
        return Ok(existing_port);
    }
    if already_started {
        return wait_for_ready(existing_port).await.map(|_| existing_port);
    }

    let model_path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("models/model.gguf");

    if !model_path.exists() {
        return Err("Модель не найдена. Скачайте модель в настройках.".into());
    }

    let port = pick_free_port();

    let sidecar = app
        .shell()
        .sidecar("binaries/llamafile-wrapper")
        .map_err(|e| format!("sidecar lookup failed: {e}"))?
        .args([
            "--server",
            "--port", &port.to_string(),
            "--nobrowser",
            "-m", model_path.to_str().unwrap(),
        ]);

    let (_, child) = sidecar.spawn().map_err(|e| format!("spawn failed: {e}"))?;

    {
        let mut s = state.lock().unwrap();
        s.child = Some(child);
        s.port = port;
        s.ready = false;
    }

    wait_for_ready(port).await?;

    {
        let mut s = state.lock().unwrap();
        s.ready = true;
    }

    Ok(port)
}

async fn wait_for_ready(port: u16) -> Result<(), String> {
    let url = format!("http://127.0.0.1:{}/v1/models", port);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .unwrap();

    for _i in 0..60 {
        match client.get(&url).send().await {
            Ok(r) if r.status().is_success() => return Ok(()),
            _ => tokio::time::sleep(std::time::Duration::from_secs(1)).await,
        }
    }
    Err("llamafile did not start within 60 seconds".into())
}
