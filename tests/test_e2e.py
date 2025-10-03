import os
import signal
import asyncio
import subprocess
import re
from pathlib import Path
from typing import List, NamedTuple
from dataclasses import dataclass

import pytest
import shutil
import contextlib
import aiohttp


@dataclass(frozen=True)
class HTTPRequest:
    path: str
    body_match: str
    method: str = "GET"


class E2ECase(NamedTuple):
    path: str
    serve_pattern: str
    http: List[HTTPRequest]


@pytest.mark.e2e
@pytest.mark.asyncio
@pytest.mark.parametrize(
    "case",
    [
        E2ECase(
            path="examples/php-nobuild",
            serve_pattern=(
                r"PHP 8\.3\.[0-9]+ Development Server \(http://localhost:8080\) started"
            ),
            http=[HTTPRequest(path="/", body_match=r"PHP Version 8\.3\.[0-9]+")],
        )
    ],
)
async def test_end_to_end(case: E2ECase):
    # Skip if `uv` is not available in PATH
    if not shutil.which("uv"):
        pytest.skip("`uv` is not available in PATH")

    repo_root = Path(__file__).resolve().parents[1]

    cmd = [
        "uv",
        "run",
        "shipit-cli",
        case.path,
        "--skip-prepare",
        "--start",
        "--regenerate",
    ]

    build_phrase = "Build complete ✅"
    serve_re = re.compile(case.serve_pattern)

    # Start process in a new session/process group to simplify termination.
    start_new_session = os.name != "nt"
    creationflags = (
        subprocess.CREATE_NEW_PROCESS_GROUP if os.name == "nt" else 0
    )

    proc = await asyncio.create_subprocess_exec(
        *cmd,
        cwd=str(repo_root),
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.STDOUT,
        start_new_session=start_new_session,
        creationflags=creationflags,
    )

    output_lines: List[str] = []
    found_build = asyncio.Event()
    found_serve = asyncio.Event()

    async def reader() -> None:
        assert proc.stdout is not None
        while True:
            line_b = await proc.stdout.readline()
            if not line_b:
                break
            line = line_b.decode("utf-8", errors="replace")
            output_lines.append(line)
            if (not found_build.is_set()) and (build_phrase in line):
                found_build.set()
            if (not found_serve.is_set()) and serve_re.search(line):
                found_serve.set()

    reader_task = asyncio.create_task(reader())

    try:
        await asyncio.wait_for(
            asyncio.gather(found_build.wait(), found_serve.wait()),
            timeout=180,
        )
    except asyncio.TimeoutError:
        # We'll handle assertion below after cleanup
        pass

    # If we saw the serve banner, exercise the HTTP endpoint before shutting
    # down to ensure it actually serves content.
    if found_serve.is_set():
        for req in case.http:
            ok = await _wait_for_http_contains(
                host="localhost",
                port=8080,
                method=req.method,
                path=req.path,
                pattern=req.body_match,
                timeout=20.0,
            )
            if not ok:
                full_output = "".join(output_lines)
                pytest.fail(
                    "Server did not return expected HTTP content.\n\n"
                    f"Expected body regex: '{req.body_match}' at path '{req.path}'\n\n"
                    f"--- Captured output start ---\n{full_output}\n"
                    "--- Captured output end ---"
                )

    # Terminate the server no matter what
    try:
        if os.name != "nt":
            os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
        else:
            proc.terminate()
    except Exception:
        pass

    # Wait briefly for process to exit, then force kill if needed
    try:
        await asyncio.wait_for(proc.wait(), timeout=10)
    except asyncio.TimeoutError:
        try:
            if os.name != "nt":
                os.killpg(os.getpgid(proc.pid), signal.SIGKILL)
            else:
                proc.kill()
        except Exception:
            pass

    # Ensure reader task is finished
    if not reader_task.done():
        reader_task.cancel()
        with contextlib.suppress(asyncio.CancelledError):
            await reader_task

    full_output = "".join(output_lines)

    if not (found_build.is_set() and found_serve.is_set()):
        code = proc.returncode
        pytest.fail(
            "End-to-end run did not reach expected state.\n"
            f"returncode={code}\n"
            f"Saw build={found_build.is_set()} serve={found_serve.is_set()}\n\n"
            f"--- Captured output start ---\n{full_output}\n--- Captured output end ---"
        )

    assert build_phrase in full_output
    assert serve_re.search(full_output), "Serve banner regex not found in output"


async def _wait_for_http_contains(
    host: str,
    port: int,
    method: str = "GET",
    path: str = "/",
    pattern: str = "",
    timeout: float = 15.0,
) -> bool:
    url = f"http://{host}:{port}{path}"
    loop = asyncio.get_running_loop()
    end = loop.time() + timeout
    async with aiohttp.ClientSession(
        timeout=aiohttp.ClientTimeout(total=5.0)
    ) as session:
        while loop.time() < end:
            try:
                async with session.request(method, url) as resp:
                    text = await resp.text()
                    if re.search(pattern, text):
                        return True
            except Exception:
                # Not ready yet; retry shortly.
                pass
            await asyncio.sleep(0.2)
    return False
