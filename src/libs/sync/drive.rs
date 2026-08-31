use super::*;

/// Driver do Google Drive.
///
/// Não conhece nenhum nome de arquivo do domínio: criar, listar, baixar e
/// remover, só. O layout remoto vive em `remote.rs`, o que mantém a regra de
/// negócio fora do transporte — e permitiu trocar o esquema de arquivos fixos
/// pelo log de mutações sem tocar aqui.
pub(crate) async fn ensure_named_folder(
    client: &Client,
    access_token: &str,
    folder_name: &str,
    parent_id: &str,
    create_if_missing: bool,
) -> Result<Option<String>> {
    let query = format!(
        "name='{}' and mimeType='{}' and trashed=false and '{}' in parents",
        folder_name, DRIVE_FOLDER_MIME_TYPE, parent_id
    );

    let response = client
        .get("https://www.googleapis.com/drive/v3/files")
        .bearer_auth(access_token)
        .query(&[
            ("q", query.as_str()),
            ("spaces", "drive"),
            ("fields", "files(id,name,mimeType,modifiedTime)"),
            ("pageSize", "10"),
        ])
        .send()
        .await
        .context("Falha ao listar pastas no Google Drive")?;

    let list: DriveFileListResponse = read_json(response, "listar pasta").await?;
    if let Some(found) = list.files.into_iter().next() {
        return Ok(Some(found.id));
    }

    if !create_if_missing {
        return Ok(None);
    }

    let create_response = client
        .post("https://www.googleapis.com/drive/v3/files")
        .bearer_auth(access_token)
        .query(&[("fields", "id,name,mimeType,modifiedTime")])
        .json(&serde_json::json!({
            "name": folder_name,
            "mimeType": DRIVE_FOLDER_MIME_TYPE,
            "parents": [parent_id],
        }))
        .send()
        .await
        .context("Falha ao criar pasta no Google Drive")?;

    let created: DriveFileMetadata = read_json(create_response, "criar pasta").await?;
    Ok(Some(created.id))
}

/// Lista a pasta inteira, seguindo a paginação.
///
/// A versão anterior pedia uma página de mil itens e ignorava o resto; com o
/// log de mutações a pasta cresce até a compactação, e perder a segunda página
/// significaria perder alterações em silêncio.
pub(crate) async fn list_drive_files(
    client: &Client,
    access_token: &str,
    folder_id: &str,
) -> Result<Vec<DriveFileMetadata>> {
    let query = format!(
        "trashed=false and '{}' in parents and mimeType!='{}'",
        folder_id, DRIVE_FOLDER_MIME_TYPE
    );

    let mut out = Vec::new();
    let mut page_token: Option<String> = None;

    loop {
        let mut request = client
            .get("https://www.googleapis.com/drive/v3/files")
            .bearer_auth(access_token)
            .query(&[
                ("q", query.as_str()),
                ("spaces", "drive"),
                (
                    "fields",
                    "nextPageToken,files(id,name,mimeType,modifiedTime)",
                ),
                ("pageSize", "1000"),
            ]);
        if let Some(token) = &page_token {
            request = request.query(&[("pageToken", token.as_str())]);
        }

        let response = request
            .send()
            .await
            .context("Falha ao listar arquivos no Google Drive")?;
        let list: DriveFileListResponse = read_json(response, "listar arquivos").await?;

        out.extend(
            list.files
                .into_iter()
                .filter(|item| item.has_storage_extension()),
        );

        match list.next_page_token {
            Some(token) if !token.is_empty() => page_token = Some(token),
            _ => break,
        }
    }

    Ok(out)
}

