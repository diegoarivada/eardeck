//! Windows Core Audio: find the headphones' endpoint, connect, make default, volume.

use windows::core::{interface, IUnknown, IUnknown_Vtbl, GUID, HRESULT, PCWSTR, PWSTR};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
    eConsole, eMultimedia, eRender, ERole, IDeviceTopology, IMMDevice, IMMDeviceEnumerator,
    MMDeviceEnumerator, DEVICE_STATE, DEVICE_STATEMASK_ALL, DEVICE_STATE_ACTIVE,
};
use windows::Win32::Media::KernelStreaming::{IKsControl, KSIDENTIFIER, KSPROPERTY_TYPE_GET};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ,
};

const KSPROPSETID_BT_AUDIO: GUID = GUID::from_u128(0x7FA06C40_B8F6_4C7E_8556_E8C33A12E54D);
const ONESHOT_RECONNECT: u32 = 0;
const ONESHOT_DISCONNECT: u32 = 1;
const BT_PEER_PREFIX: &str = "{2}.\\\\?\\bth";

const CLSID_POLICY_CONFIG: GUID = GUID::from_u128(0x870AF99C_171D_4F9E_AF0D_E63DF40C2BC9);

/// Undocumented but stable since Windows 7; it is the only way to set the default output.
#[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
unsafe trait IPolicyConfig: IUnknown {
    fn get_mix_format(&self) -> HRESULT;
    fn get_device_format(&self) -> HRESULT;
    fn reset_device_format(&self) -> HRESULT;
    fn set_device_format(&self) -> HRESULT;
    fn get_processing_period(&self) -> HRESULT;
    fn set_processing_period(&self) -> HRESULT;
    fn get_share_mode(&self) -> HRESULT;
    fn set_share_mode(&self) -> HRESULT;
    fn get_property_value(&self) -> HRESULT;
    fn set_property_value(&self) -> HRESULT;
    fn set_default_endpoint(&self, id: PCWSTR, role: ERole) -> HRESULT;
    fn set_endpoint_visibility(&self) -> HRESULT;
}

#[derive(Clone, Debug)]
pub struct Endpoint {
    pub id: String,
    pub name: String,
    pub active: bool,
    pub bluetooth: bool,
}

unsafe fn take_pwstr(p: PWSTR) -> String {
    let s = p.to_string().unwrap_or_default();
    CoTaskMemFree(Some(p.as_ptr() as _));
    s
}

/// Commands land on pool threads; a thread that already has COM just returns an error here.
pub fn com_init() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

fn enumerator() -> windows::core::Result<IMMDeviceEnumerator> {
    com_init();
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
}

/// The Bluetooth audio driver objects behind an endpoint, if it has any.
unsafe fn bt_controls(en: &IMMDeviceEnumerator, dev: &IMMDevice) -> Vec<IKsControl> {
    let mut out = vec![];
    let Ok(topo) = dev.Activate::<IDeviceTopology>(CLSCTX_ALL, None) else {
        return out;
    };
    let count = topo.GetConnectorCount().unwrap_or(0);
    for i in 0..count {
        let Ok(conn) = topo.GetConnector(i) else { continue };
        let Ok(peer) = conn.GetDeviceIdConnectedTo() else { continue };
        let peer_id = take_pwstr(peer);
        if !peer_id.to_lowercase().starts_with(BT_PEER_PREFIX) {
            continue;
        }
        let wide: Vec<u16> = peer_id.encode_utf16().chain(Some(0)).collect();
        if let Ok(peer_dev) = en.GetDevice(PCWSTR(wide.as_ptr())) {
            if let Ok(ks) = peer_dev.Activate::<IKsControl>(CLSCTX_ALL, None) {
                out.push(ks);
            }
        }
    }
    out
}

pub fn endpoints() -> windows::core::Result<Vec<Endpoint>> {
    unsafe {
        let en = enumerator()?;
        let list = en.EnumAudioEndpoints(eRender, DEVICE_STATE(DEVICE_STATEMASK_ALL))?;
        let mut out = vec![];
        for i in 0..list.GetCount()? {
            let Ok(dev) = list.Item(i) else { continue };
            let id = take_pwstr(dev.GetId()?);
            let name = dev
                .OpenPropertyStore(STGM_READ)
                .and_then(|s| s.GetValue(&PKEY_Device_FriendlyName))
                .map(|v| v.to_string())
                .unwrap_or_default();
            let active = dev.GetState()? == DEVICE_STATE_ACTIVE;
            let bluetooth = !bt_controls(&en, &dev).is_empty();
            out.push(Endpoint { id, name, active, bluetooth });
        }
        Ok(out)
    }
}

