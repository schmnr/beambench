ARG UBUNTU_VERSION=22.04
FROM ubuntu:${UBUNTU_VERSION}
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
    python3 xvfb xauth dbus-x11 libwebkit2gtk-4.1-0 \
    gstreamer1.0-tools gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
    fontconfig fonts-dejavu-core libgl1-mesa-dri libglx0 \
    && rm -rf /var/lib/apt/lists/*
RUN useradd --create-home --uid 1100 beamtest
ENV HOME=/home/beamtest
USER 1100
WORKDIR /home/beamtest
