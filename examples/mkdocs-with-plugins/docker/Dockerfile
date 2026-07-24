FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get -y --no-install-recommends install sudo curl ca-certificates locate git zip unzip \
    && rm -rf /var/lib/apt/lists/*

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

RUN curl https://pkgx.sh | sh

RUN curl https://get.wasmer.io -sSfL | sh
ENV PATH="/root/.wasmer/bin:${PATH}"
WORKDIR /app
RUN pkgm install python@3.13
RUN pkgm install uv
RUN uv init --no-managed-python
RUN \
  --mount=type=bind,source=requirements.txt,target=requirements.txt \
  uv add -r requirements.txt
COPY . .
RUN uv run mkdocs build
RUN mkdir -p /anybuild/serve/bin
RUN curl --proto '=https' --tlsv1.2 -sSfL https://get.static-web-server.net | sh

RUN cat > /anybuild/serve/bin/start <<'EOF'
#!/bin/bash
cd /app
static-web-server --log-level=info --root /app/site
EOF

RUN chmod 755 /anybuild/serve/bin/start
