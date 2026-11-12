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
   fully functional (markdown notes, search, tags, folders, favorites, offline-first). ✅ Done, verified on Windows.
2. **CI/CD** — build matrix, artifacts, release drafts. ✅ Workflows in place (`ci.yml`, `release.yml`).
3. **Calendar native app** — hour/day/week/month/year scopes, reminders, events, recurrence. ✅ Done, verified on Windows.
4. **Web** — Rust (axum) static+API; calendar.yellod.dpdns.org with auth like notes.
   ✅ Backend + dark frontends built & locally verified; deployment to VM pending tunnel config.
5. **Verify** — vision-verified on Windows; polish until professional. ✅ Both native apps verified; web suite verified locally; tunnel verification pending.

## Deployment notes

- `suite-web` (Rust/axum) serves both web apps on `127.0.0.1:8760` on the VM.
- The existing `notes.yellod.dpdns.org` deployment (wrangler on :8787) is untouched.
- Tunnel routing for `calendar.yellod.dpdns.org` → `127.0.0.1:8760` requires the
  root-owned `/etc/cloudflared/config.yml` (or Zero Trust dashboard) — user action needed.
- `deploy/suite-web.service` (systemd system unit) and `deploy/suite-web.user.service`
  provided; DB at `~hs01/suite-web/suite.db` (created on first run; WAL mode).
