# Visor TUI UX

The interface keeps hqtui and all 16 screens. A consistent shell shows the current
space, sync/cache state and rate-limit cooldown. Wide terminals show a sidebar;
compact terminals keep the header and keyboard navigation. Financial values use
Brazilian separators and income/debt colors. Money formatting and totals use checked
decimal arithmetic. Unknown totals stay unavailable, and different currencies are
never added together. Tables follow the selected record; short dashboards switch
between complete financial sections with `j` and `k`.

Transactions have keyboard filters, visible active-filter chips, searchable
pickers, a create/edit modal, field validation and inline API errors. Modal input
owns the keyboard: typing screen shortcuts or pasting text cannot navigate away
or confirm deletion. Category edit/delete actions follow the displayed hierarchy.
Changing space resets screen data and filters, including when VISOR_SPACE is set.
The selected calendar period persists across screens and space changes.

## Calendar periods

The default is the current calendar month in Brasília. A centered, accented period
bar spans the content area directly above the summaries and records. Click its
arrows to change month or its label to open the period picker. Muted keyboard hints
sit beside the label. `[` and `]` select the previous or next calendar month relative
to the range's start. `T` returns to the current month. `p` opens the period picker
with Mês, Semana, Trimestre, Ano and Personalizado. Weeks start on Monday.
Custom ranges include both endpoints and accept `DD/MM/YYYY` or `YYYY-MM-DD`.
Invalid dates and reversed ranges stay in the picker until corrected or cancelled.

Overview, Transactions, Cash Flow, Categories, Bills and Payables use the same
range. Transaction filters affect only the transaction list and its summaries.
Clearing filters preserves the selected period. Bills use their due date.
Payables shows plans whose known next payment falls in the range; the API does
not expose a historical installment schedule.

The transaction list groups records by Brasília day and shows daily subtotals.
The app fetches every page for the selected range before displaying the count
or financial totals. A loading screen appears during the fetch. There is no
manual load-more action or partial total.

Period summaries exclude ignored transactions and sum decimal amounts exactly.
BRL totals stay separate from totals in other currencies. The API supplies no
transaction exchange rates or converted amounts, so foreign-currency amounts
remain labeled in their original currency. They are never added to BRL balances.
Current balances and projections from the overview endpoint retain their current
scope; selected-period movement comes from the complete transaction range.

## Visual design

The shell uses a restrained blue-gray palette, teal selection, rounded borders,
and muted secondary text. The wide sidebar groups navigation and highlights only
the active screen. At 80 columns the sidebar yields its width to the content.
The header keeps the screen and space names visible. Period-based screens have a
separate period bar above their content. The footer shows keyboard hints and the
request status.

| Screen | Layout |
| --- | --- |
| Accounts and cards | Total balance, credit limit and used credit tiles. One row per bank account with institution, type and right-aligned balance. Individual credit-card panels show usage meters, percentage, used/available/limit amounts and closing/due dates. |
| Overview | Balance tiles, income/expense comparisons, projection panels and an upcoming-bills table. |
| Transactions | Complete period summaries, day groups with daily subtotals and filter chips. Category and account columns fit at 120 columns; compact layouts retain summaries, with full record details on Enter. Category spending is available on Categories. |
| Cash flow | Period totals and a daily flow table, with currencies kept separate. `j` / `k` also reaches the current forecast, projected balances and outgoing commitments. |
| Projection | Current, expected and projected amounts, plus the components of planned spending. |
| Net worth | Investment totals, allocation bars, one row per position and selected-position details. |
| Goals | Active count, selected current/target amounts, progress meter and dates. |
| Recurring | Tabs, active count, active monthly total, next due date and subscription records. Annual and inactive charges are excluded from the monthly figure. |
| Bills | Card selector, totals and bill records whose due dates fall in the selected range, payment meter and minimum-payment details. |
| Payables | Status tabs, plans with a known next due date in the period, remaining totals and selected payment progress. |
| Categories | Period spending alongside the editable category hierarchy, translated types and visible hidden state. |
| Tags | Count tiles, native color swatches and tag names. |
| Notifications | Unread count, inbox table and selected-message preview. |
| Connections | Active/attention/account counts, institution status table and sync details. |
| Settings | Profile, active space and protected access panels. Credential contents are never displayed. |
| Diagnostics | Health counts and labeled check results. |

Account source badges use `●` for synchronized accounts and `○` for manual ones.
`j` and `k` move through accounts and then credit cards. Credit usage changes from
green to amber at 70% and red at 90%. Over-limit labels retain the actual percentage.
The bar alone stops at 100%. Missing or zero limits do not produce a fake percentage.

Charts use the values supplied by the API. Snapshot endpoints do not provide a
history series, so these screens use comparisons and allocation bars instead of
invented trends. Compact views retain selected-record details when table columns
are hidden.

Reviewed mock-data captures:

- Accounts and cards at [120×40](docs/screenshots/120x40-accounts.png) and
  [80×24](docs/screenshots/80x24-accounts.png).
- Overview at [120×40](docs/screenshots/120x40-overview.png) and
  [80×24](docs/screenshots/80x24-overview.png).
- Transactions at [120×40](docs/screenshots/120x40-transactions.png) and
  [80×24](docs/screenshots/80x24-transactions.png).
- Cash Flow at [120×40](docs/screenshots/120x40-cash-flow.png) and
  [80×24](docs/screenshots/80x24-cash-flow.png).
- Categories at [120×40](docs/screenshots/120x40-categories.png) and
  [80×24](docs/screenshots/80x24-categories.png).
- Bills at [120×40](docs/screenshots/120x40-bills.png) and
  [80×24](docs/screenshots/80x24-bills.png).
