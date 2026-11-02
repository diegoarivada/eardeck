//! Decodes Apple's "proximity pairing" BLE advertisement (type 0x07).
//!
//! Layout, counted from the byte after Apple's 0x004C company id:
//!   0     0x07 packet type
//!   1     remaining length (0x19)
//!   3-4   model id, little endian (matches the Bluetooth PID, e.g. 0x201F)
//!   5     status: 0x02 and 0x08 are in-ear bits, 0x20 says which bud is talking
//!   6     battery nibbles: low = this bud, high = other bud
//!   7     low nibble = case battery, high nibble = charging flags
//!   8     lid
//!   9     color
//!   11+   16 encrypted bytes, not used here

use serde::Serialize;

pub const APPLE_COMPANY_ID: u16 = 0x004C;
pub const PACKET_PROXIMITY: u8 = 0x07;
const PACKET_LEN: usize = 27;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Form {
    Buds,
    Single,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Reading {
    pub model_id: u16,
    pub model: &'static str,
    pub form: Form,
    pub left: Option<u8>,
    pub right: Option<u8>,
    pub case: Option<u8>,
    /// Battery of single-unit gear such as AirPods Max.
    pub main: Option<u8>,
    pub left_charging: bool,
    pub right_charging: bool,
    pub case_charging: bool,
    pub main_charging: bool,
    pub left_in_ear: bool,
    pub right_in_ear: bool,
    pub lid_open: bool,
}

pub fn model_info(id: u16) -> Option<(&'static str, Form)> {
    Some(match id {
        0x2002 => ("AirPods (1st gen)", Form::Buds),
        0x200F => ("AirPods (2nd gen)", Form::Buds),
        0x2013 => ("AirPods (3rd gen)", Form::Buds),
        0x2019 => ("AirPods 4", Form::Buds),
        0x201B => ("AirPods 4 (ANC)", Form::Buds),
        0x200E => ("AirPods Pro", Form::Buds),
        0x2014 | 0x2024 => ("AirPods Pro 2", Form::Buds),
        0x2027 => ("AirPods Pro 3", Form::Buds),
        0x200A => ("AirPods Max", Form::Single),
        0x201F => ("AirPods Max (USB-C)", Form::Single),
        0x200B => ("Powerbeats Pro", Form::Buds),
        0x2011 => ("Beats Fit Pro", Form::Buds),
        0x2009 => ("Beats Studio 3", Form::Single),
        0x2017 => ("Beats Studio Pro", Form::Single),
        0x2006 => ("Beats Solo 3", Form::Single),
        _ => return None,
    })
}

fn level(nibble: u8) -> Option<u8> {
    match nibble {
        0..=10 => Some(nibble * 10),
        _ => None,
    }
}

pub fn parse(data: &[u8]) -> Option<Reading> {
    if data.len() != PACKET_LEN || data[0] != PACKET_PROXIMITY {
        return None;
    }
    let model_id = u16::from_le_bytes([data[3], data[4]]);
    let (model, form) = model_info(model_id)?;

    let status = data[5];
    let flipped = status & 0x20 == 0;
    let low = level(data[6] & 0x0F);
    let high = level(data[6] >> 4);
    let charge = data[7] >> 4;

    let (left, right) = if flipped { (high, low) } else { (low, high) };
    let (left_bit, right_bit) = if flipped { (0x02, 0x01) } else { (0x01, 0x02) };
    let (left_ear, right_ear) = if flipped { (0x08, 0x02) } else { (0x02, 0x08) };

    let single = form == Form::Single;
    Some(Reading {
        model_id,
        model,
        form,
        left: if single { None } else { left },
        right: if single { None } else { right },
        case: if single { None } else { level(data[7] & 0x0F) },
        main: if single { low } else { None },
        left_charging: !single && charge & left_bit != 0,
        right_charging: !single && charge & right_bit != 0,
        case_charging: !single && charge & 0x04 != 0,
        main_charging: single && charge & 0x01 != 0,
        left_in_ear: status & left_ear != 0,
        right_in_ear: status & right_ear != 0,
        lid_open: data[8] & 0x08 == 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(model: [u8; 2], status: u8, bat: u8, case: u8) -> Vec<u8> {
        let mut p = vec![0u8; PACKET_LEN];
        p[0] = 0x07;
        p[1] = 0x19;
        p[2] = 0x01;
        p[3] = model[0];
        p[4] = model[1];
        p[5] = status;
        p[6] = bat;
        p[7] = case;
        p
    }

    #[test]
    fn reads_airpods_max_usb_c() {
        let r = parse(&packet([0x1F, 0x20], 0x20, 0xF8, 0x1F)).unwrap();
        assert_eq!(r.model, "AirPods Max (USB-C)");
        assert_eq!(r.main, Some(80));
        assert!(r.main_charging);
        assert_eq!(r.left, None);
    }

    #[test]
    fn swaps_buds_when_flipped() {
        let normal = parse(&packet([0x14, 0x20], 0x22, 0x73, 0x05)).unwrap();
        assert_eq!((normal.left, normal.right), (Some(30), Some(70)));
        assert!(normal.left_in_ear);
        let flipped = parse(&packet([0x14, 0x20], 0x02, 0x73, 0x05)).unwrap();
        assert_eq!((flipped.left, flipped.right), (Some(70), Some(30)));
        assert!(flipped.right_in_ear);
    }

    /// Anyone nearby can broadcast anything, so no packet may panic or read out of range.
    #[test]
    fn survives_random_packets() {
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for round in 0..200_000usize {
            let mut p: Vec<u8> = (0..round % 64).map(|_| next() as u8).collect();
            if p.len() == PACKET_LEN {
                p[0] = PACKET_PROXIMITY;
                [p[3], p[4]] = [0x14, 0x20];
            }
            if let Some(r) = parse(&p) {
                for level in [r.left, r.right, r.case, r.main].into_iter().flatten() {
                    assert!(level <= 100);
                }
            }
        }
    }

    #[test]
    fn reads_real_airpods_4_packet() {
        let hex = "07 19 01 1B 20 2B AA 8F 12 00 04 67 3F 29 76 96 05 20 2B 34 1C A2 F4 8A D2 51 D5";
        let data: Vec<u8> = hex.split(' ').map(|b| u8::from_str_radix(b, 16).unwrap()).collect();
        let r = parse(&data).unwrap();
        assert_eq!(r.model, "AirPods 4 (ANC)");
        assert_eq!((r.left, r.right, r.case), (Some(100), Some(100), None));
        assert!(r.left_in_ear && r.right_in_ear);
    }

    #[test]
    fn rejects_other_packets() {
        assert!(parse(&[0x10, 0x05, 0x01]).is_none());
        assert!(parse(&packet([0xFF, 0xFF], 0, 0, 0)).is_none());
    }
}
