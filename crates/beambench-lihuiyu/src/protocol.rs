use beambench_common::{
    ControllerEvidenceState, ControllerModel, ControllerProductTier, DeviceCapabilities,
    TransportKind,
};
use thiserror::Error;

pub const USB_VENDOR_ID: u16 = 0x1a86;
pub const USB_PRODUCT_ID: u16 = 0x5512;
pub const BULK_WRITE_ENDPOINT: u8 = 0x02;
pub const BULK_READ_ENDPOINT: u8 = 0x82;
pub const CH341_EPP_DATA_WRITE: u8 = 0xa6;
pub const CH341_STATUS_REQUEST: u8 = 0xa0;
pub const PACKET_HEADER: u8 = 0x00;
pub const PACKET_PAYLOAD_SIZE: usize = 30;
pub const PACKET_SIZE: usize = 32;
pub const M2_MILLIMETRES_PER_STEP: f64 = 0.0254;

const M2_CLOCK_SLOPE: f64 = 12_120.0;
const M2_DIAGONAL_RATIO: f64 = 0.261_199_033_289;
const MAX_DISTANCE_STEPS: u32 = 1_000_000;

#[derive(Debug, Clone, PartialEq)]
pub struct LihuiyuCompatibilityTarget {
    pub name: &'static str,
    pub model: ControllerModel,
    pub vendor_id: u16,
    pub product_id: u16,
    pub product_tier: ControllerProductTier,
    pub evidence_state: ControllerEvidenceState,
    pub transport_kind: TransportKind,
    pub capabilities: DeviceCapabilities,
    pub hardware_set_power: bool,
}

