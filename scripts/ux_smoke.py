#!/usr/bin/env python3
"""Drive the real TUI against a loopback fixture. Requires pyte.

Run `uv run --with pyte python scripts/ux_smoke.py` after `cargo build`.
Use `--visual-only --screens accounts,goals --sizes 120x40,80x24
--captures /tmp/visor-review` to inspect selected screens as TXT and SVG.
"""

import argparse
import calendar
import codecs
from datetime import date, datetime, timedelta, timezone
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
from xml.sax.saxutils import escape
from urllib.parse import parse_qs, urlsplit

import pyte


ROOT = Path(__file__).resolve().parents[1]
BRASILIA = timezone(timedelta(hours=-3))
MONTHS = ("Janeiro", "Fevereiro", "Março", "Abril", "Maio", "Junho", "Julho", "Agosto", "Setembro", "Outubro", "Novembro", "Dezembro")
OCTOBER = ("2026-10-01", "2026-10-31")
TRANSACTION = {
    "id": "tx-1", "account_id": "account-1", "category_id": "category-1",
    "tag_ids": ["tag-1"], "amount": "1234.56", "type": "debit",
    "description": "Mercado de teste", "transacted_at": "2026-10-22T12:00:00Z",
    "source": "manual", "currency_code": "BRL", "ignored": False,
}


