//! Xiaomi MiBeacon (BLE service data UUID 0xFE95) parsing and decryption,
//! plus decoding of the Xiaomi Body Composition Scale S400 (MJTZC01YM) object.
//!
//! Protocol details follow the open-source `xiaomi-ble` project used by Home Assistant.

use aes::Aes128;
use ccm::aead::generic_array::GenericArray;
use ccm::aead::{Aead, KeyInit, Payload};
use ccm::consts::{U12, U4};
use ccm::Ccm;

type AesCcm = Ccm<Aes128, U4, U12>;

/// MiBeacon object id carrying S400 weight / heart rate / impedance.
pub const OBJ_S400_BODY: u16 = 0x6E16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MiBeaconError {
    TooShort,
    MeshUnsupported,
    LegacyVersion,
    MacMismatch,
    NoPayload,
    MissingKey,
    DecryptFailed,
}

impl std::fmt::Display for MiBeaconError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::TooShort => "advertisement too short",
            Self::MeshUnsupported => "mesh devices are not supported",
            Self::LegacyVersion => "unsupported (legacy) MiBeacon version",
            Self::MacMismatch => "MAC address does not match frame",
            Self::NoPayload => "advertisement carries no data object",
            Self::MissingKey => "advertisement is encrypted but no bindkey is set",
            Self::DecryptFailed => "decryption failed (wrong bindkey?)",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiBeaconFrame {
    pub device_id: u16,
    pub frame_counter: u8,
    pub encrypted: bool,
    /// (object type id, object bytes)
    pub objects: Vec<(u16, Vec<u8>)>,
}

/// Parse a 6-byte MAC written as `AA:BB:CC:DD:EE:FF`.
pub fn parse_mac(s: &str) -> Option<[u8; 6]> {
    let parts: Vec<&str> = s.trim().split(':').collect();
    if parts.len() != 6 {
        return None;
    }
    let mut mac = [0u8; 6];
    for (i, p) in parts.iter().enumerate() {
        mac[i] = u8::from_str_radix(p, 16).ok()?;
    }
    Some(mac)
}

/// Parse a 32 hex char bindkey.
pub fn parse_bindkey(s: &str) -> Option<[u8; 16]> {
    let bytes = parse_hex(s.trim())?;
    bytes.try_into().ok()
}

pub fn parse_hex(s: &str) -> Option<Vec<u8>> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

/// Parse (and if needed decrypt) a MiBeacon service-data frame.
///
/// * `mac` – the advertiser's address in display order (`AA:BB:..` → `[0xAA, 0xBB, ..]`).
/// * `data` – raw service data for UUID 0xFE95.
pub fn parse_frame(
    mac: [u8; 6],
    data: &[u8],
    bindkey: Option<&[u8; 16]>,
) -> Result<MiBeaconFrame, MiBeaconError> {
    if data.len() < 5 {
        return Err(MiBeaconError::TooShort);
    }
    let frctrl = u16::from_le_bytes([data[0], data[1]]);
    let mesh = (frctrl >> 7) & 1;
    let version = frctrl >> 12;
    let object_include = (frctrl >> 6) & 1;
    let capability_include = (frctrl >> 5) & 1;
    let mac_include = (frctrl >> 4) & 1;
    let encrypted = (frctrl >> 3) & 1 == 1;

    if mesh != 0 {
        return Err(MiBeaconError::MeshUnsupported);
    }
    if version < 4 {
        // v2/v3 use a different (legacy) cipher layout; the S400 uses v5.
        return Err(MiBeaconError::LegacyVersion);
    }

    let device_id = u16::from_le_bytes([data[2], data[3]]);
    let frame_counter = data[4];
    let mut i = 5usize;

    if mac_include != 0 {
        i += 6;
        if data.len() < i {
            return Err(MiBeaconError::TooShort);
        }
        let mut in_frame: [u8; 6] = data[5..11].try_into().unwrap();
        in_frame.reverse();
        if in_frame != mac {
            return Err(MiBeaconError::MacMismatch);
        }
    }

    if capability_include != 0 {
        i += 1;
        if data.len() < i {
            return Err(MiBeaconError::TooShort);
        }
        if data[i - 1] & 0x20 != 0 {
            i += 1;
            if data.len() < i {
                return Err(MiBeaconError::TooShort);
            }
        }
    }

    if object_include == 0 {
        return Ok(MiBeaconFrame {
            device_id,
            frame_counter,
            encrypted,
            objects: vec![],
        });
    }

    let payload: Vec<u8> = if encrypted {
        if data.len() < i + 9 {
            return Err(MiBeaconError::TooShort);
        }
        let key = bindkey.ok_or(MiBeaconError::MissingKey)?;
        let n = data.len();
        let mut nonce = Vec::with_capacity(12);
        let mut mac_rev = mac;
        mac_rev.reverse();
        nonce.extend_from_slice(&mac_rev);
        nonce.extend_from_slice(&data[2..5]);
        nonce.extend_from_slice(&data[n - 7..n - 4]);
        let mut msg = data[i..n - 7].to_vec();
        msg.extend_from_slice(&data[n - 4..]);

        let cipher = AesCcm::new(GenericArray::from_slice(key));
        cipher
            .decrypt(
                GenericArray::from_slice(&nonce),
                Payload {
                    msg: &msg,
                    aad: &[0x11],
                },
            )
            .map_err(|_| MiBeaconError::DecryptFailed)?
    } else {
        if data.len() < i + 3 {
            return Err(MiBeaconError::NoPayload);
        }
        data[i..].to_vec()
    };

    let mut objects = Vec::new();
    let mut p = 0usize;
    while payload.len() >= p + 3 {
        let typ = u16::from_le_bytes([payload[p], payload[p + 1]]);
        let len = payload[p + 2] as usize;
        let next = p + 3 + len;
        if payload.len() < next {
            break;
        }
        objects.push((typ, payload[p + 3..next].to_vec()));
        p = next;
    }

    Ok(MiBeaconFrame {
        device_id,
        frame_counter,
        encrypted,
        objects,
    })
}

