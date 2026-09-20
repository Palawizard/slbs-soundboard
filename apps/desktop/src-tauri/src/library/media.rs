use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};

use image::ImageReader;
use sha2::{Digest, Sha256};
use slb_audio_engine::{
    process_playback_dsp, PlaybackDspProfile, WindowedSincResampler, CHANNELS, SAMPLE_RATE,
};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use super::{LibraryError, MediaAsset};

const MAX_AUDIO_BYTES: u64 = 25 * 1024 * 1024;
const MAX_IMAGE_BYTES: u64 = 5 * 1024 * 1024;
const MAX_AUDIO_MILLISECONDS: u64 = 10 * 60 * 1_000;
const MAX_IMAGE_DIMENSION: u32 = 8_192;

pub struct MediaStore {
    root: PathBuf,
}

#[derive(Clone, Debug)]
pub struct AudioImport {
    pub asset: MediaAsset,
    pub duration_ms: u64,
    pub sample_rate: u32,
    pub channels: u16,
    pub suggested_title: String,
    pub waveform: Vec<f32>,
}

#[derive(Clone, Debug)]
pub struct DecodedAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl DecodedAudio {
    pub fn into_engine_samples_with_profile(
        self,
        pitch_semitones: f32,
        speed: f32,
    ) -> Result<Vec<f32>, LibraryError> {
        let channels = self.channels as usize;
        let input_frames = self.samples.len() / channels;
        let mut stereo = Vec::with_capacity(input_frames * CHANNELS as usize);
        for frame in self.samples.chunks_exact(channels) {
            stereo.push(frame[0]);
            stereo.push(if channels == 1 { frame[0] } else { frame[1] });
        }
        let stereo = if self.sample_rate == SAMPLE_RATE {
            stereo
        } else {
            resample_to_engine_rate(stereo, self.sample_rate, input_frames)?
        };
        let profile = PlaybackDspProfile::new(pitch_semitones, speed)
            .map_err(|_| LibraryError::InvalidPlaybackProfile)?;
        process_playback_dsp(&stereo, profile).map_err(|_| LibraryError::AudioDecode)
    }
}

fn resample_to_engine_rate(
    stereo: Vec<f32>,
    input_sample_rate: u32,
    input_frames: usize,
) -> Result<Vec<f32>, LibraryError> {
    const CHUNK_FRAMES: usize = 4_096;
    let mut resampler = WindowedSincResampler::new(
        input_sample_rate,
        SAMPLE_RATE,
        CHANNELS as usize,
        CHUNK_FRAMES,
    )
    .map_err(|_| LibraryError::AudioDecode)?;
    let expected_frames =
        (input_frames as u64 * SAMPLE_RATE as u64).div_ceil(input_sample_rate as u64) as usize;
    let mut result = Vec::with_capacity((expected_frames + 64) * CHANNELS as usize);
    let output_frames = ((CHUNK_FRAMES as u64 * SAMPLE_RATE as u64)
        .div_ceil(input_sample_rate as u64) as usize)
        + 64;
    let mut output = vec![0.0; output_frames * CHANNELS as usize];
    for chunk in stereo.chunks(CHUNK_FRAMES * CHANNELS as usize) {
        let written = resampler
            .process(chunk, &mut output)
            .map_err(|_| LibraryError::AudioDecode)?
            .output_frames_written;
        result.extend_from_slice(&output[..written * CHANNELS as usize]);
    }
    let silence = [0.0_f32; 64 * CHANNELS as usize];
    let written = resampler
        .process(&silence, &mut output)
        .map_err(|_| LibraryError::AudioDecode)?
        .output_frames_written;
    result.extend_from_slice(&output[..written * CHANNELS as usize]);
    Ok(result)
}

impl MediaStore {
    pub fn new(root: PathBuf) -> Result<Self, LibraryError> {
        fs::create_dir_all(root.join("audio"))?;
        fs::create_dir_all(root.join("images"))?;
        Ok(Self { root })
    }

    pub fn import_audio(&self, source: &Path) -> Result<AudioImport, LibraryError> {
        require_regular_file(source, MAX_AUDIO_BYTES)?;
        let decoded = decode_audio(source, true)?;
        let duration_ms = decoded.duration_ms;
        if duration_ms == 0 || duration_ms > MAX_AUDIO_MILLISECONDS {
            return Err(LibraryError::AudioTooLong);
        }
        let extension = canonical_audio_extension(source)?;
        let (hash, byte_size) = hash_file(source)?;
        let asset = MediaAsset {
            hash,
            kind: "audio".to_owned(),
            extension: extension.to_owned(),
            byte_size,
        };
        self.persist(source, &asset)?;
        Ok(AudioImport {
            asset,
            duration_ms,
            sample_rate: decoded.sample_rate,
            channels: decoded.channels,
            suggested_title: file_stem(source, "Nouveau son"),
            waveform: waveform_peaks(&decoded.samples, decoded.channels as usize, 48),
        })
    }

