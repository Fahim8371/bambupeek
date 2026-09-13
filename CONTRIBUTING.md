# Contributing

Small, focused pull requests are welcome. For a large feature or a new printer protocol, open an issue describing the intended behavior first.

## Local setup

Install [Tauri’s prerequisites](https://v2.tauri.app/start/prerequisites/), current Rust, Node.js 22.12+, and FFmpeg. Clone the repository, run `npm ci`, then `npm run tauri dev`. The development server binds only to loopback.

Before a pull request:

```sh
npm run build
npm test
cargo fmt --manifest-path src-tauri/Cargo.toml --check
npm run check:privacy
```

On macOS you can set `MACOSX_DEPLOYMENT_TARGET=12.0` when invoking Cargo directly to match Tauri’s build and avoid rebuilding dependencies with different flags.

## Tests and manual checks

Unit tests use synthetic addresses and access codes. They must not depend on a real printer or access your login Keychain. For hardware testing, enter credentials through the app UI and keep them out of source, shell arguments, captures, and issue reports.

For connection or playback changes, verify first-frame playback, disconnect/retry, cancellation, and switching connections. For persistence changes, verify save, quit/relaunch, session-only connection, replacement, forget, and secure-store failure. For window changes, check all presets, settings scrolling, pinning, dragging, and keyboard focus.

## Design and privacy

Keep the picture central. Controls float over it; status remains white text with no panel. Preserve keyboard access and useful error states. Avoid dependencies unless they solve a concrete problem.

Never log raw FFmpeg stderr, MQTT payloads, credential-store errors, camera URLs, real serial numbers, or print filenames. Do not include identifiable camera screenshots. Use synthetic fixtures and illustrations. Read [privacy.md](docs/privacy.md) before changing network or storage behavior.

Describe what changed, why, and what you actually tested. Identify untested platforms honestly. Contributions are under the project’s MIT license.
