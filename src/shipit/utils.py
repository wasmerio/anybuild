from pathlib import Path

import requests


def download_file(url: str, path: Path) -> None:
    response = requests.get(url)
    response.raise_for_status()
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(response.content)
