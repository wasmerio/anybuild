import asyncio
import contextlib
import os
import random
import re
import shlex
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import uuid
import zipfile
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import List
from urllib.parse import urlparse
from urllib.request import urlopen

import aiohttp
import pytest
import yaml


class BuildMode(Enum):
    Wasmer = "wasmer"
    WasmerAndDocker = "docker"
    Local = "local"


@dataclass(frozen=True)
class HTTPRequest:
    path: str
    body_match: str | None = None
    method: str = "GET"
    expected_status: int | None = None
    location_match: str | None = None
    follow_redirects: bool = True


@dataclass(frozen=True)
class RunCommand:
    command: str
    stdout_match: str | None = None
    stderr_match: str | None = None
    expected_returncode: int = 0


@dataclass(frozen=True)
class CompletedCommand:
    returncode: int | None
    stdout: str
    stderr: str

    @property
    def output(self) -> str:
        return f"[stdout]\n{self.stdout}\n[stderr]\n{self.stderr}"


@dataclass(frozen=True)
class E2ECase:
    serve_pattern: str
    http: List[HTTPRequest]
    path: str | None = None
    download: str | None = None
    use_random_port: bool = True
    env: dict[str, str] | None = None
    extra_env: dict[str, str] | None = None
    create_db: bool = False
    create_wp_content_volume: bool = False
    run_after_deploy: bool = False
    commands: list[RunCommand] = field(default_factory=list)
    expected_memory_limit: str | None = None
    expect_no_memory_limit: bool = False
    build_modes: tuple[BuildMode, ...] | None = None

    def __str__(self):
        if self.path:
            return self.path
        assert self.download is not None
        return Path(urlparse(self.download).path).stem

    def __repr__(self):
        return str(self)


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
                HTTPRequest(
                    path="/",
                    body_match=r"\"version\"\s*:\s*\"8\.3\.[0-9]+\"",
                ),
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
        # Full WordPress release archive, built and run through Wasmer only.
        E2ECase(
            download="https://wordpress.org/wordpress-6.9.4.zip",
            serve_pattern=(
                r"listening addr"
            ),
            http=[
                HTTPRequest(
                    path="/",
                    expected_status=200,
                    body_match=r"WordPress",
                )
            ],
            use_random_port=False,
            env={
                "DB_NAME": "test",
                "DB_USERNAME": "root",
                "DB_HOST": "127.0.0.1",
                "DB_PORT": "3306",
                "DB_PASSWORD": "",
                "SHIPIT_PHPIX": "true",
            },
            create_db=True,
            create_wp_content_volume=True,
            run_after_deploy=True,
            commands=[
                RunCommand(
                    "wp eval 'echo json_encode([\"status\" => \"ok\"]);'",
                    stdout_match=r'\{"status":"ok"\}',
                )
            ],
            build_modes=(BuildMode.Wasmer,),
        ),
        # Full WordPress release archive, built and run through Wasmer only.
        E2ECase(
            path="examples/php-wordpress-empty",
            serve_pattern=(
                r"listening addr"
            ),
            http=[
                HTTPRequest(
                    path="/",
                    expected_status=200,
                    body_match=r"WordPress",
                )
            ],
            use_random_port=False,
            env={
                "DB_NAME": "test",
                "DB_USERNAME": "root",
                "DB_HOST": "127.0.0.1",
                "DB_PORT": "3306",
                "DB_PASSWORD": "",
                "SHIPIT_PHPIX": "true",
                "SHIPIT_WP_VERSION": "latest",
                # "SHIPIT_WP_LOCALE": "en_US",
            },
            create_db=True,
            create_wp_content_volume=True,
            run_after_deploy=True,
            commands=[
                RunCommand(
                    "wp eval 'echo json_encode([\"status\" => \"ok\"]);'",
                    stdout_match=r'\{"status":"ok"\}',
                )
            ],
            build_modes=(BuildMode.Wasmer,),
        ),
        # WordPress skeleton in phpix mode (Wasmer only), validate memory cap.
        E2ECase(
            path="examples/php-wordpress",
            serve_pattern=(
                r"PHP 8\.3\.[0-9]+ Development Server \(http://localhost:[\d]+\) started"
            ),
            http=[HTTPRequest(path="/", body_match=r"WordPress")],
            extra_env={"SHIPIT_PHPIX": "true"},
            expected_memory_limit="2Gb",
        ),
        # Non-WordPress phpix mode should not force a memory capability.
        E2ECase(
            path="examples/php-nobuild",
            serve_pattern=(
                r"PHP 8\.3\.[0-9]+ Development Server \(http://localhost:[\d]+\) started"
            ),
            http=[HTTPRequest(path="/", body_match=r"PHP Version 8\.3\.[0-9]+")],
            extra_env={"SHIPIT_PHPIX": "true"},
            expect_no_memory_limit=True,
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
        # Staticfile provider redirect support via _redirects (Wasmer only).
        E2ECase(
            path="examples/staticfile-redirects",
            serve_pattern=r"server is listening on",
            http=[
                HTTPRequest(
                    path="/docs/getting-started",
                    expected_status=301,
                    location_match=r"^/guides/getting-started/$",
                    follow_redirects=False,
                ),
                HTTPRequest(
                    path="/guides/getting-started/",
                    body_match=r"Redirect target page",
                ),
            ],
            build_modes=(BuildMode.Wasmer,),
        ),
        # Generic Node HTTP server
        E2ECase(
            path="examples/node",
            serve_pattern=r"Node server listening on",
            http=[HTTPRequest(path="/", body_match=r"Hello from Node")],
            build_modes=(BuildMode.Local, BuildMode.Wasmer),
        ),
        # Hono app running on Node
        E2ECase(
            path="examples/node-hono",
            serve_pattern=r"Hono server listening on",
            http=[HTTPRequest(path="/", body_match=r"Hello from Hono on Shipit")],
            build_modes=(BuildMode.Local, BuildMode.Wasmer),
        ),
        # Fastify app running on Node
        E2ECase(
            path="examples/node-fastify",
            serve_pattern=r"Fastify server listening on",
            http=[HTTPRequest(path="/", body_match=r"Hello from Fastify on Shipit")],
            build_modes=(BuildMode.Local, BuildMode.Wasmer),
        ),
        # Next.js runtime app bundled for Node
        E2ECase(
            path="examples/node-next",
            serve_pattern=r"Next.js|started server|ready",
            http=[
                HTTPRequest(
                    path="/",
                    body_match=r"Hello from Next\.js on Shipit",
                )
            ],
            build_modes=(BuildMode.Local, BuildMode.Wasmer),
        ),
        # Astro runtime app served by the Node adapter
        E2ECase(
            path="examples/node-astro",
            serve_pattern=r"Node|Astro|Listening|ready",
            http=[HTTPRequest(path="/", body_match=r"Astro Node Example")],
            build_modes=(BuildMode.Local,),
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
        # Astro static site
        E2ECase(
            path="examples/nodestatic-astro",
            serve_pattern=r"server is listening on",
            http=[HTTPRequest(path="/", body_match=r"Astro Static Example")],
            build_modes=(BuildMode.Wasmer,),
        ),
        # Next.js static export via output: "export"
        E2ECase(
            path="examples/nodestatic-next",
            serve_pattern=r"server is listening on",
            http=[HTTPRequest(path="/", body_match=r"Get started by editing")],
            build_modes=(BuildMode.Wasmer,),
        ),
        # Nuxt static generation
        E2ECase(
            path="examples/nodestatic-nuxt",
            serve_pattern=r"server is listening on",
            http=[HTTPRequest(path="/", body_match=r"Nuxt Static Example")],
            build_modes=(BuildMode.Wasmer,),
        ),
        # Docusaurus static documentation site
        E2ECase(
            path="examples/nodestatic-docusaurus",
            serve_pattern=r"server is listening on",
            http=[HTTPRequest(path="/", body_match=r"Docusaurus Example")],
            build_modes=(BuildMode.Wasmer,),
        ),
        # SvelteKit prerendered static site
        E2ECase(
            path="examples/nodestatic-svelte",
            serve_pattern=r"server is listening on",
            http=[HTTPRequest(path="/", body_match=r"Svelte Static Example")],
            build_modes=(BuildMode.Wasmer,),
        ),
        # Remix static output served as files
        E2ECase(
            path="examples/nodestatic-remix",
            serve_pattern=r"server is listening on",
            http=[HTTPRequest(path="/", body_match=r"Remix Static Example")],
            build_modes=(BuildMode.Wasmer,),
        ),
        # Eleventy / 11ty static site
        E2ECase(
            path="examples/nodestatic-eleventy",
            serve_pattern=r"server is listening on",
            http=[HTTPRequest(path="/", body_match=r"Eleventy Example")],
            build_modes=(BuildMode.Local, BuildMode.Wasmer),
        ),
        # VitePress static documentation site
        E2ECase(
            path="examples/nodestatic-vitepress",
            serve_pattern=r"server is listening on",
            http=[HTTPRequest(path="/", body_match=r"VitePress Example")],
            build_modes=(BuildMode.Local, BuildMode.Wasmer),
        ),
        # VuePress static documentation site
        E2ECase(
            path="examples/nodestatic-vuepress",
            serve_pattern=r"server is listening on",
            http=[HTTPRequest(path="/", body_match=r"VuePress Example")],
            build_modes=(BuildMode.Local, BuildMode.Wasmer),
        ),
        # Hexo static blog
        E2ECase(
            path="examples/nodestatic-hexo",
            serve_pattern=r"server is listening on",
            http=[HTTPRequest(path="/", body_match=r"Hexo Example")],
            build_modes=(BuildMode.Local, BuildMode.Wasmer),
        ),
        # Metalsmith static site
        E2ECase(
            path="examples/nodestatic-metalsmith",
            serve_pattern=r"server is listening on",
            http=[HTTPRequest(path="/", body_match=r"Metalsmith Example")],
            build_modes=(BuildMode.Local, BuildMode.Wasmer),
        ),
        # Assemble static site
        E2ECase(
            path="examples/nodestatic-assemble",
            serve_pattern=r"server is listening on",
            http=[HTTPRequest(path="/", body_match=r"Assemble Example")],
            build_modes=(BuildMode.Local, BuildMode.Wasmer),
        ),
        # Harp static site
        E2ECase(
            path="examples/nodestatic-harp",
            serve_pattern=r"server is listening on",
            http=[HTTPRequest(path="/", body_match=r"Harp Example")],
            build_modes=(BuildMode.Local, BuildMode.Wasmer),
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
async def test_end_to_end(
    case: E2ECase,
    build_mode: BuildMode,
    tmp_path: Path,
):
    # Skip if `uv` is not available in PATH
    if not shutil.which("uv"):
        pytest.skip("`uv` is not available in PATH")
    if (case.expected_memory_limit or case.expect_no_memory_limit) and (
        build_mode != BuildMode.Wasmer
    ):
        pytest.skip("phpix memory-cap checks run in Wasmer mode only")
    if case.build_modes and build_mode not in case.build_modes:
        pytest.skip("case is not enabled for this build mode")

    repo_root = Path(__file__).resolve().parents[1]
    project_path = await _materialize_case(case, repo_root, tmp_path)

    if case.use_random_port:
        port = get_free_port()
    else:
        port = 8080  # This is the default port if not specified

    env = os.environ.copy()
    if case.env:
        env.update(case.env)
    if case.extra_env:
        env.update(case.extra_env)

    created_db_name = None
    wp_content_volume_dir = None
    try:
        if case.create_db:
            created_db_name = await _create_mysql_database(env)
            env["DB_NAME"] = created_db_name
        volume_specs = None
        if case.create_wp_content_volume:
            wp_content_volume_dir = _create_wp_content_volume(project_path)
            volume_specs = ["wp-content:/app/wp-content"]

        if case.download or case.commands:
            build_cmd = _shipit_build_command(project_path, build_mode, port)
            build_result = await _run_completed_command(
                build_cmd,
                cwd=repo_root,
                env=env,
                timeout=180,
            )
            build_output = build_result.output
            if build_result.returncode != 0 or "Build complete ✅" not in build_output:
                pytest.fail(
                    "End-to-end build command failed.\n"
                    f"command={shlex.join(build_cmd)}\n"
                    f"returncode={build_result.returncode}\n\n"
                    f"--- Captured output start ---\n{build_output}\n"
                    "--- Captured output end ---"
                )

            run_cmd = _shipit_run_command(
                project_path,
                build_mode,
                run_after_deploy=case.run_after_deploy,
                start=True,
                volume_specs=volume_specs,
            )
            await _run_server_and_check(
                case=case,
                cmd=run_cmd,
                cwd=repo_root,
                env=env,
                project_path=project_path,
                port=port,
                expect_build=False,
            )
            for command in case.commands:
                cmd = _shipit_run_command(
                    project_path,
                    build_mode,
                    command=command.command,
                    volume_specs=volume_specs,
                )
                result = await _run_completed_command(
                    cmd,
                    cwd=repo_root,
                    env=env,
                    timeout=180,
                )
                _assert_run_command(command, cmd, result)
            return

        cmd = _shipit_auto_command(
            project_path,
            build_mode,
            port,
            run_after_deploy=case.run_after_deploy,
        )
        await _run_server_and_check(
            case=case,
            cmd=cmd,
            cwd=repo_root,
            env=env,
            project_path=project_path,
            port=port,
            expect_build=True,
        )
    finally:
        if created_db_name:
            drop_result = await _drop_mysql_database(env, created_db_name)
            if drop_result.returncode != 0:
                drop_error = (
                    "Failed to drop temporary MySQL database.\n"
                    f"database={created_db_name}\n\n"
                    f"--- Captured output start ---\n{drop_result.output}\n"
                    "--- Captured output end ---"
                )
                if sys.exc_info()[0] is None:
                    try:
                        if wp_content_volume_dir:
                            shutil.rmtree(wp_content_volume_dir, ignore_errors=True)
                    finally:
                        pytest.fail(drop_error)
                print(drop_error)
        if wp_content_volume_dir:
            shutil.rmtree(wp_content_volume_dir, ignore_errors=True)


async def _run_server_and_check(
    case: E2ECase,
    cmd: list[str],
    cwd: Path,
    env: dict[str, str],
    project_path: Path,
    port: int,
    expect_build: bool,
) -> None:
    build_phrase = "Build complete ✅"
    serve_re = re.compile(case.serve_pattern)

    # Start process in a new session/process group to simplify termination.
    start_new_session = os.name != "nt"
    creationflags = subprocess.CREATE_NEW_PROCESS_GROUP if os.name == "nt" else 0

    proc = await asyncio.create_subprocess_exec(
        *cmd,
        cwd=str(cwd),
        env=env,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
        start_new_session=start_new_session,
        creationflags=creationflags,
    )

    output_lines: List[str] = []
    found_build = asyncio.Event()
    if not expect_build:
        found_build.set()
    found_serve = asyncio.Event()
    matched_serve_output = False
    verified_http_ready = False

    async def reader(label: str, stream: asyncio.StreamReader) -> None:
        nonlocal matched_serve_output
        async for line in stream:
            line = line.decode("utf-8", errors="replace")
            print(f"[{label}] {line}", end="")
            output_lines.append(f"[{label}] {line}")
            if (not found_build.is_set()) and (build_phrase in line):
                found_build.set()
            if (not found_serve.is_set()) and serve_re.search(line):
                matched_serve_output = True
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
            if (
                found_build.is_set()
                and not found_serve.is_set()
                and case.http
            ):
                readiness_request = _http_readiness_request(case.http[0])
                verified_http_ready = await _wait_for_http_response(
                    host="localhost",
                    port=port,
                    request=readiness_request,
                    timeout=0.5,
                )
                if verified_http_ready:
                    found_serve.set()
                    break
            if proc.returncode is not None:
                # Process ended early; stop waiting
                break
            await asyncio.sleep(0.05)

        # If we saw the serve banner, exercise the HTTP endpoint before shutting
        # down to ensure it actually serves content.
        if found_serve.is_set():
            if case.expected_memory_limit or case.expect_no_memory_limit:
                app_yaml_path = (
                    project_path / ".shipit" / "wasmer" / "app.yaml"
                )
                if not app_yaml_path.is_file():
                    full_output = "".join(output_lines)
                    pytest.fail(
                        "Expected generated app.yaml for Wasmer run, but it "
                        "was not found.\n\n"
                        f"Path: {app_yaml_path}\n\n"
                        f"--- Captured output start ---\n{full_output}\n"
                        "--- Captured output end ---"
                    )
                app_yaml = yaml.safe_load(app_yaml_path.read_text()) or {}
                capabilities = app_yaml.get("capabilities", {})
                memory = capabilities.get("memory", {})
                limit = memory.get("limit")
                if case.expected_memory_limit and limit != case.expected_memory_limit:
                    full_output = "".join(output_lines)
                    pytest.fail(
                        "Generated app.yaml has wrong phpix memory limit.\n\n"
                        f"Expected: {case.expected_memory_limit}\n"
                        f"Actual: {limit}\n"
                        f"Path: {app_yaml_path}\n\n"
                        f"--- Captured output start ---\n{full_output}\n"
                        "--- Captured output end ---"
                    )
                if case.expect_no_memory_limit and limit is not None:
                    full_output = "".join(output_lines)
                    pytest.fail(
                        "Generated app.yaml unexpectedly sets phpix memory limit"
                        " for non-WordPress app.\n\n"
                        f"Actual: {limit}\n"
                        f"Path: {app_yaml_path}\n\n"
                        f"--- Captured output start ---\n{full_output}\n"
                        "--- Captured output end ---"
                    )
            for req in case.http:
                ok = await _wait_for_http_response(
                    host="localhost",
                    port=port,
                    request=req,
                    timeout=20.0,
                )
                if not ok:
                    full_output = "".join(output_lines)
                    pytest.fail(
                        "Server did not return expected HTTP content.\n\n"
                        f"Request path: '{req.path}'\n"
                        f"Expected status: {req.expected_status}\n"
                        f"Expected location regex: {req.location_match!r}\n"
                        f"Expected body regex: {req.body_match!r}\n\n"
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

    if expect_build:
        assert build_phrase in full_output
    assert matched_serve_output or verified_http_ready, (
        "Serve banner regex not found in output and HTTP readiness did not pass"
    )


async def _materialize_case(
    case: E2ECase,
    repo_root: Path,
    tmp_path: Path,
) -> Path:
    if case.path and case.download:
        raise ValueError("E2ECase can define either path or download, not both")
    if case.path:
        return repo_root / case.path
    if not case.download:
        raise ValueError("E2ECase requires either path or download")
    return await asyncio.to_thread(
        _download_and_extract_archive,
        case.download,
        tmp_path,
    )


def _download_and_extract_archive(url: str, tmp_path: Path) -> Path:
    download_dir = tmp_path / "download"
    download_dir.mkdir(parents=True, exist_ok=True)
    archive_name = Path(urlparse(url).path).name or "download.zip"
    archive_path = download_dir / archive_name

    with urlopen(url, timeout=120) as response:
        with archive_path.open("wb") as output:
            shutil.copyfileobj(response, output)

    extract_dir = tmp_path / "src"
    extract_dir.mkdir(parents=True, exist_ok=True)
    if archive_path.suffix == ".zip":
        _extract_zip(archive_path, extract_dir)
    else:
        shutil.unpack_archive(str(archive_path), str(extract_dir))

    children = [path for path in extract_dir.iterdir() if path.name != "__MACOSX"]
    if len(children) == 1 and children[0].is_dir():
        return children[0]
    return extract_dir


def _extract_zip(archive_path: Path, extract_dir: Path) -> None:
    extract_root = extract_dir.resolve()
    with zipfile.ZipFile(archive_path) as archive:
        for member in archive.infolist():
            target = (extract_dir / member.filename).resolve()
            if not target.is_relative_to(extract_root):
                raise ValueError(
                    f"Archive member escapes extract dir: {member.filename}"
                )
        archive.extractall(extract_dir)


async def _create_mysql_database(env: dict[str, str]) -> str:
    name = f"shipit_e2e_{uuid.uuid4().hex}"
    result = await _run_mysql_sql(
        env,
        (
            f"CREATE DATABASE {_quote_mysql_identifier(name)} "
            "CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci"
        ),
    )
    if result.returncode != 0:
        pytest.fail(
            "Failed to create temporary MySQL database.\n"
            f"database={name}\n\n"
            f"--- Captured output start ---\n{result.output}\n"
            "--- Captured output end ---"
        )
    return name


async def _drop_mysql_database(
    env: dict[str, str],
    name: str,
) -> CompletedCommand:
    return await _run_mysql_sql(
        env,
        f"DROP DATABASE IF EXISTS {_quote_mysql_identifier(name)}",
    )


async def _run_mysql_sql(env: dict[str, str], sql: str) -> CompletedCommand:
    return await _run_completed_command(
        _mysql_command(env, sql),
        cwd=Path(__file__).resolve().parents[1],
        env=env,
        timeout=30,
    )


def _mysql_command(env: dict[str, str], sql: str) -> list[str]:
    mysql = shutil.which("mysql")
    if not mysql:
        pytest.fail(
            "`mysql` client is not available; it is required for "
            "E2ECase(create_db=True)."
        )

    cmd = [
        mysql,
        "--protocol=TCP",
        "--batch",
        "--skip-column-names",
        "--host",
        env.get("DB_HOST", "127.0.0.1"),
        "--port",
        env.get("DB_PORT", "3306"),
        "--user",
        env.get("DB_USERNAME", "root"),
    ]
    if "DB_PASSWORD" in env:
        cmd.append(f"--password={env['DB_PASSWORD']}")
    cmd.extend(["--execute", sql])
    return cmd


def _quote_mysql_identifier(name: str) -> str:
    if not re.fullmatch(r"[A-Za-z0-9_]+", name):
        raise ValueError(f"Invalid MySQL identifier: {name!r}")
    return f"`{name}`"


def _create_wp_content_volume(project_path: Path) -> Path:
    host_dir = Path(
        tempfile.mkdtemp(prefix="shipit-e2e-wp-content-", dir="/tmp")
    )
    volume_path = project_path / ".shipit" / "volumes" / "wp-content"
    volume_path.parent.mkdir(parents=True, exist_ok=True)
    if volume_path.is_symlink() or volume_path.is_file():
        volume_path.unlink()
    elif volume_path.is_dir():
        shutil.rmtree(volume_path)
    volume_path.symlink_to(host_dir, target_is_directory=True)
    return host_dir


def _shipit_auto_command(
    project_path: Path,
    build_mode: BuildMode,
    port: int,
    run_after_deploy: bool,
) -> list[str]:
    cmd = [
        "uv",
        "run",
        "shipit",
        str(project_path),
        "--skip-prepare",
        "--start",
        "--regenerate",
    ]
    if run_after_deploy:
        cmd.append("--after-deploy")
    _append_build_mode_flags(cmd, build_mode)
    cmd.append(f"--serve-port={port}")
    return cmd


def _shipit_build_command(
    project_path: Path,
    build_mode: BuildMode,
    port: int,
) -> list[str]:
    cmd = [
        "uv",
        "run",
        "shipit",
        str(project_path),
        "--skip-prepare",
        "--regenerate",
    ]
    _append_build_mode_flags(cmd, build_mode)
    cmd.append(f"--serve-port={port}")
    return cmd


def _shipit_run_command(
    project_path: Path,
    build_mode: BuildMode,
    *,
    run_after_deploy: bool = False,
    start: bool = False,
    command: str | None = None,
    volume_specs: list[str] | None = None,
) -> list[str]:
    cmd = [
        "uv",
        "run",
        "shipit",
        "run",
        str(project_path),
    ]
    if run_after_deploy:
        cmd.append("--after-deploy")
    if start:
        cmd.append("--start")
    if command:
        cmd.append(f"--command={command}")
    for spec in volume_specs or []:
        cmd.extend(["--volume", spec])
    _append_run_mode_flags(cmd, build_mode)
    return cmd


def _append_build_mode_flags(cmd: list[str], build_mode: BuildMode) -> None:
    if build_mode == BuildMode.Wasmer:
        cmd.append("--wasmer")
        cmd.append("--wasmer-registry=wasmer.io")
    elif build_mode == BuildMode.WasmerAndDocker:
        cmd.append("--wasmer")
        cmd.append("--wasmer-registry=wasmer.io")
        cmd.append("--docker")
    elif build_mode == BuildMode.Local:
        pass


def _append_run_mode_flags(cmd: list[str], build_mode: BuildMode) -> None:
    if build_mode == BuildMode.Wasmer:
        cmd.append("--wasmer")
        cmd.append("--wasmer-registry=wasmer.io")
    elif build_mode == BuildMode.WasmerAndDocker:
        cmd.append("--wasmer")
        cmd.append("--wasmer-registry=wasmer.io")
        cmd.append("--docker")
    elif build_mode == BuildMode.Local:
        pass


async def _run_completed_command(
    cmd: list[str],
    cwd: Path,
    env: dict[str, str],
    timeout: float,
) -> CompletedCommand:
    start_new_session = os.name != "nt"
    creationflags = subprocess.CREATE_NEW_PROCESS_GROUP if os.name == "nt" else 0
    proc = await asyncio.create_subprocess_exec(
        *cmd,
        cwd=str(cwd),
        env=env,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
        start_new_session=start_new_session,
        creationflags=creationflags,
    )
    try:
        stdout, stderr = await asyncio.wait_for(
            proc.communicate(),
            timeout=timeout,
        )
    except asyncio.TimeoutError:
        await _stop_process(proc)
        stdout, stderr = await proc.communicate()
    return CompletedCommand(
        returncode=proc.returncode,
        stdout=stdout.decode("utf-8", errors="replace"),
        stderr=stderr.decode("utf-8", errors="replace"),
    )


async def _stop_process(proc: asyncio.subprocess.Process) -> None:
    try:
        if os.name != "nt":
            os.killpg(os.getpgid(proc.pid), signal.SIGINT)
        else:
            proc.send_signal(signal.CTRL_BREAK_EVENT)
    except Exception:
        pass
    try:
        await asyncio.wait_for(proc.wait(), timeout=2)
    except asyncio.TimeoutError:
        if os.name != "nt":
            os.killpg(os.getpgid(proc.pid), signal.SIGKILL)
        else:
            proc.kill()
            await proc.wait()


def _assert_run_command(
    command: RunCommand,
    cmd: list[str],
    result: CompletedCommand,
) -> None:
    if result.returncode != command.expected_returncode:
        pytest.fail(
            "Run command exited with unexpected status.\n"
            f"command={shlex.join(cmd)}\n"
            f"expected_returncode={command.expected_returncode}\n"
            f"returncode={result.returncode}\n\n"
            f"--- Captured output start ---\n{result.output}\n"
            "--- Captured output end ---"
        )
    if command.stdout_match and not re.search(command.stdout_match, result.stdout):
        pytest.fail(
            "Run command stdout did not match expected regex.\n"
            f"command={shlex.join(cmd)}\n"
            f"stdout_match={command.stdout_match!r}\n\n"
            f"--- Captured output start ---\n{result.output}\n"
            "--- Captured output end ---"
        )
    if command.stderr_match and not re.search(command.stderr_match, result.stderr):
        pytest.fail(
            "Run command stderr did not match expected regex.\n"
            f"command={shlex.join(cmd)}\n"
            f"stderr_match={command.stderr_match!r}\n\n"
            f"--- Captured output start ---\n{result.output}\n"
            "--- Captured output end ---"
        )


def _http_readiness_request(request: HTTPRequest) -> HTTPRequest:
    return HTTPRequest(
        path=request.path,
        method=request.method,
        expected_status=request.expected_status or 200,
        follow_redirects=request.follow_redirects,
    )


async def _wait_for_http_response(
    host: str, port: int, request: HTTPRequest, timeout: float = 15.0
) -> bool:
    url = f"http://{host}:{port}{request.path}"
    loop = asyncio.get_running_loop()
    end = loop.time() + timeout
    request_timeout = max(0.2, min(5.0, timeout))
    async with aiohttp.ClientSession(
        timeout=aiohttp.ClientTimeout(total=request_timeout)
    ) as session:
        while loop.time() < end:
            try:
                async with session.request(
                    request.method,
                    url,
                    allow_redirects=request.follow_redirects,
                ) as resp:
                    text = await resp.text()
                    if request.expected_status is not None:
                        if resp.status != request.expected_status:
                            await asyncio.sleep(0.2)
                            continue
                    if request.location_match is not None:
                        location = resp.headers.get("Location", "")
                        if not re.search(request.location_match, location):
                            await asyncio.sleep(0.2)
                            continue
                    if request.body_match is not None and not re.search(
                        request.body_match, text
                    ):
                        await asyncio.sleep(0.2)
                        continue
                    if (
                        request.expected_status is not None
                        or request.location_match is not None
                        or request.body_match is not None
                    ):
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
