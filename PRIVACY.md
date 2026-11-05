# EarDeck Privacy Policy

Effective November 4, 2026. Also published at https://diegoarivada.dev/eardeck-privacy.html.

EarDeck doesn't collect, store, or share any personal data. Everything it does stays on your computer.

## The short version

- **No internet.** EarDeck never connects to the internet. It has no servers to send anything to.
- **No tracking.** No analytics, no ads, no crash reports, and no third-party SDKs.
- **No account.** There's nothing to sign up for, and EarDeck never asks for your name, email, or location.
- **Settings stay local.** Your preferences are saved in one file on your computer.

## What EarDeck reads

To show battery levels and pair your headphones, EarDeck reads things your computer already has access to:

- **Bluetooth broadcasts** from nearby AirPods and Beats, to show battery and whether they're in your ears.
- **Windows audio devices**, to connect your headphones, set them as the default output, and show volume.
- **Windows media controls**, to pause and resume what's playing when you take an AirPod out.

This information is used in memory while the app runs. It is never saved, logged, or sent anywhere.

## What EarDeck saves

EarDeck saves one settings file in your Windows user profile. It holds your on and off choices (auto-pause, switch audio on connect, popup on connect), your low battery alert level, and an internal Windows ID for the headphones you picked. If you turn on **Open at Login**, EarDeck also adds one startup entry to your user account in the Windows registry. Turning it off removes it.

## What EarDeck shares

Nothing. EarDeck doesn't share or sell data with anyone, because it never has any data to share.

## Microsoft Store

If you install EarDeck from the Microsoft Store, Microsoft handles the download, updates, and any data it collects as part of the Store. That is covered by the [Microsoft Privacy Statement](https://privacy.microsoft.com/privacystatement). I don't receive personal information about you from Microsoft.

## Children

EarDeck doesn't collect information from anyone, including children under 13.

## Deleting your data

Uninstalling EarDeck removes the app. To also remove your saved settings, delete the `dev.diegoarivada.eardeck` folder in `%APPDATA%` and in `%LOCALAPPDATA%`. The second one only holds the popup's display cache from Microsoft Edge WebView2.

## Changes

If this policy changes, I'll update it here and change the date at the top. The full history is in this repository.

## Contact

Questions about privacy? Open an [issue](https://github.com/diegoarivada/eardeck/issues) or use the contact form at https://diegoarivada.dev/#contact.
