# OpenSeries

Tiny tray app for SteelSeries headsets. No ads, no Moments, no GG required. ~4 MB RAM.

Currently supports **Arctis 7+** on Windows.

- Battery level and charging state in the tray icon, plus a low-battery warning
- Receiver / headset connection status
- **Default device lock**: keeps your chosen output and mic as Windows default (default + communications roles), so calls and meetings always use the headset. Reverts any app or GG that changes it. Survives USB port changes.
- Troubleshoot: driver problem codes, receiver response, current defaults, Sonar devices with no running Sonar engine, GG RAM use; can restart the receiver (admin).
- Auto-update from GitHub Releases (daily check)

Config and log: `%APPDATA%\OpenSeries\`

## Build

    go build -ldflags "-H windowsgui" -o openseries.exe .

Release: push a `vX.Y.Z` tag; CI builds and publishes the binary the updater pulls.
