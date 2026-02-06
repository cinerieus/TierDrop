<div align="left">

```
████████╗██╗███████╗██████╗ ██████╗ ██████╗  ██████╗ ██████╗
╚══██╔══╝██║██╔════╝██╔══██╗██╔══██╗██╔══██╗██╔═══██╗██╔══██╗
   ██║   ██║█████╗  ██████╔╝██║  ██║██████╔╝██║   ██║██████╔╝
   ██║   ██║██╔══╝  ██╔══██╗██║  ██║██╔══██╗██║   ██║██╔═══╝
   ██║   ██║███████╗██║  ██║██████╔╝██║  ██║╚██████╔╝██║
   ╚═╝   ╚═╝╚══════╝╚═╝  ╚═╝╚═════╝ ╚═╝  ╚═╝ ╚═════╝ ╚═╝
```

<h3>Self-hosted ZeroTier Network Controller UI</h3>
<p><i>A lightweight web dashboard for managing your ZeroTier networks</i></p>

[![Rust](https://img.shields.io/badge/rust-1.70%2B-b7410e?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-8b5cf6?style=flat-square)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-linux%20|%20windows%20|%20macos-1a1a2e?style=flat-square)]()

<p>
  <a href="#features">Features</a> •
  <a href="#installation">Installation</a> •
  <a href="#usage">Usage</a> •
  <a href="#configuration">Configuration</a> •
  <a href="#license">License</a>
</p>

</div>

---

## Overview

TierDrop is a self-hosted web UI for managing ZeroTier networks through your local node's controller API. Run your own network controller without relying on ZeroTier Central.

**[View Screenshots](screenshots/)**

```
┌─────────────┐      ┌─────────────────┐      ┌──────────────────┐
│   Browser   │ ───► │    TierDrop     │ ───► │  ZeroTier Node   │
│             │      │   Web UI :8000  │      │  Controller API  │
└─────────────┘      └─────────────────┘      └──────────────────┘
```

## Features

| Feature | Description |
|---------|-------------|
| **Network Management** | Create, configure, and delete ZeroTier networks |
| **Member Control** | Authorize members, assign IPs, set names/descriptions, remove devices |
| **IPv4 & IPv6 Support** | Auto-assign pools for both protocols, plus RFC4193 and 6PLANE modes |
| **IP Pool Management** | Configure auto-assign IP ranges for your networks |
| **Route Configuration** | Define network routes for traffic forwarding |
| **DNS Configuration** | Set search domain and DNS servers for your network |
| **Multicast Settings** | Enable ethernet broadcast and set recipient limits |
| **Flow Rules Editor** | Dual-pane DSL editor with live JSON preview and syntax validation |
| **Backup & Restore** | Export/import complete controller state including identity and networks |
| **Real-time Updates** | Live dashboard via Server-Sent Events (SSE) |
| **Multi-User Support** | Create multiple users with granular per-network permissions |
| **Two-Factor Authentication** | TOTP-based 2FA compatible with any authenticator app |
| **Dark & Light Themes** | Toggle between dark and light mode, with system preference detection |
| **Single Binary** | No external dependencies, all assets embedded |

## Installation

### Prerequisites

- **ZeroTier One** — Installed and running

### Download

Download the latest release for your platform from [GitHub Releases](https://github.com/cinerieus/TierDrop/releases).

### Build from Source

Requires **Rust 1.70+** — Install from [rustup.rs](https://rustup.rs)

```bash
git clone https://github.com/cinerieus/TierDrop.git
cd tierdrop

# Linux
make linux

# Windows (cross-compile)
make windows

# Both + checksums
make dist
```

Binaries output to `dist/`.

## Usage

### Quick Start

```bash
./tierdrop
```

Open `http://localhost:8000` in your browser. On first launch, you'll be guided through setup where you'll configure your ZeroTier auth token and admin password.

**ZeroTier auth token location:**

| Platform | Path |
|----------|------|
| Linux | `/var/lib/zerotier-one/authtoken.secret` |
| Windows | `C:\ProgramData\ZeroTier\One\authtoken.secret` |
| macOS | `/Library/Application Support/ZeroTier/One/authtoken.secret` |

### Environment Variables (Optional)

| Variable | Default | Description |
|----------|---------|-------------|
| `ZT_BASE_URL` | `http://localhost:9993` | ZeroTier API address (override if non-standard) |
| `TIERDROP_BIND` | `127.0.0.1:8000` | Address and port to bind the web server |

Create a `.env` file in the working directory to set these:

```env
ZT_BASE_URL=http://localhost:9993
TIERDROP_BIND=127.0.0.1:8000
```

## Configuration

### Running as a Service (systemd)

Create `/etc/systemd/system/tierdrop.service`:

```ini
[Unit]
Description=TierDrop ZeroTier Controller UI
After=network.target zerotier-one.service

[Service]
Type=simple
User=root
ExecStart=/opt/tierdrop/tierdrop
Restart=always

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now tierdrop
```

### Docker

The Docker image includes ZeroTier One, so everything runs in a single container.

**Using Docker Compose (recommended):**

```bash
cd docker
docker compose up -d
```

**Using Docker directly:**

```bash
# Build (from project root)
docker build -t tierdrop -f docker/Dockerfile .

# Run
docker run -d \
  --name tierdrop \
  --cap-add NET_ADMIN \
  --device /dev/net/tun \
  -p 8000:8000 \
  -p 9993:9993/udp \
  -v zerotier-data:/var/lib/zerotier-one \
  -v tierdrop-data:/root/.local/share/tierdrop \
  tierdrop
```

**Get the ZeroTier auth token from logs:**

```bash
docker logs tierdrop
```

The token is printed on startup — use it in the TierDrop setup wizard at `http://localhost:8000`.

**Notes:**
- `--cap-add NET_ADMIN` and `--device /dev/net/tun` are required for ZeroTier networking
- Port `8000` is the TierDrop web UI
- Port `9993/udp` is for ZeroTier peer connections
- Volumes persist your ZeroTier identity and TierDrop config

## Technical Details

### Stack

- **Backend**: Rust + Axum 0.8
- **Templates**: Askama (compiled templates)
- **Frontend**: HTMX + SSE for real-time updates
- **Auth**: Argon2 password hashing
- **Assets**: Embedded via rust-embed

### Data Storage

TierDrop stores configuration in the platform-appropriate location:

| Platform | Path |
|----------|------|
| Linux | `~/.local/share/tierdrop/config.json` |
| Windows | `%APPDATA%\tierdrop\config.json` |
| macOS | `~/Library/Application Support/tierdrop/config.json` |

Config includes:
- User accounts and permissions
- Member and network display names/descriptions
- Flow rules DSL source code
- 2FA secrets (encrypted)
- Theme preferences

Network and member data is stored by ZeroTier itself.

### Multi-User & Permissions

TierDrop supports multiple user accounts with granular access control:

| Permission | Description |
|------------|-------------|
| **Admin** | Full access to all networks and settings, can manage users |
| **Read** | View network details, members, and settings |
| **Authorize** | Authorize/deauthorize members |
| **Modify** | Edit network settings, member IPs, routes, etc. |
| **Delete** | Remove members from networks |

Permissions are assigned per-network, allowing fine-grained access control. The first user created during setup is always an admin.

### Two-Factor Authentication

Users can enable TOTP-based 2FA for additional security:

1. Go to **Settings > Account**
2. Click **Enable 2FA**
3. Scan the QR code with your authenticator app (Google Authenticator, Authy, 1Password, etc.)
4. Enter the 6-digit code to verify and activate

Once enabled, you'll need to enter a code from your authenticator app after entering your password.

### Themes

TierDrop supports both dark and light themes:

- **Dark**: Default theme, easy on the eyes
- **Light**: High contrast for bright environments
- **System**: Follows your OS/browser preference

Toggle via the theme button in the top navigation bar. Preference is saved per-browser.

### Backup & Restore

TierDrop can backup and restore your complete controller state via Settings > Backup / Restore.

**Backup includes:**
- ZeroTier identity files (`identity.secret`, `identity.public`)
- Controller database (`controller.d/`)
- Auth token (`authtoken.secret`)
- TierDrop config (member names, flow rules source)

**Backup types:**
- **Full**: Includes identity files (requires root/admin access to ZeroTier directory)
- **Partial**: Controller data only (when identity files aren't readable)

Backups are exported as `.tar.gz` archives. Restoring a backup will replace the current controller state and may require restarting ZeroTier and TierDrop.

## License

MIT License — See [LICENSE](LICENSE) for details.

---
