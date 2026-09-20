mod db;
mod media;
mod models;
mod service;

use std::io;

pub use db::{LibraryRepository, NewSound};
pub use media::MediaStore;
pub use models::{MediaAsset, PlaybackProfile, ReplayPolicy, Sound, Soundboard};
pub use service::LibraryService;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LibraryError {
    #[error("La bibliothèque locale est indisponible.")]
    Database(#[from] rusqlite::Error),
    #[error("Impossible d’accéder au fichier sélectionné.")]
    Io(#[from] io::Error),
    #[error("Ce nom n’est pas valide.")]
    InvalidTitle,
    #[error("Cet élément est introuvable.")]
    NotFound,
    #[error("L’ordre demandé n’est pas valide.")]
    InvalidOrder,
    #[error("Ce fichier est trop volumineux.")]
    FileTooLarge,
    #[error("Ce fichier audio n’est pas pris en charge.")]
    UnsupportedAudio,
    #[error("Ce son dépasse dix minutes.")]
    AudioTooLong,
    #[error("Cette image n’est pas prise en charge.")]
    UnsupportedImage,
    #[error("Cette image est trop grande.")]
    ImageTooLarge,
    #[error("Les données audio sont invalides.")]
    AudioDecode,
    #[error("Les réglages de lecture ne sont pas valides.")]
    InvalidPlaybackProfile,
    #[error("Ce raccourci est déjà associé à un autre son.")]
    KeybindConflict,
}
