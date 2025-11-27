import asyncio
import contextlib
import os
import random
import re
import shutil
import signal
import socket
import subprocess
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import List, NamedTuple, Optional

import aiohttp
import pytest


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
    fixed_port: Optional[int] = None

    def __str__(self):
        return self.path

    def __repr__(self):
        return self.path


EXAMPLES_ROOT = Path(__file__).resolve().parents[1] / "examples"
VALID_EXAMPLES = sorted(
    entry.name
    for entry in EXAMPLES_ROOT.iterdir()
    if entry.is_dir() and not entry.name.startswith(("fail", "skip"))
)

STATIC_SERVE_PATTERN = r"server is listening on"
PHP_SERVE_PATTERN = (
    r"PHP 8\.3\.[0-9]+ Development Server \(http://localhost:[\d]+\) started"
)
UVICORN_SERVE_PATTERN = r"Uvicorn running on .*"
STREAMLIT_SERVE_PATTERN = r".*You can now view your Streamlit app in your browser.*"
REACTPHP_SERVE_PATTERN = r"Server running at http://127\.0\.0\.1:8080"
PYTHON_HTTP_SERVE_PATTERN = r"Starting server on http://.*"


def example_case(
    name: str,
    serve_pattern: str,
    http: Optional[List[HTTPRequest]] = None,
    *,
    use_random_port: bool = True,
    fixed_port: Optional[int] = None,
) -> E2ECase:
    return E2ECase(
        path=f"examples/{name}",
        serve_pattern=serve_pattern,
        http=http or [],
        use_random_port=use_random_port,
        fixed_port=fixed_port,
    )


