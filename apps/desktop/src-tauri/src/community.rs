use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use base64::Engine as _;
use futures_util::{StreamExt, TryStreamExt};
use rand::RngCore;
use reqwest::{multipart, Client, Response};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::ipc::Channel;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::{timeout, Duration};
use tokio_util::io::ReaderStream;
use url::Url;

const CREDENTIAL_SERVICE: &str = "fr.slb.soundboard.community";
const CREDENTIAL_ACCOUNT: &str = "session";
const MAX_AUDIO_BYTES: u64 = 25 * 1024 * 1024;
const MAX_IMAGE_BYTES: u64 = 5 * 1024 * 1024;

pub struct CommunityClient {
    http: Client,
    base_url: Url,
    log_path: PathBuf,
    diagnostics_enabled: AtomicBool,
    log_lock: Mutex<()>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicUser {
    pub id: String,
    pub username: String,
    pub avatar_url: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnedMedia {
    pub id: String,
    pub kind: String,
    pub hash: String,
    pub mime_type: String,
    pub byte_size: u64,
    pub duration_ms: Option<u64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunitySound {
    pub id: String,
    pub title: String,
    pub description: String,
    pub owner: PublicUser,
    pub audio: OwnedMedia,
    pub image: Option<OwnedMedia>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthStart {
    authorization_url: String,
    state: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub token: String,
    pub expires_at: String,
    pub user: PublicUser,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub expires_at: Option<String>,
    pub user: PublicUser,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicationList {
    pub items: Vec<CommunitySound>,
    pub next_cursor: Option<String>,
}

#[derive(Deserialize)]
struct OwnedPublicationList {
    items: Vec<CommunitySound>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportMetadata {
    publication: CommunitySound,
    audio_url: String,
    image_url: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferProgress {
    pub stage: &'static str,
    pub transferred_bytes: u64,
    pub total_bytes: u64,
    pub percent: u8,
}

struct UploadProgress<'a> {
    total: u64,
    sent: &'a mut u64,
    channel: &'a Channel<TransferProgress>,
}

struct DownloadExpectation<'a> {
    byte_size: u64,
    maximum: u64,
    hash: &'a str,
}

#[derive(Deserialize)]
struct ErrorEnvelope {
    error: ApiError,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiError {
    code: String,
    message: String,
    request_id: String,
}

impl CommunityClient {
    pub fn new(app_data: &Path, diagnostics_enabled: bool) -> Result<Self, String> {
        let configured = std::env::var("SLB_COMMUNITY_API_URL")
            .ok()
            .or_else(|| option_env!("SLB_COMMUNITY_API_URL").map(str::to_owned))
            .unwrap_or_else(|| "http://127.0.0.1:3000".to_owned());
        let base_url = Url::parse(configured.trim_end_matches('/'))
            .map_err(|_| "L’adresse du service communautaire est invalide.".to_owned())?;
        let loopback = matches!(base_url.host_str(), Some("127.0.0.1" | "localhost" | "::1"));
        if base_url.scheme() != "https" && !(base_url.scheme() == "http" && loopback) {
            return Err("Le service communautaire doit utiliser HTTPS.".to_owned());
        }
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(90))
            .user_agent(concat!("slbs-soundboard/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| "Impossible d’initialiser le service communautaire.".to_owned())?;
        Ok(Self {
            http,
            base_url,
            log_path: app_data.join("diagnostics.jsonl"),
            diagnostics_enabled: AtomicBool::new(diagnostics_enabled),
            log_lock: Mutex::new(()),
        })
    }

    pub fn base_url(&self) -> String {
        self.base_url.to_string().trim_end_matches('/').to_owned()
    }

    pub fn set_diagnostics(&self, enabled: bool) {
        self.diagnostics_enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn log(&self, level: &str, event: &str, detail: &str) {
        if !self.diagnostics_enabled.load(Ordering::Relaxed) {
            return;
        }
        let Ok(_guard) = self.log_lock.lock() else {
            return;
        };
        let sanitized = detail
            .replace(['\r', '\n'], " ")
            .chars()
            .take(240)
            .collect::<String>();
        let line = serde_json::json!({
            "timestampMs": now_ms(), "level": level, "event": event, "detail": sanitized,
        });
        if let Ok(mut output) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
        {
            let _ = writeln!(output, "{line}");
        }
    }

    pub fn read_logs(&self) -> Result<String, String> {
        if !self.log_path.exists() {
            return Ok(String::new());
        }
        let bytes = fs::read(&self.log_path)
            .map_err(|_| "Impossible de lire les diagnostics.".to_owned())?;
        let start = bytes.len().saturating_sub(128 * 1024);
        Ok(String::from_utf8_lossy(&bytes[start..]).into_owned())
    }

    pub fn clear_logs(&self) -> Result<(), String> {
        if self.log_path.exists() {
            fs::remove_file(&self.log_path)
                .map_err(|_| "Impossible d’effacer les diagnostics.".to_owned())?;
        }
        Ok(())
    }

    pub async fn login(&self, app: &AppHandle) -> Result<SessionView, String> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|_| "Impossible de préparer le retour de connexion.".to_owned())?;
        let port = listener
            .local_addr()
            .map_err(|_| "Le retour de connexion est indisponible.".to_owned())?
            .port();
        let redirect_uri = format!("http://127.0.0.1:{port}/callback");
        let verifier = random_token();
        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(Sha256::digest(verifier.as_bytes()));
        let response = self
            .http
            .post(self.endpoint("v1/auth/google/start"))
            .json(&serde_json::json!({ "redirectUri": redirect_uri, "codeChallenge": challenge }))
            .send()
            .await
            .map_err(network_error)?;
        let started: AuthStart = decode(response).await?;
        app.opener()
            .open_url(&started.authorization_url, None::<&str>)
            .map_err(|_| "Impossible d’ouvrir la connexion Google.".to_owned())?;
        let (code, returned_state) = timeout(Duration::from_secs(600), receive_callback(listener))
            .await
            .map_err(|_| "La connexion Google a expiré.".to_owned())??;
        if returned_state != started.state {
            return Err("La réponse de connexion n’est pas valide.".to_owned());
        }
        let response = self.http.post(self.endpoint("v1/auth/google/exchange"))
            .json(&serde_json::json!({ "state": started.state, "code": code, "codeVerifier": verifier, "redirectUri": redirect_uri }))
            .send().await.map_err(network_error)?;
        let session: Session = decode(response).await?;
        credential()?
            .set_password(&session.token)
            .map_err(|_| "Impossible de protéger la session dans Windows.".to_owned())?;
        self.log("info", "auth.login", "Connexion réussie");
        Ok(SessionView {
            expires_at: Some(session.expires_at),
            user: session.user,
        })
    }

    pub async fn session(&self) -> Result<Option<SessionView>, String> {
        let Some(token) = session_token()? else {
            return Ok(None);
        };
        let response = self
            .http
            .get(self.endpoint("v1/me"))
            .bearer_auth(&token)
            .send()
            .await
            .map_err(network_error)?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            delete_token()?;
            return Ok(None);
        }
        Ok(Some(SessionView {
            expires_at: None,
            user: decode(response).await?,
        }))
    }

    pub async fn update_username(&self, username: &str) -> Result<PublicUser, String> {
        let token = require_token()?;
        let response = self
            .http
            .patch(self.endpoint("v1/me"))
            .bearer_auth(token)
            .json(&serde_json::json!({ "username": username.trim() }))
            .send()
            .await
            .map_err(network_error)?;
        let user = decode(response).await?;
        self.log("info", "account.rename", "Pseudonyme modifié");
        Ok(user)
    }

    pub async fn logout(&self) -> Result<(), String> {
        if let Some(token) = session_token()? {
            let response = self
                .http
                .delete(self.endpoint("v1/session"))
                .bearer_auth(token)
                .send()
                .await
                .map_err(network_error)?;
            if !response.status().is_success()
                && response.status() != reqwest::StatusCode::UNAUTHORIZED
            {
                decode_empty(response).await?;
            }
        }
        delete_token()?;
        self.log("info", "auth.logout", "Déconnexion locale terminée");
        Ok(())
    }

    pub async fn browse(
        &self,
        query: Option<String>,
        cursor: Option<String>,
    ) -> Result<PublicationList, String> {
        let mut url = self.endpoint("v1/publications");
        {
            let mut values = url.query_pairs_mut();
            values.append_pair("limit", "20");
            if let Some(query) = query.filter(|value| !value.trim().is_empty()) {
                values.append_pair("q", query.trim());
            }
            if let Some(cursor) = cursor {
                values.append_pair("cursor", &cursor);
            }
        }
        decode(self.http.get(url).send().await.map_err(network_error)?).await
    }

    pub async fn owned_publications(&self) -> Result<Vec<CommunitySound>, String> {
        let response = self
            .http
            .get(self.endpoint("v1/me/publications"))
            .bearer_auth(require_token()?)
            .send()
            .await
            .map_err(network_error)?;
        Ok(decode::<OwnedPublicationList>(response).await?.items)
    }

    pub async fn publish(
        &self,
        audio: &Path,
        image: Option<&Path>,
        title: &str,
        description: &str,
        progress: Channel<TransferProgress>,
    ) -> Result<CommunitySound, String> {
        let token = require_token()?;
        let total = fs::metadata(audio)
            .map_err(|_| "Le fichier audio local est introuvable.".to_owned())?
            .len()
            + image
                .and_then(|path| fs::metadata(path).ok())
                .map(|value| value.len())
                .unwrap_or(0);
        let mut sent = 0;
        let audio_media = self
            .upload(
                "audio",
                audio,
                &token,
                "audio",
                UploadProgress {
                    total,
                    sent: &mut sent,
                    channel: &progress,
                },
            )
            .await?;
        let image_media = match image {
            Some(path) => Some(
                self.upload(
                    "image",
                    path,
                    &token,
                    "image",
                    UploadProgress {
                        total,
                        sent: &mut sent,
                        channel: &progress,
                    },
                )
                .await?,
            ),
            None => None,
        };
        let _ = progress.send(progress_event("publication", sent, total));
        let response = self.http.post(self.endpoint("v1/publications")).bearer_auth(token)
            .json(&serde_json::json!({ "title": title, "description": description, "audioMediaId": audio_media.id, "imageMediaId": image_media.map(|media| media.id) }))
            .send().await.map_err(network_error)?;
        let publication = decode(response).await?;
        self.log("info", "publication.create", "Son publié");
        Ok(publication)
    }

    async fn upload(
        &self,
        kind: &str,
        path: &Path,
        token: &str,
        stage: &'static str,
        transfer: UploadProgress<'_>,
    ) -> Result<OwnedMedia, String> {
        let length = tokio::fs::metadata(path)
            .await
            .map_err(|_| "Un média local est introuvable.".to_owned())?
            .len();
        let file = tokio::fs::File::open(path)
            .await
            .map_err(|_| "Impossible de lire un média local.".to_owned())?;
        let base = *transfer.sent;
        let mut current = 0_u64;
        let channel = transfer.channel.clone();
        let total = transfer.total;
        let stream = ReaderStream::new(file).map_ok(move |chunk| {
            current += chunk.len() as u64;
            let _ = channel.send(progress_event(stage, base + current, total));
            chunk
        });
        let filename = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("media.bin")
            .to_owned();
        let part = multipart::Part::stream_with_length(reqwest::Body::wrap_stream(stream), length)
            .file_name(filename);
        let response = self
            .http
            .post(self.endpoint(&format!("v1/media/{kind}")))
            .bearer_auth(token)
            .multipart(multipart::Form::new().part("file", part))
            .send()
            .await
            .map_err(network_error)?;
        *transfer.sent += length;
        decode(response).await
    }

    pub async fn delete_publication(&self, id: &str) -> Result<(), String> {
        let response = self
            .http
            .delete(self.endpoint(&format!("v1/publications/{id}")))
            .bearer_auth(require_token()?)
            .send()
            .await
            .map_err(network_error)?;
        decode_empty(response).await?;
        self.log("info", "publication.delete", "Publication retirée");
        Ok(())
    }

    pub async fn download_import(
        &self,
        id: &str,
        staging: &Path,
        progress: Channel<TransferProgress>,
    ) -> Result<(CommunitySound, PathBuf, Option<PathBuf>), String> {
        let response = self
            .http
            .get(self.endpoint(&format!("v1/publications/{id}/import")))
            .bearer_auth(require_token()?)
            .send()
            .await
            .map_err(network_error)?;
        let metadata: ImportMetadata = decode(response).await?;
        fs::create_dir_all(staging).map_err(|_| "Impossible de préparer l’import.".to_owned())?;
        let audio_path = staging.join(format!(
            "audio.{}",
            extension_for_mime(&metadata.publication.audio.mime_type, "wav")
        ));
        download(
            &self.http,
            &metadata.audio_url,
            &audio_path,
            DownloadExpectation {
                byte_size: metadata.publication.audio.byte_size,
                maximum: MAX_AUDIO_BYTES,
                hash: &metadata.publication.audio.hash,
            },
            "audio",
            &progress,
        )
        .await?;
        let image_path = match (&metadata.image_url, &metadata.publication.image) {
            (Some(url), Some(image)) => {
                let path = staging.join(format!(
                    "image.{}",
                    extension_for_mime(&image.mime_type, "png")
                ));
                download(
                    &self.http,
                    url,
                    &path,
                    DownloadExpectation {
                        byte_size: image.byte_size,
                        maximum: MAX_IMAGE_BYTES,
                        hash: &image.hash,
                    },
                    "image",
                    &progress,
                )
                .await?;
                Some(path)
            }
            _ => None,
        };
        Ok((metadata.publication, audio_path, image_path))
    }

    fn endpoint(&self, path: &str) -> Url {
        self.base_url.join(path).expect("validated API path")
    }
}

async fn receive_callback(listener: TcpListener) -> Result<(String, String), String> {
    let (mut stream, _) = listener
        .accept()
        .await
        .map_err(|_| "Le retour de connexion a échoué.".to_owned())?;
    let mut buffer = vec![0_u8; 8192];
    let length = stream
        .read(&mut buffer)
        .await
        .map_err(|_| "La réponse de connexion est illisible.".to_owned())?;
    let request = String::from_utf8_lossy(&buffer[..length]);
    let target = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .ok_or_else(|| "La réponse de connexion est invalide.".to_owned())?;
    let url = Url::parse(&format!("http://127.0.0.1{target}"))
        .map_err(|_| "La réponse de connexion est invalide.".to_owned())?;
    let values = url
        .query_pairs()
        .into_owned()
        .collect::<std::collections::HashMap<_, _>>();
    let success = values.contains_key("code") && values.contains_key("state");
    let body = if success {
        "Connexion terminée. Vous pouvez fermer cette fenêtre."
    } else {
        "La connexion a été annulée. Vous pouvez fermer cette fenêtre."
    };
    let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    let _ = stream.write_all(response.as_bytes()).await;
    if let Some(error) = values.get("error") {
        return Err(format!("Connexion Google annulée ({error})."));
    }
    Ok((
        values
            .get("code")
            .cloned()
            .ok_or_else(|| "Le code de connexion est absent.".to_owned())?,
        values
            .get("state")
            .cloned()
            .ok_or_else(|| "L’état de connexion est absent.".to_owned())?,
    ))
}

async fn download(
    client: &Client,
    url: &str,
    path: &Path,
    expectation: DownloadExpectation<'_>,
    stage: &'static str,
    progress: &Channel<TransferProgress>,
) -> Result<(), String> {
    if expectation.byte_size == 0 || expectation.byte_size > expectation.maximum {
        return Err("La taille du média distant n’est pas valide.".to_owned());
    }
    let response = checked(client.get(url).send().await.map_err(network_error)?).await?;
    let mut stream = response.bytes_stream();
    let mut output = tokio::fs::File::create(path)
        .await
        .map_err(|_| "Impossible de préparer le média local.".to_owned())?;
    let mut digest = Sha256::new();
    let mut received = 0_u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(network_error)?;
        received += chunk.len() as u64;
        if received > expectation.maximum || received > expectation.byte_size {
            return Err("Le média distant dépasse la taille annoncée.".to_owned());
        }
        digest.update(&chunk);
        output
            .write_all(&chunk)
            .await
            .map_err(|_| "L’écriture du média local a échoué.".to_owned())?;
        let _ = progress.send(progress_event(stage, received, expectation.byte_size));
    }
    output
        .flush()
        .await
        .map_err(|_| "L’écriture du média local a échoué.".to_owned())?;
    if received != expectation.byte_size || format!("{:x}", digest.finalize()) != expectation.hash {
        return Err("L’intégrité du média distant n’est pas valide.".to_owned());
    }
    Ok(())
}

async fn decode<T: for<'de> Deserialize<'de>>(response: Response) -> Result<T, String> {
    checked(response)
        .await?
        .json()
        .await
        .map_err(|_| "La réponse du service est invalide.".to_owned())
}

async fn decode_empty(response: Response) -> Result<(), String> {
    checked(response).await.map(|_| ())
}

async fn checked(response: Response) -> Result<Response, String> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let error = response
        .json::<ErrorEnvelope>()
        .await
        .ok()
        .map(|value| value.error);
    match error {
        Some(error) => Err(format!(
            "{} ({} · {})",
            error.message, error.code, error.request_id
        )),
        None => Err(format!(
            "Le service communautaire a répondu avec l’état {status}."
        )),
    }
}

fn credential() -> Result<keyring::Entry, String> {
    keyring::Entry::new(CREDENTIAL_SERVICE, CREDENTIAL_ACCOUNT)
        .map_err(|_| "Le coffre d’identifiants Windows est indisponible.".to_owned())
}
fn session_token() -> Result<Option<String>, String> {
    match credential()?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err("Impossible de lire la session protégée.".to_owned()),
    }
}
fn require_token() -> Result<String, String> {
    session_token()?.ok_or_else(|| "Une connexion est requise.".to_owned())
}
fn delete_token() -> Result<(), String> {
    match credential()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("Impossible d’effacer la session protégée.".to_owned()),
    }
}
fn random_token() -> String {
    let mut bytes = [0_u8; 64];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}
fn network_error(_: reqwest::Error) -> String {
    "Le service communautaire est injoignable. Vérifiez votre connexion puis réessayez.".to_owned()
}
fn progress_event(stage: &'static str, transferred: u64, total: u64) -> TransferProgress {
    TransferProgress {
        stage,
        transferred_bytes: transferred,
        total_bytes: total,
        percent: if total == 0 {
            0
        } else {
            ((transferred.saturating_mul(100) / total).min(100)) as u8
        },
    }
}
fn extension_for_mime(mime: &str, fallback: &str) -> &'static str {
    match mime {
        "audio/mpeg" => "mp3",
        "audio/ogg" => "ogg",
        "audio/flac" => "flac",
        "audio/x-wav" | "audio/wav" => "wav",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        "image/png" => "png",
        _ if fallback == "wav" => "wav",
        _ => "png",
    }
}
fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_progress_and_maps_media_extensions() {
        assert_eq!(progress_event("audio", 150, 100).percent, 100);
        assert_eq!(extension_for_mime("audio/mpeg", "wav"), "mp3");
        assert_eq!(extension_for_mime("image/unknown", "png"), "png");
    }
}
