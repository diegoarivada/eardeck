//! Finds headphones in pairing mode and pairs them, so Windows settings are not needed.

use serde::Serialize;
use windows::core::{Ref, HSTRING};
use windows::Devices::Bluetooth::BluetoothDevice;
use windows::Devices::Enumeration::{
    DeviceInformation, DeviceInformationCustomPairing, DeviceInformationKind,
    DeviceInformationUpdate, DevicePairingKinds, DevicePairingRequestedEventArgs,
    DevicePairingResultStatus, DeviceWatcher,
};
use windows::Foundation::TypedEventHandler;
use windows_collections::IIterable;

#[derive(Clone, Debug, Serialize)]
pub struct Found {
    pub id: String,
    pub name: String,
}

pub fn is_apple_audio(name: &str) -> bool {
    let name = name.to_lowercase();
    name.contains("airpods") || name.contains("beats")
}

fn report(info: &DeviceInformation, on_found: &(impl Fn(Found) + Send + Sync)) -> windows::core::Result<()> {
    let name = info.Name()?.to_string();
    if is_apple_audio(&name) {
        on_found(Found { id: info.Id()?.to_string(), name });
    }
    Ok(())
}

/// Runs until the returned watcher is stopped. Names can arrive after the device does,
/// so updates are checked too and `on_found` can fire twice for one device.
pub fn start_scan(on_found: impl Fn(Found) + Send + Sync + Clone + 'static) -> windows::core::Result<DeviceWatcher> {
    let selector = BluetoothDevice::GetDeviceSelectorFromPairingState(false)?;
    let props: IIterable<HSTRING> = Vec::<HSTRING>::new().into();
    let watcher = DeviceInformation::CreateWatcherWithKindAqsFilterAndAdditionalProperties(
        &selector,
        &props,
        DeviceInformationKind::AssociationEndpoint,
    )?;

    let added = on_found.clone();
    watcher.Added(&TypedEventHandler::new(
        move |_: Ref<'_, DeviceWatcher>, info: Ref<'_, DeviceInformation>| {
            if let Some(info) = info.as_ref() {
                report(info, &added)?;
            }
            Ok(())
        },
    ))?;
    watcher.Updated(&TypedEventHandler::new(
        move |_: Ref<'_, DeviceWatcher>, update: Ref<'_, DeviceInformationUpdate>| {
            if let Some(update) = update.as_ref() {
                let info = DeviceInformation::CreateFromIdAsync(&update.Id()?)?.get()?;
                report(&info, &on_found)?;
            }
            Ok(())
        },
    ))?;
    // Windows only keeps the watcher running if all three events have a handler.
    watcher.Removed(&TypedEventHandler::new(
        |_: Ref<'_, DeviceWatcher>, _: Ref<'_, DeviceInformationUpdate>| Ok(()),
    ))?;
    watcher.Start()?;
    Ok(watcher)
}

pub fn pair(id: &str) -> Result<(), String> {
    let err = |e: windows::core::Error| e.message();
    let info = DeviceInformation::CreateFromIdAsync(&HSTRING::from(id))
        .and_then(|op| op.get())
        .map_err(err)?;
    if !is_apple_audio(&info.Name().map_err(err)?.to_string()) {
        return Err("EarDeck only pairs AirPods and Beats".into());
    }
    let pairing = info.Pairing().map_err(err)?;
    let custom = pairing.Custom().map_err(err)?;
    // Only the plain confirm that AirPods use is accepted. A device asking for a PIN is not
    // AirPods, and blindly accepting a PIN match would defeat the check it exists for.
    let handler = TypedEventHandler::new(
        |_: Ref<'_, DeviceInformationCustomPairing>, args: Ref<'_, DevicePairingRequestedEventArgs>| {
            if let Some(args) = args.as_ref() {
                if args.PairingKind()? == DevicePairingKinds::ConfirmOnly {
                    args.Accept()?;
                }
            }
            Ok(())
        },
    );
    let token = custom.PairingRequested(&handler).map_err(err)?;
    let result = custom
        .PairAsync(DevicePairingKinds::ConfirmOnly)
        .and_then(|op| op.get())
        .and_then(|r| r.Status());
    let _ = custom.RemovePairingRequested(token);
    match result.map_err(err)? {
        DevicePairingResultStatus::Paired | DevicePairingResultStatus::AlreadyPaired => Ok(()),
        DevicePairingResultStatus::AuthenticationTimeout | DevicePairingResultStatus::ConnectionRejected => {
            Err("They stopped responding. Put them back in pairing mode and try again.".into())
        }
        status => Err(format!("Pairing failed ({})", status.0)),
    }
}
