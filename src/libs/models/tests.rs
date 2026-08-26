use super::{ConnectionKind, ConnectionProfile, ConnectionProtocol};
use bincode::config::standard;

#[test]
fn connection_kind_bincode_indices_are_stable() {
    let bytes = bincode::serde::encode_to_vec(ConnectionKind::Both, standard()).unwrap();
    assert_eq!(bytes, vec![3u8]);

    let (both, _): (ConnectionKind, _) =
        bincode::serde::decode_from_slice(&[3u8], standard()).unwrap();
    assert_eq!(both, ConnectionKind::Both);

    let (rdp, _): (ConnectionKind, _) =
        bincode::serde::decode_from_slice(&[2u8], standard()).unwrap();
    assert_eq!(rdp, ConnectionKind::LegacyRdp);
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

    for (variant, index) in cases {
        let bytes = bincode::serde::encode_to_vec(variant.clone(), standard()).unwrap();
        assert_eq!(bytes, vec![index]);
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
    assert!(profile.kind.is_none());
}
