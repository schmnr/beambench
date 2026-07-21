use beambench_ruida::{
    ACK, NAK, RDC6442S_CARD_ID, RuidaCodec, RuidaJogAxis, RuidaManualMotionCommand,
    RuidaProcessAction, RuidaProtocolError, RuidaVirtualController, decode_i14, decode_i32,
    decode_power_percent, decode_u14, decode_u35, delete_document_command, document_name_command,
    document_name_reply, encode_i14, encode_i32, encode_power_percent, encode_speed_mm_s,
    encode_u14, encode_u35, file_transfer_command, home_xy_command, jog_speed_command,
    memory_read_command, normalize_upload_filename, parse_delete_document_command,
    parse_document_name_reply, parse_file_transfer_command, parse_manual_motion_command,
    parse_memory_reply, parse_process_control_command, parse_select_document_command,
    process_control_command, relative_jog_command, select_document_command,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Corpus {
    schema_version: u32,
    cases: Vec<DatagramCase>,
    replies: Vec<ReplyCase>,
}

#[derive(Debug, Deserialize)]
struct DatagramCase {
    name: String,
    magic: u8,
    clear_payload_hex: String,
    wire_datagram_hex: String,
}

#[derive(Debug, Deserialize)]
struct ReplyCase {
    name: String,
    magic: u8,
    clear_reply_hex: String,
    wire_reply_hex: String,
}

fn corpus() -> Corpus {
    serde_json::from_str(include_str!("fixtures/protocol_corpus.json")).unwrap()
}

fn hex(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0, "hex fixture has an even length");
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).unwrap())
        .collect()
}

#[test]
fn protocol_corpus_matches_encoder_and_decoder() {
    let corpus = corpus();
    assert_eq!(corpus.schema_version, 1);
    for case in corpus.cases {
        let codec = RuidaCodec::new(case.magic);
        let clear = hex(&case.clear_payload_hex);
        let wire = hex(&case.wire_datagram_hex);
        assert_eq!(
            codec.encode_datagram(&clear).unwrap(),
            wire,
            "{}",
            case.name
        );
        assert_eq!(
            codec.decode_datagram(&wire).unwrap(),
            clear,
            "{}",
            case.name
        );
    }
    for case in corpus.replies {
        let codec = RuidaCodec::new(case.magic);
        let clear = hex(&case.clear_reply_hex);
        let wire = hex(&case.wire_reply_hex);
        assert_eq!(codec.encode_reply(&clear).unwrap(), wire, "{}", case.name);
        assert_eq!(codec.decode_reply(&wire).unwrap(), clear, "{}", case.name);
    }
}

#[test]
fn numeric_wire_values_round_trip_at_boundaries() {
    for value in [0, 1, 0x1FFF, 0x3FFF] {
        let encoded = encode_u14(value).unwrap();
        assert_eq!(decode_u14(&encoded).unwrap(), value);
    }
    for value in [0, 1, u64::from(u32::MAX), 0x07_FFFF_FFFF] {
        let encoded = encode_u35(value).unwrap();
        assert_eq!(decode_u35(&encoded).unwrap(), value);
    }
    for value in [i32::MIN, -1, 0, 1, i32::MAX] {
        assert_eq!(decode_i32(&encode_i32(value)).unwrap(), value);
    }
    for value in [-8_192, -1, 0, 1, 8_191] {
        assert_eq!(decode_i14(&encode_i14(value).unwrap()).unwrap(), value);
    }
    for percent in [0.0, 12.5, 50.0, 100.0] {
        let encoded = encode_power_percent(percent).unwrap();
        let decoded = decode_power_percent(&encoded).unwrap();
        assert!((decoded - percent).abs() <= 100.0 / 0x3FFF as f64);
    }
    assert_eq!(encode_speed_mm_s(25.0).unwrap(), [0, 0, 1, 67, 40]);
    assert!(encode_u14(0x4000).is_err());
    assert!(encode_u35(0x08_0000_0000).is_err());
    assert!(encode_power_percent(100.1).is_err());
    assert!(encode_speed_mm_s(f64::NAN).is_err());
}

#[test]
fn checksum_corruption_is_rejected_and_virtual_controller_naks_it() {
    let codec = RuidaCodec::default();
    let mut packet = codec.encode_datagram(&memory_read_command(0x057E)).unwrap();
    packet[0] ^= 0x01;
    assert!(matches!(
        codec.decode_datagram(&packet),
        Err(RuidaProtocolError::ChecksumMismatch { .. })
    ));
    let response = RuidaVirtualController::rdc6442s().receive_datagram(&packet);
    assert!(!response.accepted);
    assert_eq!(codec.decode_reply(&response.datagrams[0]).unwrap(), [NAK]);
}

#[test]
fn virtual_rdc6442s_answers_read_only_identity_and_status_queries() {
    let codec = RuidaCodec::default();
    let mut controller = RuidaVirtualController::rdc6442s();
    controller.set_machine_status(0x0100_0001);

    let identity =
        controller.receive_datagram(&codec.encode_datagram(&memory_read_command(0x057E)).unwrap());
    assert!(identity.accepted);
    assert_eq!(codec.decode_reply(&identity.datagrams[0]).unwrap(), [ACK]);
    let identity_reply =
        parse_memory_reply(&codec.decode_reply(&identity.datagrams[1]).unwrap()).unwrap();
    assert_eq!(identity_reply.address, 0x057E);
    assert_eq!(identity_reply.value, RDC6442S_CARD_ID);

    let status =
        controller.receive_datagram(&codec.encode_datagram(&memory_read_command(0x0400)).unwrap());
    let status_reply =
        parse_memory_reply(&codec.decode_reply(&status.datagrams[1]).unwrap()).unwrap();
    assert_eq!(status_reply.address, 0x0400);
    assert_eq!(status_reply.value, 0x0100_0001);
}