/// Cria um arquivo e envia o conteúdo numa tacada.
///
/// Objetos remotos são imutáveis, então criar é a única escrita que existe:
/// nunca reescrevemos um arquivo compartilhado, que é como dois aparelhos
/// perderiam alterações um do outro num Drive sem compare-and-swap.
pub(crate) async fn create_drive_object(
    client: &Client,
    access_token: &str,
    folder_id: &str,
    file_name: &str,
    content: Vec<u8>,
) -> Result<DriveFileMetadata> {
    let created = create_drive_file(client, access_token, folder_id, file_name).await?;
    upload_file_bytes(client, access_token, &created.id, content).await
}

pub(crate) async fn create_drive_file(
    client: &Client,
    access_token: &str,
    folder_id: &str,
    file_name: &str,
) -> Result<DriveFileMetadata> {
    let response = client
        .post("https://www.googleapis.com/drive/v3/files")
        .bearer_auth(access_token)
        .query(&[("fields", "id,name,mimeType,modifiedTime")])
        .json(&serde_json::json!({
            "name": file_name,
            "parents": [folder_id],
        }))
        .send()
        .await
        .context("Falha ao criar metadata de arquivo no Drive")?;

    read_json(response, "criar arquivo").await
}

pub(crate) async fn upload_file_bytes(
    client: &Client,
    access_token: &str,
    file_id: &str,
    content: Vec<u8>,
) -> Result<DriveFileMetadata> {
    let url = format!(
        "https://www.googleapis.com/upload/drive/v3/files/{}",
        file_id
    );

    let response = client
        .patch(url)
        .bearer_auth(access_token)
        .query(&[
            ("uploadType", "media"),
            ("fields", "id,name,mimeType,modifiedTime"),
        ])
        .header("Content-Type", "application/octet-stream")
        .body(content)
        .send()
        .await
        .context("Falha ao enviar arquivo para o Drive")?;

    read_json(response, "enviar arquivo").await
}

pub(crate) async fn download_file_bytes(
    client: &Client,
    access_token: &str,
    file_id: &str,
) -> Result<Vec<u8>> {
    let url = format!("https://www.googleapis.com/drive/v3/files/{}", file_id);

    let response = client
        .get(url)
        .bearer_auth(access_token)
        .query(&[("alt", "media")])
        .send()
        .await
        .context("Falha ao baixar arquivo do Drive")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!("Falha no download do Drive ({}): {}", status, body));
    }

    response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .context("Falha ao ler bytes de download")
}

pub(crate) async fn delete_drive_file(
    client: &Client,
    access_token: &str,
    file_id: &str,
) -> Result<()> {
    let url = format!("https://www.googleapis.com/drive/v3/files/{}", file_id);
    let response = client
        .delete(url)
        .bearer_auth(access_token)
        .send()
        .await
        .context("Falha ao remover arquivo no Drive")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!(
            "Falha ao remover arquivo no Drive ({}): {}",
            status,
            body
        ));
    }

    Ok(())
}

async fn read_json<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
    action: &str,
) -> Result<T> {
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!(
            "Falha ao {} no Drive ({}): {}",
            action,
            status,
            body
        ));
    }

    response
        .json::<T>()
        .await
        .with_context(|| format!("Falha ao decodificar resposta de {action}"))
}

#[derive(Debug, Deserialize)]
pub(crate) struct DriveFileListResponse {
    #[serde(default)]
    pub(crate) files: Vec<DriveFileMetadata>,
    #[serde(rename = "nextPageToken", default)]
    pub(crate) next_page_token: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct DriveFileMetadata {
    pub(crate) id: String,
    pub(crate) name: Option<String>,
    #[serde(rename = "mimeType")]
    pub(crate) mime_type: Option<String>,
    #[serde(rename = "modifiedTime")]
    pub(crate) modified_time: Option<String>,
}

impl DriveFileMetadata {
    pub(crate) fn file_name(&self) -> &str {
        self.name.as_deref().unwrap_or_default()
    }

    fn has_storage_extension(&self) -> bool {
        self.file_name()
            .to_ascii_lowercase()
            .ends_with(&format!(".{STORAGE_FILE_EXTENSION}"))
    }
}
