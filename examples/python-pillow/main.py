from fastapi import FastAPI, Query
from fastapi.responses import HTMLResponse, StreamingResponse
from PIL import Image
import requests
from io import BytesIO

app = FastAPI()

# HTML form to input the image URL
form_html = """
<!DOCTYPE html>
<html>
    <body>
        <h2>Image Crop & Rotate</h2>
        <form action="/process" method="get">
            <label for="url">Image URL:</label><br>
            <input type="text" id="url" name="url" size="60" value="https://picsum.photos/200/300"><br><br>
            <label for="rotate">Rotate (degrees):</label><br>
            <input type="number" id="rotate" name="rotate" value="45"><br><br>
            <label for="crop">Crop (left,upper,right,lower):</label><br>
            <input type="text" id="crop" name="crop" value="0,0,200,200"><br><br>
            <input type="submit" value="Submit">
        </form>
    </body>
</html>
"""

@app.get("/", response_class=HTMLResponse)
async def read_form():
    return form_html

@app.get("/process")
async def process_image(
    url: str = Query(...),
    rotate: int = Query(0),
    crop: str = Query("0,0,200,200"),
):
    # Download the image from URL
    response = requests.get(url)
    image = Image.open(BytesIO(response.content))

    # Apply rotation
    if rotate:
        image = image.rotate(rotate, expand=True)

    # Apply crop
    try:
        crop_box = tuple(map(int, crop.split(",")))
        image = image.crop(crop_box)
    except Exception as e:
        return {"error": f"Invalid crop values: {e}"}

    # Return as image response
    img_io = BytesIO()
    image.save(img_io, format="PNG")
    img_io.seek(0)
    return StreamingResponse(img_io, media_type="image/png")
