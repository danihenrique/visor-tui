# Visor TUI UX

The interface keeps hqtui and all 16 screens. A consistent shell shows the current
space, sync/cache state and rate-limit cooldown. Wide terminals show a sidebar;
compact terminals keep the header and keyboard navigation. Financial values use
Brazilian separators and income/debt colors. Tables and financial summaries scroll
with the selected row so details remain reachable in smaller terminals.

Transactions now have keyboard filters, visible active-filter chips, searchable
pickers, a create/edit modal, field validation and inline API errors. Modal input
owns the keyboard: typing screen shortcuts or pasting text cannot navigate away
or confirm deletion. Category edit/delete actions follow the displayed hierarchy.
Changing space resets screen data and filters, including when VISOR_SPACE is set.

## Keymap

These shortcuts apply outside forms, pickers and confirmation dialogs. `?` shows
the current screen's keys. Mouse controls remain available.

| Key | Action |
| --- | --- |
| `Tab` / `Shift-Tab` | Next / previous screen, wrapping through all screens |
| `1`–`9` | Overview, Transactions, Recurring, Cash Flow, Accounts/Cards, Categories, Goals, Projection, Net Worth |
| `0`, `-`, `t` | Bills, Payables, Tags |
| `N`, `x`, `=`, `D` | Notifications, Connections, Settings, Diagnostics |
| `j` / `k`, `↓` / `↑` | Select rows or scroll financial information |
| `r` | Refresh the current screen, bypassing cache freshness |
| `?` | Open contextual help |
| `Esc` | Close help, cancel a dialog or go back from a picker |
| `q` | Quit |

| Screen | Additional keys |
| --- | --- |
| Transactions | `/` search, `f` filters, `c` clear filters, `n` new, `e` edit, `d` delete with confirmation, `Enter` details, `m` / `→` next page, `PageUp` / `PageDown` move ten rows, `Home` / `End` first / last loaded row, `E` / `Ctrl-E` export CSV |
| Categories | `n` new, `e` edit, `d` delete with confirmation, `h` show hidden |
| Tags / Goals | `n` new, `e` edit, `d` delete with confirmation |
| Recurring | `←` / `→` switch subscriptions / fixed bills; `n`, `e`, `d` manage subscriptions |
| Accounts/Cards | `h` show hidden accounts |
| Bills | `←` / `→` choose a card |
| Payables | `←` / `→` cycle all / active / completed |
| Notifications | `u` unread only; `m` / `Enter` mark selected notification read |
| Settings | `s` choose space; `l` log out with confirmation |

## Transaction forms and filters

- `Tab` / `Shift-Tab` moves between editable fields, scrolling the modal as needed.
  `Ctrl-U` clears the focused text field. `Esc` cancels without a write.
- `Enter` / `Space` opens a picker or toggles a choice. Type to filter picker
  options, use arrows to select, and `Enter` to choose. Tags allow multiple
  selections; `Esc` returns to the form.
- `Ctrl-S` submits. `Enter` also submits from text fields. Invalid fields retain
  the draft, show an error and receive focus; API validation errors appear beside
  their fields.
- Amounts accept `18,90`, `18.90` and `1.234,56`, require a positive value and allow
  at most two decimal places. Dates accept `YYYY-MM-DD` or `DD/MM/YYYY`.
- Editing preserves an unchanged transaction timestamp and original text.
  Account and merchant are read-only on edit. Imported transactions also keep
  amount, description, type and date read-only, matching API restrictions.
- After an uncertain create response, retry with `Ctrl-S` using the same body
  and idempotency key. Before cancelling and starting another transaction, check
  the list to avoid creating a duplicate.
- Filters cover date range, account, category, tag, type, ignored state and text.
  `Ctrl-S` applies; `Ctrl-L` clears the draft; `Esc` discards changes. Outside the
  panel, `c` clears active filters. CSV export includes active filters and writes
  a new file in the current directory without overwriting an existing export.

## Requests and verification

Reads use a 60-second cache. Redraws and idle time do not fetch, even after the TTL.
Screen entry and explicit actions such as applying filters, pagination and refresh
may fetch. Successful writes invalidate cached data. Reads have bounded retry and
backoff; long `Retry-After` values become a visible cooldown. Writes respect that
cooldown and are never retried automatically.

```sh
cargo build
cargo clippy --all-targets -- -D warnings
cargo test
PYTHONDONTWRITEBYTECODE=1 UV_CACHE_DIR=/tmp/visor-uv-cache \
  uv run --with pyte python scripts/ux_smoke.py --check-ttl
```

The smoke driver runs the real binary in 120×40, 80×24 and 60×18 terminals against
a loopback fixture, with temporary configuration and mock credentials. It exercises
create/edit/delete, cancellation, filters, search, CSV export, refresh, modal input,
space switching and idle behavior beyond the cache TTL. No real account is used.
