# Architecture

BambuPeek uses Tauri 2 for a native window and IPC, Rust for printer connections and local storage, and TypeScript with the system webview for video and controls. No localhost media server or cloud relay is involved.

```mermaid
flowchart LR
  Printer[Printer camera] -->|RTSPS / TLS| FFmpeg
  FFmpeg -->|fragmented MP4 via stdout| Rust
  Rust -->|bounded Tauri channel| Webview[MediaSource video]
  MQTT[Printer status] -->|MQTT / TLS| Rust
  Profile[Local printer.json] <--> Rust
  Webview -->|window controls / IPC| Rust
```

## Source map

| File | Responsibility |
| --- | --- |
| `src/main.ts` | MediaSource lifecycle, frame presentation checks, UI, settings and window presets |
| `src/styles.css` / `index.html` | Frameless viewer, white status overlay, accessible controls and setup form |
| `src-tauri/src/main.rs` | Validated connection input, session ownership and IPC commands |
| `camera.rs` | FFmpeg process, stdin manifest, fMP4 output, codec extraction and safe errors |
| `status.rs` | TLS MQTT connection, selected status fields, partial report merging and retries |
| `discovery.rs` | Local UDP discovery and validated printer metadata |
| `storage.rs` | Versioned local profile, atomic writes and private file permissions; sanitized public metadata |

## Playback

FFmpeg copies the incoming H.264 stream into fragmented MP4 without transcoding. Rust reads bounded chunks and sends binary data through Tauri channels. A 12-credit semaphore limits in-flight chunks; the frontend acknowledges completed SourceBuffer appends. The frontend also limits its queue to 8 MiB and trims old media data.

The codec string comes from the actual MP4 `avcC` record. The interface reports Live only after the video element presents a frame. Startup and playback watchdogs catch connection and decoding stalls. Stopping a session aborts its owned tasks and kills the FFmpeg child; session IDs prevent old callbacks or acknowledgements affecting a replacement connection.

## Status

MQTT uses a separate task. It subscribes to the device report topic before requesting a snapshot, accepts larger printer reports, and merges partial updates without erasing previous fields. Only progress, state, time, layers and temperatures are sent to the frontend. A disconnected status path does not stop the camera.

## Persistence

A versioned JSON profile is stored in the OS user’s Tauri app configuration directory, outside the app bundle and source tree. File operations run on blocking worker threads and are serialized. On macOS, the directory is mode 0700 and the file mode 0600. A uniquely created temporary file is flushed and atomically persisted over the destination. Failed writes retain the previous profile, and temporary files are cleaned up. The file is not encrypted; there is no Keychain dependency or password prompt.

Public metadata has a separate type containing only IP and serial. Loading seeds Rust’s connection memory without returning its access code over IPC. The app remembers one printer, saved only after successful frame presentation or an explicit save of the live session. A session ID check prevents a stale save request from selecting a different session’s credentials. The frontend serializes save/forget interactions.

## Scope

There are no printer control operations, cloud APIs, saved video, auto-updater, multi-printer library, or JPEG camera protocol. Windows source support is experimental. These are possible future additions, not current features.
