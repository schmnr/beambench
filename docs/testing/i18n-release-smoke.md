# i18n Release Smoke Tests

Pre-release verification that the translation pipeline works end-to-end in the **real Tauri app**, not just the browser-rendered Vite asset server. This is the replacement for the removed Playwright suite: an agent (Computer Use) or a human runs these against a `npx tauri dev` window before tagging a build.

These cases focus on the surface that automated unit/integration tests (Vitest + jsdom) **cannot** cover:

- Native macOS menu labels
- Settings persistence round-trip across restart
- Real font rendering of CJK / Greek / Cyrillic scripts
- Long-string layout overflow at real DPI
- React → Rust IPC label sync via `rebuild_native_menu`

Shared setup:

- Hardware Requirement: None
- Prerequisites: Fresh local app state (no persisted `display_language`, no recent files)
- Setup: `cd tauri-app && npx tauri dev`; close the dev tools panel
- Window size: default (1280×800), unless the case overrides

### Platform routing — which menu the verifier uses

Beam Bench shows different menus depending on the OS:

- **macOS**: the React `MenuBar` is **hidden** (`AppShell.tsx:72`: `{!isNativeMenuActive() && <MenuBar />}`); only the native menu bar at the top of the screen is interactive. All menu cases below execute against the **native menu**, including the Language submenu.
- **Linux / Windows**: there is no native menu and the React `MenuBar` is the only menu. All menu cases execute against the **React MenuBar** inside the app window.

When the cases below say "Language menu" or "MenuBar", use the platform-appropriate one. The native menu and the React menu carry the same set of locales, so the labels to verify are identical; only the location differs.

**Coverage note (2026-05-29):** Localization is now full-coverage, not Wave-1-scoped. Leaf items in File/Edit/Tools/Arrange/Laser Tools/Window, all panels, toolbars, the status bar, dialogs, and the hotkey editor are translated across all 23 locales. The older "leaf items fall back to English in Wave 1" caveat no longer applies, and the "Report Translation Issue" command has been removed. Treat a leaf item still showing English as a real miss to report, not an accepted gap.

## I18N-001 — Default English Launch

- Source Ref: i18n design spec, `src/i18n/index.ts:81` (default `lng: 'en'`)
- Feature / Function: First-launch locale defaults to English
- Steps:
  1. Launch fresh app
  2. Open the **Language** menu (native on macOS, React MenuBar elsewhere — see Platform routing above)
- Expected Result:
  - The Language menu label shows **Language** in English
  - The submenu opens with **English** at the top, checkmarked
  - All 23 entries present (verify count: scroll the menu, count the locales)
  - Dev-only `en-XA (Pseudo-locale)` entry — **React menu only** (dev builds); the native macOS menu does not register `en-XA`. Skip this expectation if verifying on macOS.
- Edge Cases: First launch with no settings file persisted — must default to English
- Persistence Check: Close the menu; verify no `display_language` write occurred (no UI change)
- Status: Active

## I18N-002 — Switch to German

- Source Ref: `MenuBar.tsx:1476`, `appCommands.ts:338`, `appStore.ts:82` (store-routed dispatch); `App.tsx:200` (rebuild_native_menu effect)
- Feature / Function: Language switch updates UI immediately via store, no restart
- Steps (Linux / Windows):
  1. Open the React MenuBar Language menu
  2. Click **Deutsch (German)**
- Steps (macOS):
  1. Open the native Language menu in the menu bar
  2. Click **Deutsch (German)**
- Expected Result (all platforms):
  - Menu closes immediately
  - In-app React strings translate: e.g., open SettingsDialog and confirm the window title is **Einstellungen**, AboutDialog tagline reads **Von Machern für Macher.**
- Expected Result (macOS only):
  - Native menu bar top-level relabels to: **Datei / Bearbeiten / Werkzeuge / Anordnen / Laserwerkzeuge / Fenster / Sprache / Hilfe**
  - Help submenu items relabel: **Schnellhilfe / Fehler melden... / Über Beam Bench**
  - App-menu Quit item becomes **Beam Bench beenden**
  - File submenu **Recent Projects** becomes **Zuletzt verwendet**, empty placeholder becomes **Keine aktuellen Projekte**
  - No flash of English between rebuild and state re-apply (the `rebuild_native_menu` Tauri command is atomic)
  - A checkmark moves from **English** to **Deutsch (German)** in the Language submenu
- Expected Result (Linux / Windows):
  - React MenuBar top-level label changes from **Language** to **Sprache**
  - All currently-translated React menu strings reflect the new locale
