# OpenSeries

A native, local-first replacement for SteelSeries GG, written in Rust with GPUI. No account, no ads, no Moments, no telemetry.

The tray agent idles at about **2 MB of RAM**. The window is a separate process that only exists while it's open.

## What it does

- **Device view.** Shows the real product render with live call-outs for battery, microphone, ChatMix and sound.
- **Default-device lock.** Keeps your headset and mic as the Windows default for every role, including *Communications*, so calls and meetings always use them. It puts them back when GG, a game or Windows changes them, and still recognises them if you move the receiver to another USB port.
- **ChatMix without Sonar.** The headset dial sets per-app volumes: chat apps (Discord, Teams, Zoom…) follow Chat, and everything else follows Game.
- **Onboard EQ.** A 10-band ±12 dB equalizer with presets, stored on the headset. Also controls sidetone and the auto power-off timer.
- **Troubleshooting.** Checks driver problem codes, whether the receiver and headset respond, your current defaults, Sonar leftovers and GG's RAM use. It can also restart the receiver.
- **Auto-update** from GitHub Releases.

### Supported devices

| Device | Status |
|---|---|
| Arctis 7+ / 7P+ / 7X+ | Battery, ChatMix, sidetone, EQ, auto-off |
| Any other SteelSeries USB device | Detected and shown. Controls are added per model, so open an issue with your model. |

## Layout

    crates/openseries-core   device registry + HID protocols, Windows audio, config, IPC
    crates/openseries        one binary: tray agent (default) and window (--ui)

Your config is a single plain-text file at `%APPDATA%\OpenSeries\config.toml`.

Product renders are cached in `%APPDATA%\OpenSeries\images`. They're copied from a local GG install when one is present, otherwise downloaded from a separate assets mirror. They are never stored in this repo.

## Build

    cargo build --release -p openseries

Push a `vX.Y.Z` tag to release. CI builds `openseries-windows-amd64.exe`, which is the file the updater downloads.
