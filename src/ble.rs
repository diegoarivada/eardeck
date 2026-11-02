//! Listens for the BLE packets AirPods broadcast. No pairing or driver needed.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use windows::core::Ref;
use windows::Devices::Bluetooth::Advertisement::{
    BluetoothLEAdvertisementBytePattern, BluetoothLEAdvertisementReceivedEventArgs,
    BluetoothLEAdvertisementWatcher, BluetoothLEAdvertisementWatcherStatus, BluetoothLEScanningMode,
};
use windows::Foundation::TypedEventHandler;
use windows::Storage::Streams::{DataReader, DataWriter};

use crate::parser::{self, Reading};

/// A stronger neighbor's AirPods should not win over yours because of one lucky packet.
const RSSI_MARGIN: i16 = 8;
const KEEP_BEST_SECS: u64 = 10;
/// Anything weaker is across a room or a wall, so it can't be the pair in use here.
const MIN_RSSI: i16 = -75;
const MANUFACTURER_SECTION: u8 = 0xFF;
const AFTER_COMPANY_ID: i16 = 2;

#[derive(Clone)]
pub struct Seen {
    pub reading: Reading,
    pub rssi: i16,
    pub at: Instant,
}

#[derive(Clone, Default)]
pub struct Shared {
    pub latest: Arc<Mutex<Option<Seen>>>,
    /// Name of the paired device. Packets are anonymous, so the model is what ties them to it.
    pub device_name: Arc<Mutex<String>>,
}

pub fn fits_device(device_name: &str, model: &str) -> bool {
    let name = device_name.to_lowercase();
    if name.contains("airpods max") {
        model.contains("Max")
    } else if name.contains("airpods pro") {
        model.contains("AirPods Pro")
    } else if name.contains("airpods") {
        model.contains("AirPods")
    } else {
        true
    }
}

fn accept(shared: &Shared, reading: Reading, rssi: i16) {
    if !fits_device(&shared.device_name.lock().unwrap(), reading.model) {
        return;
    }
    let mut latest = shared.latest.lock().unwrap();
    let keep_old = latest.as_ref().is_some_and(|old| {
        old.at.elapsed().as_secs() < KEEP_BEST_SECS && rssi < old.rssi - RSSI_MARGIN
    });
    if !keep_old {
        *latest = Some(Seen { reading, rssi, at: Instant::now() });
    }
}

pub struct Scanner {
    watcher: BluetoothLEAdvertisementWatcher,
}

impl Scanner {
    pub fn new(shared: Shared) -> windows::core::Result<Self> {
        let watcher = BluetoothLEAdvertisementWatcher::new()?;
        // The data is in the broadcast itself, so the radio never has to transmit.
        watcher.SetScanningMode(BluetoothLEScanningMode::Passive)?;

        // Windows drops everything but Apple proximity packets before this app wakes up.
        let writer = DataWriter::new()?;
        writer.WriteBytes(&[parser::PACKET_PROXIMITY])?;
        let pattern = BluetoothLEAdvertisementBytePattern::Create(
            MANUFACTURER_SECTION,
            AFTER_COMPANY_ID,
            &writer.DetachBuffer()?,
        )?;
        watcher.AdvertisementFilter()?.BytePatterns()?.Append(&pattern)?;

        let handler = TypedEventHandler::new(
            move |_: Ref<'_, BluetoothLEAdvertisementWatcher>,
                  args: Ref<'_, BluetoothLEAdvertisementReceivedEventArgs>| {
                let Some(args) = args.as_ref() else { return Ok(()) };
                let rssi = args.RawSignalStrengthInDBm()?;
                if rssi < MIN_RSSI {
                    return Ok(());
                }
                for section in args.Advertisement()?.ManufacturerData()? {
                    if section.CompanyId()? != parser::APPLE_COMPANY_ID {
                        continue;
                    }
                    let reader = DataReader::FromBuffer(&section.Data()?)?;
                    let mut data = vec![0u8; reader.UnconsumedBufferLength()? as usize];
                    reader.ReadBytes(&mut data)?;
                    if let Some(reading) = parser::parse(&data) {
                        accept(&shared, reading, rssi);
                    }
                }
                Ok(())
            },
        );
        watcher.Received(&handler)?;
        Ok(Self { watcher })
    }

    /// Called every poll. It also restarts a watcher that died when Bluetooth was toggled off.
    pub fn set_running(&self, on: bool) {
        let Ok(status) = self.watcher.Status() else { return };
        let started = status == BluetoothLEAdvertisementWatcherStatus::Started;
        if on && !started {
            let _ = self.watcher.Start();
        } else if !on && started {
            let _ = self.watcher.Stop();
        }
    }
}
