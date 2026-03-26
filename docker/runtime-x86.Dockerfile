FROM --platform=linux/amd64 rust:1.75-bookworm

ENV DEBIAN_FRONTEND=noninteractive
ENV http_proxy=
ENV https_proxy=
ENV HTTP_PROXY=
ENV HTTPS_PROXY=
ENV no_proxy=
ENV NO_PROXY=

RUN unset http_proxy https_proxy HTTP_PROXY HTTPS_PROXY no_proxy NO_PROXY \
    && apt-get update \
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

RUN ln -sf /usr/local/cargo/bin/cargo /usr/local/bin/cargo \
    && ln -sf /usr/local/cargo/bin/rustc /usr/local/bin/rustc \
    && ln -sf /usr/local/cargo/bin/rustdoc /usr/local/bin/rustdoc \
    && ln -sf /usr/local/cargo/bin/rustfmt /usr/local/bin/rustfmt

WORKDIR /workspace

ENV CARGO_TARGET_DIR=/workspace/target/runtime-x86-linux-amd64

CMD ["bash"]
