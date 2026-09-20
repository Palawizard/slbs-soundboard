use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    LibraryError, LibraryRepository, MediaStore, NewSound, PlaybackProfile, Sound, Soundboard,
};

pub struct LibraryService {
    repository: LibraryRepository,
    media: MediaStore,
    database_path: PathBuf,
    backup_path: PathBuf,
    recovery_notice: Option<String>,
}

impl LibraryService {
    pub fn open(root: &Path) -> Result<Self, LibraryError> {
        fs::create_dir_all(root)?;
        let database_path = root.join("library.sqlite3");
        let backup_path = root.join("library.backup.sqlite3");
        let (mut repository, recovery_notice) = match LibraryRepository::open(&database_path) {
            Ok(repository) => (repository, None),
            Err(_) if database_path.exists() => {
                let corrupt_path = root.join(format!("library.corrupt.{}.sqlite3", now_ms()));
                fs::rename(&database_path, &corrupt_path)?;
                let notice = if backup_path.exists() {
                    fs::copy(&backup_path, &database_path)?;
                    "La bibliothèque a été restaurée depuis sa sauvegarde.".to_owned()
                } else {
                    "Une nouvelle bibliothèque a été créée après une erreur locale.".to_owned()
                };
                (LibraryRepository::open(&database_path)?, Some(notice))
            }
            Err(error) => return Err(error),
        };
        repository.ensure_default_soundboard()?;
        let service = Self {
            repository,
            media: MediaStore::new(root.join("media"))?,
            database_path,
            backup_path,
            recovery_notice,
        };
        service.backup()?;
        Ok(service)
    }

    pub fn soundboards(&self) -> Result<Vec<Soundboard>, LibraryError> {
        self.repository.list_soundboards()
    }

    pub fn recovery_notice(&mut self) -> Option<String> {
        self.recovery_notice.take()
    }

    pub fn active_soundboard_id(&self) -> Result<Option<String>, LibraryError> {
        self.repository.setting("active_soundboard")
    }

    pub fn set_active_soundboard(&mut self, id: &str) -> Result<(), LibraryError> {
        if !self.soundboards()?.iter().any(|board| board.id == id) {
            return Err(LibraryError::NotFound);
        }
        self.repository.set_setting("active_soundboard", id)?;
        self.backup()
    }

    pub fn create_soundboard(&mut self, title: &str) -> Result<Soundboard, LibraryError> {
        let board = self.repository.create_soundboard(title)?;
        self.backup()?;
        Ok(board)
    }

    pub fn rename_soundboard(&mut self, id: &str, title: &str) -> Result<(), LibraryError> {
        self.repository.rename_soundboard(id, title)?;
        self.backup()
    }

    pub fn delete_soundboard(&mut self, id: &str) -> Result<(), LibraryError> {
        self.repository.delete_soundboard(id)?;
        self.repository.ensure_default_soundboard()?;
        self.backup()
    }

    pub fn reorder_soundboards(&mut self, ids: &[String]) -> Result<(), LibraryError> {
        self.repository.reorder_soundboards(ids)?;
        self.backup()
    }

    pub fn import_sound(
        &mut self,
        soundboard_id: &str,
        path: &Path,
    ) -> Result<Sound, LibraryError> {
        let imported = self.media.import_audio(path)?;
        self.repository.add_media_asset(&imported.asset)?;
        let sound = self.repository.create_sound(
            soundboard_id,
            NewSound {
                title: &imported.suggested_title,
                audio_hash: &imported.asset.hash,
                duration_ms: imported.duration_ms,
                sample_rate: imported.sample_rate,
                channels: imported.channels,
                waveform: &imported.waveform,
            },
        )?;
        self.backup()?;
        Ok(sound)
    }

