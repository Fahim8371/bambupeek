# Privacy and security model

BambuPeek has no backend, analytics, crash reporting service, cloud account integration, or remote UI assets. It connects to the printer on the local network. Package managers and GitHub contact their own services when installing, downloading, or updating; that is separate from the running application.

## Stored data

| Data | Location | Lifetime |
| --- | --- | --- |
| Saved IP, access code, optional serial | OS credential store, one profile | Until Forget; OS backups may retain copies |
| Active connection | Rust process memory; code briefly entered in webview form | Until exit or replacement |
| Pin and selected size | Local webview storage | Until app website data is cleared |
| Video frames / status | Bounded memory buffers | Replaced during playback and released on disconnect |

There is no plaintext config fallback, camera capture feature, or export of the saved access code. Credentials necessarily exist in app and FFmpeg memory while connected. This does not protect against a compromised OS, privileged debugger, or malware already able to read process memory or unlock the user’s credential store.

## Network traffic

- **RTSPS, TCP 322:** FFmpeg receives the H.264 camera stream directly from the printer.
- **MQTT over TLS, TCP 8883:** read print status. The only application publication is a `pushall` status snapshot request. There are no print, movement, heater, or firmware control commands.
- **Local discovery, UDP 2021 and multicast:** compatible printer discovery when requested, or to resolve a missing serial number for status.

Camera and MQTT accept the printer’s self-signed TLS certificate; they do not verify its identity against a trusted certificate authority or a pinned certificate. Transport encryption alone does not prevent impersonation by an attacker on the network. Use a trusted LAN and do not expose these services to the internet.

## Handling sensitive information

The camera URL is sent to FFmpeg through a private stdin pipe. It is not included in command-line arguments or written to a temporary manifest. Raw FFmpeg errors are classified in memory into fixed error codes; they are never logged or shown because raw errors can contain the camera URL. Only selected status fields reach the frontend, not print filenames or raw MQTT payloads.

Release builds do not emit playback diagnostics. Development builds may print numerical video dimensions and frame counts. Dependencies and the operating system may have their own diagnostics outside the app’s control.

Do not post real printer addresses, access codes, serial numbers, raw diagnostic output, or camera images with identifying surroundings in public issues. The repository’s example values are synthetic. The README hero is an illustration; the separate P2S camera example was shared with permission and contains no connection details.

## Reporting a vulnerability

Follow [SECURITY.md](../SECURITY.md). Do not post a working credential or exploit containing someone’s private connection information in a public issue.
