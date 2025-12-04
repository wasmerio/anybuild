# Image Processing with Pillow + Wasmer

This demo shows how to use **Pillow (PIL)** for basic image transformations such as **rotation** and **cropping**. The program happens to wrap these operations in a small FastAPI app, but the focus here is on the **Pillow usage**.

## Demo

https://python-pillow-example.wasmer.app

## How it Works (sections from `app.py`)

All logic lives in one file. Here are the **relevant code sections** related to Pillow:

### Loading an image

The image is downloaded from a user-provided URL and loaded into memory:

```python
response = requests.get(url)
image = Image.open(BytesIO(response.content))
```

Key point: `Image.open()` can read from file paths, file objects, or in this case, an in-memory `BytesIO`.

### Rotation

Rotation is straightforward with Pillow:

```python
if rotate:
    image = image.rotate(rotate, expand=True)
```

* `rotate(degrees)` rotates counterclockwise by the given angle.
* `expand=True` resizes the output image so the full rotated image fits (without clipping).

### Cropping

Cropping uses a bounding box defined by `(left, upper, right, lower)` pixel coordinates:

```python
crop_box = tuple(map(int, crop.split(",")))
image = image.crop(crop_box)
```

* Example: `"0,0,200,200"` crops the top-left 200×200 region.
* Invalid crop strings are caught with error handling.

### Saving to Bytes

Finally, the image is re-encoded into PNG format and written to a `BytesIO` buffer:

```python
img_io = BytesIO()
image.save(img_io, format="PNG")
img_io.seek(0)
```

This makes it easy to return the processed image over HTTP or save it to disk.

---

## Running Locally

1. Create a `requirements.txt`:

```
fastapi
uvicorn
pillow
requests
```

2. Install dependencies:

```bash
pip install -r requirements.txt
```

3. Run the server:

```bash
uvicorn app:app --host 0.0.0.0 --port 8000
```

4. Test locally by visiting:

```
http://127.0.0.1:8000/process?url=https://picsum.photos/200/300&rotate=45&crop=0,0,200,200
```

This downloads an image, rotates it 45°, crops it, and returns the transformed PNG.

---

## Deploying to Wasmer Edge (Overview)

1. Include `app.py` and `requirements.txt`.
2. Deploy to Wasmer Edge and run with:
   `uvicorn app:app --host 0.0.0.0 --port $PORT`
3. Visit `https://<your-subdomain>.wasmer.app/process?...` with query parameters for URL, rotation, and crop.

---

⚡ The key takeaway is how **Pillow** enables quick in-memory image processing with `Image.open`, `rotate`, `crop`, and `save`.

Would you like me to extend the README with **extra Pillow examples** (like resizing, converting formats, or adding filters) so the demo feels more complete?
