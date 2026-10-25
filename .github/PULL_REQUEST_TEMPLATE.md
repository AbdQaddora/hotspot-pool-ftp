<!-- Pull requests target the `dev` branch. See CONTRIBUTING.md. -->

## What this changes

<!-- One paragraph. Fixes #<issue> for bug fixes. -->

Fixes #

## How it was tested

<!-- Which of these ran, and on what: tsc, vite build, cargo test, cargo check (Windows target), a real Windows machine + phone (Windows version, hotspot engine). -->

## Checklist

- [ ] Targets `dev`, not `main`
- [ ] Linked issue (bug fixes must start from a confirmed issue)
- [ ] `pnpm exec tsc --noEmit`, `pnpm exec vite build` and `cargo test` pass
- [ ] `cargo check --target x86_64-pc-windows-gnu` passes
- [ ] Windows-only changes tried on a real Windows machine
- [ ] No lost `^M` in PowerShell string literals (`git diff | cat -A`)
- [ ] README updated if user-visible behaviour changed
