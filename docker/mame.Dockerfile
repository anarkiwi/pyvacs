FROM ubuntu:25.10
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        mame xvfb xauth python3 python3-pip python3-venv \
    && rm -rf /var/lib/apt/lists/*
ENV PATH=/usr/games:$PATH
