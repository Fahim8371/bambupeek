#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod camera;
mod discovery;
mod status;
mod storage;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::ipc::{Channel, Response};
use tokio::sync::Semaphore;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub ip: String,
    pub access_code: String,
    #[serde(default)]
    pub serial: String,
}
impl Connection {
    fn validate(mut self) -> Result<Self, String> {
        self.ip = self.ip.trim().to_string();
        self.access_code = self.access_code.trim().to_string();
        self.serial = self.serial.trim().to_string();
        let ip = self
            .ip
            .parse::<std::net::Ipv4Addr>()
            .map_err(|_| "invalid-address")?;
        if !(ip.is_private() || ip.is_link_local()) {
            return Err("local-address-required".into());
        }
        if self.access_code.len() != 8
            || !self.access_code.bytes().all(|b| b.is_ascii_alphanumeric())
        {
            return Err("invalid-code".into());
        }
        if !self.serial.is_empty()
            && (self.serial.len() > 32 || !self.serial.bytes().all(|b| b.is_ascii_alphanumeric()))
        {
            return Err("invalid-serial".into());
        }
        Ok(self)
    }
}
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum StreamEvent {
    Format { codec: String },
    Error { code: String },
    Status { values: status::PrintStatus },
    StatusUnavailable { reason: String },
}
pub struct OwnedTask<T>(pub tokio::task::JoinHandle<T>);
impl<T> Drop for OwnedTask<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}
struct Session {
    id: u32,
    config: Connection,
    credit: Arc<Semaphore>,
    _camera: OwnedTask<()>,
    _status: Option<OwnedTask<()>>,
}
#[derive(Default)]
struct AppState {
    session: Mutex<Option<Session>>,
    config: Mutex<Option<Connection>>,
}

#[tauri::command]
async fn connect(
    config: Option<Connection>,
    session_id: u32,
    events: Channel<StreamEvent>,
    video: Channel<Response>,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let config = {
        let mut saved = state.config.lock().map_err(|_| "internal-error")?;
        if let Some(config) = config {
            *saved = Some(config.validate()?);
        }
        saved.clone().ok_or("invalid-code")?
    };
    if camera::ffmpeg_path().is_none() {
        return Err("ffmpeg-missing".into());
    }
    let credit = Arc::new(Semaphore::new(12));
    let mut lock = state.session.lock().map_err(|_| "internal-error")?;
    lock.take();
    let status_config = config.clone();
    let status_events = events.clone();
    let status = Some(OwnedTask(tokio::spawn(async move {
        let mut config = status_config;
        if config.serial.is_empty() {
            if let Ok(printers) = discovery::discover().await {
                if let Some(printer) = printers.into_iter().find(|printer| printer.ip == config.ip)
                {
                    config.serial = printer.serial;
                }
            }
        }
        if config.serial.is_empty() {
            let _ = status_events.send(StreamEvent::StatusUnavailable {
                reason: "serial-needed".into(),
            });
        } else {
            status::run(config, status_events).await;
        }
    })));
    let camera = OwnedTask(tokio::spawn(camera::run(
        config.clone(),
        events,
        video,
        credit.clone(),
    )));
    *lock = Some(Session {
        id: session_id,
        config,
        credit,
        _camera: camera,
        _status: status,
    });
    Ok(())
}
#[tauri::command]
fn acknowledge(session_id: u32, state: tauri::State<'_, AppState>) {
    if let Ok(lock) = state.session.lock() {
        if let Some(s) = lock.as_ref() {
            if s.id == session_id && s.credit.available_permits() < 12 {
                s.credit.add_permits(1);
            }
        }
    }
}
#[tauri::command]
fn disconnect(state: tauri::State<'_, AppState>) {
    if let Ok(mut lock) = state.session.lock() {
        lock.take();
    }
}
#[tauri::command]
fn environment() -> serde_json::Value {
    serde_json::json!({"ffmpeg":camera::ffmpeg_path().is_some(),"version":env!("CARGO_PKG_VERSION")})
}

#[tauri::command]
async fn saved_printer(
    state: tauri::State<'_, AppState>,
) -> Result<Option<storage::SavedPrinter>, String> {
    let connection = tokio::task::spawn_blocking(storage::load)
        .await
        .map_err(|_| "secure-store-unavailable")??;
    let info = connection.as_ref().map(storage::SavedPrinter::from);
    if let Some(connection) = connection {
        *state.config.lock().map_err(|_| "internal-error")? = Some(connection);
    }
    Ok(info)
}
#[tauri::command]
async fn save_printer(
    session_id: u32,
    state: tauri::State<'_, AppState>,
) -> Result<storage::SavedPrinter, String> {
    let config = state
        .session
        .lock()
        .map_err(|_| "internal-error")?
        .as_ref()
        .filter(|session| session.id == session_id)
        .map(|session| session.config.clone())
        .ok_or("session-ended")?;
    tokio::task::spawn_blocking(move || storage::save(config))
        .await
        .map_err(|_| "save-failed")?
}
#[tauri::command]
async fn forget_printer() -> Result<(), String> {
    tokio::task::spawn_blocking(storage::forget)
        .await
        .map_err(|_| "forget-failed")?
}

#[tauri::command]
fn playback_sample(width: u32, height: u32, frames: u64) {
    #[cfg(not(debug_assertions))]
    let _ = (width, height, frames);
    #[cfg(debug_assertions)]
    eprintln!("[playback] {width}x{height}, {frames} frames presented");
}

fn main() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            connect,
            disconnect,
            acknowledge,
            environment,
            playback_sample,
            discovery::discover,
            saved_printer,
            save_printer,
            forget_printer
        ])
        .run(tauri::generate_context!())
        .expect("BambuPeek could not start");
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config(ip: &str, code: &str) -> Connection {
        Connection {
            ip: ip.into(),
            access_code: code.into(),
            serial: String::new(),
        }
    }
    #[test]
    fn connection_rejects_injection_and_remote_hosts() {
        assert!(config("192.168.1.2", "TEST1234").validate().is_ok());
        assert!(config("192.168.1.2", "12'\n3456").validate().is_err());
        assert!(config("8.8.8.8", "TEST1234").validate().is_err());
        assert!(config("localhost/path", "TEST1234").validate().is_err());
    }
}
