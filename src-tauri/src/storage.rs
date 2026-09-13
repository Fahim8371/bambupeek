//! The entire printer profile lives in the OS credential store, never a config file.
use crate::Connection;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

static STORE_LOCK: Mutex<()> = Mutex::new(());
const SERVICE: &str = "app.bambupeek.desktop";
const ACCOUNT: &str = "saved-printer";

#[derive(Serialize, Deserialize)]
struct Profile {
    version: u8,
    connection: Connection,
}

// Deliberately separate from Connection: access codes must not return over IPC.
#[derive(Serialize)]
pub struct SavedPrinter {
    pub ip: String,
    pub serial: String,
}
impl From<&Connection> for SavedPrinter {
    fn from(connection: &Connection) -> Self {
        Self {
            ip: connection.ip.clone(),
            serial: connection.serial.clone(),
        }
    }
}

fn entry() -> Result<keyring::Entry, String> {
    // Keyring falls back to a mock backend on unsupported platforms. Never
    // report a successful save there: this release supports macOS and Windows.
    if !cfg!(any(target_os = "macos", target_os = "windows")) {
        return Err("secure-store-unavailable".into());
    }
    keyring::Entry::new(SERVICE, ACCOUNT).map_err(|_| "secure-store-unavailable".into())
}
fn decode(raw: &str) -> Result<Connection, String> {
    let profile: Profile = serde_json::from_str(raw).map_err(|_| "saved-printer-invalid")?;
    if profile.version != 1 {
        return Err("saved-printer-invalid".into());
    }
    profile
        .connection
        .validate()
        .map_err(|_| "saved-printer-invalid".into())
}
pub fn load() -> Result<Option<Connection>, String> {
    let _guard = STORE_LOCK.lock().map_err(|_| "secure-store-unavailable")?;
    match entry()?.get_password() {
        Ok(raw) => decode(&raw).map(Some),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err("secure-store-unavailable".into()),
    }
}
pub fn save(connection: Connection) -> Result<SavedPrinter, String> {
    let _guard = STORE_LOCK.lock().map_err(|_| "secure-store-unavailable")?;
    let connection = connection.validate()?;
    let info = SavedPrinter::from(&connection);
    let raw = serde_json::to_string(&Profile {
        version: 1,
        connection,
    })
    .map_err(|_| "save-failed")?;
    entry()?.set_password(&raw).map_err(|_| "save-failed")?;
    Ok(info)
}
pub fn forget() -> Result<(), String> {
    let _guard = STORE_LOCK.lock().map_err(|_| "secure-store-unavailable")?;
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("forget-failed".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_roundtrip_and_public_metadata_omit_code() {
        let connection = Connection {
            ip: "192.168.1.2".into(),
            access_code: "TEST1234".into(),
            serial: "DEMO1234".into(),
        };
        let raw = serde_json::to_string(&Profile {
            version: 1,
            connection,
        })
        .unwrap();
        let decoded = decode(&raw).unwrap();
        assert_eq!(decoded.access_code, "TEST1234");
        let public = serde_json::to_value(SavedPrinter::from(&decoded)).unwrap();
        assert_eq!(public.as_object().unwrap().len(), 2);
        assert!(public.get("accessCode").is_none());
    }
    #[test]
    fn corrupt_future_or_invalid_profiles_are_rejected() {
        for raw in [
            "not json",
            r#"{"version":2,"connection":{"ip":"192.168.1.2","accessCode":"TEST1234"}}"#,
            r#"{"version":1,"connection":{"ip":"8.8.8.8","accessCode":"TEST1234"}}"#,
        ] {
            assert!(matches!(decode(raw), Err(error) if error == "saved-printer-invalid"));
        }
    }
}