/// One decoded S400 broadcast. A full barefoot measurement arrives as two packets:
/// packet 1 → weight + heart rate + low-frequency impedance,
/// packet 2 → high-frequency impedance only.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum S400Packet {
    /// Person stepped off the scale.
    Reset { profile_id: u8 },
    /// Weight, optional heart rate and optional low-frequency impedance (absent with socks).
    Weight {
        profile_id: u8,
        weight_kg: f32,
        heart_rate: Option<u8>,
        impedance_low: Option<f32>,
    },
    /// Final packet: high-frequency impedance.
    ImpedanceHigh { profile_id: u8, impedance_high: f32 },
}

pub fn decode_s400(obj: &[u8]) -> Option<S400Packet> {
    if obj.len() < 5 {
        return None;
    }
    let profile_id = obj[0];
    let data = u32::from_le_bytes([obj[1], obj[2], obj[3], obj[4]]);
    let mass = data & 0x7FF;
    let hr = (data >> 11) & 0x7F;
    let impedance = data >> 18;

    if mass == 0 && hr == 0 && impedance == 0 {
        return Some(S400Packet::Reset { profile_id });
    }
    if mass == 0 && hr == 0 {
        return Some(S400Packet::ImpedanceHigh {
            profile_id,
            impedance_high: impedance as f32 / 10.0,
        });
    }
    if mass == 0 {
        return None;
    }
    Some(S400Packet::Weight {
        profile_id,
        weight_kg: mass as f32 / 10.0,
        heart_rate: if hr > 0 && hr < 127 {
            Some((hr + 50) as u8)
        } else {
            None
        },
        impedance_low: if impedance != 0 {
            Some(impedance as f32 / 10.0)
        } else {
            None
        },
    })
}

/// Completed weigh-in assembled from one or more S400 packets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct S400Measurement {
    pub time_ms: i64,
    pub profile_id: u8,
    pub weight_kg: f32,
    pub heart_rate: Option<u8>,
    pub impedance_low: Option<f32>,
    pub impedance_high: Option<f32>,
}

/// Stateful assembler: feed it raw BLE service data, get finished measurements out.
#[derive(Default)]
pub struct S400Assembler {
    last_counter: Option<(u8, Vec<u8>)>,
    pending: Option<S400Measurement>,
    /// Packets that failed to decrypt since the last success (for UI diagnostics).
    pub decrypt_failures: u32,
    pub last_error: Option<String>,
    pub packets_seen: u32,
}

/// How long to wait for packet 2 before committing a measurement without it.
const PENDING_TIMEOUT_MS: i64 = 20_000;

