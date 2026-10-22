# Parity Checklist — visor-tui vs visor-cli

Exhaustive mapping of every CLI command, flag, auth flow, and API endpoint to a TUI screen or action. Status column tracks implementation progress.

**Status key:** ✅ done · 🚧 in progress · ⬜ not started

---

## 1. Commands → TUI Screens/Actions

### Auth & Session

| CLI Command | TUI Screen / Action | Status |
|---|---|---|
| `visor auth login` | Login screen: OAuth browser flow + PAT entry form | ✅ |
| `visor auth login --with-token` | PAT input form (masked, stdin-style) | ✅ |
| `visor auth login --no-browser` | Show OAuth URL for manual copy + code entry | ⬜ |
| `visor auth logout` | Settings > Logout with confirmation modal | ✅ |
| `visor auth status` | Settings screen: current token kind, user, expiry | ✅ |
| `visor whoami` | Sidebar/header: user name, email, active space | ✅ |
| `visor me` | Profile screen: full user details (name, email, locale, plan, currency) | ✅ |
| `visor doctor` | Diagnostics screen: 4 checks (version, login, API, space) with ok/warn/fail | ✅ |
| `visor open <page>` | Keyboard shortcut or menu: open dashboard page in browser | ✅ |
| Keyboard navigation (1-9,0,-,=, arrows, Tab, mouse) | Sidebar nav with distinct keys per screen | ✅ |
| Help overlay (? key) | Modal with navigation shortcuts | ✅ |

### Space Management

| CLI Command | TUI Screen / Action | Status |
|---|---|---|
| `visor spaces list` | Space selector screen: list with active marker | ✅ |
| `visor spaces use <space>` | Space selector: pick to switch active space | ✅ |

### Transactions

| CLI Command | TUI Screen / Action | Status |
|---|---|---|
| `visor transactions list` | Transações screen: scrollable table with filters | ✅ |
| `--from` / `--to` | Date range filter controls | ✅ |
| `--account` | Account filter dropdown (name-or-ID resolution) | ✅ |
| `--category` | Category filter dropdown (name/key/ID resolution) | ✅ |
| `--tag` | Tag filter multi-select | ✅ |
| `--type` (debit\|credit) | Type filter toggle | ✅ |
| `--ignored` | Show/hide ignored toggle | ⬜ |
| `--search` | Search input field | ✅ |
| `--limit` / `--after` / `--all` | Pagination controls (load more / infinite scroll) | ✅ |
| `--csv` | Export action: download/save CSV | ✅ |
| `visor transactions get <id>` | Transaction detail modal/screen | ✅ |
| `visor transactions add` | New transaction form (all fields) | ✅ |
| `--amount` | Amount input (decimal, accepts BR grouping) | ✅ |
| `-d/--description` | Description text input | ✅ |
| `-c/--category` | Category selector (hierarchical list) | ✅ |
| `-a/--account` | Account selector | ✅ |
| `--type` (debit\|credit) | Type radio/toggle | ✅ |
| `--at` | Date/time picker (defaults to now) | ✅ |
| `--merchant` | Merchant name input | ✅ |
| `--notes` | Notes textarea | ✅ |
| `--tag` (repeatable) | Tag multi-select | ⬜ |
| `--ignored` | Ignored checkbox | ⬜ |
| `--idempotency-key` | Auto-generated, hidden from user | ✅ |
| `visor transactions edit <id>` | Edit transaction form (respects source immutability) | ✅ |
| `--amount` (manual only) | Amount input (disabled for connected/apple_pay) | ✅ |
| `-d/--description` (manual only) | Description input (disabled for connected/apple_pay) | ✅ |
| `--type` (manual only) | Type toggle (disabled for connected/apple_pay) | ✅ |
| `--at` (manual only) | Date/time picker (disabled for connected/apple_pay) | ✅ |
| `--clear-tags` | Clear tags button | ✅ |
| `visor transactions delete <id>` | Delete confirmation modal | ✅ |
| `-y/--yes` | Skip confirmation (TUI always confirms) | ✅ |

### Accounts & Cards

| CLI Command | TUI Screen / Action | Status |
|---|---|---|
| `visor accounts list` | Contas e Cartões screen: accounts table | ✅ |
| `--include-hidden` | Show/hide hidden toggle | ✅ |
| `visor cards list` | Contas e Cartões screen: cards section | ✅ |
| `visor cards bills <card>` | Faturas screen: bills table for selected card | ✅ |
| `--from` / `--to` | Date range filter for bills | ⬜ |

### Categories & Tags

