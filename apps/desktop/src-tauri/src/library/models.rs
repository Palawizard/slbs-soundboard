use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Sound {
    pub id: String,
    pub title: String,
    pub audio_hash: String,
    pub audio_extension: String,
    pub duration_ms: u64,
    pub sample_rate: u32,
    pub channels: u16,
    pub image_hash: Option<String>,
    pub image_extension: Option<String>,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Soundboard {
    pub id: String,
    pub title: String,
    pub position: i64,
    pub sounds: Vec<Sound>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MediaAsset {
    pub hash: String,
    pub kind: String,
    pub extension: String,
    pub byte_size: u64,
}
