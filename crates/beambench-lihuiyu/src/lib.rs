//! Lihuiyu M2/M3 Nano controller protocol support.
//!
//! The first product row uses the documented M2-compatible contract over a
//! CH341 USB interface. Stock M2 hardware exposes binary beam on/off only;
//! optical power remains controlled by the machine's physical panel.

pub mod compiler;
pub mod protocol;
pub mod runtime;
pub mod transport;
pub mod usb;
pub mod virtual_controller;

pub use compiler::{
    LihuiyuCompilationConfig, LihuiyuCompilationError, LihuiyuCompiledJob,
    LihuiyuCoordinateTransform, LihuiyuJobSummary, compile_lihuiyu_job,
};

pub use protocol::{
    BULK_READ_ENDPOINT, BULK_WRITE_ENDPOINT, CH341_EPP_DATA_WRITE, CH341_STATUS_REQUEST,
    LIHUIYU_M2_NANO_TARGET, LihuiyuCompatibilityTarget, LihuiyuPacket, LihuiyuPadding,
    LihuiyuProtocolError, LihuiyuStatus, M2_MILLIMETRES_PER_STEP, PACKET_HEADER,
    PACKET_PAYLOAD_SIZE, PACKET_SIZE, USB_PRODUCT_ID, USB_VENDOR_ID, crc8, decode_packet,
    encode_distance, encode_epp_bulk_write, encode_m2_raster_speed, encode_m2_vector_speed,
    encode_packet, parse_status_reply, status_bulk_request,
};
pub use runtime::{
    LihuiyuRecoveryReason, LihuiyuRuntime, LihuiyuRuntimeConfig, LihuiyuRuntimeError,
    LihuiyuRuntimePhase, LihuiyuRuntimeSnapshot,
};
pub use transport::{
    LihuiyuTransferConfig, LihuiyuTransferError, LihuiyuTransferPhase, LihuiyuTransferProgress,
    LihuiyuTransferReceipt, LihuiyuTransport, LihuiyuUsbIo,
};
pub use usb::{
    LihuiyuUsbDeviceInfo, LihuiyuUsbError, LihuiyuUsbSelector, NativeLihuiyuUsbIo,
    enumerate_lihuiyu_usb_devices,
};
pub use virtual_controller::{LihuiyuVirtualController, LihuiyuVirtualUsbIo};
