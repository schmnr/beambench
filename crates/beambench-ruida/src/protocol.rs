use thiserror::Error;

pub const RUIDA_UDP_PORT: u16 = 50_200;
pub const MAX_UDP_DATAGRAM_SIZE: usize = 1_472;
pub const MAX_UDP_PAYLOAD_SIZE: usize = MAX_UDP_DATAGRAM_SIZE - 2;
pub const DEFAULT_MAGIC: u8 = 0x88;

pub const ACK: u8 = 0xCC;
pub const NAK: u8 = 0xCF;
pub const ERR: u8 = 0xCD;
pub const ENQ: u8 = 0xCE;

pub const MEMORY_CARD_ID: u16 = 0x057E;
pub const MEMORY_MACHINE_STATUS: u16 = 0x0400;
pub const MEMORY_FILE_COUNT: u16 = 0x0405;
pub const RDC6442S_CARD_ID: u64 = 0x6510_6510;
pub const MAX_CONTROLLER_FILES: u16 = 99;
pub const MAX_CONTROLLER_FILENAME_BYTES: usize = 8;
pub const MACHINE_STATUS_MOVING: u64 = 0x0100_0000;
pub const MACHINE_STATUS_PART_END: u64 = 0x0000_0002;
pub const MACHINE_STATUS_JOB_RUNNING: u64 = 0x0000_0001;
pub const KNOWN_MACHINE_STATUS_BITS: u64 =
    MACHINE_STATUS_MOVING | MACHINE_STATUS_PART_END | MACHINE_STATUS_JOB_RUNNING;

