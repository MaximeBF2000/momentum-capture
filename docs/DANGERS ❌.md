# Dangers

This file lists known security, privacy, and operational risks in the app.

The goal is not to block development. The goal is to make risky tradeoffs explicit so future contributors understand what exists, why it was accepted, and what must not be assumed when building on top of it.

## Google Drive OAuth Client Secret In Desktop App

Feature: Google Drive share link integration.

Status: development shortcut only. Do not ship this publicly.

### What It Is

The Google Drive integration currently reads `GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET` from the root `.env` file at Tauri build time. The build script exposes those values to Rust with `cargo:rustc-env`, and the app uses them during the OAuth token exchange and refresh flow.

This is the easy local implementation because Google's OAuth token endpoint is rejecting our current client without `client_secret`.

### Why We Took This Risk

We need to validate the product flow quickly:

- User clicks Authorize Google Drive.
- User grants access in Google.
- Momentum lists Drive folders.
- Momentum uploads recordings.
- Momentum creates a public share link.
- Momentum copies the public link to the clipboard.

Using the local client secret lets us test that end-to-end behavior without building an external OAuth backend first.

### Why It Is Dangerous

A desktop app cannot keep a client secret secret.

If this app is distributed with `GOOGLE_CLIENT_SECRET` embedded, a user or attacker can extract it from the shipped app bundle, binary, build artifacts, crash reports, or process memory. Obfuscation may slow extraction down, but it does not make the secret private.

If the secret leaks, someone may be able to impersonate Momentum's OAuth client. Depending on Google project configuration and granted scopes, this can create abuse, quota, reputation, verification, or account-security problems for the Google Cloud project owner.

### What This Means

Do not ship public builds that include `GOOGLE_CLIENT_SECRET`.

Do not commit `.env`.

Do not treat `GOOGLE_CLIENT_SECRET` as protected once it has been compiled into a local app.

Do not expand Google Drive scopes while this shortcut exists unless the risk is reviewed again.

### Production Direction

Before public distribution, move the Google client secret to a backend service controlled by Momentum.

The intended production model is:

1. Momentum opens a backend authorization URL.
2. The backend redirects the user to Google.
3. Google redirects back to the backend.
4. The backend exchanges the auth code using the Google client secret.
5. The backend keeps refresh tokens and the client secret off user machines.
6. Momentum receives short-lived access tokens or a Momentum backend session.
7. Momentum uploads directly to Drive with short-lived tokens, or the backend performs Drive operations.

This keeps the real secret server-side and gives us a place to revoke sessions, rotate credentials, audit usage, and handle Google verification requirements.
