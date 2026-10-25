# Contributing to Hotspot Pool

Thanks for helping. This page explains the branch model, how to report and fix a bug, how to propose a feature, and what a pull request needs before it is merged.

## Branches

| Branch | Purpose | Who pushes |
|---|---|---|
| `main` | Released code only. Every commit on `main` corresponds to a tagged release (`vX.Y.Z`) and the installers on the Releases page. | Maintainer, by merging `dev`. |
| `dev` | Development line. Everything lands here first; this is what the next release is cut from. | Maintainer, by merging pull requests. |
| `feat/<name>`, `fix/<issue>-<name>` | One branch per feature or bug fix, created from `dev`. | Everyone, in a fork. |
| `ttl` | Long-running feature branch for **internet sharing**: the Share Wi-Fi tab, the Wi-Fi Direct access point engine, Internet Connection Sharing, and TTL normalization ("Get past sharing blocks"). Not stable yet, so it is kept out of `dev` and `main` until it is. `main` and `dev` are about sharing files only. | Maintainer and anyone working on internet sharing. |

**Every pull request targets `dev`.** Pull requests against `main` are closed and asked to retarget.

## Reporting and fixing a bug

1. **Open an issue first**, using the *Bug report* template. Say which version and Windows build you run, what you expected and what happened, and paste the relevant lines from the Activity tab. If you are on the `ttl` branch build (internet sharing), say so and paste the report from **Copy diagnostics** in the Share Wi-Fi tab. Search existing issues before opening a new one.
2. Wait for the issue to be confirmed (a maintainer labels it `bug` and, if you want to work on it, assigns it to you). This avoids two people fixing the same thing and catches reports that are configuration problems rather than bugs.
3. Fork the repository and create a branch from `dev` named `fix/<issue-number>-<short-name>`, for example `fix/42-hosts-file-empty`.
4. Fix it. Add or update a test when the code has tests around it (the Rust side has unit tests and an end-to-end test against the real HTTP server).
5. Open a pull request **against `dev`**. Put `Fixes #<issue-number>` in the description so the issue closes when the fix is merged.

## Proposing a feature

1. Open an issue using the *Feature request* template and describe the problem the feature solves. Small features can go straight to a pull request, but for anything that changes the UI or adds a system dependency (a driver, a service, an elevated step) please discuss it in the issue first.
2. Fork, branch from `dev` as `feat/<short-name>`.
3. Open a pull request **against `dev`** when the feature is complete. Link the issue.

## Before you open the pull request

Run the checks that the maintainer runs. All of them must pass on your branch:

```bash
pnpm install
pnpm exec tsc --noEmit                                                             # TypeScript
pnpm exec vite build                                                               # UI bundle
cargo test --manifest-path src-tauri/Cargo.toml                                    # Rust unit + end-to-end tests
cargo check --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-gnu    # Windows-only code (needs mingw-w64 binutils on Linux/macOS)
```

The Mobile Hotspot and hosts-file code only runs on Windows and is not covered by the automated tests. If your change touches it, test it on a real Windows machine with a phone and say so in the pull request (Windows version, what you tried).

Things that bite:

- **PowerShell inside Rust strings.** `names.rs` (and on the `ttl` branch `ics.rs`, `wfd.rs`, `elevate.rs`) generate PowerShell scripts from Rust string literals, and some of those literals contain real CRLF bytes on purpose. Editors that normalize line endings silently change the generated scripts. After editing one of these files, run `git diff | cat -A` and make sure no `^M` disappeared. Never write `"$var: "` in those scripts: PowerShell parses it as a drive-qualified variable and the whole script fails to parse.
- **Share-name endings** must be real top-level domains (`link`, `site`, `net`, …). Phone browsers search the web for anything else instead of opening it.
- **Keep HTTP only.** HTTPS with a self-signed certificate makes every phone show a warning, so it was removed on purpose.

## Pull request checklist

- Targets `dev`.
- Linked to an issue (`Fixes #N` for bugs).
- One change per pull request. A refactor and a bug fix go in separate pull requests.
- Commit messages follow the existing style: `feat(scope): …`, `fix(scope): …`, `docs: …`, `ci: …`, `chore: …`.
- The checks above pass, and Windows-only changes were tried on Windows.
- README updated if the behaviour users see changed.

## Releases (maintainer)

1. Merge `dev` into `main`.
2. Bump the version in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`, commit `chore: vX.Y.Z — …`.
3. Push `main`; the *Build desktop bundles* workflow builds the installers. Push the tag `vX.Y.Z` and create the GitHub release with those artifacts.
4. Merge `main` back into `dev` so the version bump is on both.

## Code of conduct

Be kind, be specific, assume good intent. Reports and reviews are about the code, not the person.
