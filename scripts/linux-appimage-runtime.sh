#!/usr/bin/env bash
# Sourced by the generated GTK hook before the desktop process starts.
# Tauri relaunches the AppImage with the running process's environment. Remove
# paths owned by its previous mount before linuxdeploy adds this mount's paths.
# Older releases have no mount marker, but do mark their bundled font policy.
beambench_previous_appdir="${BEAMBENCH_APPIMAGE_RUNTIME_DIR:-}"
if [ -z "$beambench_previous_appdir" ] &&
    [ "${BEAMBENCH_APPIMAGE_FONT_POLICY:-}" = 1 ] &&
    [[ "${FONTCONFIG_FILE:-}" = */usr/share/beambench/fonts.conf ]]; then
  beambench_previous_appdir="${FONTCONFIG_FILE%/usr/share/beambench/fonts.conf}"
fi
# Before 0.2.31, linuxdeploy also exported its empty GStreamer directories.
# This fallback covers legacy launches with an explicit custom font config.
beambench_legacy_gst_paths="${GST_PLUGIN_SYSTEM_PATH_1_0-}:${GST_PLUGIN_SYSTEM_PATH-}"
if [ -z "$beambench_previous_appdir" ] &&
    [[ "$beambench_legacy_gst_paths" =~ (^|:)(/[^:]*/\.mount_[^:/]*)/usr/lib/gstreamer(-1\.0)?(:|$) ]]; then
  beambench_previous_appdir="${BASH_REMATCH[2]}"
fi

beambench_remove_previous_mount_paths() {
  local key="$1" rest part result="" kept=0 changed=0
  [ "${!key+x}" = x ] || return 0
  rest="${!key}"
  while :; do
    part="${rest%%:*}"
    case "$part" in
      "$beambench_previous_appdir"|"$beambench_previous_appdir"/*) changed=1 ;;
      *)
        if [ "$kept" = 1 ]; then result+=":"; fi
        result+="$part"
        kept=1
        ;;
    esac
    [[ "$rest" = *:* ]] || break
    rest="${rest#*:}"
  done
  if [ "$changed" = 1 ]; then
    if [ -n "$result" ]; then
      export "$key=$result"
    else
      unset "$key"
    fi
  fi
}

if [[ "$beambench_previous_appdir" = /* ]] &&
    [ "$beambench_previous_appdir" != / ] &&
    [ "$beambench_previous_appdir" != "$APPDIR" ]; then
  for beambench_path_key in PATH LD_LIBRARY_PATH XDG_DATA_DIRS QT_PLUGIN_PATH \
      PYTHONPATH PERLLIB GST_PLUGIN_SYSTEM_PATH GST_PLUGIN_SYSTEM_PATH_1_0 \
      GST_PLUGIN_PATH GST_PLUGIN_PATH_1_0; do
    beambench_remove_previous_mount_paths "$beambench_path_key"
  done
  if [ "${BEAMBENCH_APPIMAGE_FONT_POLICY:-}" = 1 ] &&
      [ "${FONTCONFIG_FILE:-}" = "$beambench_previous_appdir/usr/share/beambench/fonts.conf" ]; then
    unset FONTCONFIG_FILE
  fi
fi
export BEAMBENCH_APPIMAGE_RUNTIME_DIR="$APPDIR"
unset beambench_previous_appdir beambench_legacy_gst_paths beambench_path_key
unset -f beambench_remove_previous_mount_paths

# AppRun.wrapped replaces these with paths into the AppImage. Preserve the
# original values for the executable wrapper, including explicitly empty paths.
export BEAMBENCH_ORIGINAL_GST_PLUGIN_SYSTEM_PATH_SET="${GST_PLUGIN_SYSTEM_PATH+x}"
export BEAMBENCH_ORIGINAL_GST_PLUGIN_SYSTEM_PATH="${GST_PLUGIN_SYSTEM_PATH-}"
export BEAMBENCH_ORIGINAL_GST_PLUGIN_SYSTEM_PATH_1_0_SET="${GST_PLUGIN_SYSTEM_PATH_1_0+x}"
export BEAMBENCH_ORIGINAL_GST_PLUGIN_SYSTEM_PATH_1_0="${GST_PLUGIN_SYSTEM_PATH_1_0-}"

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