const GET_SETTING: [u8; 2] = [0xDA, 0x00];
const SETTING_REPLY: [u8; 2] = [0xDA, 0x01];
const DELETE_DOCUMENT: [u8; 2] = [0xE8, 0x00];
const DOCUMENT_NAME: [u8; 2] = [0xE8, 0x01];
const FILE_TRANSFER: [u8; 2] = [0xE8, 0x02];
const SELECT_DOCUMENT: [u8; 2] = [0xE8, 0x03];
const SET_FILENAME: [u8; 2] = [0xE7, 0x01];
const PROCESS_CONTROL: u8 = 0xD8;
const HOME_XY: [u8; 2] = [0xD8, 0x2A];
const JOG_SPEED: [u8; 2] = [0xC9, 0x02];
const RAPID_MOVE: u8 = 0xD9;
const RAPID_OPTION_NO_OUTPUT_RELATIVE: u8 = 0x02;
const U14_MAX: u16 = 0x3FFF;
const U35_MAX: u64 = 0x07_FFFF_FFFF;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RuidaProtocolError {
    #[error("Ruida payload must not be empty")]
    EmptyPayload,
    #[error("Ruida UDP payload is {actual} bytes; maximum is {maximum}")]
    PayloadTooLarge { actual: usize, maximum: usize },
    #[error("Ruida UDP datagram must contain a two-byte checksum and a payload")]
    DatagramTooShort,
    #[error("Ruida UDP checksum mismatch: expected {expected:#06x}, calculated {actual:#06x}")]
    ChecksumMismatch { expected: u16, actual: u16 },
    #[error("Ruida reply must not be empty")]
    EmptyReply,
    #[error("Ruida {kind} value {value} exceeds {maximum}")]
    ValueOutOfRange {
        kind: &'static str,
        value: u64,
        maximum: u64,
    },
    #[error("Ruida {kind} value must be finite and within the supported range")]
    InvalidNumericValue { kind: &'static str },
    #[error("Ruida {kind} value requires {expected} bytes, received {actual}")]
    InvalidValueLength {
        kind: &'static str,
        expected: usize,
        actual: usize,
    },
    #[error("Ruida memory reply has an invalid command header")]
    InvalidMemoryReply,
    #[error("Ruida controller file index {index} must be between 1 and {maximum}")]
    InvalidFileIndex { index: u16, maximum: u16 },
    #[error(
        "Ruida upload filename must contain 3-8 ASCII letters, digits, underscores, or hyphens and start with BB"
    )]
    InvalidUploadFilename,
    #[error("Ruida document-name reply has an invalid command header or terminator")]
    InvalidDocumentNameReply,
    #[error("Ruida document-name reply contains invalid text")]
    InvalidDocumentNameText,
    #[error("Ruida controller-storage command is malformed")]
    InvalidStorageCommand,
    #[error("Ruida process-control command is malformed")]
    InvalidProcessCommand,
    #[error("Ruida manual-motion command is malformed")]
    InvalidManualMotionCommand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuidaProcessAction {
    Start,
    Stop,
    Pause,
    Resume,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuidaJogAxis {
    X,
    Y,
    Z,
    U,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuidaManualMotionCommand {
    HomeXy,
    SetSpeed {
        micrometres_per_second: u64,
    },
    MoveRelative {
        axis: RuidaJogAxis,
        micrometres: i32,
    },
}

/// UDP packet codec for one Ruida swizzle key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuidaCodec {
    magic: u8,
}

impl Default for RuidaCodec {
    fn default() -> Self {
        Self::new(DEFAULT_MAGIC)
    }
}

impl RuidaCodec {
    pub const fn new(magic: u8) -> Self {
        Self { magic }
    }

    pub const fn magic(self) -> u8 {
        self.magic
    }

    pub fn swizzle_byte(self, byte: u8) -> u8 {
        let mut value = byte;
        value ^= value >> 7;
        value ^= value << 7;
        value ^= value >> 7;
        value ^= self.magic;
        value.wrapping_add(1)
    }

    pub fn unswizzle_byte(self, byte: u8) -> u8 {
        let mut value = byte.wrapping_sub(1);
        value ^= self.magic;
        value ^= value >> 7;
        value ^= value << 7;
        value ^= value >> 7;
        value
    }

    pub fn swizzle(self, bytes: &[u8]) -> Vec<u8> {
        bytes
            .iter()
            .copied()
            .map(|byte| self.swizzle_byte(byte))
            .collect()
    }

    pub fn unswizzle(self, bytes: &[u8]) -> Vec<u8> {
        bytes
            .iter()
            .copied()
            .map(|byte| self.unswizzle_byte(byte))
            .collect()
    }

    /// Encode a controller-bound UDP datagram.
    ///
    /// The checksum is the big-endian wrapping sum of the swizzled payload.
    pub fn encode_datagram(self, clear_payload: &[u8]) -> Result<Vec<u8>, RuidaProtocolError> {
        if clear_payload.is_empty() {
            return Err(RuidaProtocolError::EmptyPayload);
        }
        if clear_payload.len() > MAX_UDP_PAYLOAD_SIZE {
            return Err(RuidaProtocolError::PayloadTooLarge {
                actual: clear_payload.len(),
                maximum: MAX_UDP_PAYLOAD_SIZE,
            });
        }
        let payload = self.swizzle(clear_payload);
        let checksum = payload
            .iter()
            .fold(0_u16, |sum, byte| sum.wrapping_add(u16::from(*byte)));
        let mut datagram = Vec::with_capacity(payload.len() + 2);
        datagram.extend_from_slice(&checksum.to_be_bytes());
        datagram.extend_from_slice(&payload);
        Ok(datagram)
    }

    /// Verify and decode one controller-bound UDP datagram.
    pub fn decode_datagram(self, datagram: &[u8]) -> Result<Vec<u8>, RuidaProtocolError> {
        if datagram.len() < 3 {
            return Err(RuidaProtocolError::DatagramTooShort);
        }
        if datagram.len() > MAX_UDP_DATAGRAM_SIZE {
            return Err(RuidaProtocolError::PayloadTooLarge {
                actual: datagram.len() - 2,
                maximum: MAX_UDP_PAYLOAD_SIZE,
            });
        }
        let expected = u16::from_be_bytes([datagram[0], datagram[1]]);
        let payload = &datagram[2..];
        let actual = payload
            .iter()
            .fold(0_u16, |sum, byte| sum.wrapping_add(u16::from(*byte)));
        if expected != actual {
            return Err(RuidaProtocolError::ChecksumMismatch { expected, actual });
        }
        Ok(self.unswizzle(payload))
    }

    /// Encode a controller reply. Ruida UDP replies are swizzled but do not
    /// carry the two-byte host-to-controller checksum prefix.
    pub fn encode_reply(self, clear_reply: &[u8]) -> Result<Vec<u8>, RuidaProtocolError> {
        if clear_reply.is_empty() {
            return Err(RuidaProtocolError::EmptyReply);
        }
        Ok(self.swizzle(clear_reply))
    }

    pub fn decode_reply(self, wire_reply: &[u8]) -> Result<Vec<u8>, RuidaProtocolError> {
        if wire_reply.is_empty() {
            return Err(RuidaProtocolError::EmptyReply);
        }
        Ok(self.unswizzle(wire_reply))
    }
}

pub fn encode_u14(value: u16) -> Result<[u8; 2], RuidaProtocolError> {
    if value > U14_MAX {
        return Err(RuidaProtocolError::ValueOutOfRange {
            kind: "14-bit",
            value: u64::from(value),
            maximum: u64::from(U14_MAX),
        });
    }
    Ok([((value >> 7) & 0x7F) as u8, (value & 0x7F) as u8])
}

pub fn decode_u14(bytes: &[u8]) -> Result<u16, RuidaProtocolError> {
    if bytes.len() != 2 {
        return Err(RuidaProtocolError::InvalidValueLength {
            kind: "14-bit",
            expected: 2,
            actual: bytes.len(),
        });
    }
    Ok((u16::from(bytes[0] & 0x7F) << 7) | u16::from(bytes[1] & 0x7F))
}

pub fn encode_i14(value: i16) -> Result<[u8; 2], RuidaProtocolError> {
    if !(-8_192..=8_191).contains(&value) {
        return Err(RuidaProtocolError::InvalidNumericValue {
            kind: "signed 14-bit",
        });
    }
    encode_u14((i32::from(value) & i32::from(U14_MAX)) as u16)
}

pub fn decode_i14(bytes: &[u8]) -> Result<i16, RuidaProtocolError> {
    let value = decode_u14(bytes)?;
    if value > 0x1FFF {
        Ok((i32::from(value) - 0x4000) as i16)
    } else {
        Ok(value as i16)
    }
}

pub fn encode_u35(value: u64) -> Result<[u8; 5], RuidaProtocolError> {
    if value > U35_MAX {
        return Err(RuidaProtocolError::ValueOutOfRange {
            kind: "35-bit",
            value,
            maximum: U35_MAX,
        });
    }
    Ok([
        ((value >> 28) & 0x7F) as u8,
        ((value >> 21) & 0x7F) as u8,
        ((value >> 14) & 0x7F) as u8,
        ((value >> 7) & 0x7F) as u8,
        (value & 0x7F) as u8,
    ])
}

pub fn decode_u35(bytes: &[u8]) -> Result<u64, RuidaProtocolError> {
    if bytes.len() != 5 {
        return Err(RuidaProtocolError::InvalidValueLength {
            kind: "35-bit",
            expected: 5,
            actual: bytes.len(),
        });
    }
    Ok((u64::from(bytes[0] & 0x7F) << 28)
        | (u64::from(bytes[1] & 0x7F) << 21)
        | (u64::from(bytes[2] & 0x7F) << 14)
        | (u64::from(bytes[3] & 0x7F) << 7)
        | u64::from(bytes[4] & 0x7F))
}

pub fn encode_i32(value: i32) -> [u8; 5] {
    let encoded = (i64::from(value) & U35_MAX as i64) as u64;
    encode_u35(encoded).expect("masked signed coordinate always fits in 35 bits")
}

pub fn decode_i32(bytes: &[u8]) -> Result<i32, RuidaProtocolError> {
    Ok(decode_u35(bytes)? as u32 as i32)
}

pub fn encode_power_percent(percent: f64) -> Result<[u8; 2], RuidaProtocolError> {
    if !percent.is_finite() || !(0.0..=100.0).contains(&percent) {
        return Err(RuidaProtocolError::InvalidNumericValue { kind: "power" });
    }
    encode_u14((percent * f64::from(U14_MAX) / 100.0).floor() as u16)
}

pub fn decode_power_percent(bytes: &[u8]) -> Result<f64, RuidaProtocolError> {
    Ok(f64::from(decode_u14(bytes)?) * 100.0 / f64::from(U14_MAX))
}

pub fn encode_speed_mm_s(speed_mm_s: f64) -> Result<[u8; 5], RuidaProtocolError> {
    if !speed_mm_s.is_finite() || speed_mm_s < 0.0 {
        return Err(RuidaProtocolError::InvalidNumericValue { kind: "speed" });
    }
    let micrometres_per_second = speed_mm_s * 1_000.0;
    if micrometres_per_second > U35_MAX as f64 {
        return Err(RuidaProtocolError::InvalidNumericValue { kind: "speed" });
    }
    encode_u35(micrometres_per_second.floor() as u64)
}

pub const fn enquiry_command() -> [u8; 1] {
    [ENQ]
}

pub const fn memory_read_command(address: u16) -> [u8; 4] {
    [
        GET_SETTING[0],
        GET_SETTING[1],
        (address >> 8) as u8,
        address as u8,
    ]
}

pub fn memory_reply(address: u16, value: u64) -> Result<[u8; 9], RuidaProtocolError> {
    let encoded = encode_u35(value)?;
    Ok([
        SETTING_REPLY[0],
        SETTING_REPLY[1],
        (address >> 8) as u8,
        address as u8,
        encoded[0],
        encoded[1],
        encoded[2],
        encoded[3],
        encoded[4],
    ])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuidaMemoryReply {
    pub address: u16,
    pub value: u64,
}

pub fn parse_memory_reply(bytes: &[u8]) -> Result<RuidaMemoryReply, RuidaProtocolError> {
    if bytes.len() != 9 {
        return Err(RuidaProtocolError::InvalidValueLength {
            kind: "memory reply",
            expected: 9,
            actual: bytes.len(),
        });
    }
    if bytes[..2] != SETTING_REPLY {
        return Err(RuidaProtocolError::InvalidMemoryReply);
    }
    Ok(RuidaMemoryReply {
        address: u16::from_be_bytes([bytes[2], bytes[3]]),
        value: decode_u35(&bytes[4..])?,
    })
}

pub fn normalize_upload_filename(filename: &str) -> Result<String, RuidaProtocolError> {
    let normalized = filename.to_ascii_uppercase();
    if normalized.len() < 3
        || normalized.len() > MAX_CONTROLLER_FILENAME_BYTES
        || !normalized.starts_with("BB")
        || !normalized
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(RuidaProtocolError::InvalidUploadFilename);
    }
    Ok(normalized)
}

pub fn file_transfer_command(filename: &str) -> Result<Vec<u8>, RuidaProtocolError> {
    let filename = normalize_upload_filename(filename)?;
    let mut command = Vec::with_capacity(SET_FILENAME.len() + filename.len() + 3);
    command.extend_from_slice(&FILE_TRANSFER);
    command.extend_from_slice(&SET_FILENAME);
    command.extend_from_slice(filename.as_bytes());
    command.push(0);
    Ok(command)
}

pub fn parse_file_transfer_command(bytes: &[u8]) -> Result<String, RuidaProtocolError> {
    if bytes.len() < 8
        || bytes[..2] != FILE_TRANSFER
        || bytes[2..4] != SET_FILENAME
        || bytes.last() != Some(&0)
    {
        return Err(RuidaProtocolError::InvalidStorageCommand);
    }
    let name = std::str::from_utf8(&bytes[4..bytes.len() - 1])
        .map_err(|_| RuidaProtocolError::InvalidUploadFilename)?;
    let normalized = normalize_upload_filename(name)?;
    if normalized.as_bytes() != name.as_bytes() {
        return Err(RuidaProtocolError::InvalidUploadFilename);
    }
    Ok(normalized)
}

pub fn document_name_command(index: u16) -> Result<[u8; 4], RuidaProtocolError> {
    validate_file_index(index)?;
    let index = encode_u14(index)?;
    Ok([DOCUMENT_NAME[0], DOCUMENT_NAME[1], index[0], index[1]])
}

pub fn parse_document_name_reply(bytes: &[u8]) -> Result<(u16, String), RuidaProtocolError> {
    if bytes.len() < 5 || bytes[..2] != DOCUMENT_NAME || bytes.last() != Some(&0) {
        return Err(RuidaProtocolError::InvalidDocumentNameReply);
    }
    let index = decode_u14(&bytes[2..4])?;
    validate_file_index(index)?;
    let name = &bytes[4..bytes.len() - 1];
    if name.is_empty()
        || name.len() > MAX_CONTROLLER_FILENAME_BYTES
        || !name.iter().all(u8::is_ascii_graphic)
    {
        return Err(RuidaProtocolError::InvalidDocumentNameText);
    }
    Ok((
        index,
        String::from_utf8(name.to_vec())
            .map_err(|_| RuidaProtocolError::InvalidDocumentNameText)?,
    ))
}

pub fn document_name_reply(index: u16, name: &str) -> Result<Vec<u8>, RuidaProtocolError> {
    validate_file_index(index)?;
    if name.is_empty()
        || name.len() > MAX_CONTROLLER_FILENAME_BYTES
        || !name.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(RuidaProtocolError::InvalidDocumentNameText);
    }
    let index = encode_u14(index)?;
    let mut reply = Vec::with_capacity(name.len() + 5);
    reply.extend_from_slice(&DOCUMENT_NAME);
    reply.extend_from_slice(&index);
    reply.extend_from_slice(name.as_bytes());
    reply.push(0);
    Ok(reply)
}

