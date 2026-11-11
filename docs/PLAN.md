# notes+calendar — Project Plan

Two professional, cross-device, Rust-native apps in one workspace:

- **Notes** — `apps/notes` (native, Slint)
- **Calendar** — `apps/calendar` (native, Slint)

Plus Rust-based web front/backends under `web/` (axum), hosted on the VM
(`notes.yellod.dpdns.org`, `calendar.yellod.dpdns.org`) behind Caddy + Cloudflare Tunnel.

## Principles

1. Efficiency first, looks second — but looks must be genuinely professional.
2. One shared core (`crates/ycore`): data model, SQLite storage, sync-ready file format.
3. Native text rendering via Slint (tiny-skia/skia) — crisp on all DPI.
4. Fully on-device for now; self-hostable server + E2E sync later (design for it).
5. GitHub Actions builds for: Windows x86_64/arm64, macOS x86_64/arm64 (universal2),
   Linux x86_64/arm64 (Ubuntu/Debian/Fedora/Arch/openSUSE via distro packages + AppImage/deb/rpm),
   Android arm64 (APK/AAB).

## Phases

1. **Notes native app** — UI faithful to the notes.yellod.dpdns.org inspiration,
   fully functional (markdown notes, search, tags, folders, favorites, offline-first).
2. **CI/CD** — build matrix, artifacts, release drafts.
3. **Calendar native app** — hour/day/week/month/year scopes, reminders, events, recurrence.
4. **Web** — Rust (axum) static+API; calendar.yellod.dpdns.org with auth like notes.
5. **Verify** — vision-verified on Windows; polish until professional.
