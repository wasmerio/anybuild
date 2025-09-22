"""
FastAPI app that serves a tiny web UI (pure Python-rendered HTML) to enter a video URL,
downloads it with requests, and extracts the frame at 1 second using ffmpeg-python.

Quickstart (run these in your terminal):

    python -m venv .venv && source .venv/bin/activate
    pip install fastapi uvicorn ffmpeg-python requests Jinja2
    # Ensure ffmpeg binary is installed on your system and on PATH:
    #   macOS (brew):   brew install ffmpeg
    #   Ubuntu/Debian:  sudo apt-get install -y ffmpeg
    #   Windows (choco): choco install ffmpeg
    uvicorn app:app --reload

Visit http://127.0.0.1:8000/ to use the UI.
"""

from __future__ import annotations

import base64
import tempfile
from pathlib import Path
import uuid

import requests
from fastapi import FastAPI, HTTPException, Query
from fastapi.responses import HTMLResponse
import ffmpeg

# Directory to write extracted images (temporary)
OUTPUT_DIR = Path("/app/outputs")
OUTPUT_DIR.mkdir(exist_ok=True)

app = FastAPI(title="1s Video Screenshot (URL + ffmpeg-python)")


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
      input[type=url] { width: 100%; padding: .5rem; margin: 0.5rem 0 1rem; border-radius: 6px; border: 1px solid #d1d5db; }
      button { padding: .6rem 1rem; border-radius: 10px; border: 1px solid #111827; background: white; cursor: pointer; }
      .footer { color: #6b7280; font-size: .875rem; margin-top: .75rem; }
    </style>
  </head>
  <body>
    <div class="card">
      <h1>Take screenshot at 1s from a video URL</h1>
      <form action="/process" method="get">
        <label for="url">Video URL</label><br />
        <input id="url" name="url" type="url" placeholder="https://..." value="https://interactive-examples.mdn.mozilla.net/media/cc0-videos/flower.mp4" required /> <br />
        <label for="time">Time in seconds</label><br />
        <input id="time" name="time" type="number" placeholder="1" value="1" required /> <br />
        <button type="submit">Extract frame</button>
      </form>
      <div class="footer">Powered by FastAPI + ffmpeg-python + requests</div>
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
      .footer { color: #6b7280; font-size: .875rem; margin-top: .75rem; }
    </style>
  </head>
  <body>
    <div class="card">
      <h1>Frame extracted at {time}s</h1>
      <div>
        <img src="data:image/jpeg;base64,{image_b64}" alt="Screenshot from a video URL" />
      </div>
      <div class="footer">Embedded as base64 JPEG</div>
    </div>
  </body>
</html>
"""


@app.get("/", response_class=HTMLResponse)
async def index() -> HTMLResponse:
    return HTMLResponse(INDEX_HTML)


@app.get("/process", response_class=HTMLResponse)
async def process(url: str = Query(..., description="Video URL"), time: int = Query(1, description="Time in seconds")) -> HTMLResponse:
    # Download video to a temp file
    try:
        resp = requests.get(url, stream=True, timeout=30)
        resp.raise_for_status()
    except Exception as e:
        raise HTTPException(status_code=400, detail=f"Could not download video: {e}")

    suffix = ".mp4"  # default extension
    with tempfile.NamedTemporaryFile(delete=False, suffix=suffix) as tmp:
        temp_path = Path(tmp.name)
        for chunk in resp.iter_content(chunk_size=8192):
            if chunk:
                tmp.write(chunk)

    # Prepare output image path
    out_path = OUTPUT_DIR / f"{uuid.uuid4().hex}.jpg"


    # Use ffmpeg-python to seek to 1 second and write 1 frame
    try:
        (
            ffmpeg
            .input(str(temp_path), ss=time)
            .output(str(out_path), vframes=1, format='image2', vcodec='mjpeg')
            .overwrite_output()
            .run(capture_stdout=True, capture_stderr=True)
        )
    except ffmpeg.Error as e:
        err = e.stderr.decode(errors="ignore") if isinstance(e.stderr, (bytes, bytearray)) else str(e)
        try:
            temp_path.unlink(missing_ok=True)
        finally:
            pass
        raise HTTPException(status_code=500, detail=f"ffmpeg failed: {err}")
    finally:
        try:
            temp_path.unlink(missing_ok=True)
        except Exception:
            pass

    # Read image and encode as base64
    try:
        with open(out_path, "rb") as f:
            img_bytes = f.read()
        image_b64 = base64.b64encode(img_bytes).decode("ascii")
    except Exception:
        raise HTTPException(status_code=500, detail=f"Failed to read image (did the video has less than {time} seconds?)")
    finally:
        try:
            out_path.unlink(missing_ok=True)
        except Exception:
            pass

    return HTMLResponse(RESULT_HTML.replace("{time}", str(time)).replace("{image_b64}", image_b64))
