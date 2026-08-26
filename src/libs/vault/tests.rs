use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::Path,
};
use tempfile::tempdir;

use super::{
    decrypt_bin_payload, derive_key, encrypt_bin_payload, hash_bytes_hex, VaultManager,
    VaultRuntime,
};
use crate::constants::{
    KNOWN_HOSTS_FILE_NAME, MANIFEST_FILE_NAME, OPENPTL_FILE_NAME, PROFILE_FILE_NAME,
};
use crate::libs::models::{
    ConnectionKind, ConnectionProfile, ConnectionProtocol, ManifestBinPayload,
};

fn test_vault_manager(storage_root: &Path) -> VaultManager {
    fs::create_dir_all(storage_root).expect("test storage should be created");
    VaultManager {
        storage_root: storage_root.to_path_buf(),
        openptl_path: storage_root.join(OPENPTL_FILE_NAME),
        profile_path: storage_root.join(PROFILE_FILE_NAME),
        manifest_path: storage_root.join(MANIFEST_FILE_NAME),
        known_hosts_path: storage_root.join("known_hosts"),
        known_hosts_bin_path: storage_root.join(KNOWN_HOSTS_FILE_NAME),
        runtime: VaultRuntime::default(),
    }
}

fn snapshot_files(vault: &VaultManager) -> HashMap<String, Vec<u8>> {
    vault
        .list_local_bin_files()
        .expect("snapshot should list files")
        .into_iter()
        .collect()
}

#[test]
fn should_encrypt_and_decrypt_record() {
    let profile = ConnectionProfile {
        id: "6ec2a7db-c0af-4435-b38c-228f0cc9ec31".to_string(),
        name: "srv".to_string(),
        host: "127.0.0.1".to_string(),
        port: 22,
        username: "root".to_string(),
        password: Some("secret".to_string()),
        private_key: None,
        keychain_id: None,
        remote_path: Some("/".to_string()),
        protocols: vec![ConnectionProtocol::Ssh, ConnectionProtocol::Sftp],
        kind: Some(ConnectionKind::Both),
    };

    let salt = [7u8; 16];
    let key = derive_key("master-password", &salt).expect("kdf should work");
    let encrypted =
        encrypt_bin_payload(&profile, &key, &profile.id, 1_700_000_000).expect("encrypt");
    let decrypted: ConnectionProfile =
        decrypt_bin_payload(&encrypted, &key, "decrypt test").expect("decrypt");
    assert_eq!(decrypted.host, "127.0.0.1");
}

#[test]
fn should_fail_on_wrong_key() {
    let value = ManifestBinPayload {
        version: 1,
        profile: "hash-profile".to_string(),
        hosts: BTreeMap::new(),
        keychain: BTreeMap::new(),
    };

    let salt = [1u8; 16];
    let key = derive_key("correct", &salt).expect("kdf should work");
    let encrypted =
        encrypt_bin_payload(&value, &key, "manifest.bin", 1_700_000_000).expect("encrypt");

    let wrong_key = derive_key("wrong", &salt).expect("kdf should work");
    let decrypted = decrypt_bin_payload::<ManifestBinPayload>(&encrypted, &wrong_key, "decrypt");
    assert!(decrypted.is_err());
}

#[test]
fn known_hosts_round_trip_through_encrypted_store() {
    let temp_dir = tempdir().expect("temp dir should be created");
    let storage_root = temp_dir.path().join("kh-test");
    let mut vault = test_vault_manager(&storage_root);
    vault
        .init(Some("senha-super-segura".to_string()))
        .expect("vault should initialize");

    // Simulate a host learned during a session: written to the working file,
    // then captured into the encrypted store.
    let kh = "example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5\n";
    fs::write(vault.known_hosts_path(), kh).expect("write working known_hosts");
    vault.capture_known_hosts().expect("capture should persist");

    assert!(
        storage_root.join(KNOWN_HOSTS_FILE_NAME).exists(),
        "encrypted known_hosts.bin should be written"
    );

    // Lock and unlock: the working file must be re-materialized from the store.
    vault.lock();
    fs::remove_file(vault.known_hosts_path()).ok();
    vault
        .unlock(Some("senha-super-segura".to_string()))
        .expect("vault should unlock");

    let materialized =
        fs::read_to_string(vault.known_hosts_path()).expect("working file materialized");
    assert_eq!(
        materialized, kh,
        "known_hosts survives the encrypted round-trip"
    );
}

#[test]
fn should_keep_password_valid_for_snapshot_x_even_after_snapshot_y_changes() {
    let temp_dir = tempdir().expect("temp dir should be created");
    let storage_root = temp_dir.path().join("vault-test");
    let mut vault = test_vault_manager(&storage_root);
    let password = "senha-super-segura";

    vault
        .init(Some(password.to_string()))
        .expect("vault should initialize");

    // Versao X: estado inicial sincronizado.
    let snapshot_x = snapshot_files(&vault);
    let profile_hash_x = hash_bytes_hex(
        snapshot_x
            .get(PROFILE_FILE_NAME)
            .expect("version X should include profile.bin"),
    );
    let openptl_x = snapshot_x
        .get(OPENPTL_FILE_NAME)
        .expect("version X should include openptl.bin")
        .clone();

    // Versao Y: cliente altera settings/profile e persiste.
    let mut settings = vault.settings_get().expect("settings should load");
    settings.sync_interval_minutes = 7;
    settings.sync_on_settings_change = true;
    vault
        .settings_update(settings)
        .expect("settings should be updated");

    let snapshot_y = snapshot_files(&vault);
    let profile_hash_y = hash_bytes_hex(
        snapshot_y
            .get(PROFILE_FILE_NAME)
            .expect("version Y should include profile.bin"),
    );

    // Simula conflito client/server: hashes divergentes entre X (server) e Y (client).
    assert_ne!(
        profile_hash_x, profile_hash_y,
        "profile hash should diverge between snapshot X and Y"
    );

    // Mesmo com Y local, a senha atual continua valida para o openptl de X.
    assert!(
        vault
            .validate_password_for_openptl_bytes(&openptl_x, password)
            .expect("password validation should run"),
        "same password should validate against version X metadata"
    );

    // Restaurando X com a mesma senha deve continuar descriptografando normalmente.
    vault
        .replace_local_files(&snapshot_x)
        .expect("replacing with snapshot X should succeed");
    vault.lock();
    let status = vault
        .unlock(Some(password.to_string()))
        .expect("unlock with same password should succeed for snapshot X");
    assert!(!status.locked, "vault should be unlocked after restoring X");
}
