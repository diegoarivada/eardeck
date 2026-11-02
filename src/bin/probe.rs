//! Diagnostic: prints paired audio gear, audio endpoints and raw Apple BLE packets.
//! Usage: probe [seconds]

#[path = "../audio.rs"]
#[allow(dead_code)]
mod audio;
#[path = "../pairing.rs"]
#[allow(dead_code)]
mod pairing;
#[path = "../parser.rs"]
#[allow(dead_code)]
mod parser;

use std::collections::HashSet;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use windows::Devices::Bluetooth::Advertisement::{
    BluetoothLEAdvertisementReceivedEventArgs, BluetoothLEAdvertisementWatcher, BluetoothLEScanningMode,
};
use windows::Devices::Bluetooth::BluetoothDevice;
use windows::Devices::Enumeration::DeviceInformation;
use windows::Foundation::TypedEventHandler;
use windows::Storage::Streams::{DataReader, IBuffer};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect::<Vec<_>>().join(" ")
}

fn read_buffer(buf: &IBuffer) -> windows::core::Result<Vec<u8>> {
    let reader = DataReader::FromBuffer(buf)?;
    let mut out = vec![0u8; reader.UnconsumedBufferLength()? as usize];
    reader.ReadBytes(&mut out)?;
    Ok(out)
}

fn main() -> windows::core::Result<()> {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };
    let first = std::env::args().nth(1).unwrap_or_default();
    if first == "connect" || first == "disconnect" {
        let id = audio::find_headphones().expect("no paired AirPods endpoint").id;
        let result = if first == "connect" { audio::connect(&id) } else { audio::disconnect(&id) };
        println!("{first}: {result:?}");
        std::thread::sleep(Duration::from_secs(8));
        println!("active now: {}", audio::is_active(&id));
        return Ok(());
    }
    if first == "pair-scan" {
        let watcher = pairing::start_scan(|found| println!("found: {found:?}"))?;
        std::thread::sleep(Duration::from_secs(20));
        println!("watcher status: {:?}", watcher.Status()?);
        watcher.Stop()?;
        return Ok(());
    }
    let secs: u64 = first.parse().unwrap_or(15);

    println!("== paired bluetooth devices ==");
    let selector = BluetoothDevice::GetDeviceSelectorFromPairingState(true)?;
    let devices = DeviceInformation::FindAllAsyncAqsFilter(&selector)?.join()?;
    for info in devices {
        let dev = BluetoothDevice::FromIdAsync(&info.Id()?)?.join()?;
        println!(
            "{:<40} {:012X} {:?}",
            dev.Name()?.to_string(),
            dev.BluetoothAddress()?,
            dev.ConnectionStatus()?
        );
    }

    println!("\n== render endpoints ==");
    for e in audio::endpoints()? {
        println!("active={} bt={} {}", e.active, e.bluetooth, e.name);
    }
    println!("default: {:?}", audio::default_id());

    println!("\n== apple ble for {secs}s ==");
    let seen: Mutex<HashSet<Vec<u8>>> = Mutex::new(HashSet::new());
    let total = Arc::new(AtomicU32::new(0));
    let apple = Arc::new(AtomicU32::new(0));
    let (total_in_handler, apple_in_handler) = (total.clone(), apple.clone());
    let watcher = BluetoothLEAdvertisementWatcher::new()?;
    watcher.SetScanningMode(BluetoothLEScanningMode::Active)?;
    let handler = TypedEventHandler::new(
        move |_: windows::core::Ref<'_, BluetoothLEAdvertisementWatcher>,
              args: windows::core::Ref<'_, BluetoothLEAdvertisementReceivedEventArgs>| {
            let Some(args) = args.as_ref() else { return Ok(()) };
            let rssi = args.RawSignalStrengthInDBm()?;
            total_in_handler.fetch_add(1, Ordering::Relaxed);
            for m in args.Advertisement()?.ManufacturerData()? {
                if m.CompanyId()? != parser::APPLE_COMPANY_ID {
                    continue;
                }
                apple_in_handler.fetch_add(1, Ordering::Relaxed);
                let data = read_buffer(&m.Data()?)?;
                if data.first() != Some(&0x07) {
                    continue;
                }
                // The tail is re-encrypted constantly, so dedupe on the readable part.
                let key = data[..data.len().min(11)].to_vec();
                if seen.lock().unwrap().insert(key) {
                    println!("{rssi:>4} dBm  {}", hex(&data));
                    println!("         -> {:?}", parser::parse(&data));
                }
            }
            Ok(())
        },
    );
    watcher.Received(&handler)?;
    watcher.Start()?;
    std::thread::sleep(Duration::from_secs(secs));
    println!("watcher status: {:?}", watcher.Status()?);
    println!("packets: {} total, {} apple", total.load(Ordering::Relaxed), apple.load(Ordering::Relaxed));
    watcher.Stop()?;
    Ok(())
}
