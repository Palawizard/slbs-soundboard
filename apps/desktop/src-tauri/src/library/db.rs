use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use uuid::Uuid;

use super::models::{MediaAsset, PlaybackProfile, ReplayPolicy, Sound, Soundboard};
use super::LibraryError;

const MIGRATIONS: &[&str] = &[
    r#"
    CREATE TABLE media_assets (
        hash TEXT PRIMARY KEY,
        kind TEXT NOT NULL CHECK (kind IN ('audio', 'image')),
        extension TEXT NOT NULL,
        byte_size INTEGER NOT NULL CHECK (byte_size >= 0),
        created_at_ms INTEGER NOT NULL
    );

    CREATE TABLE sounds (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL CHECK (length(trim(title)) BETWEEN 1 AND 120),
        audio_hash TEXT NOT NULL REFERENCES media_assets(hash) ON DELETE RESTRICT,
        duration_ms INTEGER NOT NULL CHECK (duration_ms > 0),
        sample_rate INTEGER NOT NULL CHECK (sample_rate > 0),
        channels INTEGER NOT NULL CHECK (channels > 0),
        image_hash TEXT REFERENCES media_assets(hash) ON DELETE SET NULL,
        created_at_ms INTEGER NOT NULL
    );

    CREATE TABLE soundboards (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL CHECK (length(trim(title)) BETWEEN 1 AND 80),
        position INTEGER NOT NULL UNIQUE,
        created_at_ms INTEGER NOT NULL
    );

    CREATE TABLE soundboard_sounds (
        soundboard_id TEXT NOT NULL REFERENCES soundboards(id) ON DELETE CASCADE,
        sound_id TEXT NOT NULL REFERENCES sounds(id) ON DELETE CASCADE,
        position INTEGER NOT NULL,
        PRIMARY KEY (soundboard_id, sound_id),
        UNIQUE (soundboard_id, position)
    );

    CREATE TABLE settings (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );

    CREATE INDEX sounds_audio_hash_idx ON sounds(audio_hash);
    CREATE INDEX sounds_image_hash_idx ON sounds(image_hash);
    "#,
    r#"
    ALTER TABLE sounds ADD COLUMN waveform_json TEXT NOT NULL DEFAULT '[]';
    "#,
    r#"
    ALTER TABLE sounds ADD COLUMN volume REAL NOT NULL DEFAULT 1.0 CHECK (volume BETWEEN 0.0 AND 2.0);
    ALTER TABLE sounds ADD COLUMN pitch_semitones REAL NOT NULL DEFAULT 0.0 CHECK (pitch_semitones BETWEEN -12.0 AND 12.0);
    ALTER TABLE sounds ADD COLUMN speed REAL NOT NULL DEFAULT 1.0 CHECK (speed BETWEEN 0.5 AND 2.0);
    ALTER TABLE sounds ADD COLUMN replay_policy TEXT NOT NULL DEFAULT 'restart' CHECK (replay_policy IN ('overlap', 'toggle', 'stop', 'restart'));
    ALTER TABLE sounds ADD COLUMN keybind TEXT;
    CREATE UNIQUE INDEX sounds_keybind_idx ON sounds(keybind) WHERE keybind IS NOT NULL;
    "#,
];

pub struct LibraryRepository {
    connection: Connection,
}

#[derive(Clone, Debug)]
pub struct NewSound<'a> {
    pub title: &'a str,
    pub audio_hash: &'a str,
    pub duration_ms: u64,
    pub sample_rate: u32,
    pub channels: u16,
    pub waveform: &'a [f32],
}

impl LibraryRepository {
    pub fn open(path: &Path) -> Result<Self, LibraryError> {
        let connection = Connection::open(path)?;
        Self::from_connection(connection)
    }

