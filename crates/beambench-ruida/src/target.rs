use crate::protocol::{DEFAULT_MAGIC, KNOWN_MACHINE_STATUS_BITS, RDC6442S_CARD_ID, RUIDA_UDP_PORT};

/// Ruida protocol settings selected after the controller answers the
/// read-only compatibility probe.
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

/// Hardware fingerprints that have a dedicated compatibility row.
pub const KNOWN_RUIDA_TARGETS: &[RuidaCompatibilityTarget] = &[RDC6442S_ETHERNET_TARGET];

pub fn target_for_card_id(card_id: u64) -> Option<RuidaCompatibilityTarget> {
    KNOWN_RUIDA_TARGETS
        .iter()
        .copied()
        .find(|target| target.card_id == card_id)
}

/// Select a dedicated target when one exists, otherwise create an experimental
/// target after the controller has answered both identity and status queries.
///
/// The Ruida choice in the UI is already explicitly experimental. Requiring a
/// hard-coded card ID as well made real controller testing impossible. Unknown
/// status bits are retained for diagnostics and ignored by the generic target;
/// the runtime still requires the established idle, running, moving, and
/// part-end transitions before it reports success.
pub fn target_for_probe(
    card_id: u64,
    mainboard_version: Option<&str>,
    machine_status: Option<u64>,
) -> Option<RuidaCompatibilityTarget> {
    if let Some(target) = target_for_card_id(card_id) {
        return Some(target);
    }
    machine_status?;
    let model = mainboard_version
        .filter(|version| version.to_ascii_uppercase().contains("6445G"))
        .map_or("Ruida (unverified)", |_| "RDC6445G");
    Some(RuidaCompatibilityTarget {
        model,
        card_id,
        transport: "ethernet_udp",
        port: RUIDA_UDP_PORT,
        magic: DEFAULT_MAGIC,
        known_machine_status_bits: u64::MAX,
    })
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

    #[test]
    fn probe_accepts_rdc6445g_as_an_experimental_target() {
        let target = target_for_probe(0x1234_5678, Some("RDC6445G-V1.0"), Some(0)).unwrap();
        assert_eq!(target.model, "RDC6445G");
        assert_eq!(target.card_id, 0x1234_5678);
        assert_eq!(target.known_machine_status_bits, u64::MAX);
    }

    #[test]
    fn probe_requires_a_working_status_query_for_unknown_targets() {
        assert_eq!(target_for_probe(0x1234_5678, None, None), None);
    }
}