- Persistence Check: Quit app, relaunch — UI comes back in German, **Deutsch (German)** still checkmarked
- Leaf items: File/Edit/Tools/Arrange/Laser Tools/Window leaf items (Open, Save, Undo, etc.) are now translated too; if any still render in English, report it as a miss
- Status: Active

## I18N-003 — Switch to Japanese (CJK rendering)

- Source Ref: `src/locales/ja.json`, `nativeMenuLabels.ts`
- Feature / Function: CJK font coverage + real glyph rendering
- Steps:
  1. Language → **日本語 (Japanese)**
- Expected Result:
  - React Language menu label becomes **言語**
  - SettingsDialog title (open via menu) shows **環境設定**
  - About dialog title section shows **バージョン** label
  - Native menu top-level: **ファイル / 編集 / ツール / 整列 / レーザーツール / ウィンドウ / 言語 / ヘルプ**
  - Native Help submenu: **クイックヘルプ / バグを報告... / Beam Bench について**
- Tofu Acceptance Criteria:
  - **No empty squares (□) where text should be** — if any visible character renders as a hollow box, that's a tofu failure
  - Verify by zooming the screenshot or eyeballing each menu item
  - The brand name "Beam Bench" stays in Latin script (intentional, not a tofu)
- Edge Cases: If macOS system fonts are missing the JP CJK family, this case will visibly fail — that's a real failure mode worth catching
- Persistence Check: Quit, relaunch — Japanese persists
- Status: Active

## I18N-004 — Switch to Simplified Chinese (CJK rendering)

- Source Ref: `src/locales/zh-CN.json`
- Feature / Function: Simplified Chinese CJK rendering
- Steps:
  1. Language → **简体中文 (Simplified Chinese)**
- Expected Result:
  - React Language menu label becomes **语言**
  - Native menu top-level: **文件 / 编辑 / 工具 / 排列 / 激光工具 / 窗口 / 语言 / 帮助**
- Tofu Acceptance Criteria: Same as I18N-003 — no hollow boxes
- Status: Active

## I18N-005 — Switch to Korean (CJK rendering)

- Source Ref: `src/locales/ko.json`
- Feature / Function: Korean Hangul rendering
- Steps:
  1. Language → **한국어 (Korean)**
- Expected Result:
  - React Language menu label becomes **언어**
  - Native menu top-level: **파일 / 편집 / 도구 / 정렬 / 레이저 도구 / 창 / 언어 / 도움말**
- Tofu Acceptance Criteria: Same as I18N-003
- Status: Active

## I18N-006 — German long-string layout stress

- Source Ref: `src/locales/de.json`; layout robustness against systematically longer translations
- Feature / Function: Verify no clipping, wrap, or overlap at production string lengths
- Steps:
  1. Language → Deutsch (German)
  2. Open SettingsDialog
  3. Visit each tab: **Allgemein / Einheiten und Raster / Anzeige / Datei und Import**
- Expected Result:
  - Every form-row label fits on one line without truncation
  - No button text overflows its boundary
  - Tab labels do not wrap
  - Toggle descriptions ("Aus hält die lokale API verfügbar...") wrap cleanly within their containers
- Edge Cases: Try resizing the SettingsDialog to 640×520 (its declared `minWidth × minHeight`) — labels must still fit
- Status: Active

## I18N-007 — Pseudo-locale (`en-XA`) width-stress check

- Source Ref: `src/i18n/pseudo.ts`; dev-only stress test for layout
- Feature / Function: 30%-padded accented strings reveal layout overflow
- Steps:
  1. Language → **en-XA (Pseudo-locale)** (dev menu entry only)
  2. Open MenuBar, SettingsDialog, AboutDialog
- Expected Result:
  - Every translated string wraps in `⟦…⟧` markers
  - Strings render at ~130% of English length
  - **No clipping, no horizontal scroll** appears in any panel or dialog
  - Accented characters (ä, é, ö, ü, ǵ, ŝ etc.) render as real glyphs, not tofu
- Edge Cases: Toggle through every SettingsDialog tab, expand the Language menu, open AboutDialog — verify pseudo-locale on every surface
- Persistence Check: Quit, relaunch — `en-XA` is NOT persisted (intentional). App should come back in the last real locale or English.
- Status: Active

## I18N-008 — Cyrillic (`ru`) script rendering

- Source Ref: `src/locales/ru.json`
- Feature / Function: Cyrillic font coverage
- Steps:
  1. Language → **Русский (Russian)**
- Expected Result:
  - React Language menu label: **Язык**
  - Native menu top-level: **Файл / Правка / Инструменты / Упорядочить / Лазерные инструменты / Окно / Язык / Справка**
  - SettingsDialog title: **Настройки**
- Tofu Acceptance Criteria: Same as I18N-003 — no hollow boxes for Cyrillic characters
- Status: Active