EXAMPLE_CASES = {
    "go-hugo-staticsite": example_case(
        "go-hugo-staticsite",
        STATIC_SERVE_PATTERN,
        [HTTPRequest(path="/", body_match=r"My New Hugo Site demo")],
    ),
    "jekyll": example_case(
        "jekyll",
        STATIC_SERVE_PATTERN,
        [HTTPRequest(path="/", body_match=r"Your awesome title")],
    ),
    "js-astro-staticsite": example_case(
        "js-astro-staticsite",
        STATIC_SERVE_PATTERN,
        [HTTPRequest(path="/", body_match=r"Welcome to Wasmer\+Astro")],
    ),
    "js-docusaurus-staticsite": example_case(
        "js-docusaurus-staticsite",
        STATIC_SERVE_PATTERN,
        [HTTPRequest(path="/", body_match=r"Dinosaurs are cool")],
    ),
    "js-docusaurus2-staticsite": example_case(
        "js-docusaurus2-staticsite",
        STATIC_SERVE_PATTERN,
        [HTTPRequest(path="/", body_match=r"Dinosaurs are cool")],
    ),
    "js-docusaurusold-staticsite": example_case(
        "js-docusaurusold-staticsite",
        STATIC_SERVE_PATTERN,
        [HTTPRequest(path="/", body_match=r"A website for testing")],
    ),
    "js-gatsby-staticsite": example_case(
        "js-gatsby-staticsite",
        STATIC_SERVE_PATTERN,
        [HTTPRequest(path="/", body_match=r"Gatsby")],
    ),
    "js-next-staticsite": example_case(
        "js-next-staticsite",
        STATIC_SERVE_PATTERN,
        [HTTPRequest(path="/", body_match=r"Get started by editing")],
    ),
    "js-svelte": example_case(
        "js-svelte",
        STATIC_SERVE_PATTERN,
        [HTTPRequest(path="/", body_match=r"SvelteKit app")],
    ),
    "php-basic": example_case(
        "php-basic",
        PHP_SERVE_PATTERN,
        [HTTPRequest(path="/", body_match=r"PHP code tester")],
    ),
    "php-laravel": example_case(
        "php-laravel",
        PHP_SERVE_PATTERN,
        [HTTPRequest(path="/", body_match=r"Laravel")],
    ),
    "php-reactphp": example_case(
        "php-reactphp",
        REACTPHP_SERVE_PATTERN,
        [HTTPRequest(path="/", body_match=r"Hello World!")],
        use_random_port=False,
        fixed_port=8080,
    ),
    "php-symfony": example_case(
        "php-symfony",
        PHP_SERVE_PATTERN,
        [HTTPRequest(path="/", body_match=r"Symfony")],
    ),
    "python-django": example_case(
        "python-django",
        UVICORN_SERVE_PATTERN,
        [HTTPRequest(path="/", body_match=r"The install worked successfully")],
    ),
    "python-fastapi": example_case(
        "python-fastapi",
        UVICORN_SERVE_PATTERN,
        [HTTPRequest(path="/", body_match=r"Hello World")],
    ),
    "python-fastapi-pandoc-converter": example_case(
        "python-fastapi-pandoc-converter",
        UVICORN_SERVE_PATTERN,
        [HTTPRequest(path="/", body_match=r"Pandoc Converter")],
    ),
    "python-fastapi-pystone": example_case(
        "python-fastapi-pystone",
        UVICORN_SERVE_PATTERN,
        [HTTPRequest(path="/", body_match=r"\"version\"\\s*:\\s*\"1\.1\"")],
    ),
    "python-ffmpeg": example_case(
        "python-ffmpeg",
        UVICORN_SERVE_PATTERN,
        [HTTPRequest(path="/", body_match=r"Take screenshot at 1s")],
    ),
    "python-flask": example_case(
        "python-flask",
        UVICORN_SERVE_PATTERN,
        [HTTPRequest(path="/", body_match=r"Flask in Wasmer Edge")],
    ),
    "python-http": example_case(
        "python-http",
        PYTHON_HTTP_SERVE_PATTERN,
        [HTTPRequest(path="/", body_match=r"Python app is running with Wasmer!")],
    ),
    "python-langchain-starter": example_case(
        "python-langchain-starter",
        STREAMLIT_SERVE_PATTERN,
        [HTTPRequest(path="/", body_match=r"Streamlit")],
    ),
    "python-mcp": example_case(
        "python-mcp",
        UVICORN_SERVE_PATTERN,
        [],
        use_random_port=False,
        fixed_port=8000,
    ),
    "python-mcp-chatgpt": example_case(
        "python-mcp-chatgpt",
        UVICORN_SERVE_PATTERN,
        [],
        use_random_port=False,
        fixed_port=8000,
    ),
    "python-mkdocs": example_case(
        "python-mkdocs",
        STATIC_SERVE_PATTERN,
        [HTTPRequest(path="/", body_match=r"Welcome to MkDocs")],
    ),
    "python-pillow": example_case(
        "python-pillow",
        UVICORN_SERVE_PATTERN,
        [HTTPRequest(path="/", body_match=r"Image Crop\s*&\s*Rotate")],
    ),
    "staticsite": example_case(
        "staticsite",
        STATIC_SERVE_PATTERN,
        [],
    ),
}

_missing = sorted(set(VALID_EXAMPLES) - EXAMPLE_CASES.keys())
_extra = sorted(set(EXAMPLE_CASES.keys()) - set(VALID_EXAMPLES))
if _missing or _extra:
    problems = []
    if _missing:
        problems.append(f"missing cases for: {', '.join(_missing)}")
    if _extra:
        problems.append(f"unknown example cases: {', '.join(_extra)}")
    raise ValueError(
        "E2E test matrix is out of sync with examples directory: " + "; ".join(problems)
    )


@pytest.mark.e2e
@pytest.mark.asyncio
@pytest.mark.parametrize(
    "case",
    [EXAMPLE_CASES[name] for name in VALID_EXAMPLES],
    ids=lambda c: str(c),
)
@pytest.mark.flaky(reruns=0, reruns_delay=2)
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
    example_dir = repo_root / case.path
    if not example_dir.exists():
        pytest.skip(
            f"Example '{case.path}' is missing. Run `git submodule update --init --recursive` to fetch the examples."
        )

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
    elif build_mode == BuildMode.WasmerAndDocker:
        cmd.append("--wasmer")
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
        port = case.fixed_port or 8080
    env["PORT"] = str(port)

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
