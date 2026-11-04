"""SQLite storage for the response cache, the usage ledger and finished audits."""
from __future__ import annotations

import json
import sqlite3
import threading
import time
from pathlib import Path
from typing import Any

SCHEMA = """
CREATE TABLE IF NOT EXISTS cache (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    ts    REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS ledger (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    ts      REAL NOT NULL,
    service TEXT NOT NULL,
    units   REAL NOT NULL,
    unit    TEXT NOT NULL,
    cost    REAL NOT NULL,
    note    TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS ledger_service ON ledger(service);
CREATE INDEX IF NOT EXISTS ledger_ts ON ledger(ts);
CREATE TABLE IF NOT EXISTS audits (
    id           TEXT PRIMARY KEY,
    created      REAL NOT NULL,
    updated      REAL NOT NULL,
    status       TEXT NOT NULL,
    language     TEXT NOT NULL,
    prompt       TEXT NOT NULL,
    target       TEXT NOT NULL,
    depth        TEXT NOT NULL,
    score        REAL,
    request      TEXT NOT NULL,
    report       TEXT,
    explanations TEXT,
    error        TEXT
);
CREATE INDEX IF NOT EXISTS audits_created ON audits(created DESC);
"""

_AUDIT_JSON = ("request", "report", "explanations")
_AUDIT_COLUMNS = {"status", "language", "score", "report", "explanations", "error"}


class Store:
    def __init__(self, path: Path | str):
        self.path = str(path)
        if self.path != ":memory:":
            Path(self.path).parent.mkdir(parents=True, exist_ok=True)
        self._db = sqlite3.connect(self.path, check_same_thread=False, isolation_level=None)
        self._db.row_factory = sqlite3.Row
        self._lock = threading.RLock()
        with self._lock:
            if self.path != ":memory:":
                self._db.execute("PRAGMA journal_mode=WAL")
            self._db.executescript(SCHEMA)

    def close(self) -> None:
        with self._lock:
            self._db.close()

    # --- cache ---------------------------------------------------------

    def cache_get(self, key: str) -> str | None:
        with self._lock:
            row = self._db.execute("SELECT value FROM cache WHERE key = ?", (key,)).fetchone()
        return row["value"] if row else None

    def cache_put(self, key: str, value: str) -> None:
        with self._lock:
            self._db.execute(
                "INSERT OR REPLACE INTO cache (key, value, ts) VALUES (?, ?, ?)",
                (key, value, time.time()),
            )

    def cache_prune(self, max_age_days: float = 30) -> int:
        cutoff = time.time() - max_age_days * 86400
        with self._lock:
            cur = self._db.execute("DELETE FROM cache WHERE ts < ?", (cutoff,))
            return cur.rowcount

    # --- usage ledger --------------------------------------------------

    def ledger_add(self, service: str, units: float, unit: str, cost: float, note: str = "") -> None:
        with self._lock:
            self._db.execute(
                "INSERT INTO ledger (ts, service, units, unit, cost, note) VALUES (?, ?, ?, ?, ?, ?)",
                (time.time(), service, units, unit, cost, note),
            )

    def ledger_totals(self) -> dict[str, dict[str, Any]]:
        with self._lock:
            rows = self._db.execute(
                "SELECT service, SUM(units) AS units, MAX(unit) AS unit, SUM(cost) AS cost, COUNT(*) AS calls "
                "FROM ledger GROUP BY service"
            ).fetchall()
        return {r["service"]: dict(units=r["units"], unit=r["unit"], cost=r["cost"], calls=r["calls"]) for r in rows}

    def ledger_cost_since(self, ts: float) -> float:
        with self._lock:
            row = self._db.execute("SELECT COALESCE(SUM(cost), 0) AS c FROM ledger WHERE ts >= ?", (ts,)).fetchone()
        return float(row["c"])

    def ledger_first_ts(self) -> float | None:
        with self._lock:
            row = self._db.execute("SELECT MIN(ts) AS t FROM ledger").fetchone()
        return row["t"]

    # --- audits --------------------------------------------------------

    def audit_create(self, audit_id: str, *, language: str, prompt: str, target: str, depth: str, request: dict) -> None:
        now = time.time()
        with self._lock:
            self._db.execute(
                "INSERT INTO audits (id, created, updated, status, language, prompt, target, depth, request) "
                "VALUES (?, ?, ?, 'queued', ?, ?, ?, ?, ?)",
                (audit_id, now, now, language, prompt, target, depth, json.dumps(request, ensure_ascii=False)),
            )

    def audit_update(self, audit_id: str, **fields: Any) -> None:
        sets, values = ["updated = ?"], [time.time()]
        for name, value in fields.items():
            if name not in _AUDIT_COLUMNS:
                raise KeyError(name)
            if name in _AUDIT_JSON and value is not None:
                value = json.dumps(value, ensure_ascii=False)
            sets.append(f"{name} = ?")
            values.append(value)
        values.append(audit_id)
        with self._lock:
            self._db.execute(f"UPDATE audits SET {', '.join(sets)} WHERE id = ?", values)

    def audit_get(self, audit_id: str) -> dict[str, Any] | None:
        with self._lock:
            row = self._db.execute("SELECT * FROM audits WHERE id = ?", (audit_id,)).fetchone()
        if row is None:
            return None
        out = dict(row)
        for name in _AUDIT_JSON:
            if out.get(name):
                out[name] = json.loads(out[name])
        return out

    def audit_list(self, limit: int = 30) -> list[dict[str, Any]]:
        with self._lock:
            rows = self._db.execute(
                "SELECT id, created, status, language, prompt, target, depth, score FROM audits "
                "ORDER BY created DESC LIMIT ?",
                (limit,),
            ).fetchall()
        return [dict(r) for r in rows]

    def audit_delete(self, audit_id: str) -> bool:
        with self._lock:
            cur = self._db.execute("DELETE FROM audits WHERE id = ?", (audit_id,))
            return cur.rowcount > 0

    def audit_fail_orphans(self) -> int:
        """Anything still 'running' at startup was cut off by a restart."""
        with self._lock:
            cur = self._db.execute(
                "UPDATE audits SET status = 'failed', error = 'The server restarted while this audit was running.', "
                "updated = ? WHERE status IN ('queued', 'running')",
                (time.time(),),
            )
            return cur.rowcount