/// Stereo endpoints of paired Apple or Beats headphones. Windows names them after the
/// Bluetooth device, e.g. "Headphones (Sam's AirPods Max - Find My)".
pub fn all_headphones() -> Vec<Endpoint> {
    let mut found: Vec<Endpoint> = endpoints()
        .unwrap_or_default()
        .into_iter()
        .filter(|e| {
            let name = e.name.to_lowercase();
            e.bluetooth
                && (name.contains("airpods") || name.contains("beats"))
                && !name.contains("hands-free")
        })
        .collect();
    found.sort_by_key(|e| !e.active);
    found
}

/// Used by the probe tool; the app picks through its own device list.
#[allow(dead_code)]
pub fn find_headphones() -> Option<Endpoint> {
    all_headphones().into_iter().next()
}

/// "Headphones (Sam's AirPods Max - Find My)" becomes "Sam's AirPods Max".
pub fn display_name(endpoint_name: &str) -> String {
    let inner = endpoint_name
        .split_once('(')
        .map(|(_, rest)| rest.trim_end_matches(')'))
        .unwrap_or(endpoint_name);
    inner.replace('\u{2019}', "'").trim_end_matches(" - Find My").trim().to_string()
}

pub fn is_active(id: &str) -> bool {
    unsafe {
        let wide: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
        enumerator()
            .and_then(|en| en.GetDevice(PCWSTR(wide.as_ptr())))
            .and_then(|dev| dev.GetState())
            .is_ok_and(|s| s == DEVICE_STATE_ACTIVE)
    }
}

fn oneshot(id: &str, property: u32) -> Result<(), String> {
    unsafe {
        let en = enumerator().map_err(|e| e.message())?;
        let wide: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
        let dev = en.GetDevice(PCWSTR(wide.as_ptr())).map_err(|e| e.message())?;
        let controls = bt_controls(&en, &dev);
        if controls.is_empty() {
            return Err("No Bluetooth audio connection on this endpoint".into());
        }
        let mut prop = KSIDENTIFIER::default();
        prop.Anonymous.Anonymous.Set = KSPROPSETID_BT_AUDIO;
        prop.Anonymous.Anonymous.Id = property;
        prop.Anonymous.Anonymous.Flags = KSPROPERTY_TYPE_GET;
        let mut last = Err("Bluetooth audio driver refused the request".to_string());
        for ks in controls {
            let mut returned = 0u32;
            let hr = ks.KsProperty(
                &prop,
                size_of::<KSIDENTIFIER>() as u32,
                std::ptr::null_mut(),
                0,
                &mut returned,
            );
            if hr.is_ok() {
                last = Ok(());
            }
        }
        last
    }
}

pub fn connect(id: &str) -> Result<(), String> {
    oneshot(id, ONESHOT_RECONNECT)
}

pub fn disconnect(id: &str) -> Result<(), String> {
    oneshot(id, ONESHOT_DISCONNECT)
}

pub fn set_default(id: &str) -> Result<(), String> {
    com_init();
    unsafe {
        let policy: IPolicyConfig =
            CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL).map_err(|e| e.message())?;
        let wide: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
        policy.set_default_endpoint(PCWSTR(wide.as_ptr()), eConsole).ok().map_err(|e| e.message())?;
        policy.set_default_endpoint(PCWSTR(wide.as_ptr()), eMultimedia).ok().map_err(|e| e.message())
    }
}

pub fn default_id() -> Option<String> {
    unsafe {
        let dev = enumerator().ok()?.GetDefaultAudioEndpoint(eRender, eMultimedia).ok()?;
        Some(take_pwstr(dev.GetId().ok()?))
    }
}

fn volume_control(id: &str) -> windows::core::Result<IAudioEndpointVolume> {
    unsafe {
        let wide: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
        enumerator()?.GetDevice(PCWSTR(wide.as_ptr()))?.Activate(CLSCTX_ALL, None)
    }
}

pub fn get_volume(id: &str) -> Option<u8> {
    unsafe {
        let v = volume_control(id).ok()?.GetMasterVolumeLevelScalar().ok()?;
        Some((v * 100.0).round() as u8)
    }
}

pub fn set_volume(id: &str, percent: u8) -> Result<(), String> {
    unsafe {
        volume_control(id)
            .and_then(|v| v.SetMasterVolumeLevelScalar(percent.min(100) as f32 / 100.0, std::ptr::null()))
            .map_err(|e| e.message())
    }
}
