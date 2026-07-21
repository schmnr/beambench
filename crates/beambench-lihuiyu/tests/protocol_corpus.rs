use beambench_lihuiyu::{
    LihuiyuPadding, encode_m2_raster_speed, encode_m2_vector_speed, encode_packet,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Corpus {
    schema_version: u32,
    packets: Vec<PacketCase>,
    speed_codes: Vec<SpeedCase>,
}

#[derive(Debug, Deserialize)]
struct PacketCase {
    payload_ascii: String,
    packet_hex: String,
}

#[derive(Debug, Deserialize)]
struct SpeedCase {
    kind: String,
    speed_mm_s: f64,
    #[serde(default)]
    raster_step_mils: Option<u16>,
    encoded_ascii: String,
}

fn decode_hex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let pair = std::str::from_utf8(pair).unwrap();
            u8::from_str_radix(pair, 16).unwrap()
        })
        .collect()
}

#[test]
fn pinned_protocol_corpus_matches_encoder() {
    let corpus: Corpus =
        serde_json::from_str(include_str!("fixtures/protocol_corpus.json")).unwrap();
    assert_eq!(corpus.schema_version, 1);

    for case in corpus.packets {
        let packet = encode_packet(case.payload_ascii.as_bytes(), LihuiyuPadding::AsciiF).unwrap();
        assert_eq!(packet.as_ref(), decode_hex(&case.packet_hex));
    }

    for case in corpus.speed_codes {
        let encoded = match case.kind.as_str() {
            "vector" => encode_m2_vector_speed(case.speed_mm_s).unwrap(),
            "raster" => encode_m2_raster_speed(
                case.speed_mm_s,
                case.raster_step_mils.expect("raster corpus step"),
            )
            .unwrap(),
            other => panic!("unknown corpus speed kind {other}"),
        };
        assert_eq!(encoded, case.encoded_ascii.as_bytes());
    }
}
