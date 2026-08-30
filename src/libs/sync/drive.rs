use super::*;

pub(crate) async fn ensure_openptl_folder(
    client: &Client,
    access_token: &str,
    create_if_missing: bool,
) -> Result<Option<String>> {
    ensure_named_folder(
        client,
        access_token,
        DRIVE_ROOT_FOLDER_NAME,
        DRIVE_TOP_PARENT_ID,
        create_if_missing,
    )
    .await
}

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

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!(
            "Falha ao listar pasta no Drive ({}): {}",
            status,
            body
        ));
    }

    let list = response
        .json::<DriveFileListResponse>()
        .await
        .context("Falha ao decodificar listagem de pastas")?;

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

    let create_status = create_response.status();
    if !create_status.is_success() {
        let body = create_response.text().await.unwrap_or_default();
        return Err(anyhow!(
            "Falha ao criar pasta no Drive ({}): {}",
            create_status,
            body
        ));
    }

    let created = create_response
        .json::<DriveFileMetadata>()
        .await
        .context("Falha ao decodificar pasta criada")?;

    Ok(Some(created.id))
}

pub(crate) async fn list_drive_bin_files(
    client: &Client,
    access_token: &str,
    folder_id: &str,
) -> Result<HashMap<String, DriveFileMetadata>> {
    let query = format!(
        "trashed=false and '{}' in parents and mimeType!='{}'",
        folder_id, DRIVE_FOLDER_MIME_TYPE
    );

    let response = client
        .get("https://www.googleapis.com/drive/v3/files")
        .bearer_auth(access_token)
        .query(&[
            ("q", query.as_str()),
            ("spaces", "drive"),
            ("fields", "files(id,name,mimeType,modifiedTime)"),
            ("pageSize", "1000"),
        ])
        .send()
        .await
        .context("Falha ao listar arquivos no Google Drive")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!(
            "Falha ao listar arquivos no Drive ({}): {}",
            status,
            body
        ));
    }

    let list = response
        .json::<DriveFileListResponse>()
        .await
        .context("Falha ao decodificar listagem de arquivos")?;

    let mut out: HashMap<String, DriveFileMetadata> = HashMap::new();
    for item in list.files {
        let Some(name) = item.name.clone() else {
            continue;
        };
        if !name.to_ascii_lowercase().ends_with(".bin") {
            continue;
        }

        if let Some(current) = out.get(&name) {
            if item.modified_time > current.modified_time {
                out.insert(name, item);
            }
        } else {
            out.insert(name, item);
        }
    }
    Ok(out)
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

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!(
            "Falha ao criar arquivo no Drive ({}): {}",
            status,
            body
        ));
    }

    response
        .json::<DriveFileMetadata>()
        .await
        .context("Falha ao decodificar metadata criada")
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

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!("Falha no upload para Drive ({}): {}", status, body));
    }

    response
        .json::<DriveFileMetadata>()
        .await
        .context("Falha ao ler resposta de upload")
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

#[derive(Debug, Deserialize)]
pub(crate) struct DriveFileListResponse {
    pub(crate) files: Vec<DriveFileMetadata>,
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
