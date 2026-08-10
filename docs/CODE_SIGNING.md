# Code signing policy

This document describes how release artifacts of **Nimbus Client** are built,
signed and verified.

## Current status

| Artifact | Signature | Status |
| --- | --- | --- |
| `Nimbus.Client_<version>_x64-setup.exe` (Windows installer) | Authenticode | **Not signed yet** — application to SignPath Foundation planned |
| `Nimbus.Client_<version>_x64-setup.exe` | minisign / Tauri updater | Signed — verified by the built-in updater |
| `latest.json` (updater manifest) | — | Published by CI together with the installer |

Until an Authenticode certificate is in place, Windows SmartScreen may warn
users that the publisher is unknown. See the *Verifying a download* section
below for how to check that a file really came from this repository.

## Planned: SignPath Foundation

We intend to apply for free code signing through the SignPath Foundation
program. Once approved, the following statement will be published in the
README and on the download page:

> Free code signing provided by [SignPath.io](https://signpath.io), certificate by [SignPath Foundation](https://signpath.org)

### What will be signed

* The Windows NSIS installer (`.exe`) published on GitHub Releases.

Nothing else is distributed as a binary. There are no nightly or unofficial
builds.

## Build and signing process

* All release artifacts are built exclusively by GitHub Actions from this
  repository. Local developer builds are never published.
* The release workflow is `.github/workflows/release.yml`. It is triggered
  only by pushing a `v*` tag to the `main` branch.
* The workflow builds the frontend and the Rust backend, produces the NSIS
  installer with `tauri-apps/tauri-action`, signs the updater artifact, and
  creates the GitHub Release. No manual upload of binaries takes place.
* Every commit on `main` and every pull request is additionally validated by
  `.github/workflows/ci.yml`: `cargo fmt --check`, `cargo clippy -D warnings`,
  `cargo test`, `cargo audit`, `npm audit` and a full bundle build.
* Once SignPath is enabled, only CI-produced artifacts will be submitted for
  signing, and the signing request will require maintainer approval.

## Private keys

* The updater signing key (minisign, used by the Tauri updater) is held
  encrypted; the private key and its password exist only as the GitHub
  repository secrets `TAURI_SIGNING_PRIVATE_KEY` and
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The corresponding public key is
  embedded in `src-tauri/tauri.conf.json` and shipped with every build.
* Should an Authenticode certificate be issued by SignPath Foundation, the
  private key remains in SignPath's HSM. This project never stores it.

## Verifying a download

1. Download the installer only from the GitHub Releases page of this
   repository: <https://github.com/coddyxdev/Nimbus-Launcher/releases>
2. Compare the file size and the build provenance shown in the GitHub Actions
   run that produced the release.
3. The accompanying `.exe.sig` file is a minisign signature of the installer.
   It can be verified against the public key in `src-tauri/tauri.conf.json`
   (`plugins.updater.pubkey`). The launcher's built-in updater performs this
   check automatically before applying any update, so an update cannot be
   replaced by a third party even without Authenticode.

## Reporting a problem

If you believe a distributed artifact has been tampered with, or you find a
security issue in the launcher, please contact **coddyxdev@gmail.com** instead
of opening a public issue.