    pub fn import_community_sound(
        &mut self,
        soundboard_id: &str,
        audio_path: &Path,
        image_path: Option<&Path>,
        title: &str,
    ) -> Result<Sound, LibraryError> {
        let audio = self.media.import_audio(audio_path)?;
        let image = image_path
            .map(|path| self.media.import_image(path))
            .transpose()?;
        let result = self.repository.create_community_sound(
            soundboard_id,
            NewSound {
                title,
                audio_hash: &audio.asset.hash,
                duration_ms: audio.duration_ms,
                sample_rate: audio.sample_rate,
                channels: audio.channels,
                waveform: &audio.waveform,
            },
            &audio.asset,
            image.as_ref(),
        );
        if result.is_err() {
            for asset in std::iter::once(&audio.asset).chain(image.iter()) {
                if self.repository.media_asset(&asset.hash)?.is_none() {
                    self.media.remove(&asset.hash)?;
                }
            }
        }
        let sound = result?;
        self.backup()?;
        Ok(sound)
    }

    pub fn sound_assets(
        &self,
        sound_id: &str,
    ) -> Result<(Sound, PathBuf, Option<PathBuf>), LibraryError> {
        let sound = self
            .soundboards()?
            .into_iter()
            .flat_map(|board| board.sounds)
            .find(|sound| sound.id == sound_id)
            .ok_or(LibraryError::NotFound)?;
        let audio = self
            .media
            .asset_path("audio", &sound.audio_hash, &sound.audio_extension);
        let image = sound
            .image_hash
            .as_ref()
            .zip(sound.image_extension.as_ref())
            .map(|(hash, extension)| self.media.asset_path("images", hash, extension));
        Ok((sound, audio, image))
    }

    pub fn set_publication(
        &mut self,
        sound_id: &str,
        publication_id: &str,
        audio_hash: &str,
    ) -> Result<(), LibraryError> {
        self.repository
            .set_publication(sound_id, publication_id, audio_hash)?;
        self.backup()
    }

    pub fn clear_publication(&mut self, publication_id: &str) -> Result<(), LibraryError> {
        self.repository.clear_publication(publication_id)?;
        self.backup()
    }

    pub fn reconcile_publications(
        &mut self,
        remote: &[(String, String)],
    ) -> Result<(), LibraryError> {
        let values = self
            .soundboards()?
            .into_iter()
            .flat_map(|board| board.sounds)
            .filter_map(|sound| {
                remote
                    .iter()
                    .find(|(_, hash)| hash == &sound.audio_hash)
                    .map(|(publication_id, hash)| (sound.id, publication_id.clone(), hash.clone()))
            })
            .collect::<Vec<_>>();
        self.repository.replace_publications(&values)?;
        self.backup()
    }

    /// Output device carrying the virtual microphone, empty when routing is off.
    pub fn virtual_output_device(&self) -> Result<Option<String>, LibraryError> {
        Ok(self
            .repository
            .setting("virtual_output_device")?
            .filter(|value| !value.is_empty()))
    }

    pub fn set_virtual_output_device(&mut self, device: Option<&str>) -> Result<(), LibraryError> {
        self.repository
            .set_setting("virtual_output_device", device.unwrap_or_default())?;
        self.backup()
    }

    /// Capture device the engine reopens on launch, empty for the system default.
    pub fn input_device(&self) -> Result<Option<String>, LibraryError> {
        Ok(self
            .repository
            .setting("input_device")?
            .filter(|value| !value.is_empty()))
    }

    pub fn set_input_device(&mut self, device: Option<&str>) -> Result<(), LibraryError> {
        self.repository
            .set_setting("input_device", device.unwrap_or_default())?;
        self.backup()
    }

    /// Local monitoring of the triggered sounds, on unless the user turned it off.
    pub fn monitor_enabled(&self) -> Result<bool, LibraryError> {
        Ok(self.repository.setting("monitor_enabled")?.as_deref() != Some("false"))
    }

    pub fn set_monitor_enabled(&mut self, enabled: bool) -> Result<(), LibraryError> {
        self.repository
            .set_setting("monitor_enabled", if enabled { "true" } else { "false" })?;
        self.backup()
    }

    pub fn diagnostics_enabled(&self) -> Result<bool, LibraryError> {
        Ok(self.repository.setting("diagnostics_enabled")?.as_deref() != Some("false"))
    }

    pub fn set_diagnostics_enabled(&mut self, enabled: bool) -> Result<(), LibraryError> {
        self.repository.set_setting(
            "diagnostics_enabled",
            if enabled { "true" } else { "false" },
        )?;
        self.backup()
    }

