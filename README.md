
<p align="center">
  <img src="docs/banner.png" alt="Visor TUI — Terminal UI for Visor Finance" width="1280">
</p>

# visor-tui

Terminal UI for [Visor Finance](https://visorfinance.app) personal API.

Built with [HQTUI](https://hqtui.com). Mirrors every feature of the official [visor-cli](https://github.com/Visor-Finance/visor-cli).

## Screenshots

> Sample data only — not a real account.

<table>
  <tr>
    <td align="center" width="50%"><img src="docs/screenshots/120x40-overview.png" alt="Visão Geral"/><br/><sub>Visão Geral</sub></td>
    <td align="center" width="50%"><img src="docs/screenshots/120x40-transactions.png" alt="Transações"/><br/><sub>Transações</sub></td>
  </tr>
  <tr>
    <td align="center"><img src="docs/screenshots/120x40-cash-flow.png" alt="Fluxo de Caixa"/><br/><sub>Fluxo de Caixa</sub></td>
    <td align="center"><img src="docs/screenshots/120x40-accounts.png" alt="Contas e Cartões"/><br/><sub>Contas e Cartões</sub></td>
  </tr>
  <tr>
    <td align="center"><img src="docs/screenshots/120x40-categories.png" alt="Categorias"/><br/><sub>Categorias</sub></td>
    <td align="center"><img src="docs/screenshots/120x40-bills.png" alt="Faturas"/><br/><sub>Faturas</sub></td>
  </tr>
  <tr>
    <td align="center"><img src="docs/screenshots/120x40-payables.png" alt="Contas a pagar"/><br/><sub>Contas a pagar</sub></td>
    <td align="center"><img src="docs/screenshots/120x40-period-picker.png" alt="Seletor de período"/><br/><sub>Seletor de período</sub></td>
  </tr>
</table>

<details>
<summary>Terminal 80×24 (compact)</summary>
<br/>
<table>
  <tr>
    <td align="center" width="50%"><img src="docs/screenshots/80x24-overview.png" alt="Visão Geral 80x24"/><br/><sub>Visão Geral</sub></td>
    <td align="center" width="50%"><img src="docs/screenshots/80x24-transactions.png" alt="Transações 80x24"/><br/><sub>Transações</sub></td>
  </tr>
  <tr>
    <td align="center"><img src="docs/screenshots/80x24-cash-flow.png" alt="Fluxo de Caixa 80x24"/><br/><sub>Fluxo de Caixa</sub></td>
    <td align="center"><img src="docs/screenshots/80x24-accounts.png" alt="Contas e Cartões 80x24"/><br/><sub>Contas e Cartões</sub></td>
  </tr>
  <tr>
    <td align="center"><img src="docs/screenshots/80x24-categories.png" alt="Categorias 80x24"/><br/><sub>Categorias</sub></td>
    <td align="center"><img src="docs/screenshots/80x24-bills.png" alt="Faturas 80x24"/><br/><sub>Faturas</sub></td>
  </tr>
  <tr>
    <td align="center"><img src="docs/screenshots/80x24-payables.png" alt="Contas a pagar 80x24"/><br/><sub>Contas a pagar</sub></td>
    <td align="center"><img src="docs/screenshots/80x24-period-picker.png" alt="Seletor de período 80x24"/><br/><sub>Seletor de período</sub></td>
  </tr>
</table>
</details>

## Install

```bash
cargo install --path .
```

Or build from source:

```bash
cargo build --release
./target/release/visor-tui
```

## Authentication

The TUI shares credentials with `visor-cli`. If you are already logged in via the CLI, the TUI will use the same token automatically.

### Personal API Token

Create a token at **Settings → Connected Apps** in the Visor dashboard, then:

```bash
# Option 1: environment variable (takes precedence over stored credentials)
export VISOR_TOKEN=visor_pat_...

# Option 2: let the TUI prompt on first launch
visor-tui
```

### OAuth (CLI login)

If you previously ran `visor auth login`, the TUI reuses that credential from your OS keyring or file store.

### Environment Variables

| Variable | Purpose |
|---|---|
| `VISOR_TOKEN` | Override stored credential with a personal API token |
| `VISOR_SPACE` | Override the active space (name or UUID) |
| `VISOR_API_URL` | Override the API base URL (default: `https://api.visorfinance.app`) |
| `VISOR_AUTH_URL` | Override the OAuth authorization server URL |
| `VISOR_CONFIG_DIR` | Override config directory location |
| `VISOR_CREDENTIAL_STORE` | `keyring` (default) or `file` |
| `VISOR_DEBUG` | Enable HTTP request logging |
| `NO_COLOR` | Disable ANSI colors |

Config directory resolution order:

1. `$VISOR_CONFIG_DIR`
2. `$XDG_CONFIG_HOME/visor`
3. `~/.config/visor`

## Usage

```bash
visor-tui
```

Navigate with arrow keys or Tab. Press `?` for keyboard shortcuts.
See [UX.md](UX.md) for the full keymap, transaction forms, filters, and mock verification.

## License

MIT — see [LICENSE](LICENSE).
