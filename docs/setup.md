# Set up BambuPeek

## First launch on macOS

The initial Apple Silicon preview is ad hoc signed for local execution, but is not signed with an Apple Developer ID or notarized by Apple. macOS may block a downloaded copy. If you trust the release and have verified its checksum, try opening it, then use **System Settings → Privacy & Security → Open Anyway** if macOS offers that option. Follow [Apple’s instructions](https://support.apple.com/en-us/102445). Do not disable Gatekeeper globally.

Allow **BambuPeek** to access the local network. If the permission was previously denied, use **System Settings → Privacy & Security → Local Network**. Quit and reopen BambuPeek after changing it.

Install FFmpeg using `brew install ffmpeg` if you downloaded the ZIP. The Homebrew cask installs that dependency automatically. FFmpeg must remain installed for live video.

## Find connection details

Use the printer’s own network/device settings to find its local IP, LAN access code, and serial number. Names and menu positions can vary by firmware. For the tested P2S, enable **LAN Only Liveview**; a Bambu cloud login is not needed in BambuPeek.

The computer must be able to reach the printer directly. Guest Wi-Fi, wireless client isolation, VLAN rules, and some VPNs prevent that even when both devices have internet access. BambuPeek accepts private or link-local IPv4 addresses, not public hosts, IPv6 addresses, or remote camera URLs.

**Find printers on this network** listens for compatible local discovery announcements for about seven seconds. Select a result to fill the address and serial number; enter the access code yourself. If discovery does not work, manual connection is still available.

## Save and reconnect

BambuPeek saves **one printer profile per OS user**. The IP, access code, and optional serial number are stored in the app’s local configuration folder as `printer.json`. On macOS: `~/Library/Application Support/app.bambupeek.desktop/printer.json`. Experimental Windows builds use the current user’s app configuration directory instead.

The Save checkbox is selected initially. A successful first camera frame triggers saving. Changing printers and saving replaces the previous profile. If saving fails, the app says so and keeps the current connection running. Writes replace the file atomically so an incomplete write does not erase the previous profile.

On the next launch, BambuPeek loads the saved profile internally and tries to reconnect. No access code is returned to the web interface. There is no Keychain or credential-store access and no password prompt. The local file is unencrypted; on macOS it is readable/writable only by the current OS user (0600), inside an owner-only directory (0700). Software running as that same user and administrators may still read it.

If you originally connected without saving, open settings and choose **Save the connected printer on this device**. Use **Connect saved printer** to switch back to the saved connection without typing its code.

Unchecking Save affects the new connection only. It does not remove a previously saved printer.

## Forget a printer

Choose **Settings → Forget**. The stored profile is deleted, while any active connection may continue until you quit or connect elsewhere. On the next launch, BambuPeek will ask for printer details.

Forget the profile before uninstalling if you want to remove it. If you already uninstalled on macOS, delete the specific `printer.json` file in the app’s configuration folder. OS backups are managed by the OS and are not erased by this action.

If you tried an earlier development build that used Keychain, its old entry may remain there. This version never reads it or triggers its password prompt. You can optionally remove the old `app.bambupeek.desktop` / `saved-printer` item using Keychain Access. Enter and save your printer once in the new local-file version.

## Size and pin preferences

Choose the size icon above the picture and select Small, Medium, or Large. These are logical window dimensions at a 16:9 ratio. The last preset is applied on the next launch. The Stay on top preference is also remembered. Both preferences are local to the device’s app webview.

Small windows keep the status compact. Settings scroll when the window cannot fit the full form. Status always sits over the video picture, including when the stream is letterboxed.
