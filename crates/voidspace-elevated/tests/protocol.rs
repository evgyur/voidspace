use std::io::Cursor;

use voidspace_elevated::{
    MAX_FRAME_BYTES, PROTOCOL_VERSION, PeerClaim, ProtocolError, ProtocolGuard, Request, RequestId,
    RequestKind, read_frame, write_frame,
};

fn peer() -> PeerClaim {
    PeerClaim {
        pid: 42,
        executable: r"C:\Voidspace\voidspace.exe".into(),
        session_id: 7,
        nonce: [9; 32],
    }
}

#[test]
fn turbo_negotiation_is_explicit_about_safe_fallback() {
    assert!(matches!(
        voidspace_elevated::turbo_mode_for(std::path::Path::new(r"C:\")),
        voidspace_elevated::TurboMode::PrivilegedTraversalFallback { .. }
    ));
}

fn request(id: u64, sequence: u64) -> Request {
    Request {
        version: PROTOCOL_VERSION,
        id: RequestId(id),
        sequence,
        peer: peer(),
        kind: RequestKind::Probe,
    }
}

#[test]
fn frame_round_trip_and_bounds() {
    let expected = request(1, 1);
    let mut bytes = Vec::new();
    write_frame(&mut bytes, &expected).unwrap();
    assert_eq!(read_frame::<Request>(Cursor::new(bytes)).unwrap(), expected);

    let oversized = ((MAX_FRAME_BYTES + 1) as u32).to_le_bytes().to_vec();
    assert!(matches!(
        read_frame::<Request>(Cursor::new(oversized)),
        Err(ProtocolError::OversizedFrame)
    ));
}

#[test]
fn rejects_duplicate_non_monotonic_and_wrong_peer() {
    let mut guard = ProtocolGuard::new(peer());
    guard.accept(&request(1, 1)).unwrap();
    assert!(matches!(
        guard.accept(&request(1, 2)),
        Err(ProtocolError::DuplicateRequest)
    ));
    assert!(matches!(
        guard.accept(&request(2, 1)),
        Err(ProtocolError::NonMonotonicSequence)
    ));
    let mut wrong = request(3, 3);
    wrong.peer.pid = 99;
    assert!(matches!(
        guard.accept(&wrong),
        Err(ProtocolError::WrongPeer)
    ));
}