SCREENS = {
    "overview": ("1", "Visão Geral"),
    "transactions": ("2", "Transações"),
    "recurring": ("3", "Recorrentes"),
    "cash-flow": ("4", "Fluxo de Caixa"),
    "accounts": ("5", "Contas e Cartões"),
    "categories": ("6", "Categorias"),
    "goals": ("7", "Metas"),
    "projection": ("8", "Projeção"),
    "net-worth": ("9", "Patrimônio"),
    "bills": ("0", "Faturas"),
    "payables": ("-", "Contas a pagar"),
    "tags": ("t", "Tags"),
    "notifications": ("N", "Notificações"),
    "connections": ("x", "Conexões"),
    "settings": ("=", "Configurações"),
    "doctor": ("D", "Diagnóstico"),
}
RECORD_SCREENS = {
    "transactions", "recurring", "categories", "goals", "bills", "payables",
    "tags", "notifications", "connections",
}
ACCOUNTS = [
    {"id": "account-1", "name": "Conta principal", "balance": "12345.67", "type": "checking", "source": "connected", "provider_name": "Nubank", "currency_code": "BRL"},
    {"id": "account-2", "name": "Reserva de emergência", "balance": "18000.00", "type": "savings", "source": "connected", "provider_name": "Itaú", "currency_code": "BRL"},
    {"id": "account-3", "name": "Carteira", "balance": "280.50", "type": "cash", "source": "manual", "currency_code": "BRL"},
    {"id": "account-4", "name": "Conta conjunta", "balance": "-420.00", "type": "checking", "source": "connected", "provider_name": "Banco Inter", "currency_code": "BRL"},
]
CARDS = [
    {"id": "card-1", "name": "Nubank Ultravioleta", "provider_name": "Nubank", "currency_code": "BRL", "credit_limit": "10000.00", "used_limit": "2450.00", "available_limit": "7550.00", "current_bill_close_date": "2026-10-25", "current_bill_due_date": "2026-11-02"},
    {"id": "card-2", "name": "Itaú Personnalité", "provider_name": "Itaú", "currency_code": "BRL", "credit_limit": "8000.00", "used_limit": "5800.00", "available_limit": "2200.00", "current_bill_close_date": "2026-10-28", "current_bill_due_date": "2026-11-05"},
    {"id": "card-3", "name": "Inter Black", "provider_name": "Banco Inter", "currency_code": "BRL", "credit_limit": "5000.00", "used_limit": "4650.00", "available_limit": "350.00", "current_bill_close_date": "2026-11-01", "current_bill_due_date": "2026-11-08"},
]
CATEGORIES = [
    {"id": "category-1", "name": "Mercado", "type": "expense", "kind": "custom"},
    {"id": "category-2", "name": "Transporte", "type": "expense", "kind": "custom"},
    {"id": "category-3", "name": "Moradia", "type": "expense", "kind": "system"},
    {"id": "category-4", "name": "Restaurantes", "type": "expense", "kind": "custom"},
    {"id": "category-5", "name": "Lazer", "type": "expense", "kind": "custom"},
    {"id": "category-6", "name": "Salário", "type": "income", "kind": "system"},
    {"id": "category-7", "name": "Freelance", "type": "income", "kind": "custom"},
]
TAGS = [
    {"id": "tag-1", "name": "Viagem", "color": "#38bdf8"},
    {"id": "tag-2", "name": "Essencial", "color": "#4ade80"},
    {"id": "tag-3", "name": "Reembolsável", "color": "#a78bfa"},
    {"id": "tag-4", "name": "Família", "color": "#fbbf24"},
]
EXTRA_TRANSACTIONS = [
    {**TRANSACTION, "id": "tx-2", "description": "Salário outubro", "category_id": "category-6", "amount": "8500.00", "type": "credit", "tag_ids": [], "transacted_at": "2026-10-21T12:00:00Z"},
    {**TRANSACTION, "id": "tx-3", "description": "Aluguel apartamento", "category_id": "category-3", "amount": "2400.00", "tag_ids": ["tag-2"], "transacted_at": "2026-10-20T12:00:00Z"},
    {**TRANSACTION, "id": "tx-4", "description": "Restaurante Vila Nova", "category_id": "category-4", "amount": "186.90", "tag_ids": ["tag-4"], "transacted_at": "2026-10-19T12:00:00Z"},
    {**TRANSACTION, "id": "tx-5", "description": "Uber viagem ao escritório", "category_id": "category-2", "amount": "42.80", "tag_ids": ["tag-3"], "transacted_at": "2026-10-18T12:00:00Z"},
    {**TRANSACTION, "id": "tx-6", "description": "Projeto de design", "category_id": "category-7", "amount": "2500.00", "type": "credit", "tag_ids": [], "transacted_at": "2026-10-17T12:00:00Z"},
    {**TRANSACTION, "id": "tx-7", "description": "Cinema com a família", "category_id": "category-5", "amount": "98.00", "tag_ids": ["tag-4"], "transacted_at": "2026-10-16T12:00:00Z"},
    {**TRANSACTION, "id": "tx-8", "description": "Estorno de compra", "amount": "129.90", "type": "credit", "ignored": True, "transacted_at": "2026-10-15T12:00:00Z"},
    {**TRANSACTION, "id": "tx-9", "description": "Café no mesmo dia", "amount": "10.44"},
    {**TRANSACTION, "id": "tx-10", "description": "Último dia setembro", "amount": "99.00", "transacted_at": "2026-09-30T23:59:59-03:00"},
    {**TRANSACTION, "id": "tx-11", "description": "Ainda setembro UTC", "amount": "77.00", "transacted_at": "2026-10-01T02:59:59Z"},
    {**TRANSACTION, "id": "tx-12", "description": "Primeiro dia outubro", "amount": "100.00", "transacted_at": "2026-10-01T03:00:00Z"},
    {**TRANSACTION, "id": "tx-13", "description": "Último dia outubro", "amount": "200.00", "type": "credit", "transacted_at": "2026-11-01T02:59:59Z"},
    {**TRANSACTION, "id": "tx-14", "description": "Primeiro dia novembro", "amount": "88.00", "transacted_at": "2026-11-01T03:00:00Z"},
    {**TRANSACTION, "id": "tx-15", "description": "Compra em dólares", "amount": "12.50", "currency_code": "USD"},
    {**TRANSACTION, "id": "tx-16", "description": "Data sem horário", "amount": "2.50", "transacted_at": "2026-10-22"},
    {**TRANSACTION, "id": "tx-17", "description": "Receita em dólares", "amount": "20.00", "type": "credit", "currency_code": "USD"},
]
OVERVIEW = {
    "currency_code": "BRL", "period_start": "2026-10-01", "period_end": "2026-10-31",
    "bank_balance": "30206.17", "credit_card_debt": "12900.00", "open_bills_total": "3250.00", "net_worth": "56071.60",
    "upcoming_bills": [
        {"account_id": "account-1", "description": "Nubank Ultravioleta", "amount": "2450.00", "due_date": "2026-11-02"},
        {"account_id": "account-2", "description": "Condomínio", "amount": "680.00", "due_date": "2026-11-05"},
        {"account_id": "account-1", "description": "Energia elétrica", "amount": "120.00", "due_date": "2026-11-06", "amount_is_estimate": True},
    ],
    "period": {"income": "99000.00", "expenses": "98000.00", "result": "1000.00"},
    "projection": {
        "current_balance": "30206.17", "expected_income": "1500.00", "estimated_day_to_day": "800.00", "projected_balance": "26596.17",
        "committed_outflows": {"bills": "3250.00", "installments": "650.00", "subscriptions": "289.90", "scheduled": "120.10"},
    },
}
BILLS = [
    {"id": "bill-1", "status": "open", "close_date": "2026-10-25", "due_date": "2026-11-02", "total_amount": "2450.00", "paid_amount": "0.00", "minimum_payment": "367.50", "currency_code": "BRL"},
    {"id": "bill-2", "status": "paid", "close_date": "2026-09-25", "due_date": "2026-10-02", "total_amount": "1890.40", "paid_amount": "1890.40", "currency_code": "BRL"},
    {"id": "bill-3", "status": "paid", "close_date": "2026-08-25", "due_date": "2026-09-02", "total_amount": "2120.00", "paid_amount": "2120.00", "currency_code": "BRL"},
    {"id": "bill-4", "status": "open", "close_date": "2026-10-10", "due_date": "2026-10-18", "total_amount": "520.00", "paid_amount": "0.00", "currency_code": "BRL"},
    {"id": "bill-5", "status": "open", "close_date": "2026-10-20", "due_date": "2026-10-31", "total_amount": "360.00", "paid_amount": "0.00", "currency_code": "BRL"},
]
INVESTMENTS = [
    {"id": "investment-1", "name": "Tesouro Selic 2029", "balance": "18765.43", "type": "fixed_income", "source": "connected", "provider_name": "XP Investimentos", "contributes_to_net_worth": True},
    {"id": "investment-2", "name": "CDB liquidez diária", "balance": "12000.00", "type": "fixed_income", "source": "connected", "provider_name": "Nubank", "contributes_to_net_worth": True},
    {"id": "investment-3", "name": "Carteira de ações", "balance": "8000.00", "type": "stock", "source": "manual", "provider_name": "Corretora de teste", "contributes_to_net_worth": True},
]
GOALS = [
    {"id": "goal-1", "name": "Viagem ao Japão", "target_amount": "20000.00", "current_amount": "5000.00", "progress_percent": 25.0, "status": "active", "target_date": "2027-06-01"},
    {"id": "goal-2", "name": "Reserva de emergência", "target_amount": "30000.00", "current_amount": "22500.00", "progress_percent": 75.0, "status": "active", "target_date": "2027-01-01"},
    {"id": "goal-3", "name": "Novo computador", "target_amount": "8000.00", "current_amount": "8000.00", "progress_percent": 100.0, "status": "completed", "target_date": "2026-10-01"},
    {"id": "goal-4", "name": "Entrada do apartamento", "target_amount": "100000.00", "current_amount": "12000.00", "progress_percent": 12.0, "status": "active", "target_date": "2029-12-01"},
]
SUBSCRIPTIONS = [
    {"id": "recurring-1", "name": "Internet fibra", "amount": "99.90", "frequency": "monthly", "active": True, "next_due_date": "2026-11-01", "service_name": "Vivo"},
    {"id": "recurring-2", "name": "Netflix", "amount": "55.90", "frequency": "monthly", "active": True, "next_due_date": "2026-11-03", "service_name": "Netflix"},
    {"id": "recurring-3", "name": "Spotify Família", "amount": "34.90", "frequency": "monthly", "active": True, "next_due_date": "2026-11-08", "service_name": "Spotify"},
    {"id": "recurring-4", "name": "Armazenamento na nuvem", "amount": "99.20", "frequency": "yearly", "active": False, "next_due_date": "2027-01-15"},
]
FIXED_BILLS = [
    {"id": "fixed-1", "name": "Aluguel", "amount": "2400.00", "frequency": "monthly", "active": True, "next_due_date": "2026-11-05"},
    {"id": "fixed-2", "name": "Condomínio", "amount": "680.00", "frequency": "monthly", "active": True, "next_due_date": "2026-11-05"},
    {"id": "fixed-3", "name": "Energia elétrica", "amount": "120.00", "frequency": "monthly", "active": True, "next_due_date": "2026-11-06"},
]
INSTALLMENTS = [
    {"id": "installment-1", "description": "Notebook", "installment_amount": "500.00", "total_amount": "5000.00", "remaining_amount": "3000.00", "total_installments": 10, "paid_installments": 4, "status": "active", "next_due_date": "2026-11-02"},
    {"id": "installment-2", "description": "Passagens para Salvador", "installment_amount": "150.00", "total_amount": "1800.00", "remaining_amount": "1200.00", "total_installments": 12, "paid_installments": 4, "status": "active", "next_due_date": "2026-11-05"},
    {"id": "installment-3", "description": "Sofá da sala", "installment_amount": "320.00", "total_amount": "3200.00", "remaining_amount": "0.00", "total_installments": 10, "paid_installments": 10, "status": "completed", "next_due_date": "2026-10-10"},
    {"id": "installment-4", "description": "Bicicleta", "installment_amount": "200.00", "total_amount": "2000.00", "remaining_amount": "1400.00", "total_installments": 10, "paid_installments": 3, "status": "active", "next_due_date": "2026-10-22"},
    {"id": "installment-5", "description": "Curso de idiomas", "installment_amount": "250.00", "total_amount": "3000.00", "remaining_amount": "2000.00", "total_installments": 12, "paid_installments": 4, "status": "active", "next_due_date": "2026-10-31"},
    {"id": "installment-6", "description": "Vencimento desconhecido", "installment_amount": "10.00", "status": "active"},
]
CONNECTIONS = [
    {"id": "connection-1", "provider_name": "Nubank", "status": "connected", "last_successful_sync_at": "2026-10-22T12:00:00Z", "next_auto_sync_at": "2026-10-22T18:00:00Z", "account_ids": ["account-1"]},
    {"id": "connection-2", "provider_name": "Itaú", "status": "connected", "last_successful_sync_at": "2026-10-22T11:45:00Z", "paused": True, "account_ids": ["account-2"]},
    {"id": "connection-3", "provider_name": "Banco Inter", "status": "error", "requires_reauth": True, "last_successful_sync_at": "2026-10-20T09:15:00Z", "account_ids": ["account-4"]},
]
NOTIFICATIONS = [
    {"id": "notification-1", "topic": "bill", "title": "Sua fatura fecha em 3 dias", "body": "Nubank Ultravioleta fecha em 25/10. Confira seus lançamentos antes do fechamento.", "read": False, "created_at": "2026-10-22T12:00:00Z"},
    {"id": "notification-2", "topic": "connection", "title": "Reconecte o Banco Inter", "body": "A autorização expirou. Renove a conexão para atualizar suas contas.", "read": False, "created_at": "2026-10-22T10:00:00Z"},
    {"id": "notification-3", "topic": "goal", "title": "Meta concluída", "body": "Você alcançou a meta Novo computador. Parabéns pela conquista!", "read": True, "created_at": "2026-10-21T15:00:00Z"},
]


