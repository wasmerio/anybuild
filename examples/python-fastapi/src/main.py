import os
from fastapi import FastAPI

app = FastAPI()


@app.get("/")
async def root():
    return {"message": "Hello World from fastapi!"}


if __name__ == "__main__":
    import uvicorn
    PORT = int(os.getenv("PORT") or 8000)

    uvicorn.run(app, host="0.0.0.0", port=PORT)
