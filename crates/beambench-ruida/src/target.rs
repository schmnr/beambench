use crate::protocol::{DEFAULT_MAGIC, KNOWN_MACHINE_STATUS_BITS, RDC6442S_CARD_ID, RUIDA_UDP_PORT};

/// Exact protocol fingerprint that Beam Bench permits to mutate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuidaCompatibilityTarget {
    pub model: &'static str,
    pub card_id: u64,
    pub transport: &'static str,
    pub port: u16,
    pub magic: u8,
    pub known_machine_status_bits: u64,
}

pub const RDC6442S_ETHERNET_TARGET: RuidaCompatibilityTarget = RuidaCompatibilityTarget {
    model: "RDC6442S",
    card_id: RDC6442S_CARD_ID,
    transport: "ethernet_udp",
    port: RUIDA_UDP_PORT,
    magic: DEFAULT_MAGIC,
    known_machine_status_bits: KNOWN_MACHINE_STATUS_BITS,
};

/// Targets with enough evidence to permit upload, execution, or motion.
///
/// RDC6445G must not be added until a read-only probe supplies its real
/// fingerprint and the compatibility fixtures pass.
pub const KNOWN_RUIDA_TARGETS: &[RuidaCompatibilityTarget] = &[RDC6442S_ETHERNET_TARGET];

pub fn target_for_card_id(card_id: u64) -> Option<RuidaCompatibilityTarget> {
    KNOWN_RUIDA_TARGETS
        .iter()
        .copied()
        .find(|target| target.card_id == card_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_matches_only_evidence_backed_targets() {
        assert_eq!(
            target_for_card_id(RDC6442S_CARD_ID),
            Some(RDC6442S_ETHERNET_TARGET)
        );
        assert_eq!(target_for_card_id(0x1234_5678), None);
    }
}
