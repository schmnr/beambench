use std::collections::VecDeque;
use std::sync::{LazyLock, Mutex};

use beambench_common::feedback::DiagnosticSerialTraffic;

const MAX_RING_BYTES: usize = 1024;
const SNAPSHOT_BYTES: usize = 50;

#[derive(Default)]
struct SerialTrafficRing {
    tx: VecDeque<u8>,
    rx: VecDeque<u8>,
}

static SERIAL_TRAFFIC: LazyLock<Mutex<SerialTrafficRing>> =
    LazyLock::new(|| Mutex::new(SerialTrafficRing::default()));

pub fn reset_serial_traffic() {
    if let Ok(mut ring) = SERIAL_TRAFFIC.lock() {
        ring.tx.clear();
        ring.rx.clear();
    }
}

pub fn record_tx(bytes: &[u8]) {
    record(bytes, Direction::Tx);
}

pub fn record_rx(bytes: &[u8]) {
    record(bytes, Direction::Rx);
}

pub fn recent_serial_traffic() -> DiagnosticSerialTraffic {
    let Ok(ring) = SERIAL_TRAFFIC.lock() else {
        return DiagnosticSerialTraffic::default();
    };

    let tx = last_bytes(&ring.tx);
    let rx = last_bytes(&ring.rx);

    DiagnosticSerialTraffic {
        tx_hex: bytes_to_hex(&tx),
        tx_ascii: bytes_to_ascii(&tx),
        rx_hex: bytes_to_hex(&rx),
        rx_ascii: bytes_to_ascii(&rx),
    }
}

enum Direction {
    Tx,
    Rx,
}

fn record(bytes: &[u8], direction: Direction) {
    let Ok(mut ring) = SERIAL_TRAFFIC.lock() else {
        return;
    };

    let target = match direction {
        Direction::Tx => &mut ring.tx,
        Direction::Rx => &mut ring.rx,
    };

    for byte in bytes {
        target.push_back(*byte);
        while target.len() > MAX_RING_BYTES {
            target.pop_front();
        }
    }
}

fn last_bytes(ring: &VecDeque<u8>) -> Vec<u8> {
    ring.iter()
        .skip(ring.len().saturating_sub(SNAPSHOT_BYTES))
        .copied()
        .collect()
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn bytes_to_ascii(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| {
            if byte.is_ascii_graphic() || *byte == b' ' {
                *byte as char
            } else {
                '.'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_recent_tx_and_rx_as_hex_and_ascii() {
        reset_serial_traffic();
        record_tx(b"$I\n");
        record_rx(b"Grbl 1.1h ['$' for help]\r\n");

        let snapshot = recent_serial_traffic();

        assert_eq!(snapshot.tx_hex, "24 49 0A");
        assert_eq!(snapshot.tx_ascii, "$I.");
        assert!(snapshot.rx_hex.contains("47 72 62 6C"));
        assert!(snapshot.rx_ascii.contains("Grbl 1.1h"));
    }

    #[test]
    fn keeps_only_recent_bytes() {
        reset_serial_traffic();
        let bytes = vec![b'a'; MAX_RING_BYTES + 20];
        record_tx(&bytes);

        let snapshot = recent_serial_traffic();

        assert_eq!(snapshot.tx_ascii.len(), SNAPSHOT_BYTES);
    }
}
