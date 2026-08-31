use anyhow::{anyhow, Context, Result};
use reqwest::Client;

use super::drive::{
    create_drive_object, download_file_bytes, ensure_named_folder, DriveFileMetadata,
};
use crate::constants::{
    DRIVE_ROOT_FOLDER_NAME, DRIVE_TOP_PARENT_ID, REMOTE_HEADER_FILE_NAME, REMOTE_SNAPSHOT_PREFIX,
};
use crate::libs::mutations::RemoteHeader;

/// Layout da pasta remota.
///
/// - `header.bin` — salt e verificador da chave mestre, gravado uma vez.
/// - `snapshot-<uuidv7>.bin` — estado completo, publicado na compactação.
/// - `<uuidv7>.bin` — um lote de mutações, imutável.
///
/// Os lotes têm nome opaco de propósito: prefixá-los com o dispositivo
/// entregaria ao Google quantos aparelhos existem e qual muda mais. O prefixo
/// dos snapshots é a única concessão — sem ele, achar o snapshot obrigaria a
/// baixar a pasta inteira a cada sincronia.
#[derive(Debug, Default)]
pub(crate) struct RemoteLayout {
    pub(crate) header: Option<DriveFileMetadata>,
    pub(crate) snapshots: Vec<DriveFileMetadata>,
    pub(crate) batches: Vec<DriveFileMetadata>,
}

impl RemoteLayout {
    pub(crate) fn classify(files: Vec<DriveFileMetadata>) -> Self {
        let mut layout = Self::default();

        for file in files {
            let name = file.file_name().to_string();
            if name == REMOTE_HEADER_FILE_NAME {
                layout.header = Some(file);
            } else if name.starts_with(REMOTE_SNAPSHOT_PREFIX) {
                layout.snapshots.push(file);
            } else {
                layout.batches.push(file);
            }
        }

        // UUIDv7 no nome dá ordenação aproximada por criação, o que só serve
        // para escolher o snapshot mais recente e para depurar. A ordem que
        // vale na aplicação é a do relógio lógico, dentro do payload.
        layout
            .snapshots
            .sort_by(|a, b| a.file_name().cmp(b.file_name()));
        layout
            .batches
            .sort_by(|a, b| a.file_name().cmp(b.file_name()));
        layout
    }

    pub(crate) fn latest_snapshot(&self) -> Option<&DriveFileMetadata> {
        self.snapshots.last()
    }

    /// Snapshots antigos, que a compactação pode remover depois de publicar o
    /// novo.
    pub(crate) fn stale_snapshots(&self) -> &[DriveFileMetadata] {
        let count = self.snapshots.len();
        if count <= 1 {
            return &[];
        }
        &self.snapshots[..count - 1]
    }
}

/// Pasta remota do cofre: `OpenPtl/<vault-id>`.
///
/// Cada cofre tem a própria pasta pelo mesmo motivo que tem o próprio
/// diretório local — misturar os lotes de dois cofres faria um tentar aplicar
/// mutações que a chave dele nem abre. É também onde um cofre de empresa vai
/// se encaixar depois, apontando para uma pasta compartilhada em vez desta.
pub(crate) async fn ensure_vault_folder(
    client: &Client,
    access_token: &str,
    vault_id: &str,
    create_if_missing: bool,
) -> Result<Option<String>> {
    let Some(root_id) = ensure_named_folder(
        client,
        access_token,
        DRIVE_ROOT_FOLDER_NAME,
        DRIVE_TOP_PARENT_ID,
        create_if_missing,
    )
    .await?
    else {
        return Ok(None);
    };

    if vault_id.trim().is_empty() {
        return Err(anyhow!("Cofre sem identificador para a pasta remota"));
    }

    ensure_named_folder(client, access_token, vault_id, &root_id, create_if_missing).await
}

/// Garante que a pasta remota tenha o cabeçalho. Sem ele um aparelho novo não
/// consegue derivar a mesma chave e não abriria nenhum lote.
pub(crate) async fn ensure_remote_header(
    client: &Client,
    access_token: &str,
    folder_id: &str,
    layout: &RemoteLayout,
    header: &RemoteHeader,
) -> Result<()> {
    if layout.header.is_some() {
        return Ok(());
    }

    let bytes = serde_json::to_vec(header).context("Falha ao serializar cabeçalho remoto")?;
    create_drive_object(
        client,
        access_token,
        folder_id,
        REMOTE_HEADER_FILE_NAME,
        bytes,
    )
    .await?;
    Ok(())
}

pub(crate) async fn read_remote_header(
    client: &Client,
    access_token: &str,
    file: &DriveFileMetadata,
) -> Result<RemoteHeader> {
    let bytes = download_file_bytes(client, access_token, &file.id).await?;
    serde_json::from_slice(&bytes).context("Cabeçalho remoto inválido")
}
