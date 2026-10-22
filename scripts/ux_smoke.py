#!/usr/bin/env python3
"""Drive the real TUI against a loopback fixture. Requires pyte.

Run `uv run --with pyte python scripts/ux_smoke.py` after `cargo build`.
"""

import argparse
import codecs
import fcntl
import http.server
import json
import os
from pathlib import Path
import pty
import select
import signal
import struct
import subprocess
import tempfile
import termios
import threading
import time
from urllib.parse import parse_qs, urlsplit

import pyte


ROOT = Path(__file__).resolve().parents[1]
TRANSACTION = {
    "id": "tx-1", "account_id": "account-1", "category_id": "category-1",
    "tag_ids": ["tag-1"], "amount": "1234.56", "type": "debit",
    "description": "Mercado de teste", "transacted_at": "2026-10-22T12:00:00Z",
    "source": "manual", "currency_code": "BRL", "ignored": False,
}


class Fixture(http.server.BaseHTTPRequestHandler):
    requests = []
    transaction = dict(TRANSACTION)

    def log_message(self, *_args):
        pass

    def respond(self, body, status=200):
        data = json.dumps(body).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        path = urlsplit(self.path).path
        self.requests.append(("GET", self.path, None))
        if self.headers.get("Accept") == "text/csv":
            data = b"description,amount\nMock transaction,18.90\n"
            self.send_response(200)
            self.send_header("Content-Type", "text/csv")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)
            return
        if path == "/v1/spaces":
            data = [
                {"id": "smoke", "name": "Espaço de teste", "default": True},
                {"id": "smoke-alt", "name": "Outro espaço", "default": False},
            ]
        elif path == "/v1/me":
            data = {"id": "test", "name": "Teste", "email": "test@example.invalid", "plan": "premium"}
        elif path.endswith("/overview"):
            data = {
                "currency_code": "BRL", "bank_balance": "12345.67",
                "credit_card_debt": "1500.00", "open_bills_total": "950.00",
                "net_worth": "10845.67", "upcoming_bills": [],
                "period": {"income": "5000.00", "expenses": "2100.00", "result": "2900.00"},
                "projection": {"current_balance": "12345.67", "projected_balance": "14000.00"},
            }
        elif path.endswith("/accounts"):
            data = [{"id": "account-1", "name": "Conta de teste", "balance": "12345.67", "source": "manual"}]
        elif path.endswith("/categories"):
            data = [
                {"id": "category-1", "name": "Mercado", "type": "expense"},
                {"id": "category-2", "name": "Transporte", "type": "expense"},
            ]
        elif path.endswith("/tags"):
            data = [{"id": "tag-1", "name": "Viagem", "color": "#38bdf8"}]
        elif path.endswith("/cards"):
            data = [
                {"id": f"card-{number}", "name": f"Cartão {number}", "provider_name": "Banco de teste",
                 "currency_code": "BRL", "credit_limit": "5000.00", "used_limit": "1234.56",
                 "available_limit": "3765.44", "current_bill_close_date": "2026-10-25",
                 "current_bill_due_date": "2026-11-02"}
                for number in (1, 2)
            ]
        elif path.endswith("/bills"):
            data = [{"id": "bill-1", "status": "open", "due_date": "2026-11-02", "total_amount": "1234.56", "paid_amount": "0.00"}]
        elif path.endswith("/investments"):
            data = [{"id": "investment-1", "name": "Reserva", "balance": "8765.43", "type": "stock", "source": "manual", "provider_name": "Corretora de teste"}]
        elif path.endswith("/goals"):
            data = [{"id": "goal-1", "name": "Viagem", "target_amount": "10000.00", "current_amount": "2500.00", "progress_percent": 25.0, "status": "active"}]
        elif path.endswith("/subscriptions") or path.endswith("/fixed-bills"):
            data = [{"id": "recurring-1", "name": "Internet", "amount": "99.90", "frequency": "monthly", "active": True, "next_due_date": "2026-11-01"}]
        elif path.endswith("/installment-plans"):
            data = [{"id": "installment-1", "description": "Notebook", "installment_amount": "500.00", "total_amount": "5000.00", "remaining_amount": "3000.00", "total_installments": 10, "paid_installments": 4, "status": "active"}]
        elif path.endswith("/connections"):
            data = [{"id": "connection-1", "provider_name": "Banco de teste", "status": "connected", "last_successful_sync_at": "2026-10-22T12:00:00Z"}]
        elif path.endswith("/transactions"):
            data = [dict(self.transaction)]
        elif "/transactions/" in path:
            data = dict(self.transaction)
        else:
            data = []
        self.respond({"data": data, "next_cursor": None})

    def mutate(self):
        body = self.rfile.read(int(self.headers.get("Content-Length", "0")))
        payload = json.loads(body) if body else None
        self.requests.append((self.command, self.path, payload))
        if payload:
            self.transaction.update(payload)
        self.respond({"data": dict(self.transaction)})

    do_POST = mutate
    do_PATCH = mutate
    do_DELETE = mutate


