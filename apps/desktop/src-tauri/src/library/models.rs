use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ReplayPolicy {
    Overlap,
    Toggle,
    Stop,
    Restart,
}

impl ReplayPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Overlap => "overlap",
            Self::Toggle => "toggle",
            Self::Stop => "stop",
            Self::Restart => "restart",
        }
    }
}

impl TryFrom<&str> for ReplayPolicy {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "overlap" => Ok(Self::Overlap),
            "toggle" => Ok(Self::Toggle),
            "stop" => Ok(Self::Stop),
            "restart" => Ok(Self::Restart),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackProfile {
    pub volume: f32,
    pub pitch_semitones: f32,
    pub speed: f32,
    pub replay_policy: ReplayPolicy,
    pub keybind: Option<String>,
}

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
    pub waveform: Vec<f32>,
    pub playback: PlaybackProfile,
    pub created_at_ms: u64,
    pub publication_id: Option<String>,
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
