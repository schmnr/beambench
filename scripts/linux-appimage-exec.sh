#!/usr/bin/env bash
set -euo pipefail

# Run after linuxdeploy's AppRun.wrapped has set its environment. The AppImage
# uses host GStreamer libraries and plugins, so its empty bundled plugin path
# must not replace GStreamer's default system search. Keep explicit user paths.
if [ "${BEAMBENCH_ORIGINAL_GST_PLUGIN_SYSTEM_PATH_SET:-}" = x ]; then
  export GST_PLUGIN_SYSTEM_PATH="${BEAMBENCH_ORIGINAL_GST_PLUGIN_SYSTEM_PATH-}"
else
  unset GST_PLUGIN_SYSTEM_PATH
fi
if [ "${BEAMBENCH_ORIGINAL_GST_PLUGIN_SYSTEM_PATH_1_0_SET:-}" = x ]; then
  export GST_PLUGIN_SYSTEM_PATH_1_0="${BEAMBENCH_ORIGINAL_GST_PLUGIN_SYSTEM_PATH_1_0-}"
else
  unset GST_PLUGIN_SYSTEM_PATH_1_0
fi
unset BEAMBENCH_ORIGINAL_GST_PLUGIN_SYSTEM_PATH_SET BEAMBENCH_ORIGINAL_GST_PLUGIN_SYSTEM_PATH
unset BEAMBENCH_ORIGINAL_GST_PLUGIN_SYSTEM_PATH_1_0_SET BEAMBENCH_ORIGINAL_GST_PLUGIN_SYSTEM_PATH_1_0

exec "$(dirname -- "$(readlink -f "$0")")/beambench-tauri.bin" "$@"