class Terminal:
    def __init__(self, binary, config_dir, server, columns, rows):
        self.columns, self.rows = columns, rows
        self.screen = pyte.Screen(columns, rows)
        self.stream = pyte.Stream(self.screen)
        self.decoder = codecs.getincrementaldecoder("utf-8")("replace")
        self.master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", rows, columns, 0, 0))
        env = dict(os.environ)
        env.update(
            VISOR_CONFIG_DIR=config_dir,
            VISOR_TOKEN="visor_pat_mock_fixture_only",
            VISOR_SPACE="smoke",
            VISOR_API_URL=f"http://127.0.0.1:{server.server_port}",
            VISOR_AUTH_URL=f"http://127.0.0.1:{server.server_port}",
            VISOR_CREDENTIAL_STORE="file",
            VISOR_DEBUG="0", NO_PROXY="127.0.0.1,localhost", no_proxy="127.0.0.1,localhost",
            TERM="xterm-256color", COLORTERM="truecolor",
        )
        self.process = subprocess.Popen(
            [str(binary)], stdin=slave, stdout=slave, stderr=slave,
            cwd=config_dir, env=env, start_new_session=True,
            preexec_fn=lambda: fcntl.ioctl(0, termios.TIOCSCTTY, 0),
        )
        os.close(slave)
        self.read(0.5)

    def read(self, duration=0.15):
        until = time.monotonic() + duration
        while time.monotonic() < until:
            ready, _, _ = select.select([self.master], [], [], max(0, until - time.monotonic()))
            if ready:
                try:
                    data = os.read(self.master, 65536)
                except OSError:
                    break
                if not data:
                    break
                self.stream.feed(self.decoder.decode(data))

    def send(self, keys):
        os.write(self.master, keys.encode())
        self.read()

    def text(self):
        return "\n".join(self.screen.display)

    def expect(self, text):
        assert text in self.text(), f"Missing {text!r} at {self.columns}x{self.rows}:\n{self.text()}"

    def click(self, label):
        for y, line in enumerate(self.screen.display):
            x = line.find(label)
            if x >= 0:
                self.send(f"\x1b[<0;{x + 1};{y + 1}M\x1b[<0;{x + 1};{y + 1}m")
                return
        raise AssertionError(f"Click target {label!r} is not visible:\n{self.text()}")

    def capture(self, directory, name):
        if directory:
            directory.mkdir(parents=True, exist_ok=True)
            (directory / f"{self.columns}x{self.rows}-{name}.txt").write_text(self.text() + "\n")

    def close(self):
        if self.process.poll() is None:
            self.process.send_signal(signal.SIGTERM)
            try:
                self.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
        os.close(self.master)