impl S400Assembler {
    /// Process one advertisement. Returns a finished measurement when one is complete.
    pub fn feed(
        &mut self,
        mac: [u8; 6],
        data: &[u8],
        key: Option<&[u8; 16]>,
        now_ms: i64,
    ) -> Option<S400Measurement> {
        // The scale repeats each packet many times; skip identical repeats.
        let dedupe_key = (data.get(4).copied().unwrap_or(0), data.to_vec());
        if self.last_counter.as_ref() == Some(&dedupe_key) {
            return self.flush_if_stale(now_ms);
        }
        self.last_counter = Some(dedupe_key);

        let frame = match parse_frame(mac, data, key) {
            Ok(f) => f,
            Err(e) => {
                if e == MiBeaconError::DecryptFailed {
                    self.decrypt_failures += 1;
                }
                self.last_error = Some(e.to_string());
                return self.flush_if_stale(now_ms);
            }
        };
        self.decrypt_failures = 0;

        let mut finished = None;
        for (typ, obj) in &frame.objects {
            if *typ != OBJ_S400_BODY {
                continue;
            }
            self.packets_seen += 1;
            match decode_s400(obj) {
                Some(S400Packet::Weight {
                    profile_id,
                    weight_kg,
                    heart_rate,
                    impedance_low,
                }) => {
                    if let Some(p) = self.pending.as_mut() {
                        if (p.weight_kg - weight_kg).abs() < 0.05 && p.profile_id == profile_id {
                            // Same measurement re-sent: enrich it.
                            p.heart_rate = p.heart_rate.or(heart_rate);
                            p.impedance_low = p.impedance_low.or(impedance_low);
                            continue;
                        }
                        finished = self.pending.take();
                    }
                    self.pending = Some(S400Measurement {
                        time_ms: now_ms,
                        profile_id,
                        weight_kg,
                        heart_rate,
                        impedance_low,
                        impedance_high: None,
                    });
                }
                Some(S400Packet::ImpedanceHigh { impedance_high, .. }) => {
                    if let Some(mut p) = self.pending.take() {
                        p.impedance_high = Some(impedance_high);
                        finished = Some(p);
                    }
                }
                Some(S400Packet::Reset { .. }) => {
                    if let Some(p) = self.pending.take() {
                        finished = Some(p);
                    }
                }
                None => {}
            }
        }
        finished.or_else(|| self.flush_if_stale(now_ms))
    }

    /// Commit a pending measurement that never received its second packet.
    pub fn flush_if_stale(&mut self, now_ms: i64) -> Option<S400Measurement> {
        match self.pending {
            Some(p) if now_ms - p.time_ms > PENDING_TIMEOUT_MS => self.pending.take(),
            _ => None,
        }
    }

    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test vectors from the xiaomi-ble project (tests/test_parser.py).
    const MAC_A: &str = "8C:D0:B2:F6:BE:EF";
    const KEY_A: &str = "0728974d657a4b60964c1b1677f35f7c";
    const PKT1: &[u8] = b"HY\xd5;\n\xbc\x07\x8f\xf24\x8c\x84A8\xe90\"\x00\x00\x00\x9eS\x85\x99";
    const PKT2: &[u8] = b"HY\xd5;\x0b\xd6\xef\x0b%\xdbrx^~/F\xd6\x00\x00\x00\xd8d-\xf6";

    const MAC_B: &str = "04:AE:47:67:C6:7C";
    const KEY_B: &str = "02d2900363ef629c736a4549677acbee";
    const SOCKS: &[u8] = b"HY\xd5;\x71\x53\x04\x38\xb5\x89\x4b\x24\x2c\x20\x99\x08\xda\x00\x00\x00\x47\x9e\xcd\xa3";
    const RESET: &[u8] = b"HY\xd5;\x72\x03\x6c\x67\x94\x35\x5a\x19\xdb\xc8\x64\xbf\xb3\x00\x00\x00\xe4\x15\x1d\xc8";

    fn decode(mac: &str, key: &str, pkt: &[u8]) -> S400Packet {
        let frame = parse_frame(
            parse_mac(mac).unwrap(),
            pkt,
            Some(&parse_bindkey(key).unwrap()),
        )
        .expect("frame should decrypt");
        assert_eq!(frame.device_id, 0x3BD5);
        let (typ, obj) = &frame.objects[0];
        assert_eq!(*typ, OBJ_S400_BODY);
        decode_s400(obj).expect("valid S400 object")
    }

