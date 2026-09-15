//! A local per-user profile. Atomic writes and owner-only Unix permissions; no Keychain.
use crate::Connection;
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::Path, sync::Mutex};

static STORE_LOCK: Mutex<()> = Mutex::new(());
const FILE_NAME: &str = "printer.json";

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
pub fn load(directory: &Path) -> Result<Option<Connection>, String> {
    let _guard = STORE_LOCK.lock().map_err(|_| "store-unavailable")?;
    match fs::read_to_string(directory.join(FILE_NAME)) {
        Ok(raw) => decode(&raw).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("store-unavailable".into()),
    }
}
pub fn save(directory: &Path, connection: Connection) -> Result<SavedPrinter, String> {
    let _guard = STORE_LOCK.lock().map_err(|_| "store-unavailable")?;
    let connection = connection.validate()?;
    let info = SavedPrinter::from(&connection);
    let raw = serde_json::to_vec(&Profile {
        version: 1,
        connection,
    })
    .map_err(|_| "save-failed")?;
    fs::create_dir_all(directory).map_err(|_| "save-failed")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
            .map_err(|_| "save-failed")?;
    }
    // NamedTempFile creates a new file exclusively (0600 on Unix). Persist uses
    // atomic replacement on supported platforms, preserving the previous profile
    // if writing fails. Temporary files are removed on error.
    let mut file = tempfile::NamedTempFile::new_in(directory).map_err(|_| "save-failed")?;
    file.write_all(&raw).map_err(|_| "save-failed")?;
    file.as_file().sync_all().map_err(|_| "save-failed")?;
    file.persist(directory.join(FILE_NAME))
        .map_err(|_| "save-failed")?;
    Ok(info)
}
pub fn forget(directory: &Path) -> Result<(), String> {
    let _guard = STORE_LOCK.lock().map_err(|_| "store-unavailable")?;
    match fs::remove_file(directory.join(FILE_NAME)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("forget-failed".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn example() -> Connection {
        Connection {
            ip: "192.168.1.2".into(),
            access_code: "TEST1234".into(),
            serial: "DEMO1234".into(),
        }
    }
    #[test]
    fn disk_save_reload_replace_and_forget() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("profile");
        assert!(load(&directory).unwrap().is_none());
        save(&directory, example()).unwrap();
        assert_eq!(load(&directory).unwrap().unwrap().access_code, "TEST1234");
        let mut next = example();
        next.access_code = "DEMO5678".into();
        save(&directory, next).unwrap();
        assert_eq!(load(&directory).unwrap().unwrap().access_code, "DEMO5678");
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(directory.join(FILE_NAME))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        forget(&directory).unwrap();
        forget(&directory).unwrap();
        assert!(load(&directory).unwrap().is_none());
    }
    #[test]
    fn invalid_replacement_keeps_previous_profile_and_io_errors_are_safe() {
        let root = tempfile::tempdir().unwrap();
        save(root.path(), example()).unwrap();
        let mut bad = example();
        bad.access_code.clear();
        assert!(save(root.path(), bad).is_err());
        assert_eq!(load(root.path()).unwrap().unwrap().access_code, "TEST1234");
        let blocked = root.path().join("not-a-directory");
        fs::write(&blocked, b"test").unwrap();
        assert!(matches!(save(&blocked, example()), Err(error) if error == "save-failed"));
        fs::write(root.path().join(FILE_NAME), b"invalid").unwrap();
        assert!(matches!(load(root.path()), Err(error) if error == "saved-printer-invalid"));
    }
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
