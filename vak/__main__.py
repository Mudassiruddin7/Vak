"""`python -m vak` (or just `vak`) starts the web app."""
from __future__ import annotations

import logging

import uvicorn

from .config import Settings


def main() -> None:
    settings = Settings.from_env()
    logging.basicConfig(level=logging.INFO, format="%(levelname)s  %(message)s")
    if not settings.has_key:
        print("\n  No SARVAM_API_KEY found. Put it in a .env file next to the project (see .env.example).\n")
    print(f"\n  Vak is up at http://{settings.host}:{settings.port}\n")
    uvicorn.run("vak.app:create_app", factory=True, host=settings.host, port=settings.port, log_level="warning")


if __name__ == "__main__":
    main()