| CLI Command | TUI Screen / Action | Status |
|---|---|---|
| `visor categories list` | Categorias screen: hierarchical tree view | ✅ |
| `--include-hidden` | Show/hide hidden toggle | ✅ |
| `visor categories create` | Categories screen: "Nova Categoria" button + 'n' key opens create modal (name, type, parent, kind) | ✅ |
| `visor categories edit` | Categories screen: select + 'e' key opens edit modal (name, parent, hidden) | ✅ |
| `visor categories delete` | Categories screen: select + 'd' key opens delete confirmation modal | ✅ |
| `visor tags list` | Tags management screen: list with color indicators | ✅ |
| `visor tags create` | Tags screen: "Nova Tag" button + create modal (name + color) | ✅ |
| `visor tags edit` | Tags screen: select + 'e' key opens edit modal (name + color) | ✅ |
| `visor tags delete` | Tags screen: select + 'd' key opens delete confirmation modal | ✅ |

### Recurring Expenses

| CLI Command | TUI Screen / Action | Status |
|---|---|---|
| `visor subscriptions list` | Recorrentes screen: subscriptions tab | ✅ |
| `visor subscriptions create` | Recorrentes screen: "Nova Assinatura" button + create modal (name, amount, frequency, account, category, service) | ✅ |
| `visor subscriptions edit` | Recorrentes screen: select + 'e' key opens edit modal with active toggle | ✅ |
| `visor subscriptions delete` | Recorrentes screen: select + 'd' key opens delete confirmation modal | ✅ |
| `visor fixed-bills list` | Recorrentes screen: fixed bills tab | ✅ |
| `visor fixed-bills create` | Recorrentes screen: "Nova Conta Fixa" button + create modal (name, amount, frequency, account, category) | ✅ |
| `visor fixed-bills edit` | Recorrentes screen: select + 'e' key opens edit modal with active toggle | ✅ |
| `visor fixed-bills delete` | Recorrentes screen: select + 'd' key opens delete confirmation modal | ✅ |
| `visor installments list` | Contas a pagar screen: installment plans | ✅ |
| `--status` (active\|completed) | Status filter tabs/toggle | ✅ |

### Goals & Investments

| CLI Command | TUI Screen / Action | Status |
|---|---|---|
| `visor goals list` | Metas screen: goals list with progress bars | ✅ |
| `--status` (active\|archived\|completed) | Status filter | ⬜ |
| `visor goals create` | Metas screen: "Nova Meta" button + 'n' key opens create modal (name, target amount, kind, target date) | ✅ |
| `visor goals edit` | Metas screen: select + 'e' key opens edit modal (name, target amount, status, target date) | ✅ |
| `visor goals delete` | Metas screen: select + 'd' key opens delete confirmation modal with goal name and progress | ✅ |
| `visor investments list` | Patrimônio screen: investments table + summary footer | ✅ |

### Overview & Projections

| CLI Command | TUI Screen / Action | Status |
|---|---|---|
| `visor overview` | Visão Geral screen: balances, month so far, projection | ✅ |
| Fluxo de Caixa | Cash flow section within Visão Geral or separate screen | ✅ |
| Projeção | Projeção screen or section: projected balance, committed outflows | ✅ |

### Connections & Notifications

| CLI Command | TUI Screen / Action | Status |
|---|---|---|
| `visor connections list` | Connections/Integrations screen: sync status per provider | ✅ |
| `visor notifications list` | Notifications panel/screen: paginated list | ✅ |
| `--unread` | Filter unread toggle | ✅ |
| `--limit` / `--after` / `--all` | Pagination (load more) | ✅ |

### Utilities

| CLI Command | TUI Screen / Action | Status |
|---|---|---|
| `visor completions <shell>` | Not applicable to TUI (CLI-only feature) | ⬜ N/A |

---

## 2. API Endpoints → TUI Coverage

### User & Global

| Method | Path | TUI Screen / Action | Status |
|---|---|---|---|
| GET | `/v1/me` | Sidebar user info, Profile screen, Auth status | ✅ |
| GET | `/v1/notifications` | Notifications screen with pagination | ✅ |
| GET | `/v1/spaces` | Space selector, space resolution | ✅ |

### Space-Scoped Reads