- Payables at [120×40](docs/screenshots/120x40-payables.png) and
  [80×24](docs/screenshots/80x24-payables.png).
- Period picker at [120×40](docs/screenshots/120x40-period-picker.png) and
  [80×24](docs/screenshots/80x24-period-picker.png).

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
| `[` / `]` | Previous / next calendar month |
| `T` | Current calendar month |
| `p` | Choose month, week, quarter, year or custom range |
| `r` | Refresh the current screen, bypassing cache freshness |
| `?` | Open contextual help |
| `Esc` | Close help, cancel a dialog or go back from a picker |
| `q` | Quit |

| Screen | Additional keys |
| --- | --- |
| Transactions | `/` search, `f` filters, `c` clear filters, `n` new, `e` edit, `d` delete with confirmation, `Enter` details, `PageUp` / `PageDown` move ten rows, `Home` / `End` first / last row, `E` / `Ctrl-E` export CSV |
| Categories | `n` new, `e` edit, `d` delete with confirmation, `h` show hidden |
| Tags / Goals | `n` new, `e` edit, `d` delete with confirmation |
| Recurring | `←` / `→` switch subscriptions / fixed bills; `n`, `e`, `d` manage subscriptions |
| Accounts/Cards | `h` show hidden accounts |
| Bills | `←` / `→` choose a card |
| Payables | `←` / `→` cycle all / active / completed |
| Notifications | `u` unread only; `m` / `Enter` mark selected notification read |
| Settings | `s` choose space; `l` log out with confirmation |

`h` / `l` and `←` / `→` also navigate months where the current screen does not
already use them. Card selection, recurring and payable tabs, hidden-item toggles
and logout retain their existing bindings. Forms and pickers own all input while
open, including pasted dates and characters that are global shortcuts elsewhere.

## Mouse controls

The terminal enables mouse capture while Visor runs. The sidebar switches screens
with a click. At compact widths, **Telas** opens a screen picker. Period arrows
change month, and the period label opens the period picker.

Click a table row to select it. Double-click a transaction to open its details,
or press `Enter` after selecting it. Other record screens update their selection
and any inline details. Scroll the wheel over a list or report to move
through its contents. Recurring tabs, payable status tabs, and bill card controls
also accept clicks.

Toolbar actions use the same behavior as their keyboard shortcuts. Click a form
field to focus it, then type or choose a value. Use the wheel to reach fields that
do not fit in a transaction dialog. Dialog buttons apply, save, confirm, or cancel
the current action. An open modal owns mouse input, including clicks
outside its border, so the screen behind it cannot respond.

Hit areas come from the rendered controls and exclude clipped cells. A resize
rebuilds them for the new layout. Table targets include the displayed scroll offset,
so a click selects the visible record after scrolling.

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
- Filters cover account, category, tag, type, ignored state and text. Use `p`
  to change the shared date range.
  `Ctrl-S` applies; `Ctrl-L` clears the draft; `Esc` discards changes. Outside the
  panel, `c` clears active filters. CSV export includes the period and active filters and writes
  a new file in the current directory without overwriting an existing export.

## Requests and verification

Reads use a 60-second cache. Redraws and idle time do not fetch, even after the TTL.
Screen entry and explicit actions such as applying filters, changing period and refresh
may fetch. Successful writes invalidate cached data. Reads have bounded retry and
backoff; long `Retry-After` values become a visible cooldown. Writes respect that
cooldown and are never retried automatically.

Complete transaction results are cached by space, inclusive range and every
filter. A page failure does not publish partial results. Repeated cursors fail
with an error instead of looping. Refresh and successful mutations invalidate
the complete-range cache with the other cached reads.

```sh
cargo build
cargo clippy --all-targets -- -D warnings
cargo test
PYTHONDONTWRITEBYTECODE=1 UV_CACHE_DIR=/tmp/visor-uv-cache \
  uv run --with pyte python scripts/ux_smoke.py --check-ttl
PYTHONDONTWRITEBYTECODE=1 UV_CACHE_DIR=/tmp/visor-uv-cache \
  uv run --with pyte python scripts/ux_smoke.py --mouse-only --sizes 120x40,80x24
```

The smoke driver runs the real binary in 120×40, 80×24 and 60×18 terminals against
a loopback fixture, with temporary configuration and mock credentials. It exercises
create/edit/delete, cancellation, filters, search, CSV export, refresh, modal input,
space switching and idle behavior. Period checks use date-filtered, multi-page
fixtures and cover navigation, picker validation, shared ranges and cache reuse.
Delayed responses and failed later pages verify loading, unavailable totals and
complete recovery on retry.
Mouse checks at 120×40 and 80×24 cover capture, period controls, rows, double-click,
wheel scrolling, toolbar actions, form focus, dialog buttons, and modal isolation.
They also resize a running terminal and repeat clicks using the new layout.
The 61-second check beyond the cache TTL runs
at 120×40. No real account is used.

For a read-only visual review of all 16 screens at the two design sizes:

```sh
PYTHONDONTWRITEBYTECODE=1 uv run --with pyte python scripts/ux_smoke.py \
  --visual-only --sizes 120x40,80x24 --captures /tmp/visor-review
```

`--screens accounts,transactions` limits that review to named screens. Each capture
includes plain text and a color-preserving SVG. The driver isolates mock credentials,
forces true color independently of the parent shell, checks keyboard selection and
sidebar clicks, and rejects account mutations in visual-only mode.

Export a reviewed SVG to PNG with `rsvg-convert capture.svg -o capture.png`.

Verification includes build, all-target Clippy with warnings denied, Rust tests,
keyboard and mouse smoke, and direct inspection of every data screen at 120×40
and 80×24.