pub const LIHUIYU_M2_NANO_TARGET: LihuiyuCompatibilityTarget = LihuiyuCompatibilityTarget {
    name: "Lihuiyu M2/M3 Nano (M2-compatible mode)",
    model: ControllerModel::LihuiyuM2Nano,
    vendor_id: USB_VENDOR_ID,
    product_id: USB_PRODUCT_ID,
    product_tier: ControllerProductTier::Experimental,
    evidence_state: ControllerEvidenceState::Emulated,
    transport_kind: TransportKind::UsbPacket,
    capabilities: DeviceCapabilities {
        can_home: true,
        can_jog: true,
        can_jog_continuous: false,
        can_unlock: true,
        can_pause_resume: true,
        can_set_origin: false,
        can_frame: true,
        can_run_job: true,
        reports_absolute_position: false,
        can_manual_fire: false,
        can_adjust_overrides: false,
        supports_rotary: false,
        supports_cylinder: false,
        supports_camera_alignment: false,
    },
    hardware_set_power: true,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LihuiyuPadding {
    AsciiF,
    Zero,
}

impl LihuiyuPadding {
    const fn byte(self) -> u8 {
        match self {
            Self::AsciiF => b'F',
            Self::Zero => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LihuiyuPacket(pub [u8; PACKET_SIZE]);

impl AsRef<[u8]> for LihuiyuPacket {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LihuiyuStatus {
    SerialCorrectOrM3Finished,
    Ready,
    ChecksumError,
    Finished,
    Busy,
    Power,
    Unknown(u8),
}

impl LihuiyuStatus {
    pub const fn from_code(code: u8) -> Self {
        match code {
            0xcc => Self::SerialCorrectOrM3Finished,
            0xce => Self::Ready,
            0xcf => Self::ChecksumError,
            0xec => Self::Finished,
            0xee => Self::Busy,
            0xef => Self::Power,
            value => Self::Unknown(value),
        }
    }

    pub const fn code(self) -> u8 {
        match self {
            Self::SerialCorrectOrM3Finished => 0xcc,
            Self::Ready => 0xce,
            Self::ChecksumError => 0xcf,
            Self::Finished => 0xec,
            Self::Busy => 0xee,
            Self::Power => 0xef,
            Self::Unknown(value) => value,
        }
    }

    pub const fn is_recognized_controller_status(self) -> bool {
        !matches!(self, Self::Unknown(_))
    }

    pub const fn accepts_packet(self) -> bool {
        matches!(self, Self::Ready)
    }

    pub const fn is_busy(self) -> bool {
        matches!(self, Self::Busy)
    }

    pub const fn is_power_fault(self) -> bool {
        matches!(self, Self::Power)
    }

    pub const fn is_finished(self) -> bool {
        matches!(self, Self::Finished | Self::SerialCorrectOrM3Finished)
    }
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum LihuiyuProtocolError {
    #[error("Lihuiyu payload is {actual} bytes; at most {maximum} bytes are allowed")]
    PayloadTooLong { actual: usize, maximum: usize },
    #[error("Lihuiyu packet is {actual} bytes; exactly {expected} bytes are required")]
    InvalidPacketLength { actual: usize, expected: usize },
    #[error("Lihuiyu packet header {actual:#04x} does not match {expected:#04x}")]
    InvalidHeader { actual: u8, expected: u8 },
    #[error("Lihuiyu packet checksum {actual:#04x} does not match {expected:#04x}")]
    InvalidChecksum { actual: u8, expected: u8 },
    #[error("Lihuiyu status reply must contain at least two bytes")]
    TruncatedStatus,
    #[error("Lihuiyu distance {0} steps exceeds the bounded compiler limit")]
    DistanceTooLarge(u32),
    #[error("Lihuiyu speed must be finite and greater than zero, got {0}")]
    InvalidSpeed(f64),
    #[error("Lihuiyu raster step must be between 1 and 255 mils, got {0}")]
    InvalidRasterStep(u16),
    #[error("Lihuiyu speed {speed} mm/s cannot be represented by the M2 timer")]
    UnrepresentableSpeed { speed: f64 },
}

/// Dallas/Maxim CRC-8 used by the documented LHYMicro-GL2 packet contract.
/// The reflected polynomial `0x8c` represents `x^8 + x^5 + x^4 + 1`.
pub fn crc8(bytes: &[u8]) -> u8 {
    let mut crc = 0_u8;
    for &byte in bytes {
        let mut input = byte;
        for _ in 0..8 {
            let mix = (crc ^ input) & 1;
            crc >>= 1;
            input >>= 1;
            if mix != 0 {
                crc ^= 0x8c;
            }
        }
    }
    crc
}

pub fn encode_packet(
    payload: &[u8],
    padding: LihuiyuPadding,
) -> Result<LihuiyuPacket, LihuiyuProtocolError> {
    if payload.len() > PACKET_PAYLOAD_SIZE {
        return Err(LihuiyuProtocolError::PayloadTooLong {
            actual: payload.len(),
            maximum: PACKET_PAYLOAD_SIZE,
        });
    }
    let mut packet = [0_u8; PACKET_SIZE];
    packet[0] = PACKET_HEADER;
    packet[1..=PACKET_PAYLOAD_SIZE].fill(padding.byte());
    packet[1..1 + payload.len()].copy_from_slice(payload);
    packet[PACKET_SIZE - 1] = crc8(&packet[1..=PACKET_PAYLOAD_SIZE]);
    Ok(LihuiyuPacket(packet))
}

pub fn decode_packet(packet: &[u8]) -> Result<[u8; PACKET_PAYLOAD_SIZE], LihuiyuProtocolError> {
    if packet.len() != PACKET_SIZE {
        return Err(LihuiyuProtocolError::InvalidPacketLength {
            actual: packet.len(),
            expected: PACKET_SIZE,
        });
    }
    if packet[0] != PACKET_HEADER {
        return Err(LihuiyuProtocolError::InvalidHeader {
            actual: packet[0],
            expected: PACKET_HEADER,
        });
    }
    let expected = crc8(&packet[1..=PACKET_PAYLOAD_SIZE]);
    let actual = packet[PACKET_SIZE - 1];
    if actual != expected {
        return Err(LihuiyuProtocolError::InvalidChecksum { actual, expected });
    }
    let mut payload = [0_u8; PACKET_PAYLOAD_SIZE];
    payload.copy_from_slice(&packet[1..=PACKET_PAYLOAD_SIZE]);
    Ok(payload)
}

/// Add the CH341 EPP data-write byte before each group of at most 31 bytes.
pub fn encode_epp_bulk_write(packet: &LihuiyuPacket) -> Vec<u8> {
    let mut output = Vec::with_capacity(PACKET_SIZE + 2);
    for chunk in packet.0.chunks(31) {
        output.push(CH341_EPP_DATA_WRITE);
        output.extend_from_slice(chunk);
    }
    output
}

pub const fn status_bulk_request() -> [u8; 1] {
    [CH341_STATUS_REQUEST]
}

pub fn parse_status_reply(reply: &[u8]) -> Result<LihuiyuStatus, LihuiyuProtocolError> {
    reply
        .get(1)
        .copied()
        .map(LihuiyuStatus::from_code)
        .ok_or(LihuiyuProtocolError::TruncatedStatus)
}

/// Encode a non-negative distance in the compact LHYMicro-GL step vocabulary.
pub fn encode_distance(steps: u32) -> Result<Vec<u8>, LihuiyuProtocolError> {
    if steps > MAX_DISTANCE_STEPS {
        return Err(LihuiyuProtocolError::DistanceTooLarge(steps));
    }
    let mut output = Vec::with_capacity((steps / 255) as usize + 3);
    let repeats = steps / 255;
    output.extend(std::iter::repeat_n(b'z', repeats as usize));
    let remainder = steps % 255;
    match remainder {
        0 => {}
        1..=25 => output.push(b'a' + (remainder as u8 - 1)),
        26..=51 => {
            output.push(b'|');
            output.push(b'a' + (remainder as u8 - 26));
        }
        value => output.extend_from_slice(format!("{value:03}").as_bytes()),
    }
    Ok(output)
}

pub fn encode_m2_vector_speed(speed_mm_s: f64) -> Result<Vec<u8>, LihuiyuProtocolError> {
    validate_speed(speed_mm_s)?;
    let acceleration = vector_acceleration(speed_mm_s);
    let period_ms = 25.4 / speed_mm_s;
    let speed_value = timer_value(speed_mm_s, 8.0, M2_CLOCK_SLOPE / 12.0)?;
    let step_value = speed_mm_s.floor().clamp(0.0, 127.0) as u16 + 1;
    let diagonal =
        (M2_DIAGONAL_RATIO * (M2_CLOCK_SLOPE / 12.0) * period_ms / f64::from(step_value)) as u32;
    let diagonal = diagonal.min(u32::from(u16::MAX)) as u16;
    Ok(format!(
        "CV{}{}{:03}{}C",
        encode_u16_decimal(speed_value),
        acceleration,
        step_value,
        encode_u16_decimal(diagonal)
    )
    .into_bytes())
}

pub fn encode_m2_raster_speed(
    speed_mm_s: f64,
    raster_step_mils: u16,
) -> Result<Vec<u8>, LihuiyuProtocolError> {
    validate_speed(speed_mm_s)?;
    if !(1..=255).contains(&raster_step_mils) {
        return Err(LihuiyuProtocolError::InvalidRasterStep(raster_step_mils));
    }
    let acceleration = raster_acceleration(speed_mm_s);
    let intercept = match acceleration {
        3 => 5_632.0,
        4 => 6_144.0,
        _ => 5_120.0,
    };
    let speed_value = timer_value(speed_mm_s, intercept, M2_CLOCK_SLOPE)?;
    Ok(format!(
        "V{}{}G{:03}",
        encode_u16_decimal(speed_value),
        acceleration,
        raster_step_mils
    )
    .into_bytes())
}

fn validate_speed(speed_mm_s: f64) -> Result<(), LihuiyuProtocolError> {
    if !speed_mm_s.is_finite() || speed_mm_s <= 0.0 {
        return Err(LihuiyuProtocolError::InvalidSpeed(speed_mm_s));
    }
    Ok(())
}

fn timer_value(speed_mm_s: f64, intercept: f64, slope: f64) -> Result<u16, LihuiyuProtocolError> {
    let raw = 65_536.0 - (intercept + slope * (25.4 / speed_mm_s));
    if !(0.0..=f64::from(u16::MAX)).contains(&raw) {
        return Err(LihuiyuProtocolError::UnrepresentableSpeed { speed: speed_mm_s });
    }
    Ok(raw as u16)
}

fn encode_u16_decimal(value: u16) -> String {
    format!("{:03}{:03}", value >> 8, value & 0xff)
}

fn vector_acceleration(speed_mm_s: f64) -> u8 {
    if speed_mm_s <= 25.4 {
        1
    } else if speed_mm_s <= 60.0 {
        2
    } else if speed_mm_s < 127.0 {
        3
    } else {
        4
    }
}

fn raster_acceleration(speed_mm_s: f64) -> u8 {
    if speed_mm_s <= 25.4 {
        1
    } else if speed_mm_s < 127.0 {
        2
    } else if speed_mm_s <= 320.0 {
        3
    } else {
        4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_is_narrow_and_hardware_powered() {
        let target = std::hint::black_box(LIHUIYU_M2_NANO_TARGET.clone());
        assert_eq!(target.model, ControllerModel::LihuiyuM2Nano);
        assert_eq!(target.vendor_id, 0x1a86);
        assert_eq!(target.product_id, 0x5512);
        assert_eq!(target.product_tier, ControllerProductTier::Experimental);
        assert_eq!(target.evidence_state, ControllerEvidenceState::Emulated);
        assert!(target.hardware_set_power);
        assert!(!target.capabilities.can_set_origin);
    }

    #[test]
    fn packet_framing_matches_documented_vectors() {
        let packet = encode_packet(b"I", LihuiyuPadding::AsciiF).unwrap();
        assert_eq!(packet.0[0], 0);
        assert_eq!(&packet.0[1..], b"IFFFFFFFFFFFFFFFFFFFFFFFFFFFFF\x82");
        assert_eq!(decode_packet(&packet.0).unwrap()[0], b'I');

        let packet = encode_packet(b"IPP", LihuiyuPadding::AsciiF).unwrap();
        assert_eq!(packet.0[31], 0xe4);
    }

    #[test]
    fn corrupted_packet_is_rejected() {
        let mut packet = encode_packet(b"PN", LihuiyuPadding::AsciiF).unwrap();
        packet.0[4] ^= 0x01;
        assert!(matches!(
            decode_packet(&packet.0),
            Err(LihuiyuProtocolError::InvalidChecksum { .. })
        ));
    }

    #[test]
    fn epp_write_and_status_contract_are_exact() {
        let packet = encode_packet(b"I", LihuiyuPadding::AsciiF).unwrap();
        let encoded = encode_epp_bulk_write(&packet);
        assert_eq!(encoded.len(), 34);
        assert_eq!(encoded[0], 0xa6);
        assert_eq!(&encoded[1..32], &packet.0[..31]);
        assert_eq!(encoded[32], 0xa6);
        assert_eq!(encoded[33], packet.0[31]);
        assert_eq!(status_bulk_request(), [0xa0]);
        assert_eq!(
            parse_status_reply(&[0, 0xce]).unwrap(),
            LihuiyuStatus::Ready
        );
        assert_eq!(
            parse_status_reply(&[0, 0xec]).unwrap(),
            LihuiyuStatus::Finished
        );
        assert!(LihuiyuStatus::Busy.is_busy());
        assert!(!LihuiyuStatus::Power.is_busy());
        assert!(LihuiyuStatus::Power.is_power_fault());
    }

    #[test]
    fn compact_distances_match_the_published_vocabulary() {
        for (steps, expected) in [
            (0, ""),
            (1, "a"),
            (25, "y"),
            (26, "|a"),
            (51, "|z"),
            (52, "052"),
            (254, "254"),
            (255, "z"),
            (256, "za"),
            (510, "zz"),
        ] {
            assert_eq!(encode_distance(steps).unwrap(), expected.as_bytes());
        }
    }

    #[test]
    fn m2_speed_codes_match_independent_meerk40t_goldens() {
        assert_eq!(
            encode_m2_vector_speed(15.0).unwrap(),
            b"CV2490731016000027C"
        );
        assert_eq!(encode_m2_raster_speed(15.0, 2).unwrap(), b"V1552121G002");
    }

    #[test]
    fn speed_and_distance_bounds_fail_closed() {
        assert!(matches!(
            encode_m2_vector_speed(0.0),
            Err(LihuiyuProtocolError::InvalidSpeed(_))
        ));
        assert!(matches!(
            encode_m2_raster_speed(15.0, 0),
            Err(LihuiyuProtocolError::InvalidRasterStep(0))
        ));
        assert!(matches!(
            encode_distance(MAX_DISTANCE_STEPS + 1),
            Err(LihuiyuProtocolError::DistanceTooLarge(_))
        ));
    }
}
