# Releasing an update

Installed copies check
`https://github.com/channaofficial1996/ScreenLink-Studio/releases/latest/download/latest.json`
at startup, and from About > Check for updates. The repository must stay public, because the app
downloads release files without logging in.

Users see "A new version is available". Nothing installs until they click **Update now**, and that
button is disabled during a recording.

## Publish a new version (Windows, from the development PC)

1. Raise `version` in `src-tauri/tauri.conf.json`, `package.json` and `src-tauri/Cargo.toml`, then
   commit and push.
2. Run the release script, which builds, signs with the local key, and publishes:

   ```bash
   node scripts/release-windows.mjs
   ```

## The signing key

Updates are signed, and the app rejects anything not signed with the matching key.

- Private key: `%USERPROFILE%\.tauri\screenlink-studio.key` on the development PC. It is never
  committed.
- Public key: `plugins.updater.pubkey` in `tauri.conf.json`.

**Back the private key up somewhere safe.** If it is lost, installed copies can never receive
another update, and every user would have to reinstall by hand. If it leaks, someone else could
sign updates, so treat it like a password.

This key only signs updates. It is separate from Windows and Apple code signing
(see [BUILDING.md](BUILDING.md#signing)), which is what removes the SmartScreen warning.

## Building macOS on GitHub (optional)

macOS builds need a Mac, so they run in the Release workflow on GitHub's servers. That requires a
copy of the signing key in the repository's secrets, which only the repository owner should add:

```bash
gh secret set TAURI_SIGNING_PRIVATE_KEY -R channaofficial1996/ScreenLink-Studio < ~/.tauri/screenlink-studio.key
```

Then start the workflow from the repository's Actions tab. It has never been run yet.
