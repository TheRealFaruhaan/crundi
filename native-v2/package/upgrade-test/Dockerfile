# BASE: the oldest Ubuntu the package runs on (24.04); check.sh picks a newer
# one only when the client binary was built against a newer glibc.
ARG BASE=ubuntu:24.04
FROM ${BASE}
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
    xvfb xauth dbus-x11 procps psmisc ca-certificates curl xz-utils \
    libgtk-3-0t64 libnotify4 libnss3 libxss1 libxtst6 xdg-utils libatspi2.0-0t64 libuuid1 libsecret-1-0 \
    libayatana-appindicator3-1 libasound2t64 libgbm1 libdrm2 libxkbcommon0 libxkbcommon-x11-0 \
    libvulkan1 mesa-vulkan-drivers libegl1 libgl1 libwayland-client0 libxcb1 fontconfig fonts-dejavu-core \
 && rm -rf /var/lib/apt/lists/*
RUN useradd -m tester
USER tester
WORKDIR /home/tester