| Method | Path | TUI Screen / Action | Status |
|---|---|---|---|
| GET | `/v1/spaces/{id}/accounts` | Contas e Cartões, account filter dropdowns | ✅ |
| GET | `/v1/spaces/{id}/cards` | Contas e Cartões cards section | ✅ |
| GET | `/v1/spaces/{id}/cards/{id}/bills` | Faturas screen | ✅ |
| GET | `/v1/spaces/{id}/categories` | Categorias screen, category filter dropdowns | ✅ |
| GET | `/v1/spaces/{id}/connections` | Connections screen | ✅ |
| GET | `/v1/spaces/{id}/fixed-bills` | Recorrentes > Contas fixas | ✅ |
| GET | `/v1/spaces/{id}/goals` | Metas screen | ✅ |
| GET | `/v1/spaces/{id}/installment-plans` | Contas a pagar screen | ✅ |
| GET | `/v1/spaces/{id}/investments` | Patrimônio screen | ✅ |
| GET | `/v1/spaces/{id}/overview` | Visão Geral screen | ✅ |
| GET | `/v1/spaces/{id}/subscriptions` | Recorrentes > Assinaturas | ✅ |
| GET | `/v1/spaces/{id}/tags` | Tag filter dropdowns, tag management | ✅ |

### Transactions CRUD

| Method | Path | TUI Screen / Action | Status |
|---|---|---|---|
| GET | `/v1/spaces/{id}/transactions` | Transações list with filters + pagination | ✅ |
| POST | `/v1/spaces/{id}/transactions` | New transaction form submit (with Idempotency-Key) | ✅ |
| GET | `/v1/spaces/{id}/transactions/{id}` | Transaction detail view | ✅ |
| PATCH | `/v1/spaces/{id}/transactions/{id}` | Edit transaction form submit | ✅ |
| DELETE | `/v1/spaces/{id}/transactions/{id}` | Delete with confirmation modal | ✅ |

### OAuth Token Endpoints

| Method | Path | TUI Action | Status |
|---|---|---|---|
| POST | `/oauth/token` (auth code exchange) | OAuth login flow backend | ✅ |
| POST | `/oauth/token` (refresh) | Auto-refresh on token expiry (transparent) | ✅ |

---

## 3. Auth Flow

| Feature | TUI Implementation | Status |
|---|---|---|
| OAuth 2.0 Authorization Code + PKCE | Login screen opens browser, listens on loopback callback | ✅ |
| PKCE S256 challenge/verifier | Generated internally during OAuth flow | ✅ |
| State parameter (CSRF) | Generated and validated internally | ✅ |
| Login timeout (300s) | Countdown/cancel on login screen | ✅ |
| Scopes (`pfm:read`, `pfm:transactions:write`) | Shown during OAuth consent; write ops check scope | ✅ |
| Auto-refresh (proactive within 60s) | Background refresh, transparent to user | ✅ |
| Reactive refresh on TOKEN_EXPIRED | Retry with refreshed token, transparent | ✅ |
| Multi-process safety (adopt newer credential) | Reload from store on refresh failure | ✅ |
| Personal API Token input | Masked text input form | ✅ |
| PAT validation (must start with `visor_pat_`) | Client-side validation before storage | ✅ |
| VISOR_TOKEN env override | Read on startup, takes precedence over store | ✅ |
| Keyring credential store (default) | Reuse CLI's keyring integration | ✅ |
| File credential store (`VISOR_CREDENTIAL_STORE=file`) | Read/write `credentials.json` with 0600 perms | ✅ |
| Credential JSON format (tagged union) | Match CLI's `personal_token` / `oauth` format | ✅ |
| Config dir resolution ($VISOR_CONFIG_DIR, XDG, ~/.config/visor) | Match CLI resolution order | ✅ |
| Token redaction in debug output | Redact to `visor_pat_abcd_...` | ✅ |

---

## 4. Config & State

| Feature | TUI Implementation | Status |
|---|---|---|
| `config.toml` with `default_space` | Read/write on space switch | ✅ |
| `credentials.json` (file store) | Match CLI format and permissions | ✅ |
| `update-check.json` | Background update check (cached 24h) | ⬜ |
| VISOR_SPACE env override | Override active space on startup | ✅ |
| VISOR_API_URL env override | Override API base URL | ✅ |
| VISOR_AUTH_URL env override | Override OAuth auth server URL | ✅ |
| VISOR_DEBUG env | Debug panel or stderr logging toggle | ✅ |
| VISOR_NO_UPDATE_NOTIFIER | Skip update check | ⬜ |
| NO_COLOR | Disable ANSI colors | ⬜ |

---

## 5. Output & Display

| Feature | TUI Implementation | Status |
|---|---|---|
| Table output (default) | Native TUI tables via HQTUI | ✅ |
| JSON output (`--json`) | Raw JSON view/export option | ⬜ |
| CSV export (`--csv` on transactions) | Export action in Transações screen | ✅ |
| Key-value display (me, overview, tx get) | Detail views with aligned fields | ✅ |
| Money formatting (`amount CURRENCY`, negative for debits) | Consistent currency formatting | ✅ |
| Date formatting (RFC 3339 → local timezone) | Localized date display | ✅ |
| Hierarchical categories (indentation) | Tree view in Categorias | ✅ |
| Active space marker (`*`) | Highlight in space selector | ✅ |
| Investment summary footer | Summary row in Patrimônio | ✅ |
| Pagination hints | "Load more" indicators | ✅ |
| Activity spinner | Loading states on async operations | ✅ |

