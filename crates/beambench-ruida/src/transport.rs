use std::{
    collections::VecDeque,
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket},
    time::Duration,
};

use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    RuidaCompiledJob, RuidaVirtualController,
    protocol::{
        ACK, ERR, MAX_CONTROLLER_FILES, MAX_UDP_DATAGRAM_SIZE, MAX_UDP_PAYLOAD_SIZE,
        MEMORY_CARD_ID, MEMORY_FILE_COUNT, MEMORY_MACHINE_STATUS, NAK, RDC6442S_CARD_ID,
        RUIDA_UDP_PORT, RuidaCodec, RuidaProtocolError, decode_u35, delete_document_command,
        document_name_command, enquiry_command, file_transfer_command, memory_read_command,
        normalize_upload_filename, parse_document_name_reply, parse_memory_reply,
    },
};

pub const RUIDA_UDP_REPLY_PORT: u16 = 40_200;

pub trait RuidaDatagramIo {
    fn send(&mut self, datagram: &[u8]) -> io::Result<()>;
    fn receive(&mut self, timeout: Duration) -> io::Result<Vec<u8>>;
}

#[derive(Debug)]
pub struct RuidaUdpIo {
    socket: UdpSocket,
    target: SocketAddr,
}

impl RuidaUdpIo {
    pub fn for_controller(address: IpAddr) -> io::Result<Self> {
        Self::for_target(SocketAddr::new(address, RUIDA_UDP_PORT))
    }

    pub fn for_target(target: SocketAddr) -> io::Result<Self> {
        let local = match target.ip() {
            IpAddr::V4(_) => SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), RUIDA_UDP_REPLY_PORT),
            IpAddr::V6(_) => SocketAddr::new(Ipv6Addr::UNSPECIFIED.into(), RUIDA_UDP_REPLY_PORT),
        };
        Self::bind(target, local)
    }

    pub fn bind(target: SocketAddr, local: SocketAddr) -> io::Result<Self> {
        let socket = UdpSocket::bind(local)?;
        socket.connect(target)?;
        Ok(Self { socket, target })
    }

    pub const fn target(&self) -> SocketAddr {
        self.target
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }
}

impl RuidaDatagramIo for RuidaUdpIo {
    fn send(&mut self, datagram: &[u8]) -> io::Result<()> {
        let written = self.socket.send(datagram)?;
        if written != datagram.len() {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "Ruida UDP socket reported a partial datagram write",
            ));
        }
        Ok(())
    }

    fn receive(&mut self, timeout: Duration) -> io::Result<Vec<u8>> {
        self.socket.set_read_timeout(Some(timeout))?;
        let mut buffer = vec![0; MAX_UDP_DATAGRAM_SIZE];
        let length = self.socket.recv(&mut buffer)?;
        buffer.truncate(length);
        Ok(buffer)
    }
}

#[derive(Debug, Clone)]
pub struct RuidaVirtualIo {
    controller: RuidaVirtualController,
    replies: VecDeque<Vec<u8>>,
    drop_next_sends: usize,
    nak_next_sends: usize,
    drop_after_sends: Option<usize>,
    nak_after_sends: Option<usize>,
}

impl Default for RuidaVirtualIo {
    fn default() -> Self {
        Self::rdc6442s()
    }
}

impl RuidaVirtualIo {
    pub fn rdc6442s() -> Self {
        Self {
            controller: RuidaVirtualController::rdc6442s(),
            replies: VecDeque::new(),
            drop_next_sends: 0,
            nak_next_sends: 0,
            drop_after_sends: None,
            nak_after_sends: None,
        }
    }

    pub fn controller(&self) -> &RuidaVirtualController {
        &self.controller
    }

    pub fn controller_mut(&mut self) -> &mut RuidaVirtualController {
        &mut self.controller
    }

    pub fn drop_next_sends(&mut self, count: usize) {
        self.drop_next_sends = self.drop_next_sends.saturating_add(count);
    }

    pub fn nak_next_sends(&mut self, count: usize) {
        self.nak_next_sends = self.nak_next_sends.saturating_add(count);
    }

    /// Drop one send after allowing `count` sends through. Used to exercise a
    /// specific transfer phase without coupling tests to private client calls.
    pub fn drop_send_after(&mut self, count: usize) {
        self.drop_after_sends = Some(count);
    }

    /// Return one NAK after allowing `count` sends through.
    pub fn nak_send_after(&mut self, count: usize) {
        self.nak_after_sends = Some(count);
    }
}

