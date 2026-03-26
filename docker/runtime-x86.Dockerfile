FROM --platform=linux/amd64 rust:1.75-bookworm

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        bash \
        ca-certificates \
        clang \
        file \
        gcc \
        git \
        gnucobol \
        hyperfine \
        make \
        perl \
        pkg-config \
        python3 \
        xz-utils \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace

ENV CARGO_TARGET_DIR=/workspace/target/runtime-x86-linux-amd64

CMD ["bash"]
