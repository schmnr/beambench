//! Ruida controller protocol support.
//!
//! The implementation separates offline native-job compilation, controller
//! storage transfer, and live execution. The storage client can upload and
//! verify uniquely named files but deliberately exposes no start command.

pub mod adapter;
pub mod compiler;
pub mod protocol;
pub mod runtime;
pub mod transport;
pub mod virtual_controller;

pub use adapter::{RuidaAdapterDescriptor, RuidaEthernetAdapter};
pub use compiler::{
    RuidaCompilationConfig, RuidaCompilationError, RuidaCompiledJob, RuidaCoordinateTransform,
    RuidaLayerSummary, RuidaPowerOverride, RuidaReferencePoint, compile_ruida_job,
    compile_ruida_storage_sentinel,
};
pub use protocol::{
    ACK, DEFAULT_MAGIC, ENQ, ERR, KNOWN_MACHINE_STATUS_BITS, MACHINE_STATUS_JOB_RUNNING,
    MACHINE_STATUS_MOVING, MACHINE_STATUS_PART_END, MAX_CONTROLLER_FILENAME_BYTES,
    MAX_CONTROLLER_FILES, MAX_UDP_DATAGRAM_SIZE, MAX_UDP_PAYLOAD_SIZE, MEMORY_FILE_COUNT, NAK,
    RDC6442S_CARD_ID, RUIDA_UDP_PORT, RuidaCodec, RuidaJogAxis, RuidaManualMotionCommand,
    RuidaMemoryReply, RuidaProcessAction, RuidaProtocolError, decode_i14, decode_i32,
    decode_power_percent, decode_u14, decode_u35, delete_document_command, document_name_command,
    document_name_reply, encode_i14, encode_i32, encode_power_percent, encode_speed_mm_s,
    encode_u14, encode_u35, enquiry_command, file_transfer_command, home_xy_command,
    jog_speed_command, memory_read_command, memory_reply, normalize_upload_filename,
    parse_delete_document_command, parse_document_name_reply, parse_file_transfer_command,
    parse_manual_motion_command, parse_memory_reply, parse_process_control_command,
    parse_select_document_command, process_control_command, relative_jog_command,
    select_document_command,
};
pub use runtime::{
    RuidaRecoveryReason, RuidaRuntime, RuidaRuntimeConfig, RuidaRuntimeError, RuidaRuntimePhase,
    RuidaRuntimeSnapshot,
};
pub use transport::{
    RUIDA_UDP_REPLY_PORT, RuidaDatagramIo, RuidaStorageClient, RuidaStoredFile,
    RuidaTransferConfig, RuidaTransferError, RuidaUdpIo, RuidaUploadPhase, RuidaUploadProgress,
    RuidaUploadReceipt, RuidaVirtualIo,
};
pub use virtual_controller::{
    RDC6442S_ETHERNET_TARGET, RuidaCompatibilityTarget, RuidaVirtualController,
    RuidaVirtualExecutionState, RuidaVirtualFile, RuidaVirtualResponse,
};