def transaction_day(transaction):
    value = transaction["transacted_at"]
    if len(value) == 10:
        return date.fromisoformat(value)
    return datetime.fromisoformat(value.replace("Z", "+00:00")).astimezone(BRASILIA).date()


class Fixture(http.server.BaseHTTPRequestHandler):
    requests = []
    errors = []
    transactions = {}
    transaction_delay = 0

    @classmethod
    def reset(cls):
        cls.requests.clear()
        cls.errors.clear()
        cls.transactions = {item["id"]: dict(item) for item in [TRANSACTION, *EXTRA_TRANSACTIONS]}
        cls.transaction_delay = 0

    def transaction_page(self, query):
        start = date.fromisoformat(query["from_date"][0])
        end = date.fromisoformat(query["to_date"][0])
        data = [item for item in self.transactions.values() if start <= transaction_day(item) <= end]
        for field in ("account_id", "category_id", "type"):
            if field in query:
                data = [item for item in data if item.get(field) == query[field][0]]
        if "tag_id" in query:
            data = [item for item in data if query["tag_id"][0] in item.get("tag_ids", [])]
        if "ignored" in query:
            data = [item for item in data if item["ignored"] == (query["ignored"] == ["true"])]
        if "search" in query:
            data = [item for item in data if query["search"][0].casefold() in item["description"].casefold()]
        data.sort(key=lambda item: (transaction_day(item), item["id"]), reverse=True)
        offset = int(query["after"][0].split(":")[1].split("+")[0]) if "after" in query else 0
        page = data[offset:offset + 3]
        cursor = f"page:{offset + 3}+cursor" if offset + 3 < len(data) else None
        return {"data": page, "next_cursor": cursor}

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
        query = parse_qs(urlsplit(self.path).query)
        self.requests.append(("GET", self.path, None))
        if path.endswith("/transactions"):
            try:
                assert date.fromisoformat(query["from_date"][0]) <= date.fromisoformat(query["to_date"][0])
            except (KeyError, ValueError, AssertionError):
                self.errors.append(f"Transaction query requires an inclusive date range: {self.path}")
                self.respond({"error": {"message": "from_date and to_date are required"}}, 400)
                return
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
            data = OVERVIEW
        elif path.endswith("/accounts"):
            data = ACCOUNTS
        elif path.endswith("/categories"):
            data = CATEGORIES
        elif path.endswith("/tags"):
            data = TAGS
        elif path.endswith("/cards"):
            data = CARDS
        elif path.endswith("/bills"):
            if "from" not in query or "to" not in query:
                self.errors.append(f"Bill query requires due-date range: {self.path}")
                self.respond({"error": {"message": "from and to are required"}}, 400)
                return
            data = [item for item in BILLS if query["from"][0] <= item["due_date"] <= query["to"][0]]
        elif path.endswith("/investments"):
            data = INVESTMENTS
        elif path.endswith("/goals"):
            data = GOALS
        elif path.endswith("/subscriptions"):
            data = SUBSCRIPTIONS
        elif path.endswith("/fixed-bills"):
            data = FIXED_BILLS
        elif path.endswith("/installment-plans"):
            status = parse_qs(urlsplit(self.path).query).get("status", [None])[0]
            data = [item for item in INSTALLMENTS if not status or item["status"] == status]
        elif path.endswith("/connections"):
            data = CONNECTIONS
        elif path.endswith("/notifications"):
            unread = parse_qs(urlsplit(self.path).query).get("unread") == ["true"]
            data = [item for item in NOTIFICATIONS if not unread or not item["read"]]
        elif path.endswith("/transactions"):
            time.sleep(self.transaction_delay)
            self.respond(self.transaction_page(query))
            return
        elif "/transactions/" in path:
            data = self.transactions[path.rsplit("/", 1)[1]]
        else:
            data = []
        self.respond({"data": data, "next_cursor": None})

    def mutate(self):
        body = self.rfile.read(int(self.headers.get("Content-Length", "0")))
        payload = json.loads(body) if body else None
        self.requests.append((self.command, self.path, payload))
        transaction_id = urlsplit(self.path).path.rsplit("/", 1)[1]
        if self.command == "POST":
            transaction_id = "tx-created"
            self.transactions[transaction_id] = {**TRANSACTION, "id": transaction_id, **payload}
        elif self.command == "DELETE":
            self.transactions.pop(transaction_id)
            self.respond({"data": None})
            return
        else:
            self.transactions[transaction_id].update(payload)
        self.respond({"data": self.transactions[transaction_id]})

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
        env.pop("NO_COLOR", None)
        env.update(
            VISOR_CONFIG_DIR=config_dir,
            VISOR_TOKEN="visor_pat_mock_fixture_only",
            VISOR_SPACE="smoke",
            VISOR_API_URL=f"http://127.0.0.1:{server.server_port}",
            VISOR_AUTH_URL=f"http://127.0.0.1:{server.server_port}",
            VISOR_CREDENTIAL_STORE="file",
            VISOR_DEBUG="0", NO_PROXY="127.0.0.1,localhost", no_proxy="127.0.0.1,localhost",
            TERM="xterm-256color", COLORTERM="truecolor", FORCE_COLOR="3",
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

    def wait_until(self, predicate, message, timeout=5):
        deadline = time.monotonic() + timeout
        while not predicate() and time.monotonic() < deadline:
            self.read(0.05)
        assert predicate(), f"{message} at {self.columns}x{self.rows}:\n{self.text()}"

    def send(self, keys):
        os.write(self.master, keys.encode())
        self.read()

    def text(self):
        return "\n".join(self.screen.display)

    def body_cells(self):
        return tuple(
            self.screen.buffer[row][column]
            for row in range(2, self.rows - 2)
            for column in range(self.columns)
        )

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
            stem = directory / f"{self.columns}x{self.rows}-{name}"
            stem.with_suffix(".txt").write_text(self.text() + "\n")
            stem.with_suffix(".svg").write_text(self.svg())

    def svg(self):
        palette = {
            "black": "#000000", "red": "#cc5555", "green": "#55cc55",
            "brown": "#cdcd55", "blue": "#5555cc", "magenta": "#cc55cc",
            "cyan": "#55cccc", "white": "#cccccc", "brightblack": "#555555",
            "brightred": "#ff5555", "brightgreen": "#55ff55", "brightbrown": "#ffff55",
            "brightblue": "#5555ff", "brightmagenta": "#ff55ff",
            "brightcyan": "#55ffff", "brightwhite": "#ffffff",
        }

        def color(value, default):
            if value == "default":
                return default
            return palette.get(value, f"#{value}")

        cell_width, cell_height, padding = 8.4, 18, 16
        width = self.columns * cell_width + 2 * padding
        height = self.rows * cell_height + 2 * padding
        output = [
            f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">',
            '<rect width="100%" height="100%" rx="10" fill="#0d1117"/>',
            '<g font-family="DejaVu Sans Mono,monospace" font-size="14" xml:space="preserve" shape-rendering="crispEdges">',
        ]
        for row in range(self.rows):
            for column in range(self.columns):
                cell = self.screen.buffer[row][column]
                foreground = color(cell.fg, "#c9d1d9")
                background = color(cell.bg, "#0d1117")
                if cell.reverse:
                    foreground, background = background, foreground
                x = padding + column * cell_width
                y = padding + row * cell_height
                output.append(f'<rect x="{x:g}" y="{y:g}" width="{cell_width}" height="{cell_height}" fill="{background}"/>')
                if cell.data.strip():
                    weight = ' font-weight="bold"' if cell.bold else ""
                    output.append(f'<text x="{x:g}" y="{y + 14:g}" fill="{foreground}"{weight}>{escape(cell.data)}</text>')
        output.extend(["</g>", "</svg>"])
        return "\n".join(output) + "\n"

    def close(self):
        if self.process.poll() is None:
            self.process.send_signal(signal.SIGTERM)
            try:
                self.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
        os.close(self.master)


def transaction_requests(space=None):
    return [
        request for request in Fixture.requests
        if urlsplit(request[1]).path.endswith("/transactions")
        and request[0] == "GET"
        and (space is None or f"/spaces/{space}/" in request[1])
    ]


def expect_last_range(expected, space=None):
    query = parse_qs(urlsplit(transaction_requests(space)[-1][1]).query)
    actual = query["from_date"][0], query["to_date"][0]
    assert actual == expected, (actual, expected)


def choose_preset(terminal, index):
    terminal.send("p")
    terminal.expect("Personalizado")
    terminal.send("k" * 5 + "j" * index + "\r")


def custom_period(terminal, start, end):
    choose_preset(terminal, 4)
    terminal.send(f"\x01{start}\t\x01{end}\r")


def october(terminal):
    custom_period(terminal, *OCTOBER)
    terminal.send("[")
    terminal.send("]")
    terminal.expect("Outubro 2026")


def check_periods(terminal, captures):
    today = datetime.now(BRASILIA).date()
    current = (today.replace(day=1).isoformat(), today.replace(day=calendar.monthrange(today.year, today.month)[1]).isoformat())
    expect_last_range(current)
    terminal.expect(f"{MONTHS[today.month - 1]} {today.year}")
    terminal.send("2")
    custom_period(terminal, "01/10/2026", "31/10/2026")
    terminal.wait_until(lambda: "14 lançamentos" in terminal.text(), "Selected period did not load every page")
    for text in ("Receitas", "Despesas", "Saldo", "11.200,00", "4.075,20", "7.124,80", "USD"):
        terminal.expect(text)
    assert "Várias moedas" not in terminal.text(), terminal.text()
    if terminal.columns >= 120:
        for text in ("Categoria", "Conta", "31/10", "22/10"):
            terminal.expect(text)
    terminal.capture(captures, "transactions-period")
    queries = [parse_qs(urlsplit(request[1]).query) for request in transaction_requests()]
    october_pages = [query for query in queries if query.get("from_date") == [OCTOBER[0]] and query.get("to_date") == [OCTOBER[1]]]
    assert {query.get("after", [None])[0] for query in october_pages} >= {None, "page:3+cursor", "page:6+cursor", "page:9+cursor", "page:12+cursor"}, october_pages
    terminal.send("[")
    terminal.expect("Setembro 2026")
    terminal.expect("2 lançamentos")
    expect_last_range(("2026-09-01", "2026-09-30"))
    count = len(transaction_requests())
    terminal.send("]")
    terminal.expect("Outubro 2026")
    terminal.expect("14 lançamentos")
    assert len(transaction_requests()) == count, "Returning to a complete month refetched its pages"
    terminal.send("]")
    terminal.expect("Novembro 2026")
    terminal.expect("Primeiro dia novembro")
    expect_last_range(("2026-11-01", "2026-11-30"))
    terminal.send("[")
    for previous, following in (("h", "l"), ("\x1b[D", "\x1b[C")):
        terminal.send(previous)
        terminal.expect("Setembro 2026")
        terminal.send(following)
        terminal.expect("Outubro 2026")
    count = len(Fixture.requests)
    terminal.send("m")
    assert len(Fixture.requests) == count, "Removed load-more key made a request"
    assert "carrega mais" not in terminal.text(), terminal.text()
    terminal.send("p")
    terminal.capture(captures, "period-picker")
    for name in ("Mês", "Semana", "Trimestre", "Ano", "Personalizado"):
        terminal.expect(name)
    terminal.send("\x1b")
    assert len(Fixture.requests) == count, "Cancelling the period picker made a request"
    for start, end in (("32/13/2026", "31/10/2026"), ("2026-10-31", "2026-10-01"), ("[]Tp", "2026-10-31")):
        custom_period(terminal, start, end)
        assert len(Fixture.requests) == count, "Invalid range or picker typing escaped into navigation"
        terminal.capture(captures, "period-invalid")
        terminal.send("\x1b")
        terminal.expect("Outubro 2026")
    for index, expected in ((1, ("2026-09-28", "2026-10-04")), (2, ("2026-10-01", "2026-12-31")), (3, ("2026-01-01", "2026-12-31"))):
        october(terminal)
        choose_preset(terminal, index)
        expect_last_range(expected)
    custom_period(terminal, *OCTOBER)
    choose_preset(terminal, 0)
    terminal.expect("Outubro 2026")
    terminal.send("T")
    terminal.expect(f"{MONTHS[today.month - 1]} {today.year}")
    october(terminal)
    count = len(Fixture.requests)
    terminal.send("/[]Tp")
    terminal.expect("[]Tp")
    terminal.send("\x1b")
    assert len(Fixture.requests) == count, "Search typing changed the period"
    terminal.expect("Outubro 2026")
    terminal.send("/Café\r")
    terminal.expect("1 lançamentos")
    terminal.send("1")
    for value in ("11.200,00", "4.075,20", "7.124,80"):
        terminal.expect(value)
    terminal.send("2")
    terminal.expect("1 lançamentos")
    terminal.send("c")
    terminal.expect("14 lançamentos")
    terminal.expect("Outubro 2026")
    for name in ("overview", "cash-flow", "categories", "bills", "payables"):
        key, title = SCREENS[name]
        terminal.send(key)
        terminal.expect(title)
        terminal.expect("Outubro 2026")
        terminal.capture(captures, f"period-{name}")
        if name in {"overview", "cash-flow"}:
            for value in ("11.200,00", "4.075,20", "7.124,80"):
                terminal.expect(value)
        if name == "bills":
            query = parse_qs(urlsplit(next(request[1] for request in reversed(Fixture.requests) if "/cards/" in request[1] and "/bills?" in request[1])).query)
            assert query.get("from") == [OCTOBER[0]] and query.get("to") == [OCTOBER[1]], query
        if name == "payables":
            terminal.expect("Bicicleta")
            assert "Notebook" not in terminal.text(), "Payables included a November due date in October"
    terminal.send("=s")
    terminal.expect("espaço")
    terminal.send("j\r")
    terminal.send("2")
    expect_last_range(OCTOBER, "smoke-alt")
    terminal.expect("Outubro 2026")
    terminal.expect("14 lançamentos")
    terminal.capture(captures, "period-switched-space")
    terminal.send("=s")
    terminal.send("k\r")
    terminal.send("2")
    terminal.expect("Outubro 2026")
    assert not Fixture.errors, Fixture.errors
    assert all(request[0] == "GET" for request in Fixture.requests), "Period checks made a write"


def capture_screens(terminal, captures, screens, verify_navigation=True):
    if terminal.columns >= 110:
        terminal.click("5 Contas")
        terminal.expect("Contas e Cartões")
    for name in screens:
        key, title = SCREENS[name]
        terminal.send(key)
        terminal.expect(title)
        terminal.send("j" + "k" * 40)
        terminal.capture(captures, name)
        request_count = len(Fixture.requests)
        top = terminal.body_cells()
        terminal.send("j")
        moved = terminal.body_cells()
        if verify_navigation and name in RECORD_SCREENS:
            assert moved != top, f"j did not select the next record on {name}"
            terminal.send("k")
            assert terminal.body_cells() == top, f"k did not restore the first record on {name}"
        terminal.send("j" * 40)
        end = terminal.body_cells()
        if end != top:
            terminal.capture(captures, f"{name}-end")
            terminal.send("k")
            if verify_navigation:
                assert terminal.body_cells() != end, f"Repeated down keys prevented moving up on {name}"
        assert len(Fixture.requests) == request_count, f"Navigation made a request on {name}"
        terminal.send("k" * 40)
        if name == "recurring":
            terminal.send("\x1b[C")
            terminal.capture(captures, "recurring-fixed-bills")
            terminal.send("\x1b[D")
        if name == "bills":
            terminal.send("\x1b[C")
            terminal.capture(captures, "bills-next-card")
            assert any("/cards/card-2/bills" in request[1] for request in Fixture.requests), "Arrow did not select next card"
            terminal.send("\x1b[D")


def run(binary, captures, baseline=False, check_ttl=False, visual_only=False, screens=None, sizes=None):
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Fixture)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        for columns, rows in sizes or [(120, 40), (80, 24), (60, 18)]:
            Fixture.reset()
            with tempfile.TemporaryDirectory(prefix="visor-ux-") as config:
                terminal = Terminal(binary, config, server, columns, rows)
                try:
                    if not baseline:
                        check_periods(terminal, captures)
                    if visual_only:
                        capture_screens(terminal, captures, screens or SCREENS, verify_navigation=not baseline)
                        assert all(request[0] == "GET" for request in Fixture.requests), "Visual review made a write"
                        terminal.send("q")
                        terminal.process.wait(timeout=3)
                        assert terminal.process.returncode == 0
                        checks = "baseline captures" if baseline else "period controls, complete ranges, keyboard navigation"
                        print(f"PASS {columns}x{rows}: rendered {len(screens or SCREENS)} screens, {checks}, no writes", flush=True)
                        continue
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
                    terminal.send("/Conta qn?12\r")
                    terminal.send("e")
                    terminal.expect("Editar")
                    terminal.send("\x1525,10\x13")
                    writes = [r for r in Fixture.requests if r[0] == "PATCH"]
                    assert len(writes) == 1 and writes[0][2]["amount"] == "25.10", writes
                    assert urlsplit(writes[0][1]).path.endswith("/transactions/tx-created"), writes
                    assert not list(Path(config).glob("transactions_*.csv")), "e also exported"
                    terminal.send("c")
                    terminal.send("/")
                    query = "mercado & café + #"
                    terminal.send(query)
                    terminal.send("\r")
                    searches = [parse_qs(urlsplit(r[1]).query).get("search") for r in Fixture.requests]
                    assert [query] in searches, searches
                    terminal.send("c")
                    terminal.send("f\rConta\r\t\rTrans\r\t\rViagem\r\t\r\t\r\tcoffee & tea\x13")
                    filtered = next(parse_qs(urlsplit(r[1]).query) for r in reversed(Fixture.requests) if "/transactions?" in r[1])
                    for key, value in {"account_id": "account-1", "category_id": "category-2", "tag_id": "tag-1", "type": "credit", "ignored": "true", "search": "coffee & tea"}.items():
                        assert filtered.get(key) == [value], (key, filtered)
                    terminal.capture(captures, "active-filters")
                    terminal.send("E")
                    exports = list(Path(config).glob("transactions_*.csv"))
                    assert len(exports) == 1 and "Mock transaction,18.90" in exports[0].read_text(), "CSV export failed"
                    export_query = parse_qs(urlsplit(Fixture.requests[-1][1]).query)
                    assert export_query == {k: v for k, v in filtered.items() if k not in {"limit", "after"}}, "CSV lost active filters"
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
                        terminal.send("qn?123[]Tp")
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
                            top = terminal.body_cells()
                            terminal.send("j" * 40)
                            assert len(Fixture.requests) == request_count, "Scrolling made a request"
                            terminal.capture(captures, f"scrolled-{key.replace('=', 'settings')}")
                            bottom = terminal.body_cells()
                            terminal.send("k")
                            if bottom != top:
                                assert terminal.body_cells() != bottom, "Repeated down keys prevented moving up from the last row"
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
                    assert not Fixture.errors, Fixture.errors
                    print(f"PASS {columns}x{rows}: periods, complete ranges, forms, filters, CSV, confirmation, refresh, idle, scrolling, space switch", flush=True)
                finally:
                    terminal.close()
    finally:
        server.shutdown()
        server.server_close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/visor-tui")
    parser.add_argument("--captures", type=Path)
    parser.add_argument("--baseline", action="store_true", help="Capture pre-redesign visuals without selection assertions; in full smoke reproduce the old form bug")
    parser.add_argument("--check-ttl", action="store_true", help="Idle beyond the 60-second cache TTL at 120x40")
    parser.add_argument("--visual-only", action="store_true", help="Render and navigate screens without exercising mutations")
    parser.add_argument("--screens", help=f"Comma-separated visual review screens: {','.join(SCREENS)}")
    parser.add_argument("--sizes", help="Comma-separated terminal sizes, for example 120x40,80x24 (default also includes 60x18)")
    args = parser.parse_args()
    screens = args.screens.split(",") if args.screens else list(SCREENS)
    unknown = set(screens) - SCREENS.keys()
    if unknown:
        parser.error(f"Unknown screens: {', '.join(sorted(unknown))}")
    if args.screens and not args.visual_only:
        parser.error("--screens requires --visual-only")
    sizes = None
    if args.sizes:
        try:
            sizes = [tuple(int(value) for value in size.split("x")) for size in args.sizes.split(",")]
            if any(len(size) != 2 or min(size) < 10 for size in sizes):
                raise ValueError
        except ValueError:
            parser.error("--sizes must contain WIDTHxHEIGHT pairs of at least 10 cells")
    run(args.binary.resolve(), args.captures, args.baseline, args.check_ttl, args.visual_only, screens, sizes)
