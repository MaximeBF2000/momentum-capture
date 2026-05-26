# Google Drive Integration

This document covers the implemented Drive architecture and user flow.

## Scope

Implemented capabilities:

- OAuth authorization/re-authorization
- persistent token storage in app settings
- folder discovery for selection
- video listing for selected folder
- upload on recording completion
- automatic public-link permissions
- readiness-gated completion event

## Architecture

```mermaid
flowchart TB
  settingsUI["SettingsWindow"] --> authCmd["authorize_google_drive"]
  settingsUI --> listFolders["list_google_drive_folders"]
  settingsUI --> listVideos["list_google_drive_videos"]

  authCmd --> driveSvc["services/google_drive.rs"]
  listFolders --> driveSvc
  listVideos --> driveSvc

  stopPath["finalize_recording_file"] --> upload["upload_video"]
  upload --> driveSvc
  driveSvc --> events["drive-upload-* events"]
  events --> overlay["OverlayWindow notifications + clipboard"]
```

## Settings Fields

Drive-related persisted fields:

- `googleDrive.enabled`
- `googleDrive.folderId`
- `googleDrive.folderName`
- `googleDrive.accountEmail`
- `googleDrive.accessToken`
- `googleDrive.refreshToken`
- `googleDrive.tokenExpiresAtMs`

Related global field:

- `saveRecordingsLocally`

## OAuth Flow (PKCE + localhost callback)

```mermaid
sequenceDiagram
  participant UI as SettingsWindow
  participant CMD as authorize_google_drive
  participant GD as google_drive::authorize
  participant G as Google OAuth

  UI->>CMD: invoke authorizeGoogleDrive()
  CMD->>GD: authorize(settings_store)
  GD->>GD: bind localhost callback port
  GD->>G: open browser auth URL (PKCE challenge)
  G-->>GD: redirect with code + state
  GD->>GD: validate state
  GD->>G: token exchange (optionally with client secret)
  GD->>G: fetch account email (/drive/about)
  GD->>GD: persist tokens + expiry + email
  GD-->>CMD: updated drive settings
  CMD-->>UI: return settings + emit settings-updated
```

## Token Lifetime Handling

- Access token is used if not near expiry.
- If expired/near expiry, refresh token flow is triggered.
- Refreshed access token and new expiry are persisted.

## Folder Listing Behavior

Folder queries include:

- standard folder query
- `sharedWithMe` folder query
- all-drives support flags

Results are:

- de-duplicated by folder ID
- sorted by lowercased name

## Video Listing Behavior

`list_google_drive_videos`:

- requires selected folder ID
- filters videos in folder only
- returns newest-first
- includes `webViewLink` and `thumbnailLink`

## Upload + Share Lifecycle

```mermaid
flowchart TD
  start["recording finalized"] --> pending["emit drive-upload-pending"]
  pending --> session["create resumable upload session"]
  session --> upload["PUT video bytes"]
  upload --> public["create anyone-with-link permission"]
  public --> poll["poll file metadata"]
  poll --> ready{"metadata + thumbnail ready?"}
  ready -- yes --> complete["emit drive-upload-complete"]
  ready -- timeout/error --> fail["emit drive-upload-error"]
```

Readiness gate avoids notifying success before Drive has processed the video enough for real sharing behavior.

## Frontend UX Wiring

Settings window:

- toggle Drive on/off
- toggle local-save on/off
- authorize/re-authorize button
- folder combobox with search + refresh
- public videos page with thumbnail + copy link + copied feedback

Overlay window:

- `drive-upload-pending` -> native notification: "Your video will soon be ready to be shared"
- `drive-upload-complete` -> copy link to clipboard + notification
- `drive-upload-error` -> error notification

## Current Security Posture

The current dev flow can include `GOOGLE_CLIENT_SECRET` in desktop builds via compile-time env injection. This is explicitly non-production-safe.

Read [Dangers](./DANGERS%20%E2%9D%8C.md) before changing scope, distribution, or auth model.