    pub fn import_image(&self, source: &Path) -> Result<MediaAsset, LibraryError> {
        require_regular_file(source, MAX_IMAGE_BYTES)?;
        let reader = ImageReader::open(source)
            .map_err(LibraryError::Io)?
            .with_guessed_format()
            .map_err(LibraryError::Io)?;
        let format = reader.format().ok_or(LibraryError::UnsupportedImage)?;
        let extension = match format {
            image::ImageFormat::Png => "png",
            image::ImageFormat::Jpeg => "jpg",
            image::ImageFormat::WebP => "webp",
            image::ImageFormat::Gif => "gif",
            image::ImageFormat::Bmp => "bmp",
            _ => return Err(LibraryError::UnsupportedImage),
        };
        let decoded = reader
            .decode()
            .map_err(|_| LibraryError::UnsupportedImage)?;
        if decoded.width() > MAX_IMAGE_DIMENSION || decoded.height() > MAX_IMAGE_DIMENSION {
            return Err(LibraryError::ImageTooLarge);
        }
        let (hash, byte_size) = hash_file(source)?;
        let asset = MediaAsset {
            hash,
            kind: "image".to_owned(),
            extension: extension.to_owned(),
            byte_size,
        };
        self.persist(source, &asset)?;
        Ok(asset)
    }

    pub fn decode_asset(&self, hash: &str, extension: &str) -> Result<DecodedAudio, LibraryError> {
        let path = self.asset_path("audio", hash, extension);
        let decoded = decode_audio(&path, true)?;
        Ok(DecodedAudio {
            samples: decoded.samples,
            sample_rate: decoded.sample_rate,
            channels: decoded.channels,
        })
    }

    pub fn read_image(&self, hash: &str, extension: &str) -> Result<Vec<u8>, LibraryError> {
        Ok(fs::read(self.asset_path("images", hash, extension))?)
    }

    pub fn remove(&self, hash: &str) -> Result<(), LibraryError> {
        for kind in ["audio", "images"] {
            let directory = self.root.join(kind);
            for entry in fs::read_dir(directory)? {
                let path = entry?.path();
                if path.file_stem().and_then(|name| name.to_str()) == Some(hash) {
                    fs::remove_file(path)?;
                }
            }
        }
        Ok(())
    }

    pub fn asset_path(&self, kind: &str, hash: &str, extension: &str) -> PathBuf {
        self.root.join(kind).join(format!("{hash}.{extension}"))
    }

    fn persist(&self, source: &Path, asset: &MediaAsset) -> Result<(), LibraryError> {
        let directory = if asset.kind == "audio" {
            "audio"
        } else {
            "images"
        };
        let destination = self.asset_path(directory, &asset.hash, &asset.extension);
        if destination.exists() {
            return Ok(());
        }
        let temporary = destination.with_extension(format!("{}.part", asset.extension));
        let mut input = BufReader::new(File::open(source)?);
        let mut output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        std::io::copy(&mut input, &mut output)?;
        output.flush()?;
        output.sync_all()?;
        fs::rename(temporary, destination)?;
        Ok(())
    }
}

struct DecodeResult {
    samples: Vec<f32>,
    sample_rate: u32,
    channels: u16,
    duration_ms: u64,
}

fn decode_audio(path: &Path, retain_samples: bool) -> Result<DecodeResult, LibraryError> {
    let file = File::open(path)?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
        hint.with_extension(extension);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|_| LibraryError::UnsupportedAudio)?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or(LibraryError::UnsupportedAudio)?;
    if track.codec_params.codec == CODEC_TYPE_NULL {
        return Err(LibraryError::UnsupportedAudio);
    }
    let track_id = track.id;
    let parameters = track.codec_params.clone();
    let sample_rate = parameters
        .sample_rate
        .ok_or(LibraryError::UnsupportedAudio)?;
    let channels = parameters
        .channels
        .map(|value| value.count() as u16)
        .ok_or(LibraryError::UnsupportedAudio)?;
    if channels == 0 || channels > 32 {
        return Err(LibraryError::UnsupportedAudio);
    }
    let mut decoder = symphonia::default::get_codecs()
        .make(&parameters, &DecoderOptions::default())
        .map_err(|_| LibraryError::UnsupportedAudio)?;
    let mut total_frames = 0_u64;
    let mut samples = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break
            }
            Err(_) => return Err(LibraryError::AudioDecode),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(buffer) => {
                total_frames = total_frames.saturating_add(buffer.frames() as u64);
                if total_frames.saturating_mul(1_000) / sample_rate as u64 > MAX_AUDIO_MILLISECONDS
                {
                    return Err(LibraryError::AudioTooLong);
                }
                if retain_samples {
                    let mut converted =
                        SampleBuffer::<f32>::new(buffer.capacity() as u64, *buffer.spec());
                    converted.copy_interleaved_ref(buffer);
                    samples.extend_from_slice(converted.samples());
                }
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(_) => return Err(LibraryError::AudioDecode),
        }
    }
    if total_frames == 0 {
        return Err(LibraryError::UnsupportedAudio);
    }
    Ok(DecodeResult {
        samples,
        sample_rate,
        channels,
        duration_ms: total_frames.saturating_mul(1_000) / sample_rate as u64,
    })
}

