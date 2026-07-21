use std::{io, time::Duration};

use thiserror::Error;

use crate::{
    LihuiyuCompiledJob, LihuiyuPacket,
    protocol::{
        LihuiyuPadding, LihuiyuProtocolError, LihuiyuStatus, encode_epp_bulk_write, encode_packet,
        parse_status_reply, status_bulk_request,
    },
};

pub trait LihuiyuUsbIo {
    /// Put the CH341 interface into EPP 1.9 mode and clear stale buffers.
    fn initialize_epp_1_9(&mut self, timeout: Duration) -> io::Result<()>;
    fn bulk_write(&mut self, data: &[u8], timeout: Duration) -> io::Result<usize>;
    fn bulk_read(&mut self, maximum: usize, timeout: Duration) -> io::Result<Vec<u8>>;
}

impl<T: LihuiyuUsbIo + ?Sized> LihuiyuUsbIo for Box<T> {
    fn initialize_epp_1_9(&mut self, timeout: Duration) -> io::Result<()> {
        (**self).initialize_epp_1_9(timeout)
    }

    fn bulk_write(&mut self, data: &[u8], timeout: Duration) -> io::Result<usize> {
        (**self).bulk_write(data, timeout)
    }

    fn bulk_read(&mut self, maximum: usize, timeout: Duration) -> io::Result<Vec<u8>> {
        (**self).bulk_read(maximum, timeout)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LihuiyuTransferConfig {
    pub io_timeout: Duration,
    pub status_attempts: u16,
    /// Separate, much larger budget for Busy polls. Busy means the controller
    /// is alive and executing buffered work, which routinely lasts longer than
    /// the `status_attempts` window mid-burn; only a genuinely wedged
    /// controller should exhaust this budget.
    pub busy_status_attempts: u32,
    pub status_delay: Duration,
    /// Total sends allowed after explicit checksum rejection, including the
    /// initial send. Ambiguous I/O is never retried.
    pub checksum_attempts: u8,
}

impl Default for LihuiyuTransferConfig {
    fn default() -> Self {
        Self {
            io_timeout: Duration::from_millis(1_500),
            status_attempts: 500,
            // ~5 minutes: 50 polls at 2ms, then backed off to 20ms.
            busy_status_attempts: 15_000,
            status_delay: Duration::from_millis(2),
            checksum_attempts: 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LihuiyuTransferPhase {
    Preparing,
    Transferring,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LihuiyuTransferProgress {
    pub phase: LihuiyuTransferPhase,
    pub packets_acknowledged: usize,
    pub total_packets: usize,
    pub send_attempts: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LihuiyuTransferReceipt {
    pub packets_acknowledged: usize,
    pub send_attempts: usize,
    pub checksum_rejections: usize,
    pub observed_terminal_status: bool,
}

/// Resumable job-transfer cursor. Tracks the in-flight packet and the
/// bounded-wait budgets so a transfer can be advanced in small steps without
/// holding shared locks for the whole upload.
#[derive(Debug)]
pub struct LihuiyuJobTransfer {
    index: usize,
    total: usize,
    attempt: u8,
    awaiting_ack: bool,
    observed_preflight_terminal: bool,
    pending_ack_terminal: bool,
    checksum_rejections: usize,
    observed_terminal_status: bool,
    attempts_before: usize,
    busy_polls: u32,
    status_polls: u16,
}

impl LihuiyuJobTransfer {
    pub const fn packets_acknowledged(&self) -> usize {
        self.index
    }

    pub const fn total_packets(&self) -> usize {
        self.total
    }

    fn receipt<I>(&self, transport: &LihuiyuTransport<I>) -> LihuiyuTransferReceipt {
        LihuiyuTransferReceipt {
            packets_acknowledged: self.index,
            send_attempts: transport.send_attempts - self.attempts_before,
            checksum_rejections: self.checksum_rejections,
            observed_terminal_status: self.observed_terminal_status,
        }
    }
}

/// Outcome of one bounded transfer advance.
#[derive(Debug)]
pub enum LihuiyuTransferStep {
    /// The poll budget was exhausted without a packet acknowledgement.
    Pending,
    /// One more packet was acknowledged; more remain.
    PacketAcknowledged,
    /// Every packet was acknowledged.
    Complete(LihuiyuTransferReceipt),
}

#[derive(Debug, Error)]
pub enum LihuiyuTransferError {
    #[error(transparent)]
    Protocol(#[from] LihuiyuProtocolError),
    #[error("Lihuiyu USB I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("Lihuiyu USB write accepted {actual} of {expected} bytes")]
    ShortWrite { actual: usize, expected: usize },
    #[error("Lihuiyu transfer status_attempts must be at least one")]
    InvalidStatusLimit,
    #[error("Lihuiyu transfer checksum_attempts must be at least one")]
    InvalidChecksumLimit,
    #[error("Lihuiyu controller identity has not been positively established")]
    NotConnected,
    #[error("Lihuiyu controller returned unknown status {0:#04x}")]
    UnknownStatus(u8),
    #[error("Lihuiyu controller reported a power fault")]
    PowerFault,
    #[error("Lihuiyu controller did not become ready within the bounded status window")]
    ReadyTimeout,
    #[error("Lihuiyu controller did not acknowledge the packet within the bounded status window")]
    AcknowledgementTimeout,
    #[error("Lihuiyu controller rejected the packet checksum after {attempts} sends")]
    ChecksumRejected { attempts: u8 },
    #[error("Lihuiyu controller reported terminal completion before the final job packet")]
    PrematureTerminalStatus,
    #[error("Lihuiyu compiled job is not internally consistent or lacks a completion wait")]
    InvalidCompiledJob,
    #[error("Lihuiyu transfer state is ambiguous; reconnect before sending more commands")]
    RecoveryRequired,
}

/// Busy polls at full cadence before the status poll backs off to 10x the
/// configured delay.
const BUSY_POLL_BACKOFF_AFTER: u32 = 50;

#[derive(Debug)]
pub struct LihuiyuTransport<I> {
    io: I,
    config: LihuiyuTransferConfig,
    connected: bool,
    recovery_required: bool,
    last_status: Option<LihuiyuStatus>,
    send_attempts: usize,
}

impl<I: LihuiyuUsbIo> LihuiyuTransport<I> {
    pub fn new(io: I, config: LihuiyuTransferConfig) -> Result<Self, LihuiyuTransferError> {
        if config.status_attempts == 0 || config.busy_status_attempts == 0 {
            return Err(LihuiyuTransferError::InvalidStatusLimit);
        }
        if config.checksum_attempts == 0 {
            return Err(LihuiyuTransferError::InvalidChecksumLimit);
        }
        Ok(Self {
            io,
            config,
            connected: false,
            recovery_required: false,
            last_status: None,
            send_attempts: 0,
        })
    }

    pub fn into_inner(self) -> I {
        self.io
    }

    pub fn io(&self) -> &I {
        &self.io
    }

    pub fn io_mut(&mut self) -> &mut I {
        &mut self.io
    }

    pub const fn connected(&self) -> bool {
        self.connected
    }

    pub const fn recovery_required(&self) -> bool {
        self.recovery_required
    }

    pub const fn last_status(&self) -> Option<LihuiyuStatus> {
        self.last_status
    }

    pub const fn send_attempts(&self) -> usize {
        self.send_attempts
    }

    /// Positive identity requires successful EPP 1.9 initialization and a
    /// status byte from the documented Lihuiyu status vocabulary.
    pub fn connect(&mut self) -> Result<LihuiyuStatus, LihuiyuTransferError> {
        if self.recovery_required {
            return Err(LihuiyuTransferError::RecoveryRequired);
        }
        self.io.initialize_epp_1_9(self.config.io_timeout)?;
        let status = self.read_status_unverified()?;
        if let LihuiyuStatus::Unknown(code) = status {
            return Err(LihuiyuTransferError::UnknownStatus(code));
        }
        self.connected = true;
        self.last_status = Some(status);
        Ok(status)
    }

    pub fn read_status(&mut self) -> Result<LihuiyuStatus, LihuiyuTransferError> {
        self.ensure_connected()?;
        let status = self.read_status_unverified()?;
        self.last_status = Some(status);
        Ok(status)
    }

    pub(crate) fn send_command(
        &mut self,
        command: &[u8],
    ) -> Result<LihuiyuTransferReceipt, LihuiyuTransferError> {
        let packet = encode_packet(command, LihuiyuPadding::AsciiF)?;
        let acknowledgement = self.send_packet(&packet)?;
        Ok(LihuiyuTransferReceipt {
            packets_acknowledged: 1,
            send_attempts: acknowledgement.attempts,
            checksum_rejections: acknowledgement.checksum_rejections,
            observed_terminal_status: acknowledgement.observed_preflight_terminal
                || acknowledgement.observed_acknowledgement_terminal,
        })
    }

    pub fn send_job(
        &mut self,
        job: &LihuiyuCompiledJob,
    ) -> Result<LihuiyuTransferReceipt, LihuiyuTransferError> {
        self.send_job_with_progress(job, |_| {})
    }

    /// Validate a compiled job and return a resumable transfer cursor.
    /// Nothing is written to the controller until the cursor is advanced.
    pub fn begin_job_transfer(
        &mut self,
        job: &LihuiyuCompiledJob,
    ) -> Result<LihuiyuJobTransfer, LihuiyuTransferError> {
        self.ensure_connected()?;
        validate_compiled_job(job)?;
        Ok(LihuiyuJobTransfer {
            index: 0,
            total: job.packets.len(),
            attempt: 1,
            awaiting_ack: false,
            observed_preflight_terminal: false,
            pending_ack_terminal: false,
            checksum_rejections: 0,
            observed_terminal_status: false,
            attempts_before: self.send_attempts,
            busy_polls: 0,
            status_polls: 0,
        })
    }

    /// Advance a job transfer by at most `max_polls` status reads, sending at
    /// most one packet's worth of protocol progress before returning. This is
    /// the resumable core of the transfer: callers holding shared locks can
    /// advance in small bounded steps so emergency stop, cancel, and status
    /// remain reachable between steps.
    pub fn advance_job_transfer(
        &mut self,
        job: &LihuiyuCompiledJob,
        transfer: &mut LihuiyuJobTransfer,
        max_polls: u32,
    ) -> Result<LihuiyuTransferStep, LihuiyuTransferError> {
        self.ensure_connected()?;
        if transfer.index >= transfer.total {
            return Ok(LihuiyuTransferStep::Complete(transfer.receipt(self)));
        }
        let mut polls = 0;
        loop {
            if polls >= max_polls {
                return Ok(LihuiyuTransferStep::Pending);
            }
            if transfer.awaiting_ack {
                // Any failure while awaiting the acknowledgement of a written
                // packet leaves the controller state ambiguous.
                let status = match self.read_status() {
                    Ok(status) => status,
                    Err(error) => {
                        self.recovery_required = true;
                        return Err(error);
                    }
                };
                polls += 1;
                match status {
                    LihuiyuStatus::Ready => {
                        let is_last = transfer.index + 1 == transfer.total;
                        if (transfer.index > 0 && transfer.observed_preflight_terminal)
                            || (!is_last && transfer.pending_ack_terminal)
                        {
                            self.recovery_required = true;
                            return Err(LihuiyuTransferError::PrematureTerminalStatus);
                        }
                        if is_last {
                            transfer.observed_terminal_status |= transfer.pending_ack_terminal;
                        }
                        transfer.index += 1;
                        transfer.attempt = 1;
                        transfer.awaiting_ack = false;
                        transfer.observed_preflight_terminal = false;
                        transfer.pending_ack_terminal = false;
                        transfer.busy_polls = 0;
                        transfer.status_polls = 0;
                        if transfer.index == transfer.total {
                            return Ok(LihuiyuTransferStep::Complete(transfer.receipt(self)));
                        }
                        return Ok(LihuiyuTransferStep::PacketAcknowledged);
                    }
                    LihuiyuStatus::ChecksumError => {
                        transfer.checksum_rejections += 1;
                        if transfer.attempt >= self.config.checksum_attempts {
                            return Err(LihuiyuTransferError::ChecksumRejected {
                                attempts: self.config.checksum_attempts,
                            });
                        }
                        transfer.attempt += 1;
                        transfer.awaiting_ack = false;
                        transfer.busy_polls = 0;
                        transfer.status_polls = 0;
                    }
                    LihuiyuStatus::Busy => {
                        transfer.busy_polls += 1;
                        if transfer.busy_polls >= self.config.busy_status_attempts {
                            self.recovery_required = true;
                            return Err(LihuiyuTransferError::AcknowledgementTimeout);
                        }
                    }
                    LihuiyuStatus::Finished | LihuiyuStatus::SerialCorrectOrM3Finished => {
                        transfer.pending_ack_terminal = true;
                        transfer.status_polls += 1;
                        if transfer.status_polls >= self.config.status_attempts {
                            self.recovery_required = true;
                            return Err(LihuiyuTransferError::AcknowledgementTimeout);
                        }
                    }
                    LihuiyuStatus::Power => {
                        self.recovery_required = true;
                        return Err(LihuiyuTransferError::PowerFault);
                    }
                    LihuiyuStatus::Unknown(code) => {
                        self.recovery_required = true;
                        return Err(LihuiyuTransferError::UnknownStatus(code));
                    }
                }
            } else {
                // Pre-write ready wait: nothing has been written for this
                // attempt, so errors here do not require recovery.
                let status = self.read_status()?;
                polls += 1;
                match status {
                    LihuiyuStatus::Ready => {
                        let wire = encode_epp_bulk_write(&job.packets[transfer.index]);
                        self.send_attempts += 1;
                        let written = match self.io.bulk_write(&wire, self.config.io_timeout) {
                            Ok(written) => written,
                            Err(error) => {
                                self.recovery_required = true;
                                return Err(error.into());
                            }
                        };
                        if written != wire.len() {
                            self.recovery_required = true;
                            return Err(LihuiyuTransferError::ShortWrite {
                                actual: written,
                                expected: wire.len(),
                            });
                        }
                        transfer.awaiting_ack = true;
                        transfer.pending_ack_terminal = false;
                        transfer.busy_polls = 0;
                        transfer.status_polls = 0;
                    }
                    LihuiyuStatus::Busy => {
                        transfer.busy_polls += 1;
                        if transfer.busy_polls >= self.config.busy_status_attempts {
                            return Err(LihuiyuTransferError::ReadyTimeout);
                        }
                    }
                    LihuiyuStatus::Finished | LihuiyuStatus::SerialCorrectOrM3Finished => {
                        transfer.observed_preflight_terminal = true;
                        transfer.status_polls += 1;
                        if transfer.status_polls >= self.config.status_attempts {
                            return Err(LihuiyuTransferError::ReadyTimeout);
                        }
                    }
                    LihuiyuStatus::ChecksumError => {
                        return Err(LihuiyuTransferError::ChecksumRejected { attempts: 0 });
                    }
                    LihuiyuStatus::Power => return Err(LihuiyuTransferError::PowerFault),
                    LihuiyuStatus::Unknown(code) => {
                        return Err(LihuiyuTransferError::UnknownStatus(code));
                    }
                }
            }
            self.delay_status_poll(transfer.busy_polls);
        }
    }

    pub fn send_job_with_progress(
        &mut self,
        job: &LihuiyuCompiledJob,
        mut progress: impl FnMut(LihuiyuTransferProgress),
    ) -> Result<LihuiyuTransferReceipt, LihuiyuTransferError> {
        let mut transfer = self.begin_job_transfer(job)?;
        progress(LihuiyuTransferProgress {
            phase: LihuiyuTransferPhase::Preparing,
            packets_acknowledged: 0,
            total_packets: transfer.total,
            send_attempts: 0,
        });
        loop {
            match self.advance_job_transfer(job, &mut transfer, u32::MAX)? {
                LihuiyuTransferStep::PacketAcknowledged => {
                    progress(LihuiyuTransferProgress {
                        phase: LihuiyuTransferPhase::Transferring,
                        packets_acknowledged: transfer.index,
                        total_packets: transfer.total,
                        send_attempts: self.send_attempts - transfer.attempts_before,
                    });
                }
                LihuiyuTransferStep::Complete(receipt) => {
                    progress(LihuiyuTransferProgress {
                        phase: LihuiyuTransferPhase::Transferring,
                        packets_acknowledged: receipt.packets_acknowledged,
                        total_packets: transfer.total,
                        send_attempts: receipt.send_attempts,
                    });
                    progress(LihuiyuTransferProgress {
                        phase: LihuiyuTransferPhase::Complete,
                        packets_acknowledged: receipt.packets_acknowledged,
                        total_packets: transfer.total,
                        send_attempts: receipt.send_attempts,
                    });
                    return Ok(receipt);
                }
                LihuiyuTransferStep::Pending => {}
            }
        }
    }

    fn send_packet(
        &mut self,
        packet: &LihuiyuPacket,
    ) -> Result<PacketAcknowledgement, LihuiyuTransferError> {
        self.ensure_connected()?;
        let mut checksum_rejections = 0;
        let mut observed_preflight_terminal = false;
        for attempt in 1..=self.config.checksum_attempts {
            observed_preflight_terminal |= self.wait_until_ready()?;
            let wire = encode_epp_bulk_write(packet);
            self.send_attempts += 1;
            let written = match self.io.bulk_write(&wire, self.config.io_timeout) {
                Ok(written) => written,
                Err(error) => {
                    self.recovery_required = true;
                    return Err(error.into());
                }
            };
            if written != wire.len() {
                self.recovery_required = true;
                return Err(LihuiyuTransferError::ShortWrite {
                    actual: written,
                    expected: wire.len(),
                });
            }

            match self.wait_for_acknowledgement() {
                Ok(PacketStatus::Accepted { observed_terminal }) => {
                    return Ok(PacketAcknowledgement {
                        attempts: usize::from(attempt),
                        checksum_rejections,
                        observed_preflight_terminal,
                        observed_acknowledgement_terminal: observed_terminal,
                    });
                }
                Ok(PacketStatus::ChecksumRejected) => {
                    checksum_rejections += 1;
                    if attempt == self.config.checksum_attempts {
                        return Err(LihuiyuTransferError::ChecksumRejected {
                            attempts: self.config.checksum_attempts,
                        });
                    }
                }
                Err(error) => {
                    self.recovery_required = true;
                    return Err(error);
                }
            }
        }
        unreachable!("checksum attempt limit is validated")
    }

    fn wait_until_ready(&mut self) -> Result<bool, LihuiyuTransferError> {
        let mut observed_terminal = false;
        let mut status_polls: u16 = 0;
        let mut busy_polls: u32 = 0;
        loop {
            let status = self.read_status()?;
            match status {
                LihuiyuStatus::Ready => return Ok(observed_terminal),
                LihuiyuStatus::Busy => {
                    busy_polls += 1;
                    if busy_polls >= self.config.busy_status_attempts {
                        return Err(LihuiyuTransferError::ReadyTimeout);
                    }
                }
                LihuiyuStatus::Finished | LihuiyuStatus::SerialCorrectOrM3Finished => {
                    observed_terminal = true;
                    status_polls += 1;
                    if status_polls >= self.config.status_attempts {
                        return Err(LihuiyuTransferError::ReadyTimeout);
                    }
                }
                LihuiyuStatus::ChecksumError => {
                    return Err(LihuiyuTransferError::ChecksumRejected { attempts: 0 });
                }
                LihuiyuStatus::Power => return Err(LihuiyuTransferError::PowerFault),
                LihuiyuStatus::Unknown(code) => {
                    return Err(LihuiyuTransferError::UnknownStatus(code));
                }
            }
            self.delay_status_poll(busy_polls);
        }
    }

    fn wait_for_acknowledgement(&mut self) -> Result<PacketStatus, LihuiyuTransferError> {
        let mut observed_terminal = false;
        let mut status_polls: u16 = 0;
        let mut busy_polls: u32 = 0;
        loop {
            let status = self.read_status()?;
            match status {
                LihuiyuStatus::Ready => {
                    return Ok(PacketStatus::Accepted { observed_terminal });
                }
                LihuiyuStatus::ChecksumError => return Ok(PacketStatus::ChecksumRejected),
                LihuiyuStatus::Busy => {
                    busy_polls += 1;
                    if busy_polls >= self.config.busy_status_attempts {
                        return Err(LihuiyuTransferError::AcknowledgementTimeout);
                    }
                }
                LihuiyuStatus::Finished | LihuiyuStatus::SerialCorrectOrM3Finished => {
                    observed_terminal = true;
                    status_polls += 1;
                    if status_polls >= self.config.status_attempts {
                        return Err(LihuiyuTransferError::AcknowledgementTimeout);
                    }
                }
                LihuiyuStatus::Power => return Err(LihuiyuTransferError::PowerFault),
                LihuiyuStatus::Unknown(code) => {
                    return Err(LihuiyuTransferError::UnknownStatus(code));
                }
            }
            self.delay_status_poll(busy_polls);
        }
    }

    fn read_status_unverified(&mut self) -> Result<LihuiyuStatus, LihuiyuTransferError> {
        let request = status_bulk_request();
        let written = self.io.bulk_write(&request, self.config.io_timeout)?;
        if written != request.len() {
            return Err(LihuiyuTransferError::ShortWrite {
                actual: written,
                expected: request.len(),
            });
        }
        let reply = self.io.bulk_read(8, self.config.io_timeout)?;
        Ok(parse_status_reply(&reply)?)
    }

    fn delay_status_poll(&self, busy_polls: u32) {
        if self.config.status_delay.is_zero() {
            return;
        }
        // Back off once the controller has been Busy for a while: it is
        // executing buffered work and hammering the status endpoint at full
        // cadence only adds USB chatter.
        let delay = if busy_polls > BUSY_POLL_BACKOFF_AFTER {
            self.config.status_delay * 10
        } else {
            self.config.status_delay
        };
        std::thread::sleep(delay);
    }

    fn ensure_connected(&self) -> Result<(), LihuiyuTransferError> {
        if self.recovery_required {
            return Err(LihuiyuTransferError::RecoveryRequired);
        }
        if !self.connected {
            return Err(LihuiyuTransferError::NotConnected);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct PacketAcknowledgement {
    attempts: usize,
    checksum_rejections: usize,
    observed_preflight_terminal: bool,
    observed_acknowledgement_terminal: bool,
}

enum PacketStatus {
    Accepted { observed_terminal: bool },
    ChecksumRejected,
}

fn validate_compiled_job(job: &LihuiyuCompiledJob) -> Result<(), LihuiyuTransferError> {
    if job.packets.is_empty()
        || !job.waits_for_completion
        || !crate::compiler::compiled_job_is_consistent(job)
        || job
            .packets
            .iter()
            .any(|packet| crate::protocol::decode_packet(packet.as_ref()).is_err())
    {
        return Err(LihuiyuTransferError::InvalidCompiledJob);
    }
    Ok(())
}
