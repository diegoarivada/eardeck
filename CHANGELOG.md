# Update Log

## 0.1.1 (November 3, 2026)

A bug fix release. Everything below was found by running unit tests, static analysis (Clippy, ESLint, cargo audit, cargo deny) and dynamic analysis (5 minutes of fuzzing, a live Bluetooth probe and running the installed app with real AirPods).

### Fixed

- **Battery showed "--" for AirPods in use.** EarDeck ignored any broadcast weaker than -75 dBm, but worn AirPods were measured at -66 to -77 dBm. The cutoff is now -85 dBm, which still drops the far-away traffic around -90 dBm.
- **Low battery alerts could fire on old readings.** A level from before you charged could trigger an alert the moment you reconnected. Alerts now only use readings from the last minute.
- **Music could resume by surprise.** If Automatic Pause paused something and the AirPods then disconnected, putting them on again hours later resumed it. The pause is now forgotten on disconnect.
- **Automatic Pause resumed the wrong app.** It played whatever app was current, not the one it paused. It now resumes only the app it paused, and only if that app is still paused.
- **A stranger's AirPods could show up as yours.** Before any AirPods were paired, or with Beats paired, EarDeck accepted battery broadcasts from any nearby Apple headphones. It now only accepts the model family that matches your device, and none at all until you have one.
- **Plain AirPods matched AirPods Pro and Max broadcasts.** These are now told apart.
- **Pairing new AirPods could pick your older pair.** If an existing pair had a similar name, like "Sam's AirPods Pro" and a new "AirPods Pro", the old one got selected and connected. EarDeck now picks the device Windows just added.
- **Pairing trusted the name sent by the popup.** It now uses the name Windows reports for the device.
- **One bad audio device could hide all your AirPods.** A single device in an odd state made the whole device list come back empty. Bad entries are now skipped.
- **Names with parentheses were cut off.** "Sam's AirPods (2)" showed as "Sam's AirPods (2". Only the parenthesis Windows adds is removed now.
- **The volume slider could jump back.** Quick drags sent many changes that could finish out of order, so the volume landed on an earlier spot. Changes now go one at a time, and the last position wins.
- **The volume slider could freeze.** A drag that ended off the slider left it ignoring updates until the popup closed.
- **Double-clicking Connect undid itself.** The second click disconnected right after connecting. The button now waits for the first click to finish.
- **Low battery color ignored your setting.** The red battery color was fixed at 20% instead of following the alert level.
- **"Max" in a name showed over-ear art.** A name like "Maxine's AirPods Pro" showed the AirPods Max picture. Only whole words count now.
- **The popup opened off-screen with a top taskbar.** It now opens below the tray when the taskbar is at the top.
- **Settings could reset after a crash.** A crash or power loss mid-save could leave a broken settings file. Saves now go to a temporary file first.
- **Dependabot broke the build.** It updated `windows-core` and `windows-collections` without `windows`, and those have to move together. They are now held to the same release line.

### Tests

- 11 new unit tests, 16 in total, covering device matching, new pair selection, name cleanup, device choice, battery levels and settings loading.

### Checks run for this release

| Check | Result |
| --- | --- |
| Unit tests (`cargo test`) | 16 passed |
| Clippy, CI level (`-D warnings`) | Clean |
| Clippy, pedantic and nursery | Style suggestions only, reviewed |
| ESLint on the popup script | Clean |
| `cargo audit` | No vulnerabilities. Warnings are Linux-only crates that are not in the Windows build |
| `cargo deny` | Advisories, bans, licenses and sources all pass |
| Fuzzing the Bluetooth parser | 27.7 million inputs in 5 minutes, no crashes |
| Live run on Windows 11 with AirPods 4 | Battery, connection and output shown correctly. About 3 MB private memory and 0.2% CPU |

## 0.1.0 (November 2, 2026)

- First release.
