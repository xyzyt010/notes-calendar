# yellod suite — Notes & Calendar

Fast, private, local-first **Notes** and **Calendar** for your devices — native desktop & phone apps plus self-hosted web apps, sharing one storage core. Written in **Rust**, UI in **Slint**.

## What's inside

| Component | Path | Description |
|---|---|---|
| `ycore` | `crates/ycore` | Shared core: SQLite storage schema, FTS5 full-text search, models, DPI helpers |
| Notes (native) | `apps/notes` | Video-game-fast local notes: grid/list, folders, tags, FTS search, archive/trash/favorites/pins, dark & light themes |
| Calendar (native) | `apps/calendar` | Month/week/day/year views, reminders, recurrence, multiple calendars, dark & light themes |
| Web suite | `web/suite` | `suite-web` — Rust/axum backend + SQLite API and dark-themed web frontends for both apps (`:8760`) |

## Native apps

Built with [Slint](https://slint.dev) and native text rendering with proper DPI awareness (PerMonitorV2 manifest embedded on Windows). Cross-platform: Windows, Linux, macOS (desktop) and Android (arm64, min SDK 24).

### Features (both apps)

- **Notes**: FTS5 search, folders, tags, pin/favorite/archive/trash, auto-saving editor, word count, markdown-friendly, local SQLite.
- **Calendar**: day/week/month/year views, event creation/editing (reminders, recurrence, per-calendar colors), mini-month navigation, "upcoming events" panel.
- Both apps: dark & light themes, Inter typeface embedded, Lucide-derived icons, per-app window icon.

## Building

Prerequisites: Rust stable (`rustup`), and [Slint](https://github.com/slint-ui/slint) system dependencies per platform (Windows: none extra; Linux: `libxcb` etc., see `docs/PLAN.md`).

```powershell
# Windows
cargo build --release -p notes -p calendar

# Android (arm64) — needs Android SDK/NDK + Java 17+
$env:ANDROID_NDK = "$env:LOCALAPPDATA\Android\Sdk\ndk\27.3.13750724"
cargo build --release --target aarch64-linux-android -p notes -p calendar
cargo apk2 build --release --target aarch64-linux-android -p notes -p calendar
```

Signed release APKs can be produced with your own keystore; unsigned/debug APKs are built in CI.

## Web suite (self-hosted)

`suite-web` serves both web apps and a JSON API from a single binary:

- Notes UI at `/notes.html` (or `/notes`), Calendar UI at `/calendar.html` (or `/calendar`)
- Session-cookie auth (httpOnly, SameSite=Lax), bcrypt password hash, single-user
- Local SQLite (WAL) via `ycore` — nothing leaves your server

```bash
SUITE_STATIC_DIR=./static SUITE_DB=./suite.db SUITE_BIND=127.0.0.1:8760 ./suite-web
```

Deployment on a VM with Cloudflare Tunnel: see `deploy/` (`suite-web.service`, `deploy.sh`). The web apps are designed to be reachable at e.g. `notes.example.com` / `calendar.example.com` behind `cloudflared` + reverse proxy.

Default login is `admin` / `admin` on first run — change it by setting `SUITE_ADMIN_PASSWORD_HASH` to a bcrypt hash of your password, e.g. `htpasswd -bnBC 12 admin 'your-password' | cut -d: -f2`.

## CI / CD

GitHub Actions (`.github/workflows/`):

- **CI**: check, clippy, tests, workspace build on Linux/Windows/macOS
- **Release**: on `v*` tags — matrix build for Linux x86_64 + aarch64 (gnu & musl), Windows x86_64 + arm64, macOS universal2, Android arm64 APK; SHA256SUMS + build provenance attestation

The existing `notes.yellod.dpdns.org` site is a separate deployment; the web suite does not touch it.

## License

Source available for the author's personal use; see repository for details.
