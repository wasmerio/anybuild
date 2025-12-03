import pypandoc
from fastapi import FastAPI

app = FastAPI()


@app.get("/")
async def root():
    output = pypandoc.convert_text("# some title", "rst", format="md")
    return {"message": output}


@app.get("/convert")
async def convert(input: str, output: str, format: str = "md"):
    output = pypandoc.convert_text(input, output, format=format)
    return {"message": output}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8000)
