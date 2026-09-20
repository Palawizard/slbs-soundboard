CREATE OR REPLACE FUNCTION enforce_publication_media_ownership() RETURNS trigger AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM user_media
        WHERE id = NEW.audio_media_id AND user_id = NEW.owner_id AND deleted_at IS NULL
    ) THEN
        RAISE EXCEPTION 'audio media is not owned by publication owner' USING ERRCODE = '23514';
    END IF;
    IF NEW.image_media_id IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM user_media
        WHERE id = NEW.image_media_id AND user_id = NEW.owner_id AND deleted_at IS NULL
    ) THEN
        RAISE EXCEPTION 'image media is not owned by publication owner' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER publications_media_owner
BEFORE INSERT OR UPDATE OF owner_id, audio_media_id, image_media_id ON publications
FOR EACH ROW EXECUTE FUNCTION enforce_publication_media_ownership();
