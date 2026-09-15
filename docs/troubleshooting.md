# Troubleshooting

## The video engine is missing

Install FFmpeg with `brew install ffmpeg` and reopen BambuPeek. The app checks beside its executable, then `/opt/homebrew/bin/ffmpeg`, `/usr/local/bin/ffmpeg`, and PATH. Do not move a Homebrew FFmpeg binary out of its installation: it may depend on other Homebrew libraries.

## Cannot reach the printer / no route to host

Confirm the printer is powered on and its current IP still matches the saved profile. Allow **BambuPeek** under macOS **Privacy & Security → Local Network**, then relaunch. Check guest network isolation, VLAN/firewall rules, and VPN routing. Both camera port 322 and status port 8883 must be reachable. A browser or terminal app having permission does not imply BambuPeek has permission, and vice versa.

A DHCP reservation in your router can keep the printer’s address stable. If it changes, reconnect using the new address and save the profile again.

## Camera port closed or access rejected

Enable LAN Only Liveview on the P2S and recheck the eight-character LAN access code on the printer. A changed code needs to be entered and saved again. Confirm the model exposes an H.264 RTSPS stream; the separate JPEG protocol used by some models is not implemented.

## Video works but status is unavailable

Open settings and add the correct serial number under **Print status**, or use discovery to fill it. For the currently connected IP, leave the access code blank to reuse it. Connect again with Save selected to retain the serial number. Open settings with Command-comma on Mac if the controls are hidden. The camera does not require a serial number, but the MQTT status topic does.

Status also needs port 8883 and the printer’s firmware to allow local MQTT access. The app retries interrupted status connections. A “Last received values” label means the numbers are stale, not current. Wrong serial numbers can produce a connection with no matching status reports.

## Video freezes or disconnects

BambuPeek detects a missing first frame and stalls during playback, then offers **Try again**. Retry starts a fresh session using the details already in memory. Check Wi-Fi strength and other camera viewers competing for the printer’s stream. The app periodically catches up to the live edge; it is not a recording or frame-accurate inspection tool.

If macOS cannot decode the stream, update macOS. Playback needs H.264 MediaSource support and `requestVideoFrameCallback` in the system WebKit. Older OS versions and other printer codecs have not been validated.

## Printer does not stay saved

A profile is saved only after a video frame appears with Save selected, or after choosing **Save the connected printer on this device**. Check for a save error. Check that your user account can write to the app’s configuration folder and that the device has free disk space. The local-file version does not use Keychain or request a login password.

If the saved profile is invalid, choose **Forget**, reconnect, and save again. The saved profile is an unencrypted local file with owner-only permissions on macOS. Keep it out of repositories and shared folders.

## Window controls disappeared

Move the pointer over the video, or press Tab to focus a control. Drag the picture to move the window. Escape unpins it. The size icon opens the three presets; Escape closes that menu first.

## Reporting a problem

Include app version, OS version, architecture, printer model, broad firmware version, and steps to reproduce. State whether camera, status, saving, or window controls are affected. Use the bug-report template and remove all private identifiers and camera surroundings.
