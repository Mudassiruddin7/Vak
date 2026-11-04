"""In-memory job board. Each running audit keeps its events so a browser that drops
the connection can reconnect and pick up where it left off.
"""
from __future__ import annotations

import asyncio
import json
from collections.abc import AsyncIterator
from typing import Any

HEARTBEAT_SECONDS = 15.0


def sse_frame(seq: int, event: str, data: Any) -> str:
    payload = json.dumps(data, ensure_ascii=False)
    return f"id: {seq}\nevent: {event}\ndata: {payload}\n\n"


class Job:
    def __init__(self, job_id: str):
        self.id = job_id
        self.events: list[tuple[str, Any]] = []
        self.finished = False
        self.task: asyncio.Task | None = None
        self._cond = asyncio.Condition()

    async def emit(self, event: str, data: Any = None) -> None:
        async with self._cond:
            self.events.append((event, data if data is not None else {}))
            self._cond.notify_all()

    async def close(self) -> None:
        async with self._cond:
            self.finished = True
            self._cond.notify_all()

    async def stream(self, start: int = 0) -> AsyncIterator[str]:
        """SSE frames from event number `start` on. Sends a comment now and then to keep proxies awake."""
        index = start
        while True:
            async with self._cond:
                try:
                    await asyncio.wait_for(
                        self._cond.wait_for(lambda: index < len(self.events) or self.finished),
                        timeout=HEARTBEAT_SECONDS,
                    )
                except TimeoutError:
                    yield ": still working\n\n"
                    continue
                pending = self.events[index:]
                done = self.finished and index + len(pending) >= len(self.events)
            for offset, (event, data) in enumerate(pending):
                yield sse_frame(index + offset, event, data)
            index += len(pending)
            if done and index >= len(self.events):
                return


class JobBoard:
    def __init__(self, keep: int = 40):
        self._jobs: dict[str, Job] = {}
        self._keep = keep

    def create(self, job_id: str) -> Job:
        job = Job(job_id)
        self._jobs[job_id] = job
        finished = [k for k, j in self._jobs.items() if j.finished]
        for old in finished[: max(0, len(self._jobs) - self._keep)]:
            self._jobs.pop(old, None)
        return job

    def get(self, job_id: str) -> Job | None:
        return self._jobs.get(job_id)
