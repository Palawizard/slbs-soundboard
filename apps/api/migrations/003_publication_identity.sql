CREATE UNIQUE INDEX publications_active_owner_audio_idx
ON publications(owner_id, audio_media_id)
WHERE status = 'active';
