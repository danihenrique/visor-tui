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
        if path == "/v1/spaces":
            data = [{"id": "smoke", "name": "Espaço de teste", "default": True}]
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
            data = [{"id": "category-1", "name": "Mercado", "type": "expense"}]
        elif path.endswith("/tags"):
            data = [{"id": "tag-1", "name": "Viagem", "color": "#38bdf8"}]
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


def run(binary, captures, baseline=False):
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
                    terminal.expect("Transa")
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
                    terminal.expect("Filtro")
                    terminal.capture(captures, "filters")
                    terminal.send("\x1b")
                    terminal.send("?")
                    terminal.capture(captures, "help")
                    terminal.send("\x1b")
                    assert not [r for r in Fixture.requests if r[0] != "GET"], "Cancellation made a write"
                    request_count = len(Fixture.requests)
                    terminal.read(1)
                    assert len(Fixture.requests) == request_count, "Idle redraw made a request"
                    print(f"PASS {columns}x{rows}: browse, create/cancel, filters/cancel, help, idle")
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
    args = parser.parse_args()
    run(args.binary.resolve(), args.captures, args.baseline)
