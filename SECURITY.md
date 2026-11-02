# Security

EarDeck is an app you run on your computer, so its first job is not to make that computer less safe. It is not antivirus software and does not scan your computer.

## Report a problem

Please use [GitHub's private vulnerability reporting](https://github.com/diegoarivada/eardeck/security/advisories/new) instead of opening a public issue. Include the steps to reproduce it and the EarDeck version.

Only the latest release gets security fixes.

## What EarDeck can access

| Access | Why |
| --- | --- |
| Bluetooth broadcasts | Reads battery and wear status |
| Bluetooth pairing | Pairs AirPods and Beats that you pick |
| Windows audio settings | Connect, default output and volume |
| Media controls | Pause and resume |
| One registry value in your user account | Open at Login, only if you turn it on |
| One settings file in your user profile | Saves your preferences |

EarDeck runs without admin rights, installs no drivers and needs no internet connection. It sends no data anywhere.

## Threat model

The main risk is **someone nearby**, because any Bluetooth device in range can broadcast anything and pick any name.

| Threat | Protection |
| --- | --- |
| Malformed Bluetooth packets | Written in Rust, packets must match an exact size and format, and the parser is tested against 200,000 random packets |
| A device name built to run code in the popup | Names are escaped, and the content security policy blocks inline and remote scripts |
| A fake device that tries to pair | Pairing only happens when you pick a device. Only AirPods and Beats names are accepted, and EarDeck refuses any device that asks for a PIN |
| Fake broadcasts that change what EarDeck shows or pause your media | Broadcasts must match your model and be close by. Auto-pause only runs while your AirPods are connected and are the current output |
| The popup being sent to a website | Navigation is blocked, and the popup can only load the pages built into the app |
| A compromised popup page reaching more of the app | It gets only the permissions it needs, and the backend checks inputs itself |
| A planted program at startup | The Open at Login path is quoted, which stops unquoted path attacks |
| Remote debugging through an environment variable | Cleared when the app starts |
| A hand-edited settings file | Values are range checked when loaded |
| A vulnerable dependency | `cargo audit` runs on every change and weekly, and Dependabot proposes updates |

## Known limits

- **Fake broadcasts can't be fully stopped.** AirPods broadcasts are not authenticated, so someone nearby could show false battery levels or trigger a pause. They can't reach your files or your audio.
- **Pairing relies on the device's name.** AirPods use Bluetooth pairing without a PIN, so a fake device could use an AirPods name. Only pick a device you just put in pairing mode.
- **Releases are not code signed yet.** Windows will show a warning when you install. Compare the SHA-256 value on the releases page to check your download.
- **The popup depends on Microsoft Edge WebView2.** Keep Windows up to date so it gets security fixes.
- **Malware already running as your user is out of scope.** It could already change anything EarDeck can.