    pub fn set_sound_image(&mut self, sound_id: &str, path: &Path) -> Result<(), LibraryError> {
        let image = self.media.import_image(path)?;
        self.repository.add_media_asset(&image)?;
        let orphaned = self
            .repository
            .set_sound_image(sound_id, Some(&image.hash))?;
        for hash in orphaned {
            self.media.remove(&hash)?;
        }
        self.backup()
    }

    pub fn rename_sound(&mut self, sound_id: &str, title: &str) -> Result<(), LibraryError> {
        self.repository.rename_sound(sound_id, title)?;
        self.backup()
    }

    pub fn update_playback_profile(
        &mut self,
        sound_id: &str,
        profile: &PlaybackProfile,
    ) -> Result<(), LibraryError> {
        self.repository.update_playback_profile(sound_id, profile)?;
        self.backup()
    }

    pub fn remove_sound(
        &mut self,
        soundboard_id: &str,
        sound_id: &str,
    ) -> Result<(), LibraryError> {
        let orphaned = self.repository.delete_sound(soundboard_id, sound_id)?;
        for hash in orphaned {
            self.media.remove(&hash)?;
        }
        self.backup()
    }

    pub fn reorder_sounds(
        &mut self,
        soundboard_id: &str,
        ids: &[String],
    ) -> Result<(), LibraryError> {
        self.repository.reorder_sounds(soundboard_id, ids)?;
        self.backup()
    }

    pub fn prepare_sound(&self, sound_id: &str) -> Result<(Sound, Vec<f32>), LibraryError> {
        let sound = self
            .repository
            .list_soundboards()?
            .into_iter()
            .flat_map(|board| board.sounds)
            .find(|sound| sound.id == sound_id)
            .ok_or(LibraryError::NotFound)?;
        let decoded = self
            .media
            .decode_asset(&sound.audio_hash, &sound.audio_extension)?;
        let samples = decoded.into_engine_samples_with_profile(
            sound.playback.pitch_semitones,
            sound.playback.speed,
        )?;
        Ok((sound, samples))
    }

    pub fn image_data(&self, hash: &str) -> Result<(Vec<u8>, String), LibraryError> {
        let asset = self
            .repository
            .media_asset(hash)?
            .ok_or(LibraryError::NotFound)?;
        if asset.kind != "image" {
            return Err(LibraryError::NotFound);
        }
        Ok((
            self.media.read_image(hash, &asset.extension)?,
            asset.extension,
        ))
    }

    fn backup(&self) -> Result<(), LibraryError> {
        let temporary = self.backup_path.with_extension("sqlite3.part");
        if temporary.exists() {
            fs::remove_file(&temporary)?;
        }
        self.repository.backup_to(&temporary)?;
        if self.backup_path.exists() {
            fs::remove_file(&self.backup_path)?;
        }
        fs::rename(temporary, &self.backup_path)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn database_path(&self) -> &Path {
        &self.database_path
    }
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

    #[test]
    fn restores_a_damaged_database_from_backup() {
        let temporary = tempfile::tempdir().unwrap();
        {
            let mut service = LibraryService::open(temporary.path()).unwrap();
            service.create_soundboard("Sauvegardé").unwrap();
        }
        fs::write(temporary.path().join("library.sqlite3"), b"broken database").unwrap();
        let mut recovered = LibraryService::open(temporary.path()).unwrap();
        assert!(recovered
            .soundboards()
            .unwrap()
            .iter()
            .any(|board| board.title == "Sauvegardé"));
        assert!(recovered.recovery_notice().unwrap().contains("restaurée"));
        assert!(fs::read_dir(temporary.path()).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("library.corrupt.")));
    }

    #[test]
    fn remembers_the_capture_device_across_sessions() {
        let temporary = tempfile::tempdir().unwrap();
        {
            let mut service = LibraryService::open(temporary.path()).unwrap();
            assert_eq!(service.input_device().unwrap(), None);
            service.set_input_device(Some("microphone-1")).unwrap();
        }
        let mut service = LibraryService::open(temporary.path()).unwrap();
        assert_eq!(
            service.input_device().unwrap().as_deref(),
            Some("microphone-1")
        );
        service.set_input_device(None).unwrap();
        assert_eq!(service.input_device().unwrap(), None);
    }
}
