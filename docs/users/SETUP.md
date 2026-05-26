# Momentum Setup Guide (macOS + Google Drive)

This guide is for a new developer who just cloned the repo and wants Momentum + Google Drive integration working on Mac.

If you follow every step below, you should be able to run the app and authorize Google Drive from Settings.

## 1) Prerequisites on Mac

Open Terminal and run:

```bash
xcode-select --install
```

Install Homebrew if you do not have it:

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

Install required tools:

```bash
brew install node ffmpeg rustup-init
```

Install Rust toolchain:

```bash
rustup-init -y
source "$HOME/.cargo/env"
```

Check everything is available:

```bash
node -v
npm -v
rustc -V
cargo -V
ffmpeg -version
```

## 2) Clone and install the project

```bash
git clone git@github.com:MaximeBF2000/momentum-capture.git
cd momentum-capture
npm install
```

## 3) Create your Google Cloud project for Drive

Open Google Cloud Console:

- [https://console.cloud.google.com/](https://console.cloud.google.com/)

### 3.1 Create/select a project

Create a new project (for example `Momentum Capture Dev`) or select an existing one.

### 3.2 Enable Google Drive API

Open API Library:

- [https://console.cloud.google.com/apis/library](https://console.cloud.google.com/apis/library)

Search `Google Drive API` and click **Enable**.

### 3.3 Configure OAuth consent screen

Open Google Auth Platform:

- [https://console.cloud.google.com/auth/overview](https://console.cloud.google.com/auth/overview)

Configure:

1. App name: `Momentum Capture`
2. Support email: your email
3. Developer contact email: your email

For now, keep audience in **Testing** if you are just developing.

### 3.4 Add test users (important in Testing mode)

In **Audience** or **OAuth consent screen**, add your Google account as a test user.

If you skip this, Google login will fail with `403 access_denied`.

### 3.5 Create OAuth credentials

Go to **Credentials**:

- [https://console.cloud.google.com/apis/credentials](https://console.cloud.google.com/apis/credentials)

Create OAuth Client ID:

1. Click **Create Credentials** -> **OAuth client ID**
2. Application type: **Desktop app**
3. Name: `Momentum Desktop Dev`
4. Create

Copy:

- `Client ID`
- `Client secret`

## 4) Create `.env` in repo root

At project root (`momentum-capture`), create a `.env` file:

```bash
cat > .env << 'EOF'
GOOGLE_CLIENT_ID=YOUR_CLIENT_ID.apps.googleusercontent.com
GOOGLE_CLIENT_SECRET=YOUR_CLIENT_SECRET
EOF
```

Replace both values with the ones from Google Cloud.

## 5) Run the app

From project root:

```bash
npm run build-app
```

When prompted, move [Moment.app](http://Moment.app) into Applications folder.

Then open [Momentum.app](http://Momentum.app) from your Applications folder, and give the permissions on the first run.

## 6) Authorize Drive inside Momentum

In the app:

1. Open **Settings**
2. Enable **Drive integration**
3. Click **Authorize**
4. Sign in with Google and approve access
5. Return to Momentum
6. Click folder refresh if needed, then pick your Drive folder
7. Save settings

You should now be able to stop a recording and get a shareable Drive link.

## 7) Common issues

### `403 access_denied` on Google login

Cause:

- OAuth app is in Testing and your account is not a test user.

Fix:

- Add your email to test users in Google Auth Platform.

### `client_secret is missing`

Cause:

- `GOOGLE_CLIENT_SECRET` is missing from `.env`, or app was started before `.env` was set.

Fix:

1. Ensure `.env` contains both vars.
2. Restart `npm run tauri dev`.

### Folder picker shows no folders

Fix:

1. Confirm authorization succeeded.
2. Click folder refresh button.
3. Check that your account actually has accessible folders (My Drive or shared folders).

