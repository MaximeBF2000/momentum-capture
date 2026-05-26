use crate::error::{AppError, AppResult};
use crate::models::{AppSettings, GoogleDriveSettings};
use crate::services::settings::SettingsStore;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio_util::codec::{BytesCodec, FramedRead};
use url::Url;

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const DRIVE_API: &str = "https://www.googleapis.com/drive/v3";
const DRIVE_UPLOAD_API: &str = "https://www.googleapis.com/upload/drive/v3";
const DRIVE_SCOPE: &str = "https://www.googleapis.com/auth/drive";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveFolder {
    pub id: String,
    pub name: String,
    pub modified_time: Option<String>,
    pub web_view_link: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveVideo {
    pub id: String,
    pub name: String,
    pub created_time: Option<String>,
    pub modified_time: Option<String>,
    pub web_view_link: Option<String>,
    pub thumbnail_link: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveUploadResult {
    pub id: String,
    pub name: String,
    pub web_view_link: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct DriveListResponse<T> {
    files: Vec<T>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DriveFileResponse {
    id: String,
    name: String,
    #[serde(default)]
    web_view_link: Option<String>,
    #[serde(default)]
    thumbnail_link: Option<String>,
    #[serde(default)]
    video_media_metadata: Option<DriveVideoMediaMetadata>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DriveVideoMediaMetadata {
    #[serde(default)]
    width: Option<i32>,
    #[serde(default)]
    height: Option<i32>,
    #[serde(default)]
    duration_millis: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DriveAboutResponse {
    user: DriveUserResponse,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DriveUserResponse {
    email_address: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OAuthCallback {
    code: String,
    state: String,
}

pub async fn authorize(settings_store: &SettingsStore) -> AppResult<GoogleDriveSettings> {
    println!("[GoogleDrive] Starting OAuth authorization");
    let mut settings = settings_store.load()?;
    let client_id = required_client_id(&settings.google_drive)?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let redirect_uri = format!("http://127.0.0.1:{}/oauth2callback", port);
    println!(
        "[GoogleDrive] OAuth callback listening on {}; client_id suffix={}",
        redirect_uri,
        client_id_suffix(&client_id)
    );
    let state = format!("momentum-{}", current_time_ms());
    let verifier = format!(
        "{}{}",
        URL_SAFE_NO_PAD.encode(uuid::Uuid::new_v4().as_bytes()),
        URL_SAFE_NO_PAD.encode(uuid::Uuid::new_v4().as_bytes())
    );
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));

    let mut auth_url = Url::parse(AUTH_URL)
        .map_err(|err| AppError::GoogleDrive(format!("Invalid auth URL: {}", err)))?;
    auth_url
        .query_pairs_mut()
        .append_pair("client_id", &client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", DRIVE_SCOPE)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent")
        .append_pair("state", &state)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256");

    println!("[GoogleDrive] Opening Google authorization URL");
    open_url(auth_url.as_str())?;

    let callback = tauri::async_runtime::spawn_blocking(move || wait_for_oauth_callback(listener))
        .await
        .map_err(|err| AppError::GoogleDrive(format!("Authorization task failed: {}", err)))??;
    println!("[GoogleDrive] OAuth callback received");

    if callback.state != state {
        println!("[GoogleDrive] OAuth state mismatch");
        return Err(AppError::GoogleDrive(
            "Authorization state did not match. Please try again.".into(),
        ));
    }

    println!("[GoogleDrive] Exchanging OAuth code for tokens");
    let client = reqwest::Client::new();
    let client_secret = oauth_client_secret();
    let mut token_form = vec![
        ("client_id", client_id.as_str()),
        ("code", callback.code.as_str()),
        ("code_verifier", verifier.as_str()),
        ("grant_type", "authorization_code"),
        ("redirect_uri", redirect_uri.as_str()),
    ];
    if let Some(secret) = client_secret.as_ref() {
        println!("[GoogleDrive] Using bundled OAuth client secret for token exchange");
        token_form.push(("client_secret", secret.as_str()));
    }
    let token = client
        .post(TOKEN_URL)
        .form(&token_form)
        .send()
        .await?;
    let token = parse_google_response::<TokenResponse>(token).await?;
    println!(
        "[GoogleDrive] Token exchange succeeded; refresh_token={}",
        token.refresh_token.is_some()
    );

    let account_email = get_account_email(&client, &token.access_token).await.ok();

    settings.google_drive.access_token = Some(token.access_token);
    settings.google_drive.refresh_token =
        token.refresh_token.or(settings.google_drive.refresh_token);
    settings.google_drive.token_expires_at_ms = token
        .expires_in
        .map(|seconds| current_time_ms() + seconds.saturating_mul(1000));
    if let Some(email) = account_email {
        settings.google_drive.account_email = Some(email);
    }
    settings_store.save(&settings)?;
    println!("[GoogleDrive] OAuth tokens saved");

    Ok(settings.google_drive)
}

pub async fn list_folders(settings_store: &SettingsStore) -> AppResult<Vec<DriveFolder>> {
    println!("[GoogleDrive] Loading Drive folders");
    let token = valid_access_token(settings_store).await?;
    let client = reqwest::Client::new();
    let queries = [
        "mimeType = 'application/vnd.google-apps.folder' and trashed = false",
        "sharedWithMe = true and mimeType = 'application/vnd.google-apps.folder' and trashed = false",
    ];

    let mut dedup: BTreeMap<String, DriveFolder> = BTreeMap::new();
    for query in queries {
        let response = client
            .get(format!("{}/files", DRIVE_API))
            .bearer_auth(&token)
            .query(&[
                ("q", query),
                ("fields", "files(id,name,modifiedTime,webViewLink)"),
                ("orderBy", "name_natural"),
                ("pageSize", "1000"),
                ("includeItemsFromAllDrives", "true"),
                ("supportsAllDrives", "true"),
            ])
            .send()
            .await?;
        let folders = parse_google_response::<DriveListResponse<DriveFolder>>(response)
            .await?
            .files;
        for folder in folders {
            dedup.insert(folder.id.clone(), folder);
        }
    }

    let mut folders: Vec<DriveFolder> = dedup.into_values().collect();
    folders.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    println!("[GoogleDrive] Loaded {} Drive folders", folders.len());
    Ok(folders)
}

pub async fn list_videos(settings_store: &SettingsStore) -> AppResult<Vec<DriveVideo>> {
    let settings = settings_store.load()?;
    let folder_id = settings
        .google_drive
        .folder_id
        .clone()
        .ok_or_else(|| AppError::GoogleDrive("Choose a Drive folder first.".into()))?;
    let token = valid_access_token(settings_store).await?;
    let client = reqwest::Client::new();
    let query = format!(
        "'{}' in parents and mimeType contains 'video/' and trashed = false",
        folder_id.replace('\'', "\\'")
    );
    let response = client
        .get(format!("{}/files", DRIVE_API))
        .bearer_auth(token)
        .query(&[
            ("q", query.as_str()),
            (
                "fields",
                "files(id,name,createdTime,modifiedTime,webViewLink,thumbnailLink)",
            ),
            ("orderBy", "createdTime desc"),
            ("pageSize", "1000"),
        ])
        .send()
        .await?;
    Ok(
        parse_google_response::<DriveListResponse<DriveVideo>>(response)
            .await?
            .files,
    )
}

async fn get_account_email(client: &reqwest::Client, token: &str) -> AppResult<String> {
    let response = client
        .get(format!("{}/about", DRIVE_API))
        .bearer_auth(token)
        .query(&[("fields", "user(emailAddress)")])
        .send()
        .await?;
    parse_google_response::<DriveAboutResponse>(response)
        .await?
        .user
        .email_address
        .ok_or_else(|| AppError::GoogleDrive("Google did not return account email.".into()))
}

pub async fn upload_video(
    settings_store: &SettingsStore,
    path: &Path,
) -> AppResult<DriveUploadResult> {
    let settings = settings_store.load()?;
    if !settings.google_drive.enabled {
        return Err(AppError::GoogleDrive(
            "Google Drive integration is disabled.".into(),
        ));
    }
    let folder_id = settings
        .google_drive
        .folder_id
        .clone()
        .ok_or_else(|| AppError::GoogleDrive("Choose a Drive folder first.".into()))?;
    let token = valid_access_token(settings_store).await?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::GoogleDrive("Recording file has no valid name.".into()))?
        .to_string();
    let file_size = std::fs::metadata(path)?.len();

    let client = reqwest::Client::new();
    let metadata = json!({
        "name": file_name,
        "parents": [folder_id],
        "mimeType": "video/mp4"
    });
    let session_response = client
        .post(format!("{}/files", DRIVE_UPLOAD_API))
        .bearer_auth(&token)
        .query(&[
            ("uploadType", "resumable"),
            ("fields", "id,name,webViewLink"),
        ])
        .header(CONTENT_TYPE, "application/json; charset=UTF-8")
        .json(&metadata)
        .send()
        .await?;

    if !session_response.status().is_success() {
        return Err(google_error_from_response(session_response).await);
    }

    let upload_url = session_response
        .headers()
        .get("location")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| AppError::GoogleDrive("Google did not return an upload URL.".into()))?
        .to_string();

    let file = tokio::fs::File::open(path).await?;
    let stream = FramedRead::new(file, BytesCodec::new());
    let upload_response = client
        .put(upload_url)
        .header(CONTENT_TYPE, "video/mp4")
        .header(CONTENT_LENGTH, file_size)
        .body(reqwest::Body::wrap_stream(stream))
        .send()
        .await?;
    let uploaded = parse_google_response::<DriveFileResponse>(upload_response).await?;
    make_public(&client, &token, &uploaded.id).await?;
    let final_file = wait_for_video_ready(&client, &token, &uploaded.id).await?;
    let web_view_link = final_file
        .web_view_link
        .or(uploaded.web_view_link)
        .ok_or_else(|| AppError::GoogleDrive("Google did not return a share link.".into()))?;

    Ok(DriveUploadResult {
        id: final_file.id,
        name: final_file.name,
        web_view_link,
    })
}

async fn wait_for_video_ready(
    client: &reqwest::Client,
    token: &str,
    file_id: &str,
) -> AppResult<DriveFileResponse> {
    let started = Instant::now();
    let timeout = Duration::from_secs(10 * 60);
    let mut attempts = 0_u32;

    loop {
        attempts += 1;
        let file = get_file(client, token, file_id).await?;
        if is_drive_video_ready(&file) {
            println!(
                "[GoogleDrive] Video ready after {:?} ({} checks)",
                started.elapsed(),
                attempts
            );
            return Ok(file);
        }

        if started.elapsed() >= timeout {
            return Err(AppError::GoogleDrive(
                "Video uploaded, but Google Drive is still processing it. The link was not copied yet.".into(),
            ));
        }

        println!(
            "[GoogleDrive] Waiting for Drive video processing (attempt {}, elapsed {:?})",
            attempts,
            started.elapsed()
        );
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

fn is_drive_video_ready(file: &DriveFileResponse) -> bool {
    let metadata_ready = file
        .video_media_metadata
        .as_ref()
        .map(|metadata| {
            metadata.duration_millis.is_some()
                || (metadata.width.unwrap_or_default() > 0
                    && metadata.height.unwrap_or_default() > 0)
        })
        .unwrap_or(false);
    metadata_ready && file.thumbnail_link.is_some()
}

async fn make_public(client: &reqwest::Client, token: &str, file_id: &str) -> AppResult<()> {
    let response = client
        .post(format!("{}/files/{}/permissions", DRIVE_API, file_id))
        .bearer_auth(token)
        .query(&[("sendNotificationEmail", "false")])
        .json(&json!({
            "role": "reader",
            "type": "anyone",
            "allowFileDiscovery": false
        }))
        .send()
        .await?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(google_error_from_response(response).await)
    }
}

async fn get_file(
    client: &reqwest::Client,
    token: &str,
    file_id: &str,
) -> AppResult<DriveFileResponse> {
    let response = client
        .get(format!("{}/files/{}", DRIVE_API, file_id))
        .bearer_auth(token)
        .query(&[(
            "fields",
            "id,name,webViewLink,thumbnailLink,videoMediaMetadata(width,height,durationMillis)",
        )])
        .send()
        .await?;
    parse_google_response(response).await
}

async fn valid_access_token(settings_store: &SettingsStore) -> AppResult<String> {
    let settings = settings_store.load()?;
    if let (Some(token), Some(expires_at)) = (
        settings.google_drive.access_token.clone(),
        settings.google_drive.token_expires_at_ms,
    ) {
        if expires_at > current_time_ms() + 60_000 {
            return Ok(token);
        }
    }
    refresh_access_token(settings_store, settings).await
}

async fn refresh_access_token(
    settings_store: &SettingsStore,
    mut settings: AppSettings,
) -> AppResult<String> {
    let client_id = required_client_id(&settings.google_drive)?;
    let refresh_token = settings
        .google_drive
        .refresh_token
        .clone()
        .ok_or_else(|| AppError::GoogleDrive("Authorize Google Drive first.".into()))?;
    let client = reqwest::Client::new();
    let client_secret = oauth_client_secret();
    let mut token_form = vec![
        ("client_id", client_id.as_str()),
        ("refresh_token", refresh_token.as_str()),
        ("grant_type", "refresh_token"),
    ];
    if let Some(secret) = client_secret.as_ref() {
        println!("[GoogleDrive] Using bundled OAuth client secret for token refresh");
        token_form.push(("client_secret", secret.as_str()));
    }
    let response = client
        .post(TOKEN_URL)
        .form(&token_form)
        .send()
        .await?;
    let token = parse_google_response::<TokenResponse>(response).await?;
    settings.google_drive.access_token = Some(token.access_token.clone());
    settings.google_drive.token_expires_at_ms = token
        .expires_in
        .map(|seconds| current_time_ms() + seconds.saturating_mul(1000));
    settings_store.save(&settings)?;
    Ok(token.access_token)
}

fn required_client_id(settings: &GoogleDriveSettings) -> AppResult<String> {
    if let Some(client_id) = settings
        .client_id
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
    {
        return Ok(client_id);
    }

    option_env!("GOOGLE_CLIENT_ID")
        .or(option_env!("GOOGLE_DRIVE_CLIENT_ID"))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            AppError::GoogleDrive(
                "Build Momentum with GOOGLE_CLIENT_ID set before authorizing Google Drive.".into(),
            )
        })
}

fn oauth_client_secret() -> Option<String> {
    option_env!("GOOGLE_CLIENT_SECRET")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

async fn parse_google_response<T: for<'de> Deserialize<'de>>(
    response: reqwest::Response,
) -> AppResult<T> {
    if response.status().is_success() {
        Ok(response.json::<T>().await?)
    } else {
        Err(google_error_from_response(response).await)
    }
}

async fn google_error_from_response(response: reqwest::Response) -> AppError {
    let status = response.status();
    let text = response
        .text()
        .await
        .unwrap_or_else(|_| "No response body".to_string());
    println!("[GoogleDrive] Google API error {}: {}", status, text);
    AppError::GoogleDrive(format!("Google returned {}: {}", status, text))
}

fn client_id_suffix(client_id: &str) -> &str {
    client_id
        .split('-')
        .nth(1)
        .unwrap_or(client_id)
        .split('.')
        .next()
        .unwrap_or("unknown")
}

fn wait_for_oauth_callback(listener: TcpListener) -> AppResult<OAuthCallback> {
    let (mut stream, _) = listener.accept()?;
    let mut buffer = [0_u8; 4096];
    let read = stream.read(&mut buffer)?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let first_line = request
        .lines()
        .next()
        .ok_or_else(|| AppError::GoogleDrive("OAuth callback was empty.".into()))?;
    let path = first_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| AppError::GoogleDrive("OAuth callback was malformed.".into()))?;
    let url = Url::parse(&format!("http://127.0.0.1{}", path))
        .map_err(|err| AppError::GoogleDrive(format!("OAuth callback was invalid: {}", err)))?;
    let code = url
        .query_pairs()
        .find(|(key, _)| key == "code")
        .map(|(_, value)| value.to_string())
        .ok_or_else(|| AppError::GoogleDrive("Google did not return an auth code.".into()))?;
    let state = url
        .query_pairs()
        .find(|(key, _)| key == "state")
        .map(|(_, value)| value.to_string())
        .ok_or_else(|| AppError::GoogleDrive("Google did not return auth state.".into()))?;

    let body = "Google Drive is authorized. You can return to Momentum.";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes())?;
    Ok(OAuthCallback { code, state })
}

fn open_url(url: &str) -> AppResult<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(url).spawn()?;
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()?;
        return Ok(());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open").arg(url).spawn()?;
        return Ok(());
    }
}

fn current_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_millis()
        .min(i64::MAX as u128) as i64
}