#[test]
fn controller_storage_commands_match_observed_wire_contract() {
    assert_eq!(
        file_transfer_command("bb7a2f").unwrap(),
        b"\xE8\x02\xE7\x01BB7A2F\0"
    );
    assert_eq!(
        parse_file_transfer_command(b"\xE8\x02\xE7\x01BB7A2F\0").unwrap(),
        "BB7A2F"
    );
    assert_eq!(document_name_command(1).unwrap(), [0xE8, 0x01, 0, 1]);
    let name_reply = document_name_reply(1, "BB7A2F").unwrap();
    assert_eq!(name_reply, b"\xE8\x01\0\x01BB7A2F\0");
    assert_eq!(
        parse_document_name_reply(&name_reply).unwrap(),
        (1, "BB7A2F".to_string())
    );
    assert_eq!(
        delete_document_command(1).unwrap(),
        [0xE8, 0x00, 0, 1, 0, 1]
    );
    assert_eq!(
        parse_delete_document_command(&[0xE8, 0x00, 0, 1, 0, 1]).unwrap(),
        1
    );
    assert_eq!(normalize_upload_filename("bb_test").unwrap(), "BB_TEST");
    assert!(normalize_upload_filename("USERFILE").is_err());
    assert!(document_name_command(0).is_err());
    assert!(delete_document_command(0).is_err());
}

#[test]
fn controller_process_commands_match_observed_wire_contract() {
    assert_eq!(select_document_command(1).unwrap(), [0xE8, 0x03, 0, 1]);
    assert_eq!(
        parse_select_document_command(&[0xE8, 0x03, 0, 1]).unwrap(),
        1
    );
    for (action, bytes) in [
        (RuidaProcessAction::Start, [0xD8, 0x00]),
        (RuidaProcessAction::Stop, [0xD8, 0x01]),
        (RuidaProcessAction::Pause, [0xD8, 0x02]),
        (RuidaProcessAction::Resume, [0xD8, 0x03]),
    ] {
        assert_eq!(process_control_command(action), bytes);
        assert_eq!(parse_process_control_command(&bytes).unwrap(), action);
    }
    assert!(select_document_command(0).is_err());
    assert!(parse_process_control_command(&[0xD8, 0x04]).is_err());
}

#[test]
fn manual_motion_commands_match_observed_output_disabled_contract() {
    assert_eq!(home_xy_command(), [0xD8, 0x2A]);
    assert_eq!(
        parse_manual_motion_command(&home_xy_command()).unwrap(),
        RuidaManualMotionCommand::HomeXy
    );

    let speed = jog_speed_command(36_000.0).unwrap();
    assert_eq!(speed, [0xC9, 0x02, 0, 0, 0x24, 0x4F, 0x40]);
    assert_eq!(
        parse_manual_motion_command(&speed).unwrap(),
        RuidaManualMotionCommand::SetSpeed {
            micrometres_per_second: 600_000
        }
    );

    let x = relative_jog_command(RuidaJogAxis::X, 1.5).unwrap();
    assert_eq!(x, [0xD9, 0x00, 0x02, 0, 0, 0, 0x0B, 0x5C]);
    assert_eq!(
        parse_manual_motion_command(&x).unwrap(),
        RuidaManualMotionCommand::MoveRelative {
            axis: RuidaJogAxis::X,
            micrometres: 1_500
        }
    );

    let y = relative_jog_command(RuidaJogAxis::Y, -2.0).unwrap();
    assert_eq!(
        parse_manual_motion_command(&y).unwrap(),
        RuidaManualMotionCommand::MoveRelative {
            axis: RuidaJogAxis::Y,
            micrometres: -2_000
        }
    );
    assert_eq!(y[..3], [0xD9, 0x01, 0x02]);

    let z = relative_jog_command(RuidaJogAxis::Z, 0.5).unwrap();
    assert_eq!(z, [0xD9, 0x02, 0x02, 0, 0, 0, 0x03, 0x74]);
    assert_eq!(
        parse_manual_motion_command(&z).unwrap(),
        RuidaManualMotionCommand::MoveRelative {
            axis: RuidaJogAxis::Z,
            micrometres: 500
        }
    );

    let u = relative_jog_command(RuidaJogAxis::U, -0.5).unwrap();
    assert_eq!(u[..3], [0xD9, 0x03, 0x02]);
    assert_eq!(
        parse_manual_motion_command(&u).unwrap(),
        RuidaManualMotionCommand::MoveRelative {
            axis: RuidaJogAxis::U,
            micrometres: -500
        }
    );
    assert!(jog_speed_command(0.0).is_err());
    assert!(jog_speed_command(0.01).is_err());
    assert!(relative_jog_command(RuidaJogAxis::X, 0.0).is_err());
    assert!(relative_jog_command(RuidaJogAxis::X, 0.000_1).is_err());
    assert!(parse_manual_motion_command(&[0xD9, 0x10, 0x02]).is_err());
}
