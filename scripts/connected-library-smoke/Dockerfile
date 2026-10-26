# Test host only. These libraries never enter an AppImage or release build.
FROM ubuntu:24.04
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates python3 python3-gi python3-dbus python3-pyatspi \
    xvfb xauth dbus-x11 at-spi2-core gnome-keyring libsecret-tools \
    libwebkit2gtk-4.1-0 webkit2gtk-driver openbox xdotool wmctrl scrot \
    kwalletmanager kwalletcli kdialog libqca-qt5-2-plugins \
    fonts-dejavu-core libgl1-mesa-dri libglx0 \
    gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
    && rm -rf /var/lib/apt/lists/*
RUN useradd --create-home --uid 1100 beamtest
ENV HOME=/home/beamtest
USER 1100
WORKDIR /home/beamtest