impl RuidaDatagramIo for RuidaVirtualIo {
    fn send(&mut self, datagram: &[u8]) -> io::Result<()> {
        if scheduled_fault_due(&mut self.drop_after_sends) {
            return Ok(());
        }
        if scheduled_fault_due(&mut self.nak_after_sends) {
            self.replies.push_back(
                RuidaCodec::default()
                    .encode_reply(&[NAK])
                    .expect("virtual NAK is encodable"),
            );
            return Ok(());
        }
        if self.drop_next_sends > 0 {
            self.drop_next_sends -= 1;
            return Ok(());
        }
        if self.nak_next_sends > 0 {
            self.nak_next_sends -= 1;
            self.replies.push_back(
                RuidaCodec::default()
                    .encode_reply(&[NAK])
                    .expect("virtual NAK is encodable"),
            );
            return Ok(());
        }
        let response = self.controller.receive_datagram(datagram);
        self.replies.extend(response.datagrams);
        Ok(())
    }

    fn receive(&mut self, _timeout: Duration) -> io::Result<Vec<u8>> {
        self.replies.pop_front().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                "virtual Ruida controller has no queued reply",
            )
        })
    }
}

fn scheduled_fault_due(remaining: &mut Option<usize>) -> bool {
    let Some(count) = remaining.as_mut() else {
        return false;
    };
    if *count == 0 {
        *remaining = None;
        true
    } else {
        *count -= 1;
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuidaTransferConfig {
    pub acknowledgement_timeout: Duration,
    pub reply_timeout: Duration,
    /// Total sends permitted for a datagram, including the first attempt.
    pub max_attempts: u8,
    pub storage_verification_attempts: u8,
    pub storage_verification_delay: Duration,
}

impl Default for RuidaTransferConfig {
    fn default() -> Self {
        Self {
            acknowledgement_timeout: Duration::from_millis(250),
            reply_timeout: Duration::from_millis(250),
            max_attempts: 4,
            storage_verification_attempts: 4,
            storage_verification_delay: Duration::from_millis(100),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuidaStoredFile {
    pub index: u16,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuidaUploadPhase {
    Preparing,
    Transferring,
    Verifying,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuidaUploadProgress {
    pub phase: RuidaUploadPhase,
    pub bytes_sent: usize,
    pub total_bytes: usize,
    pub packets_acknowledged: usize,
    pub total_packets: usize,
}

/// The protocol's end-of-file marker: every compiled job's clear stream ends
/// with this byte, and the controller leaves its upload state upon
/// receiving it (see the compiler's END_OF_FILE emission).
const UPLOAD_END_OF_FILE: &[u8] = &[0xD7];

/// Resumable upload cursor: tracks the announcement/chunk/verify stage so an
/// upload can be advanced in bounded steps without holding shared locks for
/// the whole file transfer.
#[derive(Debug)]
pub struct RuidaUploadCursor {
    filename: String,
    stage: RuidaUploadStage,
    bytes_sent: usize,
    packets_acknowledged: usize,
    total_packets: usize,
    attempts_before: usize,
}

impl RuidaUploadCursor {
    pub const fn packets_acknowledged(&self) -> usize {
        self.packets_acknowledged
    }

    pub const fn total_packets(&self) -> usize {
        self.total_packets
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuidaUploadStage {
    AnnounceFile,
    Chunks,
    Verify,
}

/// Outcome of one bounded upload advance.
#[derive(Debug)]
pub enum RuidaUploadStep {
    /// More work remains.
    Pending,
    /// The upload is confirmed present in controller storage.
    Complete(RuidaUploadReceipt),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuidaUploadReceipt {
    controller_card_id: u64,
    file: RuidaStoredFile,
    clear_byte_len: usize,
    clear_sha256: String,
    packets_acknowledged: usize,
    send_attempts: usize,
}

impl RuidaUploadReceipt {
    pub const fn controller_card_id(&self) -> u64 {
        self.controller_card_id
    }

    pub const fn file(&self) -> &RuidaStoredFile {
        &self.file
    }

    pub const fn clear_byte_len(&self) -> usize {
        self.clear_byte_len
    }

    pub fn clear_sha256(&self) -> &str {
        &self.clear_sha256
    }

    pub const fn packets_acknowledged(&self) -> usize {
        self.packets_acknowledged
    }

    pub const fn send_attempts(&self) -> usize {
        self.send_attempts
    }
}

#[derive(Debug, Error)]
pub enum RuidaTransferError {
    #[error(transparent)]
    Protocol(#[from] RuidaProtocolError),
    #[error("Ruida UDP I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("Ruida controller did not acknowledge the datagram after {attempts} attempts")]
    AcknowledgementTimeout { attempts: u8 },
    #[error("Ruida controller acknowledged the command but did not return its data reply")]
    ReplyTimeout,
    #[error("Ruida controller rejected the command")]
    ControllerError,
    #[error("Ruida controller returned an unexpected reply: {0:02x?}")]
    UnexpectedReply(Vec<u8>),
    #[error("Ruida transfer max_attempts must be at least one")]
    InvalidAttemptLimit,
    #[error("Ruida storage_verification_attempts must be at least one")]
    InvalidVerificationLimit,
    #[error("Ruida could not allocate a unique namespaced controller filename")]
    FilenameAllocationFailed,
    #[error(
        "Ruida upload requires an internally consistent compiled job ending in a valid file sum"
    )]
    InvalidCompiledJob,
    #[error(
        "Ruida upload state is ambiguous; reset the controller connection before sending another command"
    )]
    RecoveryRequired,
    #[error(
        "Ruida identity mismatch: expected RDC6442S card ID {expected:#x}, received {actual:#x}"
    )]
    IdentityMismatch { expected: u64, actual: u64 },
    #[error("Ruida controller reported an invalid stored-file count: {0}")]
    InvalidFileCount(u64),
    #[error("Ruida controller storage is full (99 files)")]
    StorageFull,
    #[error("Ruida controller already contains a file named {0}")]
    DuplicateFilename(String),
    #[error(
        "Ruida upload was acknowledged but its filename was not present exactly once afterward"
    )]
    UploadNotConfirmed,
    #[error("Ruida upload receipt no longer identifies exactly one controller file")]
    ReceiptNoLongerUnique,
    #[error("Ruida deletion was acknowledged but the uniquely scoped file is still listed")]
    DeleteNotConfirmed,
}

#[derive(Debug)]
pub struct RuidaStorageClient<I> {
    io: I,
    codec: RuidaCodec,
    config: RuidaTransferConfig,
    verified_card_id: Option<u64>,
    send_attempts: usize,
    recovery_required: bool,
}

impl<I: RuidaDatagramIo> RuidaStorageClient<I> {
    pub fn new(io: I, config: RuidaTransferConfig) -> Result<Self, RuidaTransferError> {
        if config.max_attempts == 0 {
            return Err(RuidaTransferError::InvalidAttemptLimit);
        }
        if config.storage_verification_attempts == 0 {
            return Err(RuidaTransferError::InvalidVerificationLimit);
        }
        Ok(Self {
            io,
            codec: RuidaCodec::default(),
            config,
            verified_card_id: None,
            send_attempts: 0,
            recovery_required: false,
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

    pub const fn verified_card_id(&self) -> Option<u64> {
        self.verified_card_id
    }

    pub const fn recovery_required(&self) -> bool {
        self.recovery_required
    }

    pub fn connect(&mut self) -> Result<u64, RuidaTransferError> {
        if self.recovery_required {
            return Err(RuidaTransferError::RecoveryRequired);
        }
        self.exchange(&enquiry_command(), false)?;
        let card_id = self.read_memory_unverified(MEMORY_CARD_ID)?;
        if card_id != RDC6442S_CARD_ID {
            return Err(RuidaTransferError::IdentityMismatch {
                expected: RDC6442S_CARD_ID,
                actual: card_id,
            });
        }
        self.verified_card_id = Some(card_id);
        Ok(card_id)
    }

    pub fn list_files(&mut self) -> Result<Vec<RuidaStoredFile>, RuidaTransferError> {
        self.ensure_connected()?;
        self.list_files_verified()
    }

    pub fn upload_job(
        &mut self,
        job: &RuidaCompiledJob,
    ) -> Result<RuidaUploadReceipt, RuidaTransferError> {
        self.upload_job_with_progress(job, |_| {})
    }

    pub fn upload_job_with_progress(
        &mut self,
        job: &RuidaCompiledJob,
        mut progress: impl FnMut(RuidaUploadProgress),
    ) -> Result<RuidaUploadReceipt, RuidaTransferError> {
        let mut cursor = self.begin_upload(job)?;
        progress(RuidaUploadProgress {
            phase: RuidaUploadPhase::Preparing,
            bytes_sent: 0,
            total_bytes: job.clear_bytes.len(),
            packets_acknowledged: 0,
            total_packets: cursor.total_packets,
        });
        loop {
            match self.advance_upload(job, &mut cursor, 1)? {
                RuidaUploadStep::Pending => {
                    progress(RuidaUploadProgress {
                        phase: if cursor.stage == RuidaUploadStage::Verify {
                            RuidaUploadPhase::Verifying
                        } else {
                            RuidaUploadPhase::Transferring
                        },
                        bytes_sent: cursor.bytes_sent,
                        total_bytes: job.clear_bytes.len(),
                        packets_acknowledged: cursor.packets_acknowledged,
                        total_packets: cursor.total_packets,
                    });
                }
                RuidaUploadStep::Complete(receipt) => {
                    progress(RuidaUploadProgress {
                        phase: RuidaUploadPhase::Complete,
                        bytes_sent: cursor.bytes_sent,
                        total_bytes: job.clear_bytes.len(),
                        packets_acknowledged: cursor.total_packets,
                        total_packets: cursor.total_packets,
                    });
                    return Ok(receipt);
                }
            }
        }
    }

    /// Validate the job, allocate a controller filename, and return a
    /// resumable upload cursor. Nothing is sent until the cursor is advanced.
    pub fn begin_upload(
        &mut self,
        job: &RuidaCompiledJob,
    ) -> Result<RuidaUploadCursor, RuidaTransferError> {
        validate_compiled_job(job, &self.codec)?;
        self.ensure_connected()?;
        let existing_files = self.list_files_verified()?;
        if existing_files.len() >= usize::from(MAX_CONTROLLER_FILES) {
            return Err(RuidaTransferError::StorageFull);
        }
        let filename = allocate_upload_filename(&existing_files)?;
        Ok(RuidaUploadCursor {
            filename,
            stage: RuidaUploadStage::AnnounceFile,
            bytes_sent: 0,
            packets_acknowledged: 0,
            total_packets: job.clear_bytes.len().div_ceil(MAX_UDP_PAYLOAD_SIZE),
            attempts_before: self.send_attempts,
        })
    }

    /// Advance an upload by a bounded amount of work: the file announcement,
    /// up to `max_chunks` acknowledged data chunks, or the final storage
    /// verification. Callers holding shared locks can advance in small steps
    /// so stop, cancel, and status stay reachable between steps.
    pub fn advance_upload(
        &mut self,
        job: &RuidaCompiledJob,
        cursor: &mut RuidaUploadCursor,
        max_chunks: usize,
    ) -> Result<RuidaUploadStep, RuidaTransferError> {
        self.ensure_connected()?;
        match cursor.stage {
            RuidaUploadStage::AnnounceFile => {
                if let Err(error) = self.exchange(&file_transfer_command(&cursor.filename)?, false)
                {
                    self.recovery_required = true;
                    return Err(error);
                }
                cursor.stage = RuidaUploadStage::Chunks;
                Ok(RuidaUploadStep::Pending)
            }
            RuidaUploadStage::Chunks => {
                for _ in 0..max_chunks.max(1) {
                    if cursor.packets_acknowledged >= cursor.total_packets {
                        break;
                    }
                    let start = cursor.packets_acknowledged * MAX_UDP_PAYLOAD_SIZE;
                    let end = (start + MAX_UDP_PAYLOAD_SIZE).min(job.clear_bytes.len());
                    if let Err(error) = self.exchange(&job.clear_bytes[start..end], false) {
                        self.recovery_required = true;
                        return Err(error);
                    }
                    cursor.packets_acknowledged += 1;
                    cursor.bytes_sent = end;
                }
                if cursor.packets_acknowledged >= cursor.total_packets {
                    cursor.stage = RuidaUploadStage::Verify;
                }
                Ok(RuidaUploadStep::Pending)
            }
            RuidaUploadStage::Verify => {
                let file = self
                    .find_unique_file_with_retry(&cursor.filename)?
                    .ok_or(RuidaTransferError::UploadNotConfirmed)?;
                Ok(RuidaUploadStep::Complete(RuidaUploadReceipt {
                    controller_card_id: self
                        .verified_card_id
                        .expect("upload requires an exact verified controller identity"),
                    file,
                    clear_byte_len: job.clear_bytes.len(),
                    clear_sha256: sha256_hex(&job.clear_bytes),
                    packets_acknowledged: cursor.total_packets,
                    send_attempts: self.send_attempts - cursor.attempts_before,
                }))
            }
        }
    }

    /// Verified abort of a staged upload. After the announcement the
    /// controller consumes every datagram as file data, so the stream must
    /// first be closed with the protocol's own end-of-file terminator (the
    /// same byte that ends every normal upload); the truncated file the
    /// controller then finalizes is deleted with confirmation. Returns Ok
    /// only when the controller is verifiably out of the upload state with
    /// no leftover file; any error leaves the state ambiguous.
    pub fn abort_upload(&mut self, cursor: &RuidaUploadCursor) -> Result<(), RuidaTransferError> {
        self.ensure_connected()?;
        match cursor.stage {
            // Nothing has been sent: the controller never entered the
            // upload state.
            RuidaUploadStage::AnnounceFile => Ok(()),
            RuidaUploadStage::Chunks => {
                if let Err(error) = self.exchange(UPLOAD_END_OF_FILE, false) {
                    self.recovery_required = true;
                    return Err(error);
                }
                self.delete_file_by_name_confirmed(&cursor.filename)
            }
            // Every byte was already sent; the file is complete on the
            // controller and just needs deleting.
            RuidaUploadStage::Verify => self.delete_file_by_name_confirmed(&cursor.filename),
        }
    }

    fn delete_file_by_name_confirmed(&mut self, filename: &str) -> Result<(), RuidaTransferError> {
        let Some(file) = self.find_unique_file_with_retry(filename)? else {
            return Err(RuidaTransferError::UploadNotConfirmed);
        };
        self.exchange(&delete_document_command(file.index)?, false)?;
        for attempt in 1..=self.config.storage_verification_attempts {
            if !self
                .list_files_verified()?
                .iter()
                .any(|listed| listed.name.eq_ignore_ascii_case(filename))
            {
                return Ok(());
            }
            if attempt < self.config.storage_verification_attempts {
                std::thread::sleep(self.config.storage_verification_delay);
            }
        }
        Err(RuidaTransferError::DeleteNotConfirmed)
    }

    pub fn inspect_receipt(
        &mut self,
        receipt: &RuidaUploadReceipt,
    ) -> Result<RuidaStoredFile, RuidaTransferError> {
        self.ensure_connected()?;
        if receipt.controller_card_id != self.verified_card_id.unwrap_or_default() {
            return Err(RuidaTransferError::ReceiptNoLongerUnique);
        }
        let matches: Vec<_> = self
            .list_files_verified()?
            .into_iter()
            .filter(|file| file.name == receipt.file.name)
            .collect();
        if matches.len() != 1 {
            return Err(RuidaTransferError::ReceiptNoLongerUnique);
        }
        Ok(matches
            .into_iter()
            .next()
            .expect("one receipt match exists"))
    }

    pub fn delete_receipt(
        &mut self,
        receipt: &RuidaUploadReceipt,
    ) -> Result<(), RuidaTransferError> {
        let file = self.inspect_receipt(receipt)?;
        self.exchange(&delete_document_command(file.index)?, false)?;
        for attempt in 1..=self.config.storage_verification_attempts {
            if !self
                .list_files_verified()?
                .iter()
                .any(|listed| listed.name == receipt.file.name)
            {
                return Ok(());
            }
            if attempt < self.config.storage_verification_attempts {
                std::thread::sleep(self.config.storage_verification_delay);
            }
        }
        Err(RuidaTransferError::DeleteNotConfirmed)
    }

    pub(crate) fn read_machine_status(&mut self) -> Result<u64, RuidaTransferError> {
        self.ensure_connected()?;
        self.read_memory_unverified(MEMORY_MACHINE_STATUS)
    }

    pub(crate) fn send_acknowledged_command(
        &mut self,
        clear_payload: &[u8],
    ) -> Result<(), RuidaTransferError> {
        self.ensure_connected()?;
        self.exchange(clear_payload, false)?;
        Ok(())
    }

    fn ensure_connected(&mut self) -> Result<(), RuidaTransferError> {
        if self.recovery_required {
            return Err(RuidaTransferError::RecoveryRequired);
        }
        if self.verified_card_id.is_none() {
            self.connect()?;
        }
        Ok(())
    }

    fn list_files_verified(&mut self) -> Result<Vec<RuidaStoredFile>, RuidaTransferError> {
        let count = self.read_memory_unverified(MEMORY_FILE_COUNT)?;
        if count > u64::from(MAX_CONTROLLER_FILES) {
            return Err(RuidaTransferError::InvalidFileCount(count));
        }
        let mut files = Vec::with_capacity(count as usize);
        for index in 1..=count as u16 {
            let reply = self
                .exchange(&document_name_command(index)?, true)?
                .expect("document-name query requests a reply");
            let (reply_index, name) = parse_document_name_reply(&reply)?;
            if reply_index != index {
                return Err(RuidaTransferError::UnexpectedReply(reply));
            }
            files.push(RuidaStoredFile { index, name });
        }
        Ok(files)
    }

    fn find_unique_file_with_retry(
        &mut self,
        filename: &str,
    ) -> Result<Option<RuidaStoredFile>, RuidaTransferError> {
        for attempt in 1..=self.config.storage_verification_attempts {
            let matches: Vec<_> = self
                .list_files_verified()?
                .into_iter()
                .filter(|file| file.name.eq_ignore_ascii_case(filename))
                .collect();
            match matches.len() {
                1 => return Ok(matches.into_iter().next()),
                count if count > 1 => return Ok(None),
                _ if attempt < self.config.storage_verification_attempts => {
                    std::thread::sleep(self.config.storage_verification_delay);
                }
                _ => {}
            }
        }
        Ok(None)
    }

    fn read_memory_unverified(&mut self, address: u16) -> Result<u64, RuidaTransferError> {
        let reply = self
            .exchange(&memory_read_command(address), true)?
            .expect("memory query requests a reply");
        let reply = parse_memory_reply(&reply)?;
        if reply.address != address {
            return Err(RuidaTransferError::UnexpectedReply(Vec::from(
                reply.address.to_be_bytes(),
            )));
        }
        Ok(reply.value)
    }

    fn exchange(
        &mut self,
        clear_payload: &[u8],
        expect_reply: bool,
    ) -> Result<Option<Vec<u8>>, RuidaTransferError> {
        let datagram = self.codec.encode_datagram(clear_payload)?;
        for attempt in 1..=self.config.max_attempts {
            self.io.send(&datagram)?;
            self.send_attempts += 1;
            let acknowledgement = match self.io.receive(self.config.acknowledgement_timeout) {
                Ok(reply) => self.codec.decode_reply(&reply)?,
                Err(error) if is_timeout(&error) => {
                    return Err(RuidaTransferError::AcknowledgementTimeout { attempts: attempt });
                }
                Err(error) => return Err(error.into()),
            };
            match acknowledgement.as_slice() {
                [ACK] => {
                    if !expect_reply {
                        return Ok(None);
                    }
                    let reply = self
                        .io
                        .receive(self.config.reply_timeout)
                        .map_err(|error| {
                            if is_timeout(&error) {
                                RuidaTransferError::ReplyTimeout
                            } else {
                                error.into()
                            }
                        })?;
                    return Ok(Some(self.codec.decode_reply(&reply)?));
                }
                [NAK] => continue,
                [ERR] => return Err(RuidaTransferError::ControllerError),
                _ if expect_reply => return Ok(Some(acknowledgement)),
                _ => return Err(RuidaTransferError::UnexpectedReply(acknowledgement)),
            }
        }
        Err(RuidaTransferError::AcknowledgementTimeout {
            attempts: self.config.max_attempts,
        })
    }
}

fn allocate_upload_filename(
    existing_files: &[RuidaStoredFile],
) -> Result<String, RuidaTransferError> {
    for _ in 0..16 {
        let token = Uuid::new_v4().simple().to_string();
        let filename = normalize_upload_filename(&format!("BB{}", &token[..6]))?;
        if !existing_files
            .iter()
            .any(|file| file.name.eq_ignore_ascii_case(&filename))
        {
            return Ok(filename);
        }
    }
    Err(RuidaTransferError::FilenameAllocationFailed)
}

fn validate_compiled_job(
    job: &RuidaCompiledJob,
    codec: &RuidaCodec,
) -> Result<(), RuidaTransferError> {
    const FILE_SUM_TRAILER_LEN: usize = 8;
    let Some(trailer_start) = job.clear_bytes.len().checked_sub(FILE_SUM_TRAILER_LEN) else {
        return Err(RuidaTransferError::InvalidCompiledJob);
    };
    let trailer = &job.clear_bytes[trailer_start..];
    if trailer[..2] != [0xE5, 0x05]
        || trailer.last() != Some(&0xD7)
        || codec.swizzle(&job.clear_bytes) != job.rd_file_bytes
    {
        return Err(RuidaTransferError::InvalidCompiledJob);
    }
    let encoded_sum =
        decode_u35(&trailer[2..7]).map_err(|_| RuidaTransferError::InvalidCompiledJob)?;
    let calculated_sum = job.clear_bytes[..trailer_start]
        .iter()
        .try_fold(0_u64, |sum, byte| sum.checked_add(u64::from(*byte)))
        .and_then(|sum| sum.checked_add(0xD7))
        .ok_or(RuidaTransferError::InvalidCompiledJob)?;
    if encoded_sum != calculated_sum || job.clear_file_sum != calculated_sum {
        return Err(RuidaTransferError::InvalidCompiledJob);
    }
    Ok(())
}

fn is_timeout(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    )
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use beambench_common::{Bounds, Point2D};
    use beambench_planner::{ExecutionPlan, PlanSegment};

    use super::*;
    use crate::{RuidaCompilationConfig, compile_ruida_job};

    fn compiled_job() -> RuidaCompiledJob {
        compiled_vector_job(vec![Point2D::new(1.0, 2.0), Point2D::new(11.0, 12.0)])
    }

    fn compiled_vector_job(polyline: Vec<Point2D>) -> RuidaCompiledJob {
        let bounds =
            polyline
                .iter()
                .skip(1)
                .fold(Bounds::new(polyline[0], polyline[0]), |bounds, point| {
                    Bounds::new(
                        Point2D::new(bounds.min.x.min(point.x), bounds.min.y.min(point.y)),
                        Point2D::new(bounds.max.x.max(point.x), bounds.max.y.max(point.y)),
                    )
                });
        let plan = ExecutionPlan {
            id: Default::default(),
            project_id: Default::default(),
            revision_hash: "ruida-upload-test".to_string(),
            created_at: Default::default(),
            bounds,
            total_distance_mm: 0.0,
            estimated_duration_secs: 0.0,
            segments: vec![PlanSegment::Vector {
                polyline,
                closed: false,
                power_percent: 40.0,
                speed_mm_min: 1_200.0,
                layer_id: "layer-1".to_string(),
                cut_entry_id: "cut-1".to_string(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            }],
            layer_order: vec!["layer-1".to_string()],
            warnings: Vec::new(),
            failed_entries: Vec::new(),
        };
        compile_ruida_job(&plan, &RuidaCompilationConfig::default()).unwrap()
    }

    #[test]
    fn virtual_storage_round_trip_never_starts_the_job() {
        let job = compiled_job();
        let mut client =
            RuidaStorageClient::new(RuidaVirtualIo::rdc6442s(), RuidaTransferConfig::default())
                .unwrap();
        let mut progress = Vec::new();
        let receipt = client
            .upload_job_with_progress(&job, |value| progress.push(value))
            .unwrap();
        assert_eq!(receipt.controller_card_id(), RDC6442S_CARD_ID);
        assert_eq!(receipt.clear_byte_len(), job.clear_bytes.len());
        assert_eq!(receipt.clear_sha256().len(), 64);
        assert_eq!(progress.first().unwrap().phase, RuidaUploadPhase::Preparing);
        assert_eq!(progress.last().unwrap().phase, RuidaUploadPhase::Complete);
        assert_eq!(client.list_files().unwrap(), vec![receipt.file().clone()]);
        assert_eq!(client.inspect_receipt(&receipt).unwrap(), *receipt.file());
        assert_eq!(
            client
                .io()
                .controller()
                .file(&receipt.file().name)
                .unwrap()
                .clear_bytes,
            job.clear_bytes
        );
        assert_eq!(client.io().controller().machine_status(), 0);

        client.delete_receipt(&receipt).unwrap();
        assert!(client.list_files().unwrap().is_empty());
    }

    #[test]
    fn retries_an_explicit_data_packet_nak_without_duplicate_storage() {
        let job = compiled_job();
        let mut client =
            RuidaStorageClient::new(RuidaVirtualIo::rdc6442s(), RuidaTransferConfig::default())
                .unwrap();
        client.connect().unwrap();
        // Allow file-count and transfer-begin packets through, then reject the
        // first data packet once before accepting the retry.
        client.io_mut().nak_send_after(2);
        let receipt = client.upload_job(&job).unwrap();
        assert!(receipt.send_attempts() > receipt.packets_acknowledged());
        assert_eq!(client.io().controller().files().len(), 1);
        assert_eq!(
            client.io().controller().files()[0].name,
            receipt.file().name
        );
    }

    #[test]
    fn acknowledgement_timeout_does_not_resend_an_ambiguous_datagram() {
        let mut io = RuidaVirtualIo::rdc6442s();
        io.drop_next_sends(1);
        let mut client = RuidaStorageClient::new(io, RuidaTransferConfig::default()).unwrap();
        assert!(matches!(
            client.connect(),
            Err(RuidaTransferError::AcknowledgementTimeout { attempts: 1 })
        ));
        assert_eq!(client.send_attempts, 1);
        assert!(client.io().controller().files().is_empty());
    }

    #[test]
    fn partial_upload_is_discarded_on_virtual_reset() {
        let mut io = RuidaVirtualIo::rdc6442s();
        let codec = RuidaCodec::default();
        io.send(
            &codec
                .encode_datagram(&file_transfer_command("BBPART").unwrap())
                .unwrap(),
        )
        .unwrap();
        io.receive(Duration::ZERO).unwrap();
        io.send(&codec.encode_datagram(&[0xD8, 0x10]).unwrap())
            .unwrap();
        io.receive(Duration::ZERO).unwrap();
        assert!(io.controller().has_pending_upload());
        io.controller_mut().reset_connection();
        assert!(!io.controller().has_pending_upload());
        assert!(io.controller().files().is_empty());
    }

    #[test]
    fn ambiguous_data_timeout_requires_external_reset_before_more_io() {
        let job = compiled_job();
        let mut client =
            RuidaStorageClient::new(RuidaVirtualIo::rdc6442s(), RuidaTransferConfig::default())
                .unwrap();
        client.connect().unwrap();
        client.io_mut().drop_send_after(2);
        assert!(matches!(
            client.upload_job(&job),
            Err(RuidaTransferError::AcknowledgementTimeout { attempts: 1 })
        ));
        assert!(client.recovery_required());
        assert!(client.io().controller().has_pending_upload());
        assert!(matches!(
            client.list_files(),
            Err(RuidaTransferError::RecoveryRequired)
        ));

        let mut io = client.into_inner();
        io.controller_mut().reset_connection();
        let mut recovered = RuidaStorageClient::new(io, RuidaTransferConfig::default()).unwrap();
        assert!(recovered.list_files().unwrap().is_empty());
    }

    #[test]
    fn valid_large_job_is_split_across_multiple_packets_without_byte_loss() {
        let points = (0..700)
            .map(|index| {
                Point2D::new(
                    1.0 + f64::from(index) * 0.02,
                    if index % 2 == 0 { 2.0 } else { 2.25 },
                )
            })
            .collect();
        let job = compiled_vector_job(points);
        assert!(job.clear_bytes.len() > MAX_UDP_PAYLOAD_SIZE * 2);
        let mut client =
            RuidaStorageClient::new(RuidaVirtualIo::rdc6442s(), RuidaTransferConfig::default())
                .unwrap();
        let receipt = client.upload_job(&job).unwrap();
        assert_eq!(
            receipt.packets_acknowledged(),
            job.clear_bytes.len().div_ceil(MAX_UDP_PAYLOAD_SIZE)
        );
        assert_eq!(
            client
                .io()
                .controller()
                .file(&receipt.file().name)
                .unwrap()
                .clear_bytes,
            job.clear_bytes
        );
    }

    #[test]
    fn duplicate_names_invalid_config_and_unscoped_indices_are_rejected() {
        let job = compiled_job();
        let mut client =
            RuidaStorageClient::new(RuidaVirtualIo::rdc6442s(), RuidaTransferConfig::default())
                .unwrap();
        let receipt = client.upload_job(&job).unwrap();
        let filename = receipt.file().name.clone();
        let codec = RuidaCodec::default();
        let mut io = client.into_inner();
        io.send(
            &codec
                .encode_datagram(&file_transfer_command(&filename).unwrap())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            codec
                .decode_reply(&io.receive(Duration::ZERO).unwrap())
                .unwrap(),
            vec![ERR]
        );

        let invalid = RuidaTransferConfig {
            storage_verification_attempts: 0,
            ..RuidaTransferConfig::default()
        };
        assert!(matches!(
            RuidaStorageClient::new(RuidaVirtualIo::rdc6442s(), invalid),
            Err(RuidaTransferError::InvalidVerificationLimit)
        ));
        assert!(delete_document_command(0).is_err());
    }

    #[test]
    fn inconsistent_compiled_job_is_rejected_before_controller_io() {
        let mut job = compiled_job();
        job.clear_bytes.pop();
        let mut client =
            RuidaStorageClient::new(RuidaVirtualIo::rdc6442s(), RuidaTransferConfig::default())
                .unwrap();
        assert!(matches!(
            client.upload_job(&job),
            Err(RuidaTransferError::InvalidCompiledJob)
        ));
        assert_eq!(client.send_attempts, 0);
        assert!(client.io().controller().files().is_empty());
    }

    #[test]
    fn virtual_controller_enforces_observed_storage_limit() {
        let job = compiled_job();
        let codec = RuidaCodec::default();
        let mut io = RuidaVirtualIo::rdc6442s();
        for index in 0..MAX_CONTROLLER_FILES {
            let filename = format!("BB{index:06}");
            io.send(
                &codec
                    .encode_datagram(&file_transfer_command(&filename).unwrap())
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(
                codec
                    .decode_reply(&io.receive(Duration::ZERO).unwrap())
                    .unwrap(),
                vec![ACK]
            );
            io.send(&codec.encode_datagram(&job.clear_bytes).unwrap())
                .unwrap();
            assert_eq!(
                codec
                    .decode_reply(&io.receive(Duration::ZERO).unwrap())
                    .unwrap(),
                vec![ACK]
            );
        }
        assert_eq!(
            io.controller().files().len(),
            usize::from(MAX_CONTROLLER_FILES)
        );
        io.send(
            &codec
                .encode_datagram(&file_transfer_command("BBFULL").unwrap())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            codec
                .decode_reply(&io.receive(Duration::ZERO).unwrap())
                .unwrap(),
            vec![ERR]
        );

        let mut client = RuidaStorageClient::new(io, RuidaTransferConfig::default()).unwrap();
        assert!(matches!(
            client.upload_job(&job),
            Err(RuidaTransferError::StorageFull)
        ));
        assert!(!client.recovery_required());
    }
}