def run(binary, captures, baseline=False, check_ttl=False):
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Fixture)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        for columns, rows in [(120, 40), (80, 24), (60, 18)]:
            Fixture.requests.clear()
            Fixture.transaction = dict(TRANSACTION)
            with tempfile.TemporaryDirectory(prefix="visor-ux-") as config:
                terminal = Terminal(binary, config, server, columns, rows)
                try:
                    terminal.capture(captures, "overview")
                    terminal.send("2")
                    assert any("/transactions?" in request[1] for request in Fixture.requests), "2 did not load transactions"
                    terminal.capture(captures, "transactions")
                    terminal.send("n")
                    terminal.capture(captures, "new")
                    if baseline:
                        assert "Nova transa" not in terminal.text(), "Baseline bug no longer present"
                        print(f"Reproduced: n does not create a transaction at {columns}x{rows}")
                        continue
                    terminal.expect("Nova transa")
                    terminal.send("\x1b")
                    terminal.send("f")
                    terminal.expect("Filtrar transa")
                    terminal.capture(captures, "filters")
                    terminal.send("\x1b")
                    terminal.send("?")
                    terminal.capture(captures, "help")
                    terminal.send("\x1b")
                    assert not [r for r in Fixture.requests if r[0] != "GET"], "Cancellation made a write"
                    terminal.send("n")
                    terminal.send("\x13")
                    assert not any(r[0] == "POST" for r in Fixture.requests), "Invalid draft made a request"
                    terminal.expect("Nova transa")
                    terminal.capture(captures, "validation")
                    terminal.send("18,90\tConta qn?12\t\t\r")
                    terminal.send("Conta\r\t\r")
                    terminal.send("Merc\r")
                    terminal.send("\t\rViagem\r")
                    terminal.send("\x1b")
                    terminal.send("\tLoja de teste\tNota de teste\t\x1522/10/2026\t ")
                    terminal.capture(captures, "filled-form")
                    terminal.send("\x13")
                    writes = [r for r in Fixture.requests if r[0] == "POST"]
                    assert len(writes) == 1, f"Expected one create, got {writes}:\n{terminal.text()}"
                    payload = writes[0][2]
                    assert payload["amount"] == "18.90", payload
                    assert payload["description"] == "Conta qn?12", payload
                    assert payload["type"] == "debit", payload
                    assert payload["category_id"] == "category-1", payload
                    assert payload["account_id"] == "account-1", payload
                    assert payload["tag_ids"] == ["tag-1"], payload
                    assert payload["merchant_name"] == "Loja de teste", payload
                    assert payload["notes"] == "Nota de teste", payload
                    assert payload["transacted_at"].startswith("2026-10-22T"), payload
                    assert payload["ignored"] is True, payload
                    terminal.send("e")
                    terminal.expect("Editar")
                    terminal.send("\x1525,10\x13")
                    writes = [r for r in Fixture.requests if r[0] == "PATCH"]
                    assert len(writes) == 1 and writes[0][2]["amount"] == "25.10", writes
                    assert not list(Path(config).glob("transactions_*.csv")), "e also exported"
                    terminal.send("/")
                    query = "mercado & café + #"
                    terminal.send(query)
                    terminal.send("\r")
                    searches = [parse_qs(urlsplit(r[1]).query).get("search") for r in Fixture.requests]
                    assert [query] in searches, searches
                    terminal.send("c")
                    terminal.send("f")
                    request_count = len(Fixture.requests)
                    terminal.send("32/13/2026\x13")
                    assert len(Fixture.requests) == request_count, "Invalid filter made a request"
                    terminal.send("\x152026-10-01\t2026-10-31\x13")
                    queries = [parse_qs(urlsplit(r[1]).query) for r in Fixture.requests]
                    assert any(q.get("from_date") == ["2026-10-01"] and q.get("to_date") == ["2026-10-31"] for q in queries), queries
                    terminal.send("f\t\t\rConta\r\t\rTrans\r\t\rViagem\r\t\r\t\r\tcoffee & tea\x13")
                    filtered = next(parse_qs(urlsplit(r[1]).query) for r in reversed(Fixture.requests) if "/transactions?" in r[1])
                    for key, value in {"account_id": "account-1", "category_id": "category-2", "tag_id": "tag-1", "type": "credit", "ignored": "true", "search": "coffee & tea"}.items():
                        assert filtered.get(key) == [value], (key, filtered)
                    terminal.capture(captures, "active-filters")
                    terminal.send("E")
                    exports = list(Path(config).glob("transactions_*.csv"))
                    assert len(exports) == 1 and "Mock transaction,18.90" in exports[0].read_text(), "CSV export failed"
                    export_query = parse_qs(urlsplit(Fixture.requests[-1][1]).query)
                    assert export_query == {k: v for k, v in filtered.items() if k != "limit"}, "CSV lost active filters"
                    terminal.send("\x1b")
                    terminal.send("c")
                    terminal.send("d")
                    terminal.capture(captures, "delete")
                    terminal.send("\x1b")
                    assert not any(r[0] == "DELETE" for r in Fixture.requests), "Cancel deleted a transaction"
                    terminal.send("d\t\r")
                    assert sum(r[0] == "DELETE" for r in Fixture.requests) == 1, Fixture.requests
                    request_count = len(Fixture.requests)
                    terminal.read(61 if check_ttl and columns == 120 else 1)
                    assert len(Fixture.requests) == request_count, "Idle redraw made a request"
                    terminal.send("r")
                    assert len(Fixture.requests) > request_count, "r did not bypass the fresh cache"
                    request_count = len(Fixture.requests)
                    terminal.click("r Atualizar")
                    assert len(Fixture.requests) > request_count, "Mouse refresh did not bypass the fresh cache"
                    for key, title in [("t", "Tags"), ("6", "Categorias"), ("7", "Metas"), ("3", "Recorrentes")]:
                        terminal.send(key)
                        terminal.expect(title)
                        request_count = len(Fixture.requests)
                        terminal.send("n")
                        terminal.capture(captures, title.lower() + "-form")
                        terminal.send("qn?123")
                        terminal.send("\t\x1b")
                        assert len(Fixture.requests) == request_count, f"Typing in {title} leaked into navigation"
                        terminal.capture(captures, title.lower())
                    terminal.send("t")
                    terminal.send("d")
                    request_count = len(Fixture.requests)
                    terminal.send("\x1b[200~yes\x1b[201~")
                    assert len(Fixture.requests) == request_count, "Pasting into confirmation performed an action"
                    terminal.send("\x1b")
                    for key, title in [("4", "Fluxo"), ("5", "Contas"), ("8", "Proje"), ("9", "Patrim"), ("0", "Faturas"), ("-", "Contas a pagar"), ("N", "Notifica"), ("x", "Conex"), ("=", "Configura")]:
                        terminal.send(key)
                        terminal.expect(title)
                        terminal.capture(captures, f"screen-{key.replace('=', 'settings')}")
                        if key in {"4", "5", "8", "9", "="}:
                            request_count = len(Fixture.requests)
                            terminal.send("j" * 40)
                            expected = {"4": "14.000,00 BRL", "5": "2026-11-02", "8": "14.000,00 BRL", "9": "Corretora de teste", "=": "credentials.json"}
                            terminal.expect(expected[key])
                            assert len(Fixture.requests) == request_count, "Scrolling made a request"
                            terminal.capture(captures, f"scrolled-{key.replace('=', 'settings')}")
                        if key == "0":
                            terminal.send("\x1b[C")
                            assert any("/cards/card-2/bills" in r[1] for r in Fixture.requests), "Arrow did not select next card"
                    terminal.send("s")
                    terminal.expect("espaço")
                    terminal.send("j\r")
                    terminal.send("2")
                    assert any("/spaces/smoke-alt/transactions" in r[1] for r in Fixture.requests), "Space selector did not override VISOR_SPACE"
                    terminal.capture(captures, "switched-space")
                    terminal.send("\t")
                    terminal.expect("Recorrentes")
                    terminal.send("\x1b[Z")
                    terminal.expect("Transa")
                    terminal.send("q")
                    terminal.process.wait(timeout=3)
                    assert terminal.process.returncode == 0
                    print(f"PASS {columns}x{rows}: forms, filters, CSV, confirmation, refresh, idle, scrolling, space switch", flush=True)
                finally:
                    terminal.close()
    finally:
        server.shutdown()
        server.server_close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/visor-tui")
    parser.add_argument("--captures", type=Path)
    parser.add_argument("--baseline", action="store_true")
    parser.add_argument("--check-ttl", action="store_true", help="Idle beyond the 60-second cache TTL at 120x40")
    args = parser.parse_args()
    run(args.binary.resolve(), args.captures, args.baseline, args.check_ttl)
