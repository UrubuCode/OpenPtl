//! Testes do índice de cofres.

use super::*;
use tempfile::tempdir;

fn registry(dir: &Path) -> VaultRegistry {
    VaultRegistry::new_in(dir.to_path_buf()).expect("registry")
}

#[test]
fn a_fresh_install_starts_with_no_vaults() {
    let temp = tempdir().expect("temp");
    let registry = registry(temp.path());
    assert!(registry.list().is_empty());
    assert!(registry.selected_id().is_none());
}

#[test]
fn creating_a_vault_selects_it() {
    let temp = tempdir().expect("temp");
    let mut registry = registry(temp.path());
    let entry = registry.create("Empresa").expect("create");

    assert_eq!(registry.selected_id().as_deref(), Some(entry.id.as_str()));
    assert!(registry.path_of(&entry.id).expect("path").is_dir());
}

#[test]
fn two_vaults_never_share_a_directory() {
    let temp = tempdir().expect("temp");
    let mut registry = registry(temp.path());
    let first = registry.create("Pessoal").expect("create");
    let second = registry.create("Empresa").expect("create");

    assert_ne!(first.id, second.id);
    assert_ne!(
        registry.path_of(&first.id).expect("path"),
        registry.path_of(&second.id).expect("path")
    );
}

#[test]
fn labels_do_not_repeat() {
    let temp = tempdir().expect("temp");
    let mut registry = registry(temp.path());
    registry.create("Empresa").expect("create");
    assert!(registry.create("  empresa ").is_err());
}

#[test]
fn the_selection_survives_a_reopen() {
    let temp = tempdir().expect("temp");
    let chosen = {
        let mut registry = registry(temp.path());
        registry.create("Pessoal").expect("create");
        let second = registry.create("Empresa").expect("create");
        registry.select(&second.id).expect("select");
        second.id
    };

    let reopened = registry(temp.path());
    assert_eq!(reopened.selected_id(), Some(chosen));
    assert_eq!(reopened.list().len(), 2);
}

#[test]
fn removing_the_selected_vault_falls_back_to_another() {
    let temp = tempdir().expect("temp");
    let mut registry = registry(temp.path());
    let first = registry.create("Pessoal").expect("create");
    let second = registry.create("Empresa").expect("create");

    registry.remove(&second.id).expect("remove");
    assert_eq!(registry.selected_id(), Some(first.id));
    assert!(!registry.vaults_root.join(&second.id).exists());
}

#[test]
fn a_single_vault_install_is_adopted_instead_of_lost() {
    let temp = tempdir().expect("temp");
    let storage_root = temp.path().join(STORAGE_DIR_NAME);
    fs::create_dir_all(&storage_root).expect("dir");
    fs::write(storage_root.join(OPENPTL_FILE_NAME), b"metadados").expect("write");
    fs::write(storage_root.join("notes.bin"), b"notas").expect("write");
    fs::write(storage_root.join("known_hosts"), b"host").expect("write");

    let registry = registry(temp.path());
    let entries = registry.list();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].label, DEFAULT_VAULT_LABEL);
    assert!(entries[0].initialized);

    let moved = registry.path_of(&entries[0].id).expect("path");
    assert!(moved.join(OPENPTL_FILE_NAME).exists());
    assert!(moved.join("notes.bin").exists());
    assert!(!storage_root.join(OPENPTL_FILE_NAME).exists());
    assert!(
        !moved.join("known_hosts").exists(),
        "o known_hosts em claro e regenerado, nao migrado"
    );
}

/// Grava um `openptl.bin` com a versão pedida, sem passar pelo cofre.
fn write_metadata(dir: &Path, version: u32) {
    fs::create_dir_all(dir).expect("dir");
    let file = super::super::OpenPtlBin {
        version,
        key_mode: crate::libs::models::KeyMode::Password,
        salt: Some([1u8; 16]),
        key_check: [2u8; 32],
        created_at: 1,
        updated_at: 1,
    };
    write_bin_file(&dir.join(OPENPTL_FILE_NAME), &file).expect("write");
}

#[test]
fn a_single_vault_of_the_previous_format_is_discarded_instead_of_adopted() {
    let temp = tempdir().expect("temp");
    let storage_root = temp.path().join(STORAGE_DIR_NAME);
    write_metadata(&storage_root, CURRENT_STORAGE_VERSION - 1);
    fs::write(storage_root.join("notes.bin"), b"notas").expect("write");

    let registry = registry(temp.path());
    assert!(
        registry.list().is_empty(),
        "um cofre que esta versao nao abre nao entra no indice"
    );
    assert!(!storage_root.join(OPENPTL_FILE_NAME).exists());
    assert!(!storage_root.join("notes.bin").exists());
}

#[test]
fn a_registered_vault_of_the_previous_format_is_removed() {
    let temp = tempdir().expect("temp");
    let stale = {
        let mut registry = registry(temp.path());
        registry.create("Atual").expect("create");
        let old = registry.create("Antigo").expect("create");
        write_metadata(
            &registry.vaults_root.join(&old.id),
            CURRENT_STORAGE_VERSION - 1,
        );
        old.id
    };

    let reopened = registry(temp.path());
    assert_eq!(reopened.list().len(), 1);
    assert!(reopened.list().iter().all(|entry| entry.id != stale));
    assert!(!reopened.vaults_root.join(&stale).exists());
}

#[test]
fn a_vault_from_a_newer_version_is_left_alone() {
    let temp = tempdir().expect("temp");
    let future = {
        let mut registry = registry(temp.path());
        let entry = registry.create("Futuro").expect("create");
        write_metadata(
            &registry.vaults_root.join(&entry.id),
            CURRENT_STORAGE_VERSION + 1,
        );
        entry.id
    };

    // Apagar aqui destruiria um cofre gravado por uma versao posterior do
    // aplicativo, que ainda e dado valido do usuario.
    let reopened = registry(temp.path());
    assert_eq!(reopened.list().len(), 1);
    assert_eq!(reopened.list()[0].id, future);
}

#[test]
fn leftovers_of_the_tauri_install_are_swept() {
    let temp = tempdir().expect("temp");
    let storage_root = temp.path().join(STORAGE_DIR_NAME);
    fs::create_dir_all(storage_root.join(LEGACY_PROFILE_DIR_NAME)).expect("dir");
    fs::write(temp.path().join(LEGACY_VAULT_FILE_NAME), b"antigo").expect("write");

    let _ = registry(temp.path());

    assert!(!temp.path().join(LEGACY_VAULT_FILE_NAME).exists());
    assert!(!storage_root.join(LEGACY_PROFILE_DIR_NAME).exists());
}

#[test]
fn a_vault_whose_directory_vanished_leaves_the_index() {
    let temp = tempdir().expect("temp");
    let orphan = {
        let mut registry = registry(temp.path());
        registry.create("Pessoal").expect("create");
        let gone = registry.create("Empresa").expect("create");
        fs::remove_dir_all(registry.vaults_root.join(&gone.id)).expect("remove");
        gone.id
    };

    let reopened = registry(temp.path());
    assert_eq!(reopened.list().len(), 1);
    assert!(reopened.list().iter().all(|entry| entry.id != orphan));
}
