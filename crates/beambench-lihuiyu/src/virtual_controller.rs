use std::{collections::VecDeque, io, time::Duration};

use crate::{
    LihuiyuPacket, LihuiyuStatus,
    protocol::{CH341_EPP_DATA_WRITE, CH341_STATUS_REQUEST, PACKET_SIZE, decode_packet},
    transport::LihuiyuUsbIo,
};

#[derive(Debug, Clone)]
pub struct LihuiyuVirtualController {
    initialized: bool,
    accepted_packets: Vec<LihuiyuPacket>,
    controller_bytes: Vec<u8>,
    status_queue: VecDeque<LihuiyuStatus>,
    reject_next_packets: usize,
    reject_after_accepted: Option<usize>,
    unknown_next_status: Option<u8>,
    running: bool,
    paused: bool,
    output_active: bool,
    homed: bool,
    unlocked: bool,
    completed_jobs: usize,
}

impl Default for LihuiyuVirtualController {
    fn default() -> Self {
        Self::m2_nano()
    }
}

impl LihuiyuVirtualController {
    pub fn m2_nano() -> Self {
        Self {
            initialized: false,
            accepted_packets: Vec::new(),
            controller_bytes: Vec::new(),
            status_queue: VecDeque::new(),
            reject_next_packets: 0,
            reject_after_accepted: None,
            unknown_next_status: None,
            running: false,
            paused: false,
            output_active: false,
            homed: false,
            unlocked: false,
            completed_jobs: 0,
        }
    }

    pub const fn initialized(&self) -> bool {
        self.initialized
    }

    pub fn accepted_packets(&self) -> &[LihuiyuPacket] {
        &self.accepted_packets
    }

    pub fn controller_bytes(&self) -> &[u8] {
        &self.controller_bytes
    }

    pub const fn running(&self) -> bool {
        self.running
    }

    pub const fn paused(&self) -> bool {
        self.paused
    }

    pub const fn output_active(&self) -> bool {
        self.output_active
    }

    pub const fn homed(&self) -> bool {
        self.homed
    }

    pub const fn unlocked(&self) -> bool {
        self.unlocked
    }

    pub const fn completed_jobs(&self) -> usize {
        self.completed_jobs
    }

    pub fn reject_next_packets(&mut self, count: usize) {
        self.reject_next_packets = self.reject_next_packets.saturating_add(count);
    }

    /// Reject every packet once `accepted` packets have been accepted, so
    /// tests can force a failure mid-transfer with real work already queued.
    pub fn reject_packets_after_accepted(&mut self, accepted: usize) {
        self.reject_after_accepted = Some(accepted);
    }

    pub fn return_unknown_status_once(&mut self, code: u8) {
        self.unknown_next_status = Some(code);
    }

    pub fn report_status_once(&mut self, status: LihuiyuStatus) {
        self.status_queue.push_back(status);
    }

    pub fn complete_job(&mut self) {
        if self.running {
            self.running = false;
            self.paused = false;
            self.output_active = false;
            self.completed_jobs += 1;
            self.status_queue.push_back(LihuiyuStatus::Finished);
        }
    }

    fn initialize(&mut self) {
        self.initialized = true;
        self.status_queue.clear();
    }