## I18N-009 — Greek (`el`) script rendering

- Source Ref: `src/locales/el.json`
- Feature / Function: Greek font coverage
- Steps:
  1. Language → **Ελληνικά (Greek)**
- Expected Result:
  - React Language menu label: **Γλώσσα**
  - SettingsDialog title: **Προτιμήσεις**
- Tofu Acceptance Criteria: Same — no hollow boxes
- Status: Active

## I18N-010 — Settings persistence round-trip

- Source Ref: `persist::load_settings`, `settings.rs::normalize_display_language`
- Feature / Function: `display_language` persists across app restart
- Steps:
  1. Set language to Italian (Italiano)
  2. Quit app fully
  3. Relaunch
- Expected Result:
  - App boots in Italian — MenuBar shows **Lingua**, SettingsDialog title would be **Preferenze**
  - Italian (**Italiano**) is checkmarked in the Language submenu
  - Native menu (macOS) installs in English baseline at startup, then rebuilds to Italian within ~100ms once settings hydrate
- Edge Cases:
  - Manually corrupt the `display_language` field to a garbage value (e.g., open the settings file in your editor, set `"display_language": "klingon"`, relaunch)
  - Expected: app starts in English (normalize-or-fallback policy on the load path), settings file gets rewritten to `"en"` on first save
- Status: Active

## I18N-011 — Update API rejects unknown locale (automated)

- Source Ref: `apply_app_settings_update` (`crates/beambench-service/src/ops/app.rs`); `validate_locale_code` (`crates/beambench-core/src/settings.rs`); reject-on-update policy
- Feature / Function: Update API rejects unknown locale codes
- Verification path: **Automated, not a Computer Use case.** `tauri.conf.json` does not set `withGlobalTauri`, so the `window.__TAURI__` object is not exposed in the dev console; there is no first-class way for an agent to call `invoke('update_app_settings', ...)` from outside the React app code. Use the existing test coverage instead:
  - Rust: `cargo nextest run -p beambench-core settings` exercises `validate_locale_code` against unknown inputs (returns `InvalidLocaleError`).
  - Rust: `cargo nextest run -p beambench-service` exercises `apply_app_settings_update` rejecting an unknown `display_language`.
  - TS: `npx vitest run src/commands/appCommands.test.ts` covers the dispatch path; combined with the Rust suite this gives end-to-end coverage of the reject-on-update policy.
- Expected Result: All three test suites pass; no Computer Use action required for this case
- Status: Active (verified via automated suites in CI)

## I18N-012 — Translation-issue report link (REMOVED)

- Status: Obsolete. The "Report Translation Issue" command and its footer link were removed; translation feedback now flows through the community channel. No verification needed.

## I18N-013 — Language menu list completeness

- Source Ref: `SUPPORTED_LOCALES` (`src/i18n/index.ts`); `LANGUAGE_DISPLAY` (`MenuBar.tsx`)
- Feature / Function: 23 locales present in both React and native menus
- Steps:
  1. Open React Language menu, scroll through all entries
  2. On macOS, open native Language menu, scroll through all entries
- Expected Result: **23 entries** in both menus (excluding the dev-only `en-XA` entry and the footer link):
  - English, Deutsch (German), Español (Spanish), Español, Latinoamérica (Spanish, Latin America), Français (French), Italiano (Italian), Português, Brasil (Portuguese, Brazil), Nederlands (Dutch), Polski (Polish), Čeština (Czech), Svenska (Swedish), Norsk bokmål (Norwegian Bokmål), Dansk (Danish), Suomi (Finnish), Magyar (Hungarian), Türkçe (Turkish), Ελληνικά (Greek), Русский (Russian), Slovenščina (Slovenian), 日本語 (Japanese), 한국어 (Korean), 简体中文 (Simplified Chinese), 繁體中文 (Traditional Chinese)
- Edge Cases: Native menu and React menu must show **identical** ordering and labels
- Status: Active

## Pre-tag release ritual

Before tagging a release build, run the entire I18N-001 through I18N-013 set against an `npx tauri dev` window. Spot-check at least:
- One CJK locale (I18N-003 or I18N-004)
- en-XA (I18N-007)
- German (I18N-006)
- Persistence round-trip (I18N-010)

Document failures by case ID. Any case failing blocks the tag.

## Out of scope

- The ~169-command hotkey registry was the last surface localized; it is now in scope and translated (reuses the menu translation keys). There is no remaining deliberate English-only gap in the UI.
- Pixel-level tofu detection (screenshot diff or font-metric inspection). Current visual cases rely on the verifier's vision to spot hollow boxes; if a CJK font silently regresses on Windows, this checklist catches it only if the verifier looks closely.
