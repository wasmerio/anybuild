# Video Frame Extraction with ffmpeg-python + Wasmer

This demo shows how to use **ffmpeg-python** to extract a single frame from a video file. The app happens to include a small FastAPI wrapper, but the main focus is on how **ffmpeg is invoked from Python**.

## Demo

https://python-ffmpeg-example.wasmer.app

## How it Works (sections from `app.py`)

All logic lives in one file. Here are the **relevant code sections** related to ffmpeg:

### Downloading the video

A video is fetched from a provided URL and written into a temporary file:

```python
resp = requests.get(url, stream=True, timeout=30)
with tempfile.NamedTemporaryFile(delete=False, suffix=".mp4") as tmp:
    for chunk in resp.iter_content(chunk_size=8192):
        if chunk:
            tmp.write(chunk)
    temp_path = Path(tmp.name)
```

This prepares the video for frame extraction.

### Running ffmpeg with Python

The core operation uses **ffmpeg-python**, which provides a fluent interface for building ffmpeg command lines:

```python
out_path = OUTPUT_DIR / f"{uuid.uuid4().hex}.jpg"

(
    ffmpeg
    .input(str(temp_path), ss=time)   # seek to timestamp (in seconds)
    .output(str(out_path), vframes=1, format='image2', vcodec='mjpeg')  
    .overwrite_output()
    .run(capture_stdout=True, capture_stderr=True)
)
```

Key points:

* `.input(file, ss=time)` → loads the video and seeks to the desired timestamp (in seconds).
* `.output(out_path, vframes=1, format='image2', vcodec='mjpeg')` → configures ffmpeg to write just one frame as a JPEG.
* `.overwrite_output()` → allows overwriting an existing file.
* `.run(...)` → executes the ffmpeg command, capturing logs if needed.

### Reading the result

The extracted image is read from disk and used (e.g., embedded in HTML or returned over an API):

```python
with open(out_path, "rb") as f:
    img_bytes = f.read()
```

---

## Running Locally

1. Create a `requirements.txt`:

```
fastapi
uvicorn
ffmpeg-python
requests
```

2. Install dependencies:

```bash
pip install -r requirements.txt
```

3. Make sure **ffmpeg binary** is installed on your system and in your PATH. Examples:

   * macOS: `brew install ffmpeg`
   * Ubuntu/Debian: `sudo apt-get install -y ffmpeg`
   * Windows (choco): `choco install ffmpeg`

4. Run the server:

```bash
uvicorn app:app --host 0.0.0.0 --port 8000
```

5. Test frame extraction:

```bash
curl "http://127.0.0.1:8000/process?url=https://.../video.mp4&time=1"
```

---

## Deploying to Wasmer Edge (Overview)

1. Include `app.py` and `requirements.txt`.
2. Ensure the **ffmpeg binary** is available in your deployment environment.
3. Deploy to Wasmer Edge and run the app with:
   `uvicorn app:app --host 0.0.0.0 --port $PORT`
4. Visit `https://<your-subdomain>.wasmer.app/process?...` to extract frames.

---

⚡ The key takeaway is how **ffmpeg-python** mirrors the ffmpeg CLI syntax in Python, making it easy to seek into a video and output a still frame with just a few chained calls.

Would you like me to also show the **equivalent raw ffmpeg CLI command** that this Python chain produces, so readers can compare the two directly?