    #[test]
    fn packet1_weight_hr_impedance_low() {
        match decode(MAC_A, KEY_A, PKT1) {
            S400Packet::Weight {
                profile_id,
                weight_kg,
                heart_rate,
                impedance_low,
            } => {
                assert_eq!(profile_id, 1);
                assert!((weight_kg - 69.9).abs() < 1e-4);
                assert_eq!(heart_rate, Some(92));
                assert!((impedance_low.unwrap() - 543.2).abs() < 1e-3);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn packet2_impedance_high() {
        match decode(MAC_A, KEY_A, PKT2) {
            S400Packet::ImpedanceHigh {
                profile_id,
                impedance_high,
            } => {
                assert_eq!(profile_id, 1);
                assert!((impedance_high - 497.6).abs() < 1e-3);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn socks_weight_only() {
        match decode(MAC_B, KEY_B, SOCKS) {
            S400Packet::Weight {
                weight_kg,
                heart_rate,
                impedance_low,
                ..
            } => {
                assert!((weight_kg - 74.7).abs() < 1e-4);
                assert_eq!(heart_rate, None);
                assert_eq!(impedance_low, None);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn reset_packet() {
        assert_eq!(
            decode(MAC_B, KEY_B, RESET),
            S400Packet::Reset { profile_id: 1 }
        );
    }

    #[test]
    fn wrong_key_fails() {
        let err = parse_frame(
            parse_mac(MAC_A).unwrap(),
            PKT1,
            Some(&parse_bindkey(KEY_B).unwrap()),
        )
        .unwrap_err();
        assert_eq!(err, MiBeaconError::DecryptFailed);
    }

    #[test]
    fn missing_key() {
        let err = parse_frame(parse_mac(MAC_A).unwrap(), PKT1, None).unwrap_err();
        assert_eq!(err, MiBeaconError::MissingKey);
    }

    #[test]
    fn assembler_merges_two_packets() {
        let mac = parse_mac(MAC_A).unwrap();
        let key = parse_bindkey(KEY_A).unwrap();
        let mut a = S400Assembler::default();
        assert!(a.feed(mac, PKT1, Some(&key), 1_000).is_none());
        // repeated identical broadcast is ignored
        assert!(a.feed(mac, PKT1, Some(&key), 1_100).is_none());
        let m = a.feed(mac, PKT2, Some(&key), 1_500).expect("complete");
        assert!((m.weight_kg - 69.9).abs() < 1e-4);
        assert_eq!(m.heart_rate, Some(92));
        assert!((m.impedance_low.unwrap() - 543.2).abs() < 1e-3);
        assert!((m.impedance_high.unwrap() - 497.6).abs() < 1e-3);
        assert_eq!(m.time_ms, 1_000);
        assert!(!a.has_pending());
    }

    #[test]
    fn assembler_socks_then_reset() {
        let mac = parse_mac(MAC_B).unwrap();
        let key = parse_bindkey(KEY_B).unwrap();
        let mut a = S400Assembler::default();
        assert!(a.feed(mac, SOCKS, Some(&key), 0).is_none());
        let m = a.feed(mac, RESET, Some(&key), 2_000).expect("complete on reset");
        assert!((m.weight_kg - 74.7).abs() < 1e-4);
        assert_eq!(m.impedance_low, None);
    }

    #[test]
    fn assembler_timeout_flush() {
        let mac = parse_mac(MAC_B).unwrap();
        let key = parse_bindkey(KEY_B).unwrap();
        let mut a = S400Assembler::default();
        assert!(a.feed(mac, SOCKS, Some(&key), 0).is_none());
        assert!(a.flush_if_stale(5_000).is_none());
        assert!(a.flush_if_stale(25_000).is_some());
    }

    #[test]
    fn hex_and_mac_parsing() {
        assert_eq!(parse_hex("0aFF"), Some(vec![0x0a, 0xff]));
        assert_eq!(parse_hex("abc"), None);
        assert!(parse_bindkey("00").is_none());
        assert_eq!(
            parse_mac("8C:D0:B2:F6:BE:EF"),
            Some([0x8c, 0xd0, 0xb2, 0xf6, 0xbe, 0xef])
        );
    }
}
