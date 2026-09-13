use crate::{Connection, OwnedTask, StreamEvent};
use std::{path::PathBuf, process::Stdio, sync::Arc, time::Duration};
use tauri::ipc::{Channel, Response};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::Semaphore,
};

pub fn ffmpeg_path() -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    let mut paths = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            paths.push(dir.join(name));
        }
    }
    paths.extend([
        PathBuf::from("/opt/homebrew/bin/ffmpeg"),
        PathBuf::from("/usr/local/bin/ffmpeg"),
    ]);
    if let Some(path) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&path).map(|p| p.join(name)));
    }
    paths.into_iter().find(|p| p.is_file())
}

// A one-entry concat manifest lets FFmpeg open a live RTSP input without putting its URL in argv.
// Only validated IPv4 and ASCII alphanumeric code values can reach this function.
fn manifest(config: &Connection) -> String {
    format!("ffconcat version 1.0\nfile 'rtsps://bblp:{}@{}:322/streaming/live/1'\noption rtsp_transport tcp\noption tls_verify 0\noption timeout 12000000\noption use_wallclock_as_timestamps 1\noption analyzeduration 1000000\noption probesize 1000000\n",config.access_code,config.ip)
}

fn arguments() -> Vec<&'static str> {
    vec![
        "-hide_banner",
        "-nostdin",
        "-loglevel",
        "error",
        "-f",
        "concat",
        "-safe",
        "0",
        "-protocol_whitelist",
        "pipe,tcp,tls,rtp,udp,crypto",
        "-i",
        "pipe:0",
        "-map",
        "0:v:0",
        "-an",
        "-c:v",
        "copy",
        "-map_metadata",
        "-1",
        "-movflags",
        "+empty_moov+default_base_moof+frag_every_frame",
        "-flush_packets",
        "1",
        "-f",
        "mp4",
        "pipe:1",
    ]
}

pub fn classify_error(raw: &str) -> &'static str {
    let raw = raw.to_ascii_lowercase();
    if raw.contains("401") || raw.contains("unauthorized") {
        "access-rejected"
    } else if raw.contains("no route to host") || raw.contains("operation not permitted") {
        "network-blocked"
    } else if raw.contains("connection refused") {
        "camera-disabled"
    } else if raw.contains("timed out") || raw.contains("timeout") {
        "printer-timeout"
    } else {
        "camera-failed"
    }
}

fn codec(data: &[u8]) -> Option<String> {
    let start = data.windows(4).position(|b| b == b"avcC")?;
    let avcc = data.get(start + 4..start + 8)?;
    (avcc[0] == 1).then(|| format!("avc1.{:02x}{:02x}{:02x}", avcc[1], avcc[2], avcc[3]))
}

pub async fn run(
    config: Connection,
    events: Channel<StreamEvent>,
    video: Channel<Response>,
    credit: Arc<Semaphore>,
) {
    let Some(path) = ffmpeg_path() else {
        let _ = events.send(StreamEvent::Error {
            code: "ffmpeg-missing".into(),
        });
        return;
    };
    let mut command = Command::new(path);
    command
        .args(arguments())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command
        .env_remove("FFREPORT")
        .env_remove("BAMBUPEEK_ACCESS_CODE")
        .env_remove("BAMBUPEEK_IP");
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(_) => {
            let _ = events.send(StreamEvent::Error {
                code: "ffmpeg-start-failed".into(),
            });
            return;
        }
    };
    let mut stdin = child.stdin.take().unwrap();
    if stdin.write_all(manifest(&config).as_bytes()).await.is_err() {
        return;
    }
    drop(stdin);
    drop(config);
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let stderr_task = tokio::spawn(async move {
        let mut raw = Vec::new();
        // Never log stderr: third-party errors may contain the camera URL and access code.
        let _ = (&mut stderr).take(64 * 1024).read_to_end(&mut raw).await;
        classify_error(&String::from_utf8_lossy(&raw)).to_string()
    });
    let mut stderr_task = OwnedTask(stderr_task);
    let mut chunk = vec![0u8; 32 * 1024];
    let mut initial = Vec::new();
    let mut initialized = false;
    loop {
        let permit =
            match tokio::time::timeout(Duration::from_secs(20), credit.clone().acquire_owned())
                .await
            {
                Ok(Ok(permit)) => permit,
                _ => {
                    let _ = events.send(StreamEvent::Error {
                        code: "playback-stalled".into(),
                    });
                    break;
                }
            };
        let read = tokio::time::timeout(Duration::from_secs(18), stdout.read(&mut chunk)).await;
        match read {
            Ok(Ok(0)) => {
                let code =
                    match tokio::time::timeout(Duration::from_secs(1), &mut stderr_task.0).await {
                        Ok(Ok(code)) => code,
                        _ => "camera-ended".into(),
                    };
                let _ = events.send(StreamEvent::Error { code });
                break;
            }
            Ok(Ok(n)) => {
                let bytes = if !initialized {
                    initial.extend_from_slice(&chunk[..n]);
                    if let Some(codec) = codec(&initial) {
                        if events.send(StreamEvent::Format { codec }).is_err() {
                            break;
                        }
                        initialized = true;
                        std::mem::take(&mut initial)
                    } else if initial.len() > 1024 * 1024 {
                        let _ = events.send(StreamEvent::Error {
                            code: "unsupported-video".into(),
                        });
                        break;
                    } else {
                        continue;
                    }
                } else {
                    chunk[..n].to_vec()
                };
                if video.send(Response::new(bytes)).is_err() {
                    break;
                }
                permit.forget();
            }
            _ => {
                let _ = events.send(StreamEvent::Error {
                    code: "printer-timeout".into(),
                });
                break;
            }
        }
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn credentials_are_only_in_the_stdin_manifest() {
        let c = Connection {
            ip: "192.0.2.20".into(),
            access_code: "TEST1234".into(),
            serial: String::new(),
        };
        let args = arguments().join(" ");
        assert!(!args.contains(&c.ip));
        assert!(!args.contains(&c.access_code));
        assert!(
            manifest(&c).contains("file 'rtsps://bblp:TEST1234@192.0.2.20:322/streaming/live/1'")
        );
    }
    #[test]
    fn errors_are_classified_without_returning_secrets() {
        assert_eq!(
            classify_error("401 for rtsps://bblp:SECRET@address"),
            "access-rejected"
        );
        assert_eq!(classify_error("No route to host"), "network-blocked");
        assert_eq!(classify_error("sensitive unknown error"), "camera-failed");
    }
    #[test]
    fn codec_is_read_from_the_stream() {
        assert_eq!(
            codec(b"xxxxavcC\x01\x64\x00\x29rest"),
            Some("avc1.640029".into())
        );
        assert_eq!(codec(b"avcC\x01\x64"), None);
    }
}