    #[cfg(test)]
    pub fn in_memory() -> Result<Self, LibraryError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(connection: Connection) -> Result<Self, LibraryError> {
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY);",
        )?;
        let current: i64 = connection.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )?;
        for (index, migration) in MIGRATIONS.iter().enumerate().skip(current as usize) {
            connection.execute_batch("BEGIN IMMEDIATE")?;
            let result = connection.execute_batch(migration).and_then(|_| {
                connection.execute(
                    "INSERT INTO schema_migrations(version) VALUES (?1)",
                    [index as i64 + 1],
                )?;
                connection.execute_batch("COMMIT")
            });
            if let Err(error) = result {
                let _ = connection.execute_batch("ROLLBACK");
                return Err(error.into());
            }
        }
        Ok(Self { connection })
    }

    pub fn ensure_default_soundboard(&mut self) -> Result<(), LibraryError> {
        if self.soundboard_count()? == 0 {
            self.create_soundboard("Mes sons")?;
        }
        Ok(())
    }

    pub fn soundboard_count(&self) -> Result<u64, LibraryError> {
        Ok(self
            .connection
            .query_row("SELECT COUNT(*) FROM soundboards", [], |row| row.get(0))?)
    }

    pub fn list_soundboards(&self) -> Result<Vec<Soundboard>, LibraryError> {
        let mut statement = self.connection.prepare(
            "SELECT id, title, position FROM soundboards ORDER BY position, created_at_ms",
        )?;
        let boards = statement
            .query_map([], |row| {
                Ok(Soundboard {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    position: row.get(2)?,
                    sounds: Vec::new(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);

        boards
            .into_iter()
            .map(|mut board| {
                board.sounds = self.list_sounds(&board.id)?;
                Ok(board)
            })
            .collect()
    }

    pub fn create_soundboard(&mut self, title: &str) -> Result<Soundboard, LibraryError> {
        let title = validate_title(title, 80)?;
        let transaction = self.connection.transaction()?;
        let position: i64 = transaction.query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM soundboards",
            [],
            |row| row.get(0),
        )?;
        let id = Uuid::new_v4().to_string();
        transaction.execute(
            "INSERT INTO soundboards(id, title, position, created_at_ms) VALUES (?1, ?2, ?3, ?4)",
            params![id, title, position, now_ms()],
        )?;
        transaction.commit()?;
        Ok(Soundboard {
            id,
            title: title.to_owned(),
            position,
            sounds: Vec::new(),
        })
    }

    pub fn rename_soundboard(&self, id: &str, title: &str) -> Result<(), LibraryError> {
        let title = validate_title(title, 80)?;
        require_changed(self.connection.execute(
            "UPDATE soundboards SET title = ?1 WHERE id = ?2",
            params![title, id],
        )?)
    }

    pub fn delete_soundboard(&mut self, id: &str) -> Result<(), LibraryError> {
        let transaction = self.connection.transaction()?;
        require_changed(transaction.execute("DELETE FROM soundboards WHERE id = ?1", [id])?)?;
        normalize_positions(&transaction, "soundboards", None)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn reorder_soundboards(&mut self, ordered_ids: &[String]) -> Result<(), LibraryError> {
        let transaction = self.connection.transaction()?;
        let count: u64 =
            transaction.query_row("SELECT COUNT(*) FROM soundboards", [], |row| row.get(0))?;
        if count != ordered_ids.len() as u64 {
            return Err(LibraryError::InvalidOrder);
        }
        transaction.execute("UPDATE soundboards SET position = -position - 1", [])?;
        for (position, id) in ordered_ids.iter().enumerate() {
            require_changed(transaction.execute(
                "UPDATE soundboards SET position = ?1 WHERE id = ?2",
                params![position as i64, id],
            )?)?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn add_media_asset(&self, asset: &MediaAsset) -> Result<(), LibraryError> {
        self.connection.execute(
            "INSERT OR IGNORE INTO media_assets(hash, kind, extension, byte_size, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![asset.hash, asset.kind, asset.extension, asset.byte_size, now_ms()],
        )?;
        Ok(())
    }

    pub fn create_sound(
        &mut self,
        soundboard_id: &str,
        sound: NewSound<'_>,
    ) -> Result<Sound, LibraryError> {
        let title = validate_title(sound.title, 120)?;
        let transaction = self.connection.transaction()?;
        let position: i64 = transaction.query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM soundboard_sounds WHERE soundboard_id = ?1",
            [soundboard_id],
            |row| row.get(0),
        )?;
        let id = Uuid::new_v4().to_string();
        let created_at_ms = now_ms();
        transaction.execute(
            "INSERT INTO sounds(id, title, audio_hash, duration_ms, sample_rate, channels, waveform_json, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, title, sound.audio_hash, sound.duration_ms, sound.sample_rate, sound.channels, serde_json::to_string(sound.waveform).map_err(|_| LibraryError::AudioDecode)?, created_at_ms],
        )?;
        transaction.execute(
            "INSERT INTO soundboard_sounds(soundboard_id, sound_id, position) VALUES (?1, ?2, ?3)",
            params![soundboard_id, id, position],
        )?;
        transaction.commit()?;
        self.sound_by_id(&id)?.ok_or(LibraryError::NotFound)
    }

    pub fn rename_sound(&self, id: &str, title: &str) -> Result<(), LibraryError> {
        let title = validate_title(title, 120)?;
        require_changed(self.connection.execute(
            "UPDATE sounds SET title = ?1 WHERE id = ?2",
            params![title, id],
        )?)
    }

    pub fn update_playback_profile(
        &self,
        id: &str,
        profile: &PlaybackProfile,
    ) -> Result<(), LibraryError> {
        validate_playback_profile(profile)?;
        require_changed(self.connection.execute(
            "UPDATE sounds SET volume = ?1, pitch_semitones = ?2, speed = ?3, replay_policy = ?4, keybind = ?5 WHERE id = ?6",
            params![profile.volume, profile.pitch_semitones, profile.speed, profile.replay_policy.as_str(), profile.keybind, id],
        )?)
    }

    pub fn set_sound_image(
        &mut self,
        sound_id: &str,
        hash: Option<&str>,
    ) -> Result<Vec<String>, LibraryError> {
        let transaction = self.connection.transaction()?;
        require_changed(transaction.execute(
            "UPDATE sounds SET image_hash = ?1 WHERE id = ?2",
            params![hash, sound_id],
        )?)?;
        let orphaned = orphaned_asset_hashes(&transaction)?;
        for orphaned_hash in &orphaned {
            transaction.execute("DELETE FROM media_assets WHERE hash = ?1", [orphaned_hash])?;
        }
        transaction.commit()?;
        Ok(orphaned)
    }

    pub fn delete_sound(
        &mut self,
        soundboard_id: &str,
        sound_id: &str,
    ) -> Result<Vec<String>, LibraryError> {
        let transaction = self.connection.transaction()?;
        require_changed(transaction.execute(
            "DELETE FROM soundboard_sounds WHERE soundboard_id = ?1 AND sound_id = ?2",
            params![soundboard_id, sound_id],
        )?)?;
        normalize_positions(&transaction, "soundboard_sounds", Some(soundboard_id))?;
        let references: u64 = transaction.query_row(
            "SELECT COUNT(*) FROM soundboard_sounds WHERE sound_id = ?1",
            [sound_id],
            |row| row.get(0),
        )?;
        if references == 0 {
            transaction.execute("DELETE FROM sounds WHERE id = ?1", [sound_id])?;
        }
        let orphaned = orphaned_asset_hashes(&transaction)?;
        for hash in &orphaned {
            transaction.execute("DELETE FROM media_assets WHERE hash = ?1", [hash])?;
        }
        transaction.commit()?;
        Ok(orphaned)
    }

    pub fn reorder_sounds(
        &mut self,
        soundboard_id: &str,
        ordered_ids: &[String],
    ) -> Result<(), LibraryError> {
        let transaction = self.connection.transaction()?;
        let count: u64 = transaction.query_row(
            "SELECT COUNT(*) FROM soundboard_sounds WHERE soundboard_id = ?1",
            [soundboard_id],
            |row| row.get(0),
        )?;
        if count != ordered_ids.len() as u64 {
            return Err(LibraryError::InvalidOrder);
        }
        transaction.execute(
            "UPDATE soundboard_sounds SET position = -position - 1 WHERE soundboard_id = ?1",
            [soundboard_id],
        )?;
        for (position, id) in ordered_ids.iter().enumerate() {
            require_changed(transaction.execute(
                "UPDATE soundboard_sounds SET position = ?1 WHERE soundboard_id = ?2 AND sound_id = ?3",
                params![position as i64, soundboard_id, id],
            )?)?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>, LibraryError> {
        Ok(self
            .connection
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), LibraryError> {
        self.connection.execute(
            "INSERT INTO settings(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn backup_to(&self, path: &Path) -> Result<(), LibraryError> {
        self.connection.backup(rusqlite::MAIN_DB, path, None)?;
        Ok(())
    }

    pub fn media_asset(&self, hash: &str) -> Result<Option<MediaAsset>, LibraryError> {
        Ok(self
            .connection
            .query_row(
                "SELECT hash, kind, extension, byte_size FROM media_assets WHERE hash = ?1",
                [hash],
                |row| {
                    Ok(MediaAsset {
                        hash: row.get(0)?,
                        kind: row.get(1)?,
                        extension: row.get(2)?,
                        byte_size: row.get(3)?,
                    })
                },
            )
            .optional()?)
    }

    fn list_sounds(&self, soundboard_id: &str) -> Result<Vec<Sound>, LibraryError> {
        let mut statement = self.connection.prepare(
            "SELECT s.id, s.title, s.audio_hash, audio.extension, s.duration_ms, s.sample_rate, s.channels, s.image_hash, image.extension, s.waveform_json, s.volume, s.pitch_semitones, s.speed, s.replay_policy, s.keybind, s.created_at_ms
             FROM soundboard_sounds ss
             JOIN sounds s ON s.id = ss.sound_id
             JOIN media_assets audio ON audio.hash = s.audio_hash
             LEFT JOIN media_assets image ON image.hash = s.image_hash
             WHERE ss.soundboard_id = ?1 ORDER BY ss.position",
        )?;
        let sounds = statement
            .query_map([soundboard_id], map_sound)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(sounds)
    }

    fn sound_by_id(&self, sound_id: &str) -> Result<Option<Sound>, LibraryError> {
        Ok(self.connection.query_row(
            "SELECT s.id, s.title, s.audio_hash, audio.extension, s.duration_ms, s.sample_rate, s.channels, s.image_hash, image.extension, s.waveform_json, s.volume, s.pitch_semitones, s.speed, s.replay_policy, s.keybind, s.created_at_ms
             FROM sounds s JOIN media_assets audio ON audio.hash = s.audio_hash
             LEFT JOIN media_assets image ON image.hash = s.image_hash WHERE s.id = ?1",
            [sound_id], map_sound,
        ).optional()?)
    }
}

fn map_sound(row: &rusqlite::Row<'_>) -> rusqlite::Result<Sound> {
    Ok(Sound {
        id: row.get(0)?,
        title: row.get(1)?,
        audio_hash: row.get(2)?,
        audio_extension: row.get(3)?,
        duration_ms: row.get(4)?,
        sample_rate: row.get(5)?,
        channels: row.get(6)?,
        image_hash: row.get(7)?,
        image_extension: row.get(8)?,
        waveform: serde_json::from_str(&row.get::<_, String>(9)?).unwrap_or_default(),
        playback: PlaybackProfile {
            volume: row.get(10)?,
            pitch_semitones: row.get(11)?,
            speed: row.get(12)?,
            replay_policy: ReplayPolicy::try_from(row.get::<_, String>(13)?.as_str())
                .unwrap_or(ReplayPolicy::Restart),
            keybind: row.get(14)?,
        },
        created_at_ms: row.get(15)?,
    })
}

fn validate_title(title: &str, maximum: usize) -> Result<&str, LibraryError> {
    let title = title.trim();
    if title.is_empty() || title.chars().count() > maximum {
        Err(LibraryError::InvalidTitle)
    } else {
        Ok(title)
    }
}

fn validate_playback_profile(profile: &PlaybackProfile) -> Result<(), LibraryError> {
    if !profile.volume.is_finite()
        || !profile.pitch_semitones.is_finite()
        || !profile.speed.is_finite()
        || !(0.0..=2.0).contains(&profile.volume)
        || !(-12.0..=12.0).contains(&profile.pitch_semitones)
        || !(0.5..=2.0).contains(&profile.speed)
        || profile
            .keybind
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
    {
        Err(LibraryError::InvalidPlaybackProfile)
    } else {
        Ok(())
    }
}

fn require_changed(changed: usize) -> Result<(), LibraryError> {
    if changed == 1 {
        Ok(())
    } else {
        Err(LibraryError::NotFound)
    }
}

fn normalize_positions(
    transaction: &Transaction<'_>,
    table: &str,
    parent: Option<&str>,
) -> Result<(), LibraryError> {
    let (select, update) = if table == "soundboards" {
        (
            "SELECT id FROM soundboards ORDER BY position",
            "UPDATE soundboards SET position = ?1 WHERE id = ?2",
        )
    } else {
        (
            "SELECT sound_id FROM soundboard_sounds WHERE soundboard_id = ?1 ORDER BY position",
            "UPDATE soundboard_sounds SET position = ?1 WHERE soundboard_id = ?2 AND sound_id = ?3",
        )
    };
    let ids = if let Some(parent) = parent {
        transaction
            .prepare(select)?
            .query_map([parent], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?
    } else {
        transaction
            .prepare(select)?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?
    };
    for (position, id) in ids.iter().enumerate() {
        if let Some(parent) = parent {
            transaction.execute(update, params![position as i64, parent, id])?;
        } else {
            transaction.execute(update, params![position as i64, id])?;
        }
    }
    Ok(())
}

fn orphaned_asset_hashes(transaction: &Transaction<'_>) -> Result<Vec<String>, LibraryError> {
    let mut statement = transaction.prepare(
        "SELECT hash FROM media_assets WHERE hash NOT IN (SELECT audio_hash FROM sounds UNION SELECT image_hash FROM sounds WHERE image_hash IS NOT NULL)",
    )?;
    let hashes = statement
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(hashes)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio_asset(hash: &str) -> MediaAsset {
        MediaAsset {
            hash: hash.to_owned(),
            kind: "audio".to_owned(),
            extension: "wav".to_owned(),
            byte_size: 44,
        }
    }

    #[test]
    fn migrates_and_manages_ordered_soundboards() {
        let mut repository = LibraryRepository::in_memory().unwrap();
        repository.ensure_default_soundboard().unwrap();
        let second = repository.create_soundboard("Répliques").unwrap();
        let boards = repository.list_soundboards().unwrap();
        repository
            .reorder_soundboards(&[second.id.clone(), boards[0].id.clone()])
            .unwrap();
        let reordered = repository.list_soundboards().unwrap();
        assert_eq!(
            reordered
                .iter()
                .map(|board| board.title.as_str())
                .collect::<Vec<_>>(),
            ["Répliques", "Mes sons"]
        );
        repository.rename_soundboard(&second.id, "Jeux").unwrap();
        repository.delete_soundboard(&second.id).unwrap();
        assert_eq!(repository.soundboard_count().unwrap(), 1);
    }

    #[test]
    fn stores_sounds_settings_and_collects_orphaned_assets() {
        let mut repository = LibraryRepository::in_memory().unwrap();
        let board = repository.create_soundboard("Tests").unwrap();
        repository.add_media_asset(&audio_asset("abc")).unwrap();
        let sound = repository
            .create_sound(
                &board.id,
                NewSound {
                    title: "Cloche",
                    audio_hash: "abc",
                    duration_ms: 500,
                    sample_rate: 48_000,
                    channels: 2,
                    waveform: &[0.2, 0.8],
                },
            )
            .unwrap();
        repository
            .set_setting("active_soundboard", &board.id)
            .unwrap();
        repository
            .update_playback_profile(
                &sound.id,
                &PlaybackProfile {
                    volume: 1.4,
                    pitch_semitones: 3.0,
                    speed: 1.25,
                    replay_policy: ReplayPolicy::Overlap,
                    keybind: Some("Control+Shift+KeyA".to_owned()),
                },
            )
            .unwrap();
        assert_eq!(
            repository.setting("active_soundboard").unwrap(),
            Some(board.id.clone())
        );
        let stored_sound = repository.list_soundboards().unwrap()[0].sounds[0].clone();
        assert_eq!(stored_sound.id, sound.id);
        assert_eq!(stored_sound.playback.volume, 1.4);
        assert_eq!(stored_sound.playback.replay_policy, ReplayPolicy::Overlap);
        let orphaned = repository.delete_sound(&board.id, &sound.id).unwrap();
        assert_eq!(orphaned, ["abc"]);
        assert!(repository.media_asset("abc").unwrap().is_none());
    }

    #[test]
    fn rejects_incomplete_orders_and_invalid_titles() {
        let mut repository = LibraryRepository::in_memory().unwrap();
        let board = repository.create_soundboard("Valide").unwrap();
        assert!(matches!(
            repository.create_soundboard("  "),
            Err(LibraryError::InvalidTitle)
        ));
        assert!(matches!(
            repository.reorder_soundboards(&[]),
            Err(LibraryError::InvalidOrder)
        ));
        assert!(repository
            .list_soundboards()
            .unwrap()
            .iter()
            .any(|item| item.id == board.id));
    }

    #[test]
    fn rolls_back_a_failed_migration() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("migration.sqlite3");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch("CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY);")
            .unwrap();
        connection.execute_batch(MIGRATIONS[0]).unwrap();
        connection
            .execute("INSERT INTO schema_migrations(version) VALUES (1)", [])
            .unwrap();
        connection
            .execute_batch(
                "ALTER TABLE sounds ADD COLUMN waveform_json TEXT NOT NULL DEFAULT '[]';",
            )
            .unwrap();
        drop(connection);

        assert!(LibraryRepository::open(&path).is_err());
        let connection = Connection::open(path).unwrap();
        let version: i64 = connection
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, 1);
    }
}