pub fn delete_document_command(index: u16) -> Result<[u8; 6], RuidaProtocolError> {
    validate_file_index(index)?;
    let index = encode_u14(index)?;
    Ok([
        DELETE_DOCUMENT[0],
        DELETE_DOCUMENT[1],
        index[0],
        index[1],
        index[0],
        index[1],
    ])
}

pub fn parse_delete_document_command(bytes: &[u8]) -> Result<u16, RuidaProtocolError> {
    if bytes.len() != 6 || bytes[..2] != DELETE_DOCUMENT || bytes[2..4] != bytes[4..6] {
        return Err(RuidaProtocolError::InvalidStorageCommand);
    }
    let index = decode_u14(&bytes[2..4])?;
    validate_file_index(index)?;
    Ok(index)
}

pub fn select_document_command(index: u16) -> Result<[u8; 4], RuidaProtocolError> {
    validate_file_index(index)?;
    let index = encode_u14(index)?;
    Ok([SELECT_DOCUMENT[0], SELECT_DOCUMENT[1], index[0], index[1]])
}

pub fn parse_select_document_command(bytes: &[u8]) -> Result<u16, RuidaProtocolError> {
    if bytes.len() != 4 || bytes[..2] != SELECT_DOCUMENT {
        return Err(RuidaProtocolError::InvalidStorageCommand);
    }
    let index = decode_u14(&bytes[2..4])?;
    validate_file_index(index)?;
    Ok(index)
}

