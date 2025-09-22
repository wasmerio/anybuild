"""
FastAPI app that serves a tiny web UI (pure Python-rendered HTML) to upload a video
and extract the frame at 1 second using ffmpeg-python.

Quickstart (run these in your terminal):

    python -m venv .venv && source .venv/bin/activate
    pip install fastapi uvicorn ffmpeg-python python-multipart Jinja2
    # Ensure ffmpeg binary is installed on your system and on PATH:
    #   macOS (brew):   brew install ffmpeg
    #   Ubuntu/Debian:  sudo apt-get install -y ffmpeg
    #   Windows (choco): choco install ffmpeg
    uvicorn app:app --reload

Visit http://127.0.0.1:8000/ to use the UI.
"""

from __future__ import annotations

import os
import uuid
import tempfile
from pathlib import Path

from fastapi import FastAPI, File, UploadFile, HTTPException
from fastapi.responses import HTMLResponse, FileResponse
import ffmpeg

# Directory to write extracted images
OUTPUT_DIR = Path("outputs")
OUTPUT_DIR.mkdir(exist_ok=True)

app = FastAPI(title="1s Video Screenshot (FastAPI + ffmpeg-python)")


INDEX_HTML = """
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>1s Video Screenshot</title>
    <style>
      body { font-family: system-ui, -apple-system, Segoe UI, Roboto, Helvetica, Arial, sans-serif; margin: 2rem; }
      .card { max-width: 640px; border: 1px solid #e5e7eb; border-radius: 12px; padding: 1.25rem; box-shadow: 0 1px 3px rgba(0,0,0,0.06); }
      h1 { margin-top: 0; }
      input[type=file] { margin: 0.5rem 0 1rem; }
      button { padding: .6rem 1rem; border-radius: 10px; border: 1px solid #111827; background: white; cursor: pointer; }
      .result { margin-top: 1.25rem; }
      .footer { color: #6b7280; font-size: .875rem; margin-top: .75rem; }
    </style>
  </head>
  <body>
    <div class="card">
      <h1>Take screenshot at 1s</h1>
      <form action="/upload" method="post" enctype="multipart/form-data">
        <label for="video">Choose a video file (mp4, mov, webm, mkv, etc.)</label><br />
        <input id="video" name="video" type="file" accept="video/*" required /> <br />
        <button type="submit">Extract frame</button>
      </form>
      <div class="footer">Powered by FastAPI + ffmpeg-python</div>
    </div>
  </body>
</html>
"""


RESULT_HTML = """
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Result - 1s Video Screenshot</title>
    <style>
      body { font-family: system-ui, -apple-system, Segoe UI, Roboto, Helvetica, Arial, sans-serif; margin: 2rem; }
      .card { max-width: 720px; border: 1px solid #e5e7eb; border-radius: 12px; padding: 1.25rem; box-shadow: 0 1px 3px rgba(0,0,0,0.06); }
      h1 { margin-top: 0; }
      img { max-width: 100%; height: auto; border-radius: 10px; border: 1px solid #e5e7eb; }
      .row { display: flex; gap: 1rem; align-items: center; flex-wrap: wrap; }
      .actions a { display: inline-block; margin-right: 0.75rem; padding: .5rem .9rem; border: 1px solid #111827; border-radius: 10px; text-decoration: none; color: #111827; }
      .footer { color: #6b7280; font-size: .875rem; margin-top: .75rem; }
    </style>
  </head>
  <body>
    <div class="card">
      <h1>Frame extracted at 1s</h1>
      <div class="row">
        <img src="/image/{image_name}" alt="Screenshot at 1s" />
      </div>
      <div class="actions" style="margin-top:1rem;">
        <a href="/image/{image_name}" download>Download image</a>
        <a href="/">Process another video</a>
      </div>
      <div class="footer">Saved as <code>{image_name}</code> in <code>outputs/</code></div>
    </div>
  </body>
</html>
"""


@app.get("/", response_class=HTMLResponse)
async def index() -> HTMLResponse:
    return HTMLResponse(INDEX_HTML)


@app.post("/upload", response_class=HTMLResponse)
async def upload(video: UploadFile = File(...)) -> HTMLResponse:
    # Basic validation
    if not video.content_type or not video.content_type.startswith("video/"):
        raise HTTPException(status_code=400, detail="Please upload a valid video file.")

    # Persist upload to a temp file
    suffix = Path(video.filename or "uploaded").suffix or ".mp4"
    with tempfile.NamedTemporaryFile(delete=False, suffix=suffix) as tmp:
        temp_path = Path(tmp.name)
        content = await video.read()
        tmp.write(content)

    # Prepare output image path
    image_name = f"{uuid.uuid4().hex}.jpg"
    out_path = OUTPUT_DIR / image_name

    # Use ffmpeg-python to seek to 1 second and write 1 frame
    # Equivalent shell command: ffmpeg -ss 1 -i input.mp4 -frames:v 1 output.jpg
    try:
        (
            ffmpeg
            .input(str(temp_path), ss=1)
            .output(str(out_path), vframes=1, format='image2', vcodec='mjpeg')
            .overwrite_output()
            .run(capture_stdout=True, capture_stderr=True)
        )
    except ffmpeg.Error as e:
        # surface stderr to help debug common issues
        err = e.stderr.decode(errors="ignore") if isinstance(e.stderr, (bytes, bytearray)) else str(e)
        # Clean up temp file on failure
        try:
            temp_path.unlink(missing_ok=True)  # type: ignore[arg-type]
        finally:
            pass
        raise HTTPException(status_code=500, detail=f"ffmpeg failed: {err}")
    finally:
        # Best-effort cleanup of uploaded temp file
        try:
            temp_path.unlink(missing_ok=True)  # type: ignore[arg-type]
        except Exception:
            pass

    # Return result page with the generated image
    return HTMLResponse(RESULT_HTML.replace("{image_name}", image_name))


@app.get("/image/{image_name}")
async def get_image(image_name: str) -> FileResponse:
    image_path = OUTPUT_DIR / image_name
    if not image_path.exists():
        raise HTTPException(status_code=404, detail="Image not found")
    return FileResponse(path=str(image_path), media_type="image/jpeg", filename=image_name)
