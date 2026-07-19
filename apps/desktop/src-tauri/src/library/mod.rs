mod db;
mod models;

use std::io;

pub use db::{LibraryRepository, NewSound};
pub use models::{MediaAsset, Sound, Soundboard};
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
}