pub const fn process_control_command(action: RuidaProcessAction) -> [u8; 2] {
    let operation = match action {
        RuidaProcessAction::Start => 0x00,
        RuidaProcessAction::Stop => 0x01,
        RuidaProcessAction::Pause => 0x02,
        RuidaProcessAction::Resume => 0x03,
    };
    [PROCESS_CONTROL, operation]
}

pub fn parse_process_control_command(
    bytes: &[u8],
) -> Result<RuidaProcessAction, RuidaProtocolError> {
    match bytes {
        [PROCESS_CONTROL, 0x00] => Ok(RuidaProcessAction::Start),
        [PROCESS_CONTROL, 0x01] => Ok(RuidaProcessAction::Stop),
        [PROCESS_CONTROL, 0x02] => Ok(RuidaProcessAction::Pause),
        [PROCESS_CONTROL, 0x03] => Ok(RuidaProcessAction::Resume),
        _ => Err(RuidaProtocolError::InvalidProcessCommand),
    }
}

pub const fn home_xy_command() -> [u8; 2] {
    HOME_XY
}

pub fn jog_speed_command(speed_mm_min: f64) -> Result<[u8; 7], RuidaProtocolError> {
    if !speed_mm_min.is_finite() || speed_mm_min <= 0.0 || speed_mm_min / 60.0 * 1_000.0 < 1.0 {
        return Err(RuidaProtocolError::InvalidNumericValue { kind: "jog speed" });
    }
    let speed = encode_speed_mm_s(speed_mm_min / 60.0)?;
    Ok([
        JOG_SPEED[0],
        JOG_SPEED[1],
        speed[0],
        speed[1],
        speed[2],
        speed[3],
        speed[4],
    ])
}

