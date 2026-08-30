use crate::libs::models::*;
use bincode::config::standard;

// These indices are the on-disk vault format. If any assertion here fails,
// existing encrypted vaults will fail to decode -> data loss. Do not "fix"
// a failure by changing the expected index; preserve the enum order instead.
#[test]
fn connection_kind_bincode_indices_are_stable() {
    let bytes = bincode::serde::encode_to_vec(ConnectionKind::Both, standard()).unwrap();
    assert_eq!(
        bytes,
        vec![3u8],
        "ConnectionKind::Both must stay at index 3"
    );

    let (both, _): (ConnectionKind, _) =
        bincode::serde::decode_from_slice(&[3u8], standard()).unwrap();
    assert_eq!(both, ConnectionKind::Both);

    let (rdp, _): (ConnectionKind, _) =
        bincode::serde::decode_from_slice(&[2u8], standard()).unwrap();
    assert_eq!(rdp, ConnectionKind::LegacyRdp);

    let (vnc, _): (ConnectionKind, _) =
        bincode::serde::decode_from_slice(&[4u8], standard()).unwrap();
    assert_eq!(vnc, ConnectionKind::LegacyVnc);
}

#[test]
fn connection_protocol_bincode_indices_are_stable() {
    let cases = [
        (ConnectionProtocol::Ssh, 0u8),
        (ConnectionProtocol::Sftp, 1),
        (ConnectionProtocol::LegacyFtp, 2),
        (ConnectionProtocol::LegacyFtps, 3),
        (ConnectionProtocol::LegacySmb, 4),
        (ConnectionProtocol::LegacyRdp, 5),
        (ConnectionProtocol::LegacyVnc, 6),
    ];
    for (variant, idx) in cases {
        let bytes = bincode::serde::encode_to_vec(variant.clone(), standard()).unwrap();
        assert_eq!(
            bytes,
            vec![idx],
            "ConnectionProtocol index drifted for {:?}",
            variant
        );
    }
}

#[test]
fn normalize_strips_legacy_protocols() {
    let mut profile = ConnectionProfile {
        protocols: vec![
            ConnectionProtocol::LegacyRdp,
            ConnectionProtocol::Ssh,
            ConnectionProtocol::LegacySmb,
            ConnectionProtocol::Sftp,
        ],
        ..ConnectionProfile::default()
    };
    profile.normalize_protocols();
    assert_eq!(
        profile.protocols,
        vec![ConnectionProtocol::Ssh, ConnectionProtocol::Sftp]
    );
}

#[test]
fn normalize_legacy_only_profile_falls_back_to_ssh_sftp() {
    let mut profile = ConnectionProfile {
        protocols: vec![ConnectionProtocol::LegacyRdp],
        ..ConnectionProfile::default()
    };
    profile.normalize_protocols();
    assert_eq!(
        profile.protocols,
        vec![ConnectionProtocol::Ssh, ConnectionProtocol::Sftp]
    );
}
