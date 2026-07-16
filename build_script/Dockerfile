FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive
ENV ANDROID_HOME=/opt/android-sdk
ENV ANDROID_NDK_HOME=/opt/android-ndk
ENV PATH="${ANDROID_HOME}/cmdline-tools/latest/bin:${ANDROID_HOME}/platform-tools:${PATH}"
ENV CARGO_HOME=/root/.cargo
ENV PATH="${CARGO_HOME}/bin:${PATH}"

RUN apt-get update && apt-get install -y \
    curl \
    build-essential \
    pkg-config \
    musl-tools \
    openjdk-17-jdk-headless \
    unzip \
    && rm -rf /var/lib/apt/lists/*

ENV CC_x86_64_unknown_linux_musl=musl-gcc
ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=musl-gcc

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable

RUN rustup target add x86_64-unknown-linux-musl
RUN rustup target add aarch64-linux-android
RUN rustup target add x86_64-linux-android

RUN cargo install cargo-ndk

ENV ANDROID_CMDLINE_TOOLS_URL=https://dl.google.com/android/repository/commandlinetools-linux-11076708_latest.zip
RUN curl -sSfLo /tmp/cmdline-tools.zip "${ANDROID_CMDLINE_TOOLS_URL}" \
    && mkdir -p "${ANDROID_HOME}/cmdline-tools" \
    && unzip -q /tmp/cmdline-tools.zip -d /tmp/cmdline-tools-tmp \
    && mv /tmp/cmdline-tools-tmp/cmdline-tools "${ANDROID_HOME}/cmdline-tools/latest" \
    && rm -rf /tmp/cmdline-tools.zip /tmp/cmdline-tools-tmp

RUN yes | "${ANDROID_HOME}/cmdline-tools/latest/bin/sdkmanager" --sdk_root="${ANDROID_HOME}" \
    "platform-tools" \
    "platforms;android-35" \
    "build-tools;35.0.0" \
    "ndk;27.0.12077973"

ENV ANDROID_NDK_HOME="${ANDROID_HOME}/ndk/27.0.12077973"

WORKDIR /workspace

COPY . .

COPY build_script/build_all.sh /workspace/build_all.sh
RUN chmod +x /workspace/build_all.sh

ENTRYPOINT ["/workspace/build_all.sh"]