pub fn relative_jog_command(
    axis: RuidaJogAxis,
    delta_mm: f64,
) -> Result<[u8; 8], RuidaProtocolError> {
    let micrometres = delta_mm * 1_000.0;
    let rounded_micrometres = micrometres.round();
    if !delta_mm.is_finite()
        || delta_mm == 0.0
        || rounded_micrometres == 0.0
        || rounded_micrometres < f64::from(i32::MIN)
        || rounded_micrometres > f64::from(i32::MAX)
    {
        return Err(RuidaProtocolError::InvalidNumericValue {
            kind: "jog distance",
        });
    }
    let coordinate = encode_i32(rounded_micrometres as i32);
    Ok([
        RAPID_MOVE,
        match axis {
            RuidaJogAxis::X => 0x00,
            RuidaJogAxis::Y => 0x01,
            RuidaJogAxis::Z => 0x02,
            RuidaJogAxis::U => 0x03,
        },
        RAPID_OPTION_NO_OUTPUT_RELATIVE,
        coordinate[0],
        coordinate[1],
        coordinate[2],
        coordinate[3],
        coordinate[4],
    ])
}

pub fn parse_manual_motion_command(
    bytes: &[u8],
) -> Result<RuidaManualMotionCommand, RuidaProtocolError> {
    if bytes == HOME_XY {
        return Ok(RuidaManualMotionCommand::HomeXy);
    }
    if bytes.len() == 7 && bytes[..2] == JOG_SPEED {
        return Ok(RuidaManualMotionCommand::SetSpeed {
            micrometres_per_second: decode_u35(&bytes[2..])?,
        });
    }
    if bytes.len() == 8 && bytes[0] == RAPID_MOVE && bytes[2] == RAPID_OPTION_NO_OUTPUT_RELATIVE {
        let axis = match bytes[1] {
            0x00 => RuidaJogAxis::X,
            0x01 => RuidaJogAxis::Y,
            0x02 => RuidaJogAxis::Z,
            0x03 => RuidaJogAxis::U,
            _ => return Err(RuidaProtocolError::InvalidManualMotionCommand),
        };
        return Ok(RuidaManualMotionCommand::MoveRelative {
            axis,
            micrometres: decode_i32(&bytes[3..])?,
        });
    }
    Err(RuidaProtocolError::InvalidManualMotionCommand)
}

fn validate_file_index(index: u16) -> Result<(), RuidaProtocolError> {
    if !(1..=MAX_CONTROLLER_FILES).contains(&index) {
        return Err(RuidaProtocolError::InvalidFileIndex {
            index,
            maximum: MAX_CONTROLLER_FILES,
        });
    }
    Ok(())
}
