# Setting up Google Drive, OneDrive, Dropbox and iCloud sync

Marrow signs in to each service as an app that you, the publisher, register
once with that service. Registering is free. Each one gives you an ID,
which goes into [`crates/marrow-cloud/keys.toml`](../crates/marrow-cloud/keys.toml).
The IDs aren't secret (every copy of the app carries them), so they can be
committed. A service whose ID is empty simply doesn't appear in the app.

Your users never set anything up: they tap "Sign in to Google Drive", sign
in, and their notes sync, using the free storage their account already has.

Instead of editing `keys.toml`, the IDs can also be given when building, as
environment variables: `MARROW_GOOGLE_DESKTOP_ID`, `MARROW_GOOGLE_DESKTOP_SECRET`,
`MARROW_GOOGLE_IOS_ID`, `MARROW_ONEDRIVE_ID`, `MARROW_DROPBOX_KEY`.

The privacy policy the services ask for is [PRIVACY.md](../PRIVACY.md):
link to `https://github.com/mahbub-khan25/marrow/blob/main/PRIVACY.md`.

---

## Google Drive

Marrow asks only for `drive.file`: access to the files it creates itself.
Google counts that as a non-sensitive permission, so the app doesn't need
Google's security review.

1. Go to <https://console.cloud.google.com/> and make a new project called
   **Marrow**.
2. **APIs & Services → Library**: search for **Google Drive API** and click
   **Enable**.
3. **Google Auth Platform** (or **OAuth consent screen**) → **Get started**:
   * App name **Marrow**, your email as the support email.
   * Audience: **External**.
   * Contact email: yours. Agree, **Create**.
4. **Branding**: add the home page `https://github.com/mahbub-khan25/marrow`,
   the privacy policy link above, and optionally the logo
   (`apps/ios/Marrow/Assets.xcassets/AppIcon.appiconset`).
5. **Data access → Add or remove scopes**: tick
   `.../auth/drive.file` ("See, edit, create and delete only the specific
   Google Drive files you use with this app"). **Update**, **Save**.
6. **Audience → Publish app** (status **In production**). This matters:
   while an app is in "Testing", only listed test users can sign in and
   their sign-in stops working after 7 days.
7. **Clients → Create client**, three times:
   * **Desktop app**, name "Marrow desktop". Copy the **Client ID** into
     `google_desktop_id` and the **Client secret** into
     `google_desktop_secret`. (Google says a desktop app's secret isn't
     secret; it's fine in the open.)
   * **iOS**, bundle ID `app.marrow.notes`. Copy its **Client ID** into
     `google_ios_id`.
   * **Android**, package name `app.marrow.notes`, and the SHA-1 of the
     key the app is signed with. Nothing goes in `keys.toml`: Android
     recognises the app by these. Get the SHA-1 with
     `keytool -list -v -keystore your.keystore -alias your-alias`. Make one
     Android client for each signing key: your debug key
     (`~/.android/debug.keystore`, password `android`), your release key,
     and, once the app is on Google Play, the **App signing key** shown in
     Play Console → Setup → App signing.

Notes go in **My Drive › Marrow › (vault name)**.

## OneDrive

1. Go to <https://entra.microsoft.com/> → **App registrations** (under
   Identity → Applications) → **New registration**.
   * Name **Marrow** (OneDrive names the folder after it: `Apps/Marrow`).
   * Supported account types: **Accounts in any organizational directory
     and personal Microsoft accounts**.
   * Redirect URI: **Public client/native (mobile & desktop)**,
     `http://localhost`. **Register**.
2. **Authentication → Add a platform → Mobile and desktop applications**,
   custom redirect URI `app.marrow.notes://oauth`. Keep `http://localhost`.
   **Configure**.
3. **API permissions → Add a permission → Microsoft Graph → Delegated**:
   `Files.ReadWrite.AppFolder`, `offline_access`, `openid`, `profile`.
   **Add permissions**. (No admin consent is needed.)
4. **Overview**: copy the **Application (client) ID** into `onedrive_id`.

Optional: **Branding & properties → Publisher verification** removes the
"unverified" label work and school accounts see. It needs a Microsoft
Partner Network ID; personal accounts don't show the label.

Notes go in **Apps › Marrow › (vault name)**.

## Dropbox

1. Go to <https://www.dropbox.com/developers/apps> → **Create app**.
   * **Scoped access**, **App folder**.
   * Name: it has to be unique across Dropbox, e.g. **Marrow Notes**.
2. **Permissions**: tick `files.metadata.read`, `files.content.read` and
   `files.content.write` (`account_info.read` is already on). **Submit**.
3. **Settings**:
   * **App folder name**: `Marrow`.
   * **Redirect URIs**: add `http://localhost:53682/` and
     `db-YOURAPPKEY://2/token` (with your app key in it).
   * **Allow public clients (Implicit Grant & PKCE)**: **Allow**.
   * Copy the **App key** into `dropbox_key`.
4. A new Dropbox app is in development mode, for up to 500 people. When
   it's ready, **Apply for production** on the Settings page (free; Dropbox
   checks the app works and follows their branding rules).

Notes go in **Apps › Marrow › (vault name)**.

## iCloud (iPhone, iPad and Mac)

iCloud needs the Apple Developer Program ($99 a year), which publishing on
the App Store needs anyway. A free Apple account can't use iCloud: to build
with one, delete the `entitlements:` lines in `apps/ios/project.yml`.

1. <https://developer.apple.com/account/> → **Certificates, Identifiers &
   Profiles → Identifiers → +** → **iCloud Containers**, identifier
   `iCloud.app.marrow.notes`.
2. **Identifiers → App IDs**: open (or make) `app.marrow.notes`, tick
   **iCloud** (with CloudKit support off is fine), **Edit**, choose the
   container from step 1, **Save**.
3. In Xcode, choose your team under **Signing & Capabilities**. The app
   already asks for iCloud Documents with that container.

On the phone, Settings › Sync › **iCloud** keeps the vault in iCloud Drive ›
Marrow. On a Mac, that folder is in Finder's iCloud Drive as **Marrow**:
Marrow for Mac lists it as "iCloud (Marrow on iPhone and iPad)" when you set
up sync, and iCloud for Windows shows it too.

## After adding the IDs

Commit `keys.toml` and build as usual. To check a sign-in without
publishing: build the desktop app (`npm run tauri dev`), open a vault,
Settings → Vault → Sync → Set up, and pick the service.
