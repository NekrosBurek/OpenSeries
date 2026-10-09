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

- **Per-game sound profiles.** Recognises games from Steam, Epic, Riot, GOG, Xbox, EA, Ubisoft and Battle.net, and switches the headset EQ while the game runs. Competitive titles get a footsteps-forward EQ out of the box, and you can assign any game to any profile.

### Supported devices

"Verified" means tested on real hardware. Everything else is ported byte-for-byte from [HeadsetControl](https://github.com/Sapd/HeadsetControl) (headsets), [rivalcfg](https://github.com/flozz/rivalcfg) (mice) and [OpenRGB](https://gitlab.com/CalcProgrammer1/OpenRGB) (keyboards), with tests that check our packets against their examples.

| Device | Controls | Status |
|---|---|---|
| Arctis 7+ / 7P+ / 7X+ | Battery, ChatMix, sidetone, 10-band EQ, auto-off | **Verified** |
| Arctis Nova 7 / 7X / Gen 2 / Diablo / WoW, Nova 7P | Battery, ChatMix, sidetone, EQ, auto-off | Ported |
| Arctis Nova 5 / 5X | Battery, ChatMix, sidetone, EQ, auto-off | Ported |
| Arctis Nova Pro Wireless | Battery, sidetone, EQ, auto-off | Ported |
| Arctis 7 (2017/2019), Arctis Pro / GameDAC, Arctis 1 Wireless, 7X, 7P | Battery, ChatMix (Arctis 7/Pro), sidetone, auto-off | Ported |
| Aerox 3 / 5 / 9 Wireless, Prime Wireless / Prime Mini Wireless | Battery, DPI stages, polling rate, lighting | Ported |
| Aerox 3 / 5, Prime Mini, Rival 3, Rival 3 Wireless, Prime / Prime+ | DPI stages, polling rate, lighting (battery on Rival 3 Wireless) | Ported |
| Apex Pro / Pro TKL (all gens), Apex 7 / 7 TKL / 5 / 9 TKL / 9 Mini, Apex 3 / 3 TKL | Lighting colour and brightness | Ported |
| Any other SteelSeries USB device | Detected and shown with its render | Open an issue with your model |

## Layout

    crates/openseries-core   device registry + HID protocols, Windows audio, config, IPC
    crates/openseries        one binary: tray agent (default) and window (--ui)

Your config is a single plain-text file at `%APPDATA%\OpenSeries\config.toml`.

Product renders are cached in `%APPDATA%\OpenSeries\images`. They're copied from a local GG install when one is present, otherwise downloaded from a separate assets mirror. They are never stored in this repo.

## Build

    cargo build --release -p openseries

Push a `vX.Y.Z` tag to release. CI builds `openseries-windows-amd64.exe`, which is the file the updater downloads.
