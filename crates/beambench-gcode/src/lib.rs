//! Shared line-oriented G-code protocol foundations.
//!
//! This crate deliberately owns no serial or network transport and does not
//! assume GRBL realtime behavior. Controller adapters classify their incoming
//! lines and use the acknowledgement flow to enforce one-command-per-ack
//! operation until a stronger dialect-specific flow contract is proven.

#![forbid(unsafe_code)]

mod acknowledgement;
mod response;

pub use acknowledgement::{
    AckFlowConfig, AckFlowError, AckFlowProgress, AckFlowUpdate, AcknowledgedLineFlow, ReadyLine,
};
pub use response::{
    AcknowledgedGcodeDialect, LineProtocolEvent, classify_marlin_response, classify_response,
    classify_smoothieware_response,
};
