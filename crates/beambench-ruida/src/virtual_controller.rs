use crate::protocol::{
    ACK, DEFAULT_MAGIC, ENQ, ERR, MACHINE_STATUS_JOB_RUNNING, MACHINE_STATUS_MOVING,
    MACHINE_STATUS_PART_END, MAX_CONTROLLER_FILES, MEMORY_CARD_ID, MEMORY_FILE_COUNT,
    MEMORY_MACHINE_STATUS, NAK, RDC6442S_CARD_ID, RUIDA_UDP_PORT, RuidaCodec, RuidaJogAxis,
    RuidaManualMotionCommand, RuidaProcessAction, RuidaProtocolError, document_name_reply,
    memory_reply, parse_delete_document_command, parse_file_transfer_command,
    parse_manual_motion_command, parse_process_control_command, parse_select_document_command,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuidaCompatibilityTarget {
    pub model: &'static str,
    pub card_id: u64,
    pub transport: &'static str,
    pub port: u16,
    pub magic: u8,
}

pub const RDC6442S_ETHERNET_TARGET: RuidaCompatibilityTarget = RuidaCompatibilityTarget {
    model: "RDC6442S",
    card_id: RDC6442S_CARD_ID,
    transport: "ethernet_udp",
    port: RUIDA_UDP_PORT,
    magic: DEFAULT_MAGIC,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuidaVirtualResponse {
    /// Controller-to-host UDP payloads. These are swizzled and intentionally
    /// omit the checksum prefix used in the opposite direction.
    pub datagrams: Vec<Vec<u8>>,
    pub accepted: bool,
    pub protocol_error: Option<RuidaProtocolError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuidaVirtualFile {
    pub name: String,
    pub clear_bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RuidaVirtualExecutionState {
    #[default]
    Idle,
    Running,
    Paused,
    PartEnd,
    ManualMotion,
}

#[derive(Debug, Clone)]
struct PendingUpload {
    name: String,
    clear_bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct RuidaVirtualController {
    codec: RuidaCodec,
    target: RuidaCompatibilityTarget,
    machine_status: u64,
    files: Vec<RuidaVirtualFile>,
    pending_upload: Option<PendingUpload>,
    selected_filename: Option<String>,
    execution_state: RuidaVirtualExecutionState,
    position_micrometres: (i32, i32),
    table_position_micrometres: (i32, i32),
    jog_speed_micrometres_per_second: Option<u64>,
}

impl Default for RuidaVirtualController {
    fn default() -> Self {
        Self::rdc6442s()
    }
}

impl RuidaVirtualController {
    pub fn rdc6442s() -> Self {
        Self {
            codec: RuidaCodec::new(RDC6442S_ETHERNET_TARGET.magic),
            target: RDC6442S_ETHERNET_TARGET,
            machine_status: 0,
            files: Vec::new(),
            pending_upload: None,
            selected_filename: None,
            execution_state: RuidaVirtualExecutionState::Idle,
            position_micrometres: (0, 0),
            table_position_micrometres: (0, 0),
            jog_speed_micrometres_per_second: None,
        }
    }

    pub const fn target(&self) -> RuidaCompatibilityTarget {
        self.target
    }

    pub const fn machine_status(&self) -> u64 {
        self.machine_status
    }

    pub fn set_machine_status(&mut self, machine_status: u64) {
        self.machine_status = machine_status;
        self.execution_state = if machine_status & MACHINE_STATUS_PART_END != 0 {
            RuidaVirtualExecutionState::PartEnd
        } else if machine_status & MACHINE_STATUS_JOB_RUNNING != 0
            && machine_status & MACHINE_STATUS_MOVING == 0
        {
            RuidaVirtualExecutionState::Paused
        } else if machine_status & MACHINE_STATUS_JOB_RUNNING != 0 {
            RuidaVirtualExecutionState::Running
        } else if machine_status & MACHINE_STATUS_MOVING != 0 {
            RuidaVirtualExecutionState::ManualMotion
        } else {
            RuidaVirtualExecutionState::Idle
        };
    }

    pub fn files(&self) -> &[RuidaVirtualFile] {
        &self.files
    }

    pub fn file(&self, name: &str) -> Option<&RuidaVirtualFile> {
        self.files.iter().find(|file| file.name == name)
    }

    pub fn has_pending_upload(&self) -> bool {
        self.pending_upload.is_some()
    }

    pub const fn execution_state(&self) -> RuidaVirtualExecutionState {
        self.execution_state
    }

    pub fn selected_file(&self) -> Option<&RuidaVirtualFile> {
        let filename = self.selected_filename.as_deref()?;
        self.file(filename)
    }

    pub const fn output_active(&self) -> bool {
        matches!(self.execution_state, RuidaVirtualExecutionState::Running)
    }

    pub const fn position_micrometres(&self) -> (i32, i32) {
        self.position_micrometres
    }

    /// Simulated (Z, U) positions used to verify lift-table commands.
    pub const fn table_position_micrometres(&self) -> (i32, i32) {
        self.table_position_micrometres
    }

    pub const fn jog_speed_micrometres_per_second(&self) -> Option<u64> {
        self.jog_speed_micrometres_per_second
    }

    pub fn complete_execution(&mut self) -> bool {
        if !matches!(
            self.execution_state,
            RuidaVirtualExecutionState::Running | RuidaVirtualExecutionState::Paused
        ) {
            return false;
        }
        self.execution_state = RuidaVirtualExecutionState::PartEnd;
        self.machine_status = MACHINE_STATUS_PART_END;
        true
    }

    pub fn settle_completion(&mut self) -> bool {
        if self.execution_state != RuidaVirtualExecutionState::PartEnd {
            return false;
        }
        self.execution_state = RuidaVirtualExecutionState::Idle;
        self.machine_status = 0;
        true
    }

    pub fn settle_manual_motion(&mut self) -> bool {
        if self.execution_state != RuidaVirtualExecutionState::ManualMotion {
            return false;
        }
        self.execution_state = RuidaVirtualExecutionState::Idle;
        self.machine_status = 0;
        true
    }

    /// Simulate a controller/network reset. Completed files remain stored;
    /// an incomplete transfer is discarded rather than exposed as a file.
    pub fn reset_connection(&mut self) {
        self.pending_upload = None;
    }

    pub fn reset_controller(&mut self) {
        self.pending_upload = None;
        self.selected_filename = None;
        self.execution_state = RuidaVirtualExecutionState::Idle;
        self.machine_status = 0;
        self.position_micrometres = (0, 0);
        self.table_position_micrometres = (0, 0);
        self.jog_speed_micrometres_per_second = None;
    }

    pub fn receive_datagram(&mut self, datagram: &[u8]) -> RuidaVirtualResponse {
        let clear = match self.codec.decode_datagram(datagram) {
            Ok(clear) => clear,
            Err(error @ RuidaProtocolError::ChecksumMismatch { .. }) => {
                return RuidaVirtualResponse {
                    datagrams: vec![self.reply_byte(NAK)],
                    accepted: false,
                    protocol_error: Some(error),
                };
            }
            Err(error) => {
                return RuidaVirtualResponse {
                    datagrams: vec![self.reply_byte(ERR)],
                    accepted: false,
                    protocol_error: Some(error),
                };
            }
        };

        if self.pending_upload.is_some() {
            return self.receive_upload_chunk(clear);
        }

        let mut replies = vec![self.reply_byte(ACK)];
        match clear.as_slice() {
            [ENQ] => {}
            [0xDA, 0x00, 0x05, 0x7E] => {
                replies.push(self.reply_memory(MEMORY_CARD_ID, self.target.card_id))
            }
            [0xDA, 0x00, 0x04, 0x00] => {
                replies.push(self.reply_memory(MEMORY_MACHINE_STATUS, self.machine_status))
            }
            [0xDA, 0x00, 0x04, 0x05] => {
                replies.push(self.reply_memory(MEMORY_FILE_COUNT, self.files.len() as u64))
            }
            _ => {
                if let Ok(filename) = parse_file_transfer_command(&clear) {
                    if self.files.len() >= usize::from(MAX_CONTROLLER_FILES)
                        || self.files.iter().any(|file| file.name == filename)
                    {
                        return self.error_response();
                    }
                    self.pending_upload = Some(PendingUpload {
                        name: filename,
                        clear_bytes: Vec::new(),
                    });
                } else if clear.starts_with(&[0xE8, 0x01]) {
                    let Ok(index) =
                        crate::protocol::decode_u14(clear.get(2..4).unwrap_or_default())
                    else {
                        return self.error_response();
                    };
                    let Some(file) = index
                        .checked_sub(1)
                        .and_then(|index| self.files.get(usize::from(index)))
                    else {
                        return self.error_response();
                    };
                    let Ok(reply) = document_name_reply(index, &file.name) else {
                        return self.error_response();
                    };
                    replies.push(self.reply_clear(&reply));
                } else if clear.starts_with(&[0xE8, 0x00]) {
                    let Ok(index) = parse_delete_document_command(&clear) else {
                        return self.error_response();
                    };
                    let offset = usize::from(index - 1);
                    if offset >= self.files.len() {
                        return self.error_response();
                    }
                    let removed = self.files.remove(offset);
                    if self.selected_filename.as_deref() == Some(removed.name.as_str()) {
                        self.selected_filename = None;
                    }
                } else if clear.starts_with(&[0xE8, 0x03]) {
                    let Ok(index) = parse_select_document_command(&clear) else {
                        return self.error_response();
                    };
                    let Some(file) = index
                        .checked_sub(1)
                        .and_then(|index| self.files.get(usize::from(index)))
                    else {
                        return self.error_response();
                    };
                    self.selected_filename = Some(file.name.clone());
                } else if matches!(clear.first(), Some(0xC9) | Some(0xD9))
                    || clear.as_slice() == [0xD8, 0x2A]
                {
                    let Ok(command) = parse_manual_motion_command(&clear) else {
                        return self.error_response();
                    };
                    if !self.apply_manual_motion_command(command) {
                        return self.error_response();
                    }
                } else if clear.first() == Some(&0xD8) {
                    let Ok(action) = parse_process_control_command(&clear) else {
                        return self.error_response();
                    };
                    if !self.apply_process_action(action) {
                        return self.error_response();
                    }
                } else {
                    return self.error_response();
                }
            }
        }

        RuidaVirtualResponse {
            datagrams: replies,
            accepted: true,
            protocol_error: None,
        }
    }

    fn receive_upload_chunk(&mut self, clear: Vec<u8>) -> RuidaVirtualResponse {
        let complete = clear.last() == Some(&0xD7);
        let pending = self
            .pending_upload
            .as_mut()
            .expect("upload chunk is handled only while a transfer is pending");
        pending.clear_bytes.extend_from_slice(&clear);
        if complete {
            let pending = self
                .pending_upload
                .take()
                .expect("completed transfer remains pending until committed");
            self.files.push(RuidaVirtualFile {
                name: pending.name,
                clear_bytes: pending.clear_bytes,
            });
        }
        RuidaVirtualResponse {
            datagrams: vec![self.reply_byte(ACK)],
            accepted: true,
            protocol_error: None,
        }
    }

    fn apply_process_action(&mut self, action: RuidaProcessAction) -> bool {
        match action {
            RuidaProcessAction::Start
                if self.execution_state == RuidaVirtualExecutionState::Idle
                    && self.selected_file().is_some() =>
            {
                self.execution_state = RuidaVirtualExecutionState::Running;
                self.machine_status = MACHINE_STATUS_MOVING | MACHINE_STATUS_JOB_RUNNING;
                true
            }
            RuidaProcessAction::Stop => {
                self.execution_state = RuidaVirtualExecutionState::Idle;
                self.machine_status = 0;
                true
            }
            RuidaProcessAction::Pause
                if self.execution_state == RuidaVirtualExecutionState::Running =>
            {
                self.execution_state = RuidaVirtualExecutionState::Paused;
                self.machine_status = MACHINE_STATUS_JOB_RUNNING;
                true
            }
            RuidaProcessAction::Resume
                if self.execution_state == RuidaVirtualExecutionState::Paused =>
            {
                self.execution_state = RuidaVirtualExecutionState::Running;
                self.machine_status = MACHINE_STATUS_MOVING | MACHINE_STATUS_JOB_RUNNING;
                true
            }
            _ => false,
        }
    }

    fn apply_manual_motion_command(&mut self, command: RuidaManualMotionCommand) -> bool {
        match command {
            RuidaManualMotionCommand::SetSpeed {
                micrometres_per_second,
            } if self.execution_state == RuidaVirtualExecutionState::Idle
                && micrometres_per_second > 0 =>
            {
                self.jog_speed_micrometres_per_second = Some(micrometres_per_second);
                true
            }
            RuidaManualMotionCommand::HomeXy
                if self.execution_state == RuidaVirtualExecutionState::Idle =>
            {
                self.position_micrometres = (0, 0);
                self.execution_state = RuidaVirtualExecutionState::ManualMotion;
                self.machine_status = MACHINE_STATUS_MOVING;
                true
            }
            RuidaManualMotionCommand::MoveRelative { axis, micrometres }
                if matches!(
                    self.execution_state,
                    RuidaVirtualExecutionState::Idle | RuidaVirtualExecutionState::ManualMotion
                ) && self.jog_speed_micrometres_per_second.is_some() =>
            {
                let coordinate = match axis {
                    RuidaJogAxis::X => self.position_micrometres.0.checked_add(micrometres),
                    RuidaJogAxis::Y => self.position_micrometres.1.checked_add(micrometres),
                    RuidaJogAxis::Z => self.table_position_micrometres.0.checked_add(micrometres),
                    RuidaJogAxis::U => self.table_position_micrometres.1.checked_add(micrometres),
                };
                let Some(coordinate) = coordinate else {
                    return false;
                };
                match axis {
                    RuidaJogAxis::X => self.position_micrometres.0 = coordinate,
                    RuidaJogAxis::Y => self.position_micrometres.1 = coordinate,
                    RuidaJogAxis::Z => self.table_position_micrometres.0 = coordinate,
                    RuidaJogAxis::U => self.table_position_micrometres.1 = coordinate,
                }
                self.execution_state = RuidaVirtualExecutionState::ManualMotion;
                self.machine_status = MACHINE_STATUS_MOVING;
                true
            }
            _ => false,
        }
    }

    fn error_response(&self) -> RuidaVirtualResponse {
        RuidaVirtualResponse {
            datagrams: vec![self.reply_byte(ERR)],
            accepted: false,
            protocol_error: None,
        }
    }

    fn reply_byte(&self, byte: u8) -> Vec<u8> {
        self.codec
            .encode_reply(&[byte])
            .expect("one-byte virtual controller reply is valid")
    }

    fn reply_clear(&self, clear: &[u8]) -> Vec<u8> {
        self.codec
            .encode_reply(clear)
            .expect("virtual controller clear reply is non-empty")
    }

    fn reply_memory(&self, address: u16, value: u64) -> Vec<u8> {
        self.codec
            .encode_reply(
                &memory_reply(address, value)
                    .expect("virtual controller memory values fit in the Ruida wire format"),
            )
            .expect("virtual controller memory reply is non-empty")
    }
}