---

## 6. Other Features

| Feature | TUI Implementation | Status |
|---|---|---|
| Update checker (background, 24h cache) | Toast/banner on new version | ⬜ |
| Doctor command (4 checks) | Diagnostics screen | ✅ |
| Banner (pixel art, TrueColor/Ansi256/None) | Splash/loading screen | ⬜ |
| Color mode detection (NO_COLOR, COLORTERM, TERM=dumb) | Theme selection respecting env | ⬜ |
| Shell completions | N/A (CLI-only) | ⬜ N/A |
| Debug mode (HTTP request logging) | Debug panel showing requests | ⬜ |
| HTTP retries (429 with Retry-After, 5xx exponential) | Built into API client | ⬜ |
| Rate limit handling (60 req/min) | Rate limit indicator / backoff | ⬜ |
| Idempotency (POST transactions) | Auto-generated keys in client | ✅ |
| Name-or-ID resolution (accounts, categories, tags, spaces) | Fuzzy matching in filter dropdowns | ✅ |
| Input validation (amounts, dates, UUIDs, idempotency keys) | Client-side form validation | ✅ |
| Brazilian amount formatting (`1.234,56` → `1234.56`) | Accept both formats in forms | ✅ |

---

## 7. Exit Codes & Error Handling

| Error Code | Meaning | TUI Behavior | Status |
|---|---|---|---|
| 0 | Success | Normal operation | ✅ |
| 1 | General failure | Error toast/notification | ✅ |
| 2 | Invalid usage | Form validation error display | ✅ |
| 3 | Not authenticated | Redirect to login screen | ✅ |
| 4 | Not found | "Not found" error display | ✅ |
| 5 | Request rejected | Validation/conflict error display | ✅ |
| 6 | Unavailable | Retry/backoff notification | ✅ |

| Error Code (API) | TUI Handling | Status |
|---|---|---|
| `INVALID_TOKEN` (401) | Redirect to login or re-auth prompt | ✅ |
| `INSUFFICIENT_SCOPE` (403) | Show scope/plan requirement message | ✅ |
| `NOT_FOUND` (404) | Resource not found display | ✅ |
| `BAD_REQUEST` (400) | Show parameter/field error | ✅ |
| `VALIDATION_FAILED` (422) | Show field-level errors on form | ✅ |
| `RATE_LIMITED` (429) | Show retry-after countdown | ✅ |
| `IDEMPOTENCY_CONFLICT` (409) | Show conflict error on create | ✅ |
| `IMMUTABLE_PROVIDER_FIELD` (409) | Show immutability error on edit | ✅ |
| `TRANSACTION_CONFLICT` (409) | Show dependency error on delete/edit | ✅ |

---

## 8. Web Dashboard Screen Mapping

| Dashboard Section | TUI Screen | Key Endpoints | Status |
|---|---|---|---|
| Visão Geral | Overview screen | `/overview` | ✅ |
| Transações | Transactions screen | `/transactions` (list, CRUD) | 🚧 |
| Recorrentes | Recurring screen (tabs) | `/subscriptions`, `/fixed-bills` | ✅ |
| Fluxo de Caixa | Cash flow section/screen | `/overview` (period data) | ✅ |
| Contas e Cartões | Accounts & Cards screen | `/accounts`, `/cards` | ✅ |
| Categorias | Categories screen | `/categories` | ✅ |
| Metas | Goals screen | `/goals` | ✅ |
| Projeção | Projection screen | `/overview` (projection data) | ✅ |
| Patrimônio | Net Worth / Investments screen | `/investments` | ✅ |
| Faturas | Bills screen | `/cards/{id}/bills` | ✅ |
| Contas a pagar | Bills to pay screen | `/installment-plans` | ✅ |

---

## Summary

| Category | Total Items | Done | In Progress | Not Started |
|---|---|---|---|---|
| Commands → Screens | 62 | 56 | 0 | 6 |
| API Endpoints | 19 | 19 | 0 | 0 |
| Auth Flow | 16 | 15 | 0 | 1 |
| Config & State | 9 | 6 | 0 | 3 |
| Output & Display | 11 | 10 | 0 | 1 |
| Other Features | 12 | 7 | 0 | 5 |
| Exit Codes & Errors | 16 | 0 | 0 | 16 |
| Dashboard Screens | 11 | 11 | 0 | 0 |
| **Total** | **156** | **124** | **0** | **32** |