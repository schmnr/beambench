#!/usr/bin/env bash
# Sourced by the generated GTK hook before the desktop process starts.
if [ -z "${GDK_BACKEND:-}" ]; then
  if [ -n "${DISPLAY:-}" ]; then
    if [ -n "${WAYLAND_DISPLAY:-}" ]; then
      export GDK_BACKEND=x11,wayland
    else
      export GDK_BACKEND=x11
    fi
  elif [ -n "${WAYLAND_DISPLAY:-}" ]; then
    export GDK_BACKEND=wayland
  fi
fi

if [ -z "${FONTCONFIG_FILE:-}" ]; then
  export FONTCONFIG_FILE="$APPDIR/usr/share/beambench/fonts.conf"
  export BEAMBENCH_APPIMAGE_FONT_POLICY=1
fi
