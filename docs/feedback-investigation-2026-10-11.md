# Beam Bench feedback investigation October 11 2026

This records the initial investigation. Subsequent fixes and the final
disposition of all 16 reports are in [next-release validation](testing/feedback-fixes-2026-10-11.md).
That work proceeds from submitted evidence without contacting reporters.

The 16 reports reviewed in the September 11 through October 11 intake (UTC dates) contain one reproducible failure in the released Linux renderer, a separate GTK startup failure, three requests for machine presets, and several reports that have relevant fixes in newer releases. Fourteen issues remain open and two are marked fixed. These are report counts, not counts of affected people or independent defects.

The first engineering priority is the AppImage's COLRv1 font crash. The shipped WebKit 2.50.4 crashes with the exact assertion in report #54 when rendering a test color font. A plain-text control survives, and the same font probe survives with the container's WebKit 2.52.6. Report #51 has a different failure: GTK cannot initialize before the frontend starts. The shipped launcher forces X11, making the availability of XWayland relevant on Wayland desktops.

## Findings for every report

| Report | Finding and evidence | Disposition |
| --- | --- | --- |
| [54](https://github.com/schmnr/beambench-feedback/issues/54) | Fedora 44, 0.2.26, JPG imports freeze while vector imports work. The supplied `colrv1_configure_skpaint` assertion reproduces with the released WebKit library and a COLRv1 font. Image import, JPEG metadata, and decoder tests pass. | Confirmed released renderer defect. The exact import UI trigger still needs the reporter's launch log and reproduction sequence. Fix the Linux rendering/package compatibility before treating this as a JPEG decoder defect. |
| [51](https://github.com/schmnr/beambench-feedback/issues/51) | Mint 22.3, 0.2.26, two main-thread GTK initialization panics. The same AppImage produces the exact panic without a usable display and reaches `Frontend ready` with Xvfb. Its generated GTK hook unconditionally exports `GDK_BACKEND=x11`. | Confirmed startup failure stage. Missing or inaccessible X11/XWayland is a plausible cause, not a confirmed description of this user's session. Obtain session/display details and review the forced backend. |
| [50](https://github.com/schmnr/beambench-feedback/issues/50) | Windows, 0.2.25, no active machine profile in the bundle. The exact message remains a deliberate precondition in project-profile binding and camera operations. The report captures only `error_toast`, without the triggering action. Profile activation/binding tests and missing-profile guard tests pass. | Setup or action-guidance problem remains possible. No specific broken operation can be established from this report. Capture the action and offer profile setup at the originating control. |
| [49](https://github.com/schmnr/beambench-feedback/issues/49) | Windows, 0.2.25, missing project file, OS error 2. In 0.2.26, `open_project_from_path` identifies a missing file, removes its Recent Projects entry, and returns the localized `project_file_missing` error. The regression test passes. | Relevant fix released in 0.2.26. Ready for an owner retest or closure based on the reproduced missing-file case. This improves error handling; it does not recover a deleted file. |
| [48](https://github.com/schmnr/beambench-feedback/issues/48) | Windows, 0.2.25. Five attempts reach status validation on TCP port 5001, then fail after about one second. Ports 23 and 80 refuse TCP connections, which is a separate outcome. The successful TCP opens rule out a blanket claim that the controller is unreachable. | 0.2.26 adds three-second polling, repeated read-only status queries, and network traffic capture. Existing local changes add CR-only and fragmented-line handling and distinguish silence from unrecognized bytes. Seven TCP tests pass. Exact device compatibility remains unconfirmed because the old report contains no received traffic or controller identity. |
| [47](https://github.com/schmnr/beambench-feedback/issues/47) | Windows, 0.2.24. COM3 appears in discovery, but both opens fail immediately with `Acesso negado` and `serial_port_unavailable`. Neither reaches protocol probing. | Investigate exclusive port ownership or device availability first. The report does not identify which process owns the port. Current detection tests preserve transport errors rather than disguising them as protocol failures. |
| [46](https://github.com/schmnr/beambench-feedback/issues/46) | Windows, 0.1.4. COM4 opens repeatedly; one attempt reaches Ready after a soft reset, while four attempts time out waiting for the banner. The detected USB identity is `303a:4003`, not the CH340 identity in #47. | Retest on 0.2.26. Current discovery first tries a status handshake without a reset and only uses a reset fallback for explicitly selected classic GRBL. The old log proves intermittent communication, not a permanently unsupported device. Hardware confirmation is still needed. |
| [45](https://github.com/schmnr/beambench-feedback/issues/45) | Windows, 0.2.24. Same combined encrypted-PDF/object-stream rejection as #43, minutes later with the same profile and several shared layer IDs. | Likely repeat from the same session. Group with #43 without assuming the files are identical. |
| [44](https://github.com/schmnr/beambench-feedback/issues/44) | Windows, 0.2.24. PDF `Do` operator rejected. Version 0.2.25 adds nested Form XObjects and supported embedded images, with resource scopes, transforms, and clipping. PDF tests pass. | Relevant support released in 0.2.25. Test the original file before declaring resolution; unsupported effects and image formats can still be rejected. |
| [43](https://github.com/schmnr/beambench-feedback/issues/43) | Windows, 0.2.24. The old error combines two causes: encryption and compressed object streams. Version 0.2.25 supports compressed object streams; the regression verifies equivalent classic and compressed encodings. | Resolved capability gap if the file was unencrypted and used object streams. Encryption remains unsupported. No source PDF was attached, so the exact branch cannot be determined. |
| [42](https://github.com/schmnr/beambench-feedback/issues/42) | Sculpfun S30 Pro 10W preset request, sent from 0.2.24. `sculpfun_s30_pro_10w` was added in the 0.2.25 release commit, and its evidence document explicitly names this request. Preset application and generated GRBL-output tests pass. | Implemented and released. Physical operation remains unverified. The preset uses the documented smaller 380 by 385 mm workspace and keeps homing disabled initially. |
| [41](https://github.com/schmnr/beambench-feedback/issues/41) | Windows, 0.2.24. Preview rejects an image with too many separate raster movements. The current limit remains four million burn runs per image. The boundary test accepts exactly four million and rejects 4,000,001. | Expected bounded rejection. Lower DPI, physical size, or fragmentation. The 0.2.25 memory improvements do not remove this limit. The unrelated update-check error in the bundle does not explain the preview rejection. |
| [40](https://github.com/schmnr/beambench-feedback/issues/40) | Linux, 0.2.23. PDF `gs` operator rejected. Version 0.2.24 resolves ordinary opaque graphics states; tests cover direct and inherited resources and retain explicit rejection for unsupported effects. | Already closed and marked fixed. Relevant regression tests pass. |
| [39](https://github.com/schmnr/beambench-feedback/issues/39) | BlazeX M3, macOS, 0.2.22. TCP handshake succeeds and reaches Ready; Home on connect then fails because the generic G-code capability omits homing. Version 0.2.23 corrects the GRBL compatibility capability and failure classification. The OEM compatibility homing test passes. | Already closed and marked fixed. This was a homing capability defect after connection, despite the old static triage calling it a network failure. |
| [52](https://github.com/schmnr/beambench-feedback/issues/52) | Creality Falcon2 Pro 40W preset request, 0.2.26. No connection was attempted in this report, and the catalog has no exact preset. The [manufacturer manual](https://cdn.creality.com/ow/official/ab140496-6ba2-4e3d-95f2-174bdc184b50.pdf) gives a 400 by 415 mm work area for the 40W model and lists LaserGRBL and LightBurn. | Genuine catalog gap. Manufacturer documentation supports further preset work, but the report supplies no firmware settings, power scale, homing configuration, or owner test. Avoid substituting defaults for another Falcon model. |
| [53](https://github.com/schmnr/beambench-feedback/issues/53) | Two Trees TT-5.5S preset request, 0.2.26. No connection attempt or saved profile is included, and the catalog has no exact preset. The [official help center](https://twotrees3d.com/pages/help-center-1) lists the exact model, but its English manual link returns HTTP 404. | Genuine catalog gap. Obtain an accessible model manual and controller settings before adding defaults. TT-5.5S and TTS-55 should not be treated as interchangeable from the similar names alone. |

## Linux renderer reproduction

The downloaded [0.2.26 AppImage](https://github.com/schmnr/beambench/releases/tag/v0.2.26) matches the release's `SHA256SUMS`:

```text
834ada69b7090bbb21c4a7c5280912144e6b3b7045290ee1d7b0b40322b5f5d6
```

The test used an Ubuntu 24.04 amd64 container under Docker's emulation on the ARM Mac, Xvfb, and the released payload extracted with `unsquashfs`. The AppImage's static runtime could not execute under this emulation, so the payload was extracted directly. The released desktop frontend reaches readiness in this environment. This does not reproduce Fedora's entire desktop or the original JPG workflow.

A small PyGObject webview probe loads four color emoji from the [Noto COLRv1 v2.048 font](https://fuchsia.googlesource.com/third_party/github.com/googlefonts/noto-emoji/+/refs/tags/v2.048/fonts/Noto-COLRv1.ttf). Its SHA-256 is `94c6c870925aaafd3fdda338fc4b2e4dfb74260c9aefa87d4aa8a145a993a5a2`. The font is an external test input, not a new application asset.

| Probe | Result |
| --- | --- |
| Shipped WebKit 2.50.4, COLRv1 font | Exact reported Skia vector-index assertion, then `WEBKIT_WEB_PROCESS_CRASHED`. |
| Shipped WebKit 2.50.4, plain ASCII control | Load completes and survives the ten-second observation. |
| Host WebKit 2.52.6, identical COLRv1 probe | Load completes and survives the ten-second observation. |
| Released desktop without a usable display | Exact GTK initialization panic, process exits 101. |
| Released desktop with Xvfb | `Frontend ready`; no GTK initialization panic. |

The shipped WebKit library loads FreeType and Fontconfig from the host. A [merged fix in another Tauri application](https://github.com/unslothai/unsloth/pull/9473) addresses this same bundled-WebKit/host-color-font failure using an AppImage-specific bitmap emoji fallback. It is a possible mitigation to evaluate, not a patch applied here. A library upgrade also needs portability testing across the supported Linux distributions.

The probe interpreter must sit in the extracted payload's `usr/bin` so WebKit resolves its matching subprocess executables. A probe launched from the system Python path initially mixed the system subprocess with the bundled library and crashed even on plain text; that invalid setup is excluded from the evidence below.

Preserved evidence and the probe source are in [testing/feedback-2026-10-11](testing/feedback-2026-10-11/). No sandbox-disabling override was used for the valid probes.

## Initial verification

All targeted checks pass. The saved [validation record](testing/feedback-2026-10-11/validation.json) contains 238 test passes across 16 invocations; some filtered invocations overlap, so this is not a count of unique tests. It includes 51 PDF tests, seven TCP tests, 74 frontend tests, raster planning and the exact complexity boundary, image/JPEG/decoder checks, profile guards and catalog tests, port-error detection, missing-project cleanup, and OEM compatibility homing.

The tests used the current checkout, including the pre-existing network and planner changes. CR-only TCP behavior is still under Unreleased; delayed status retries and network traffic capture shipped in 0.2.26. No application code, release, report status, or correspondence was changed during this initial investigation. Implementation followed in the same checkout; see the next-release record above.

The following recommendations preceded the user's instruction to implement what
the existing evidence supports without reporter correspondence. They are
superseded by the dispositions in the next-release record:

1. Resolve the released COLRv1 renderer crash, then exercise actual JPG import on Fedora with the hostile font installed.
2. Review the forced X11 launcher and test X11, Wayland with XWayland, and Wayland without XWayland. Confirm report #51's launch environment before assigning that specific cause.
3. Get a fresh 0.2.26 report for #48 to distinguish silence, unsupported response syntax, and slow replies. The existing report cannot choose between them.
4. Confirm original PDF files for #43 through #45, retest #46 on the current serial handshake, and identify port ownership for #47.
5. Complete manufacturer evidence and owner testing for the two missing presets. Verify the S30 Pro request against the released preset.
6. Capture the originating action for #50 and improve recovery guidance for setup preconditions and raster complexity limits.

Linux diagnostics need attention too. `logging::install_startup_diagnostics` truncates `diagnostics/startup.log` on each launch, and the feedback bundle does not include that native log. Earlier startup environment and renderer stderr can therefore be lost before a report is submitted. Preserve the previous log and provide a scrubbed attachment path so the next Linux report can establish the session and failure stage.
