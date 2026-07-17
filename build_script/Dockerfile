FROM ubuntu:24.04

ARG ANDROID_COMMAND_LINE_TOOLS=11076708
ARG ANDROID_NDK_VERSION=27.0.12077973

ENV DEBIAN_FRONTEND=noninteractive \
    ANDROID_HOME=/opt/android-sdk \
    ANDROID_NDK_HOME=/opt/android-sdk/ndk/27.0.12077973 \
    CARGO_HOME=/root/.cargo \
    CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=musl-gcc \
    CC_x86_64_unknown_linux_musl=musl-gcc
ENV PATH="${CARGO_HOME}/bin:${ANDROID_HOME}/cmdline-tools/latest/bin:${ANDROID_HOME}/platform-tools:${PATH}"

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        curl \
        musl-tools \
        openjdk-17-jdk-headless \
        pkg-config \
        unzip \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --default-toolchain stable \
    && rustup target add \
        x86_64-unknown-linux-musl \
        aarch64-linux-android \
        x86_64-linux-android \
    && cargo install cargo-ndk --locked

RUN curl -fsSL \
        "https://dl.google.com/android/repository/commandlinetools-linux-${ANDROID_COMMAND_LINE_TOOLS}_latest.zip" \
        -o /tmp/android-tools.zip \
    && mkdir -p "${ANDROID_HOME}/cmdline-tools" /tmp/android-tools \
    && unzip -q /tmp/android-tools.zip -d /tmp/android-tools \
    && mv /tmp/android-tools/cmdline-tools "${ANDROID_HOME}/cmdline-tools/latest" \
    && rm -rf /tmp/android-tools /tmp/android-tools.zip \
    && yes | sdkmanager --sdk_root="${ANDROID_HOME}" --licenses >/dev/null || true

RUN sdkmanager --sdk_root="${ANDROID_HOME}" \
        "platform-tools" \
        "platforms;android-35" \
        "build-tools;34.0.0" \
        "build-tools;35.0.0" \
        "ndk;${ANDROID_NDK_VERSION}"

WORKDIR /workspace
COPY . .

# The open-source builds never enable the private `official` feature, but
# Cargo still resolves its optional path dependency. Supply a metadata-only
# stub at the path expected when ws is the Docker build context.
RUN mkdir -p /data/src \
    && printf '%s\n' \
        '[package]' \
        'name = "data"' \
        'version = "0.0.0"' \
        'edition = "2024"' \
        > /data/Cargo.toml \
    && printf '%s\n' '#![allow(dead_code)]' > /data/src/lib.rs \
    && chmod +x /workspace/build_script/build_all.sh

ENTRYPOINT ["/workspace/build_script/build_all.sh"]