    fn accept_packet(&mut self, packet: LihuiyuPacket) {
        if self
            .reject_after_accepted
            .is_some_and(|limit| self.accepted_packets.len() >= limit)
        {
            self.status_queue.push_back(LihuiyuStatus::ChecksumError);
            return;
        }
        if self.reject_next_packets > 0 {
            self.reject_next_packets -= 1;
            self.status_queue.push_back(LihuiyuStatus::ChecksumError);
            return;
        }
        let payload = match decode_packet(packet.as_ref()) {
            Ok(payload) => payload,
            Err(_) => {
                self.status_queue.push_back(LihuiyuStatus::ChecksumError);
                return;
            }
        };
        self.accepted_packets.push(packet);
        let trailing_padding = payload
            .iter()
            .rev()
            .take_while(|byte| **byte == b'F')
            .count();
        let payload_end = if trailing_padding == 1 {
            payload.len()
        } else {
            payload.len().saturating_sub(trailing_padding)
        };
        let command = &payload[..payload_end];
        let previous_len = self.controller_bytes.len();
        self.controller_bytes.extend_from_slice(command);

        if command == b"IPP" {
            self.homed = true;
            self.running = false;
            self.paused = false;
            self.output_active = false;
        } else if command == b"IS2P" {
            self.unlocked = true;
            self.output_active = false;
        } else if command == b"PN" {
            self.paused = !self.paused;
        } else if command.starts_with(b"IU") && command.ends_with(b"S1P") {
            self.running = false;
            self.paused = false;
            self.output_active = false;
        } else {
            let new_bytes = &self.controller_bytes[previous_len.saturating_sub(3)..];
            if contains_sequence(new_bytes, b"S1E") {
                self.running = true;
            }
            if command.contains(&b'D') {
                self.output_active = true;
            }
            if command.contains(&b'U') {
                self.output_active = false;
            }
            if contains_sequence(new_bytes, b"FNSE") {
                // The controller may still be executing buffered work. Tests
                // explicitly complete it to produce the terminal status.
                self.running = true;
            }
        }
        self.status_queue.push_back(LihuiyuStatus::Busy);
        self.status_queue.push_back(LihuiyuStatus::Ready);
    }

    fn next_status(&mut self) -> LihuiyuStatus {
        if let Some(code) = self.unknown_next_status.take() {
            return LihuiyuStatus::Unknown(code);
        }
        self.status_queue
            .pop_front()
            .unwrap_or(LihuiyuStatus::Ready)
    }
}

fn contains_sequence(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[derive(Debug, Clone)]
pub struct LihuiyuVirtualUsbIo {
    controller: LihuiyuVirtualController,
    pending_status_read: bool,
    fail_next_writes: usize,
    fail_next_packet_writes: usize,
    fail_next_reads: usize,
    fail_next_packet_ack_reads: usize,
    short_next_writes: usize,
}

impl Default for LihuiyuVirtualUsbIo {
    fn default() -> Self {
        Self::m2_nano()
    }
}

impl LihuiyuVirtualUsbIo {
    pub fn m2_nano() -> Self {
        Self {
            controller: LihuiyuVirtualController::m2_nano(),
            pending_status_read: false,
            fail_next_writes: 0,
            fail_next_packet_writes: 0,
            fail_next_reads: 0,
            fail_next_packet_ack_reads: 0,
            short_next_writes: 0,
        }
    }

    pub fn controller(&self) -> &LihuiyuVirtualController {
        &self.controller
    }

    pub fn controller_mut(&mut self) -> &mut LihuiyuVirtualController {
        &mut self.controller
    }

    pub fn fail_next_writes(&mut self, count: usize) {
        self.fail_next_writes = self.fail_next_writes.saturating_add(count);
    }

    pub fn fail_next_reads(&mut self, count: usize) {
        self.fail_next_reads = self.fail_next_reads.saturating_add(count);
    }

    pub fn fail_next_packet_writes(&mut self, count: usize) {
        self.fail_next_packet_writes = self.fail_next_packet_writes.saturating_add(count);
    }

    pub fn fail_next_packet_ack_reads(&mut self, count: usize) {
        self.fail_next_packet_ack_reads = self.fail_next_packet_ack_reads.saturating_add(count);
    }

    pub fn short_next_writes(&mut self, count: usize) {
        self.short_next_writes = self.short_next_writes.saturating_add(count);
    }
}

impl LihuiyuUsbIo for LihuiyuVirtualUsbIo {
    fn initialize_epp_1_9(&mut self, _timeout: Duration) -> io::Result<()> {
        self.controller.initialize();
        Ok(())
    }

    fn bulk_write(&mut self, data: &[u8], _timeout: Duration) -> io::Result<usize> {
        if self.fail_next_writes > 0 {
            self.fail_next_writes -= 1;
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "scheduled virtual Lihuiyu write failure",
            ));
        }
        if self.short_next_writes > 0 {
            self.short_next_writes -= 1;
            return Ok(data.len().saturating_sub(1));
        }
        if !self.controller.initialized {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "virtual Lihuiyu controller is not initialized",
            ));
        }
        if data == [CH341_STATUS_REQUEST] {
            self.pending_status_read = true;
            return Ok(data.len());
        }
        if self.fail_next_packet_writes > 0 {
            self.fail_next_packet_writes -= 1;
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "scheduled virtual Lihuiyu packet-write failure",
            ));
        }
        let packet = decode_epp_write(data)?;
        self.controller.accept_packet(packet);
        if self.fail_next_packet_ack_reads > 0 {
            self.fail_next_packet_ack_reads -= 1;
            self.fail_next_reads += 1;
        }
        Ok(data.len())
    }

    fn bulk_read(&mut self, maximum: usize, _timeout: Duration) -> io::Result<Vec<u8>> {
        if self.fail_next_reads > 0 {
            self.fail_next_reads -= 1;
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "scheduled virtual Lihuiyu read timeout",
            ));
        }
        if !self.pending_status_read {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "virtual Lihuiyu status was read without a request",
            ));
        }
        self.pending_status_read = false;
        if maximum < 2 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "virtual Lihuiyu status buffer is too small",
            ));
        }
        Ok(vec![0, self.controller.next_status().code()])
    }
}

