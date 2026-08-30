use super::*;

impl VaultManager {
    pub fn notes_list(&self) -> Result<Vec<Note>> {
        self.assert_unlocked()?;
        let mut notes = self.read_notes_store()?.notes;
        // Fixadas primeiro; dentro de cada grupo, a mais recente no topo.
        notes.sort_by(|left, right| {
            right
                .pinned
                .cmp(&left.pinned)
                .then(right.updated_at.cmp(&left.updated_at))
        });
        Ok(notes)
    }

    pub fn note_save(&mut self, mut note: Note) -> Result<Note> {
        self.assert_unlocked()?;

        note.title = note.title.trim().to_string();
        let now = Utc::now().timestamp();

        let mut payload = self.read_notes_store()?;
        match payload.notes.iter_mut().find(|item| item.id == note.id) {
            Some(existing) => {
                note.created_at = existing.created_at;
                note.updated_at = now;
                *existing = note.clone();
            }
            None => {
                note.id = uuid::Uuid::new_v4().to_string();
                note.created_at = now;
                note.updated_at = now;
                payload.notes.push(note.clone());
            }
        }

        self.write_notes_store(&payload)?;
        Ok(note)
    }

    pub fn note_delete(&mut self, id: &str) -> Result<()> {
        self.assert_unlocked()?;

        let mut payload = self.read_notes_store()?;
        let before = payload.notes.len();
        payload.notes.retain(|note| note.id != id);
        if payload.notes.len() == before {
            return Err(anyhow!("Nota nao encontrada"));
        }

        self.write_notes_store(&payload)
    }

    fn read_notes_store(&self) -> Result<NotesBinPayload> {
        if !self.notes_path.exists() {
            return Ok(NotesBinPayload {
                version: CURRENT_PAYLOAD_VERSION,
                notes: Vec::new(),
            });
        }
        let key = self.current_key()?;
        let encrypted: EncryptedBin = read_bin_file(&self.notes_path)?;
        decrypt_bin_payload(&encrypted, &key, NOTES_FILE_NAME)
    }

    fn write_notes_store(&self, payload: &NotesBinPayload) -> Result<()> {
        let key = self.current_key()?;
        let encrypted =
            encrypt_bin_payload(payload, &key, NOTES_FILE_NAME, Utc::now().timestamp())?;
        write_bin_file(&self.notes_path, &encrypted)
    }
}
