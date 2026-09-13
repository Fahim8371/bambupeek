# Release guide

The initial distribution is a macOS Apple Silicon preview with external FFmpeg. It is ad hoc signed, not Developer ID signed or notarized. Do not label untested builds as supported or submit this unsigned preview to the official Homebrew cask collection.

1. Update the version consistently in `package.json`, package lock, Cargo manifest/lock, Tauri config, README and release notes.
2. Run the checks in CONTRIBUTING.md and verify camera, status, save/restart, forget and all window controls on hardware.
3. Inspect the exact Git tree and history before pushing. Check commit author/email, documentation, screenshots, metadata, credentials and paths. `npm run check:privacy` is a source heuristic, not proof that every secret format is absent.
4. Build on Apple Silicon with local paths remapped out of Rust diagnostics:

   ```sh
   RUSTFLAGS="--remap-path-prefix=$PWD=. --remap-path-prefix=$HOME/.cargo/registry=cargo-registry" npm run tauri build -- --bundles app
   ```

5. Copy only the `.app` bundle into a clean staging folder. Do not include runtime webview data, Keychain exports, local configuration, captures or build caches. Verify the executable’s architecture and minimum OS using `file` and `otool -l`. For this unsigned preview, seal the completed bundle using `codesign --force --deep --sign - --identifier app.bambupeek.desktop BambuPeek.app`, then verify it with `codesign --verify --deep --strict BambuPeek.app`. This is ad hoc signing, not Developer ID signing or notarization.
6. Inspect every release file for personal paths and known private test identifiers, without printing the values. Review embedded assets and metadata. Archive without resource forks or extended attributes, and calculate SHA-256.
7. Create a Git tag and GitHub prerelease, attach the archive and checksum file, and include installation requirements and limitations. Download and compare the hosted artifact before announcing it.
8. Update `Fahim8371/homebrew-bambupeek` with the exact release URL, version and SHA-256. Keep the FFmpeg dependency and architecture restriction. Validate the cask with Homebrew and test installation into an isolated app directory.

A future signed release will need an Apple Developer identity, notarization credentials stored as CI secrets, and an update/keychain migration check. Do not put signing certificates or credentials in Git. Additional architectures and operating systems need their own builds and hardware validation.
