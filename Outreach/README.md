# outreach

A fast, keyboard-driven terminal email client for Microsoft Outlook, powered by the Microsoft Graph API.

Built in Rust with `ratatui`.

---

## Setup

### 1. Register an Azure App

1. Go to [Azure App Registrations](https://portal.azure.com/#blade/Microsoft_AAD_RegisteredApps)
2. Click **New registration**
3. Name it anything (e.g. `outreach`)
4. Select **Accounts in any organizational directory and personal Microsoft accounts**
5. No redirect URI needed (device code flow)
6. After creation, go to **Authentication** → enable **Allow public client flows**
7. Under **API permissions**, add:
   - `Mail.Read`
   - `Mail.ReadWrite`
   - `Mail.Send`
   - `User.Read`
   - `offline_access`
8. Copy the **Application (client) ID** and **Directory (tenant) ID**

### 2. Configure outreach

Create `~/.config/outreach/config.toml`:

```toml
[auth]
client_id = "your-client-id-here"
tenant_id = "common"   # or your specific tenant ID
```

### 3. Build & Run

```bash
cargo build --release
./target/release/outreach
```

On first run, you'll be shown a device code URL — open it in any browser and sign in.

---

## Keybindings

| Key | Action |
|-----|--------|
| `j` / `↓` | Next email |
| `k` / `↑` | Previous email |
| `Enter` / `Tab` | Switch to body panel |
| `Esc` / `Tab` | Back to list |
| `q` | Quit |

---

## Project Structure

```
src/
├── main.rs       # Entry point, wires everything together
├── config.rs     # Config loading (~/.config/outreach/config.toml)
├── auth/         # OAuth2 device code flow + token cache
├── api/          # Microsoft Graph API client
├── db/           # SQLite local email cache
└── tui/          # ratatui terminal UI
```

---

## Roadmap

- [ ] v0.2 — Reply, delete, mark read from TUI
- [ ] v0.3 — Compose new emails
- [ ] v0.4 — Folder navigation (Sent, Drafts, etc.)
- [ ] v0.5 — Background sync, notifications
- [ ] v1.0 — Calendar view
