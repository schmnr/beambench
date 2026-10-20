//! Keep public D1 serial observations distinct from a validated GRBL session.
//! Source and scope are recorded in the fixture and docs/machines/xtool-d1.md.

use beambench_common::{MachineRunState, SessionState};
use beambench_grbl::GrblSession;
use beambench_serial::MockSerialTransport;
use serde::Deserialize;

#[derive(Deserialize)]
struct D1Observations {
    status_replies: Vec<String>,
    identity_reply_orders: Vec<Vec<String>>,
}

#[test]
fn d1_position_and_acknowledgements_do_not_establish_grbl_readiness() {
    let observations: D1Observations =
        serde_json::from_str(include_str!("fixtures/xtool_d1_serial_v1.json")).unwrap();

    for identity_replies in &observations.identity_reply_orders {
        let mut transport = MockSerialTransport::new("d1-public-capture");
        for reply in observations.status_replies.iter().chain(identity_replies) {
            transport.enqueue_response(reply);
        }
        let handle = transport.handle();
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.poll().unwrap();

        // A position without an explicit run state cannot establish Idle,
        // completion, or permission to send a job.
        assert_eq!(session.last_status().run_state, MachineRunState::Unknown);
        assert_eq!(session.session_state(), SessionState::WaitingForBanner);
        assert!(session.grbl_family_identity().positive_identity().is_none());
        assert!(handle.sent_lines().is_empty());
        assert!(handle.sent_bytes().is_empty());
    }
}
