import os
import random
import socket
import signal
import asyncio
import subprocess
import re
from pathlib import Path
from typing import List, NamedTuple
from dataclasses import dataclass

import pytest
import shlex
import shutil
import contextlib
import aiohttp
from enum import Enum


class BuildMode(Enum):
    Wasmer = "wasmer"
    WasmerAndDocker = "docker"
    Local = "local"


@dataclass(frozen=True)
class HTTPRequest:
    path: str
    body_match: str
    method: str = "GET"


class E2ECase(NamedTuple):
    path: str
    serve_pattern: str
    http: List[HTTPRequest]
    use_random_port: bool = True

    def __str__(self):
        return self.path

    def __repr__(self):
        return self.path


@pytest.mark.e2e
@pytest.mark.asyncio
@pytest.mark.parametrize(
    "case",
    [
        # Simple PHP site that calls phpinfo()
        E2ECase(
            path="examples/cdn",
            serve_pattern=r"server is listening on",
            http=[HTTPRequest(path="/", body_match=r"My CDN")],
        ),
        # Simple PHP site that calls phpinfo()
        E2ECase(
            path="examples/php-nobuild",
            serve_pattern=(
                r"PHP 8\.3\.[0-9]+ Development Server \(http://localhost:[\d]+\) started"
            ),
            http=[HTTPRequest(path="/", body_match=r"PHP Version 8\.3\.[0-9]+")],
        ),
        # Simple PHP site that calls phpinfo() with no port
        E2ECase(
            path="examples/php-nobuild",
            serve_pattern=(
                r"PHP 8\.3\.[0-9]+ Development Server \(http://localhost:[\d]+\) started"
            ),
            http=[HTTPRequest(path="/", body_match=r"PHP Version 8\.3\.[0-9]+")],
        ),
        # PHP API example with JSON at / and greeting endpoint
        E2ECase(
            path="examples/php-api",
            serve_pattern=(
                r"PHP 8\.3\.[0-9]+ Development Server \(http://localhost:[\d]+\) started"
            ),
            http=[
                HTTPRequest(path="/", body_match=r"\"version\"\s*:\s*\"8\.3\.[0-9]+\""),
                HTTPRequest(path="/api/greet/Alice", body_match=r"Hello, Alice!"),
            ],
        ),
        # WordPress skeleton that echoes a simple string
        E2ECase(
            path="examples/php-wordpress",
            serve_pattern=(
                r"PHP 8\.3\.[0-9]+ Development Server \(http://localhost:[\d]+\) started"
            ),
            http=[HTTPRequest(path="/", body_match=r"WordPress")],
        ),
        # Static site copied as-is (no build step beyond copy)
        E2ECase(
            path="examples/static-nobuild",
            # static-web-server banner varies; rely on HTTP check with generous pattern
            serve_pattern=r"server is listening on",
            http=[HTTPRequest(path="/", body_match=r"Test")],
        ),
        # Staticfile provider serving content under site/
        E2ECase(
            path="examples/staticfile",
            serve_pattern=r"server is listening on",
            http=[HTTPRequest(path="/", body_match=r"Hello from static site!")],
        ),
        # Hugo static site (built via Hugo, served with static-web-server)
        E2ECase(
            path="examples/hugo",
            serve_pattern=r"server is listening on",
            http=[HTTPRequest(path="/", body_match=r"My New Hugo Site")],
        ),
        # MkDocs site (built with mkdocs, served with static-web-server)
        E2ECase(
            path="examples/mkdocs",
            serve_pattern=r"server is listening on",
            http=[HTTPRequest(path="/", body_match=r"Welcome to MkDocs")],
        ),
        # MkDocs with plugins
        E2ECase(
            path="examples/mkdocs-with-plugins",
            serve_pattern=r"server is listening on",
            http=[HTTPRequest(path="/", body_match=r"Welcome to MkDocs with Plugins")],
        ),
        # Python FastAPI app on Uvicorn
        E2ECase(
            path="examples/python-fastapi",
            serve_pattern=r"Uvicorn running on .*",
            http=[HTTPRequest(path="/", body_match=r"Hello World from fastapi!")],
        ),
        # Python Flask app served via Uvicorn WSGI
        E2ECase(
            path="examples/python-flask",
            serve_pattern=r"Uvicorn running on .*",
            http=[HTTPRequest(path="/", body_match=r"Welcome to Flask")],
        ),
        # Python Django via Uvicorn WSGI (check admin login)
        E2ECase(
            path="examples/python-django",
            serve_pattern=r"Uvicorn running on .*",
            http=[HTTPRequest(path="/", body_match=r"Django")],
        ),
        # Python ffmpeg demo (FastAPI), homepage is static HTML form
        E2ECase(
            path="examples/python-ffmpeg",
            serve_pattern=r"Uvicorn running on .*",
            http=[HTTPRequest(path="/", body_match=r"Take screenshot at 1s")],
        ),
        # Python Pillow demo (FastAPI), homepage has form title
        E2ECase(
            path="examples/python-pillow",
            serve_pattern=r"Uvicorn running on .*",
            http=[HTTPRequest(path="/", body_match=r"Image Crop\s*&\s*Rotate")],
        ),
        # Python Pandoc demo: app may require pandoc binary; only assert serve started
        E2ECase(
            path="examples/python-pandoc",
            serve_pattern=r"Uvicorn running on .*",
            http=[],
        ),
        # Python Procfile demo using python -m http.server
        E2ECase(
            path="examples/python-procfile",
            serve_pattern=r"Serving HTTP on .*",
            http=[HTTPRequest(path="/", body_match=r"Test")],
        ),
        # Python Streamlit app
        E2ECase(
            path="examples/python-streamlit",
            serve_pattern=r".*You can now view your Streamlit app in your browser.*",
            http=[HTTPRequest(path="/", body_match=r"Streamlit")],
        ),
    ],
    ids=lambda c: str(c),
)
@pytest.mark.flaky(reruns=2, reruns_delay=2)
@pytest.mark.parametrize(
    "build_mode",
    [
        BuildMode.Local,
        BuildMode.Wasmer,
        BuildMode.WasmerAndDocker,
    ],
)
async def test_end_to_end(case: E2ECase, build_mode: BuildMode):
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
        # "--wasmer",
        # "--docker",
        "--regenerate",
    ]
    if build_mode == BuildMode.Wasmer:
        cmd.append("--wasmer")
        cmd.append("--wasmer-registry=wasmer.io")
    elif build_mode == BuildMode.WasmerAndDocker:
        cmd.append("--wasmer")
        cmd.append("--wasmer-registry=wasmer.io")
        cmd.append("--docker")
    elif build_mode == BuildMode.Local:
        # The default
        pass

    build_phrase = "Build complete ✅"
    serve_re = re.compile(case.serve_pattern)

    # Start process in a new session/process group to simplify termination.
    start_new_session = os.name != "nt"
    creationflags = subprocess.CREATE_NEW_PROCESS_GROUP if os.name == "nt" else 0

    env = os.environ.copy()
    if case.use_random_port:
        port = get_free_port()
    else:
        port = 8080  # This is the default port if not specified

    cmd.append(f"--serve-port={port}")

    proc = await asyncio.create_subprocess_exec(
        *cmd,
        cwd=str(repo_root),
        env=env,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
        start_new_session=start_new_session,
        creationflags=creationflags,
    )

    output_lines: List[str] = []
    found_build = asyncio.Event()
    found_serve = asyncio.Event()

    async def reader(label: str, stream: asyncio.StreamReader) -> None:
        async for line in stream:
            line = line.decode("utf-8", errors="replace")
            print(f"[{label}] {line}", end="")
            output_lines.append(f"[{label}] {line}")
            if (not found_build.is_set()) and (build_phrase in line):
                found_build.set()
            if (not found_serve.is_set()) and serve_re.search(line):
                found_serve.set()

    assert proc.stdout is not None and proc.stderr is not None
    reader_out_task = asyncio.create_task(reader("stdout", proc.stdout))
    reader_err_task = asyncio.create_task(reader("stderr", proc.stderr))

    try:
        # Wait until both events are seen, the process exits, or timeout elapses.
        loop = asyncio.get_running_loop()
        end = loop.time() + 180
        while loop.time() < end:
            if found_build.is_set() and found_serve.is_set():
                break
            if proc.returncode is not None:
                # Process ended early; stop waiting
                break
            await asyncio.sleep(0.05)

        # If we saw the serve banner, exercise the HTTP endpoint before shutting
        # down to ensure it actually serves content.
        if found_serve.is_set():
            for req in case.http:
                ok = await _wait_for_http_contains(
                    host="localhost",
                    port=port,
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
    finally:
        # Try graceful shutdown first with Ctrl-C (SIGINT), then kill if needed
        try:
            if os.name != "nt":
                os.killpg(os.getpgid(proc.pid), signal.SIGINT)
            else:
                # For Windows with CREATE_NEW_PROCESS_GROUP
                proc.send_signal(signal.CTRL_BREAK_EVENT)
        except Exception:
            pass

        # Wait briefly for graceful exit, then force kill if still running
        try:
            await asyncio.wait_for(proc.wait(), timeout=2)
        except asyncio.TimeoutError:
            if os.name != "nt":
                os.killpg(os.getpgid(proc.pid), signal.SIGKILL)
            else:
                proc.kill()

        # Ensure reader tasks are finished
        for t in (reader_out_task, reader_err_task):
            if not t.done():
                t.cancel()
        for t in (reader_out_task, reader_err_task):
            with contextlib.suppress(asyncio.CancelledError):
                await t

    full_output = "".join(output_lines)

    if not (found_build.is_set() and found_serve.is_set()):
        code = proc.returncode
        pytest.fail(
            "End-to-end run did not reach expected state.\n"
            f"command={shlex.join(cmd)}\n"
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


def get_free_port(min_port=1024, max_port=65535):
    while True:
        port = random.randint(min_port, max_port)
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            try:
                s.bind(("", port))  # Bind to the port on all interfaces
                return port
            except OSError:
                # Port is already in use, try another one
                continue