fn require_regular_file(path: &Path, maximum_bytes: u64) -> Result<(), LibraryError> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(LibraryError::NotFound);
    }
    if metadata.len() == 0 || metadata.len() > maximum_bytes {
        return Err(LibraryError::FileTooLarge);
    }
    Ok(())
}

fn canonical_audio_extension(path: &Path) -> Result<&'static str, LibraryError> {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("wav" | "wave") => Ok("wav"),
        Some("mp3") => Ok("mp3"),
        Some("flac") => Ok("flac"),
        Some("ogg" | "oga") => Ok("ogg"),
        Some("m4a" | "mp4" | "aac") => Ok("m4a"),
        _ => Err(LibraryError::UnsupportedAudio),
    }
}

fn hash_file(path: &Path) -> Result<(String, u64), LibraryError> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut byte_size = 0_u64;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        byte_size += read as u64;
    }
    Ok((format!("{:x}", hasher.finalize()), byte_size))
}

fn file_stem(path: &Path, fallback: &str) -> String {
    path.file_stem()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
        .chars()
        .take(120)
        .collect()
}

fn waveform_peaks(samples: &[f32], channels: usize, bins: usize) -> Vec<f32> {
    let frames = samples.len() / channels;
    if frames == 0 || bins == 0 {
        return Vec::new();
    }
    (0..bins)
        .map(|bin| {
            let start = bin * frames / bins;
            let end = ((bin + 1) * frames / bins).max(start + 1).min(frames);
            samples[start * channels..end * channels]
                .iter()
                .fold(0.0_f32, |value, sample| value.max(sample.abs()))
                .clamp(0.04, 1.0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_wav(path: &Path) {
        let sample_rate = 8_000_u32;
        let frames = 800_u32;
        let data_size = frames * 2;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_size).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt \x10\0\0\0\x01\0\x01\0");
        bytes.extend_from_slice(&sample_rate.to_le_bytes());
        bytes.extend_from_slice(&(sample_rate * 2).to_le_bytes());
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&16_u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_size.to_le_bytes());
        for frame in 0..frames {
            let sample = ((frame as f32 / 8.0).sin() * 8_000.0) as i16;
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn validates_hashes_and_decodes_audio() {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("cloche.wav");
        write_wav(&source);
        let store = MediaStore::new(temporary.path().join("media")).unwrap();
        let imported = store.import_audio(&source).unwrap();
        assert_eq!(imported.duration_ms, 100);
        assert_eq!(imported.sample_rate, 8_000);
        assert_eq!(imported.channels, 1);
        assert_eq!(imported.suggested_title, "cloche");
        assert_eq!(imported.waveform.len(), 48);
        assert!(store
            .asset_path("audio", &imported.asset.hash, "wav")
            .is_file());
        assert_eq!(
            store
                .decode_asset(&imported.asset.hash, "wav")
                .unwrap()
                .samples
                .len(),
            800
        );
    }

    #[test]
    fn rejects_fake_audio_and_oversized_input() {
        let temporary = tempfile::tempdir().unwrap();
        let fake = temporary.path().join("fake.wav");
        fs::write(&fake, b"not audio").unwrap();
        let store = MediaStore::new(temporary.path().join("media")).unwrap();
        assert!(matches!(
            store.import_audio(&fake),
            Err(LibraryError::UnsupportedAudio)
        ));
    }

    #[test]
    fn validates_and_stores_decodable_images() {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("cover.png");
        image::RgbImage::from_pixel(16, 16, image::Rgb([30, 80, 160]))
            .save(&source)
            .unwrap();
        let store = MediaStore::new(temporary.path().join("media")).unwrap();
        let imported = store.import_image(&source).unwrap();
        assert_eq!(imported.extension, "png");
        assert_eq!(
            store.read_image(&imported.hash, "png").unwrap(),
            fs::read(source).unwrap()
        );
    }

    #[test]
    fn converts_decoded_audio_to_engine_format() {
        let input = DecodedAudio {
            samples: vec![0.5; 8_000],
            sample_rate: 8_000,
            channels: 1,
        };
        let output = input.into_engine_samples_with_profile(0.0, 1.0).unwrap();
        assert!(output.len() >= 47_000 * 2);
        assert!(output.len() <= 49_000 * 2);
        assert!(output.chunks_exact(2).all(|frame| frame[0] == frame[1]));
    }
}