fn decode_epp_write(data: &[u8]) -> io::Result<LihuiyuPacket> {
    let mut decoded = Vec::with_capacity(PACKET_SIZE);
    let mut cursor = 0;
    while cursor < data.len() {
        if data[cursor] != CH341_EPP_DATA_WRITE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "virtual Lihuiyu EPP write lacks the CH341 command prefix",
            ));
        }
        cursor += 1;
        let chunk_len = (PACKET_SIZE - decoded.len()).min(31);
        if cursor + chunk_len > data.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "virtual Lihuiyu EPP write is truncated",
            ));
        }
        decoded.extend_from_slice(&data[cursor..cursor + chunk_len]);
        cursor += chunk_len;
    }
    if decoded.len() != PACKET_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "virtual Lihuiyu EPP write does not contain one complete packet",
        ));
    }
    let bytes: [u8; PACKET_SIZE] = decoded.try_into().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "virtual Lihuiyu packet length changed during decode",
        )
    })?;
    Ok(LihuiyuPacket(bytes))
}

#[cfg(test)]
mod tests {
    use crate::protocol::{LihuiyuPadding, encode_epp_bulk_write, encode_packet};

    use super::*;

    #[test]
    fn virtual_usb_requires_init_and_roundtrips_epp_packets() {
        let packet = encode_packet(b"IPP", LihuiyuPadding::AsciiF).unwrap();
        let wire = encode_epp_bulk_write(&packet);
        let mut io = LihuiyuVirtualUsbIo::m2_nano();
        assert!(io.bulk_write(&wire, Duration::ZERO).is_err());
        io.initialize_epp_1_9(Duration::ZERO).unwrap();
        assert_eq!(io.bulk_write(&wire, Duration::ZERO).unwrap(), wire.len());
        assert!(io.controller().homed());
        assert_eq!(io.controller().accepted_packets(), &[packet]);
    }

    #[test]
    fn completion_is_an_explicit_terminal_status() {
        let mut controller = LihuiyuVirtualController::m2_nano();
        controller.initialize();
        controller.running = true;
        controller.output_active = true;
        controller.complete_job();
        assert_eq!(controller.next_status(), LihuiyuStatus::Finished);
        assert!(!controller.output_active());
        assert_eq!(controller.completed_jobs(), 1);
    }
}
