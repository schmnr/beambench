# Signed connected-library validation

The Windows and Linux release workflows have an opt-in
`validate_connected_library_only` mode. Set the candidate `tag` and the successful
original `artifact_run_id`. The job verifies that the build source matches the
tag and tests the downloaded package without rebuilding it. Required source CI
remains a separate gate.

Select `library_provider` for the service under test. An owner-approved, expiring
`CRAFTGINEER_PHASE2_TEST_ACCOUNTS` or `PRINTCUTCARVE_DESKTOP_TEST_ACCOUNTS`
repository secret supplies two disposable accounts. The script rejects personal
identities and enforces the selected provider's synthetic IDs, named pilot and
`example.invalid` addresses. PrintCutCarve uses approved temporary database
website sessions, without email or Google sign-in. Hosted consent, PKCE and
desktop token exchange remain real. Its lifecycle also checks that paid
subscriptions suppress the startup ad and a coupon-pack purchase does not.
The project check inserts an original design, saves the project, verifies undo
and redo in the saved geometry, and reopens/saves it with cloud connections
refused through a reserved loopback proxy. The remembered paid-period cache must
keep the ad hidden during that offline restart. The driver then restores normal
network access and verifies that the remembered login still works. Native file
choosers handle all project paths; no app command is injected. Project archives
remain in the disposable runner and are excluded from uploaded evidence.
Provisioning, expiry and cleanup must be arranged before dispatch. Never put
passwords, cookies, token values or authorization URLs in workflow inputs or
committed fixtures. Remove temporary secrets and accounts after validation.

Windows uses native accessibility on the installed Authenticode-signed app.
Linux uses native WebKitWebDriver on the original AppImage, with GNOME Keyring
or KDE Wallet selected by `wallet_provider`. The Docker image is only a test
host. It does not change the Ubuntu 22.04 release builder or bundle any of these
test-host libraries into the AppImage.

The app starts its real PKCE flow. A disposable desktop URL handler receives the
authorization URL, and the test submits the normal hosted credentials/consent
forms and callback. No Tauri login command is mocked and no token is injected.
Login restoration must occur after an actual app restart without another hosted
sign-in. Disconnect must clear the account and operating-system credential.

The Linux full run also locks the actual password service, responds to its
native unlock/deny dialogs, tests session-only recovery, and checks missing-bus
fallback. Evidence reads credential metadata only. Xvfb exercises the real
password services and app, but does not establish native KWin/Wayland or physical
camera/laser coverage. Use a native x86 Linux runner for the release AppImage;
Apple Silicon container emulation does not execute its runtime reliably.

Run the Linux `deletion` scenario only after the other tests using account B
finish. It requires a fresh, guarded disposable-account proof in
`CRAFTGINEER_PHASE2_DELETE_PROOF`. It checks the actual confirmation endpoint,
session revocation, cached native library access, credential cleanup and restart.
It does not request a confirmation email or contact Stripe.

Only the allowlisted result JSON, native startup logs and screenshots are
uploaded. Capture files, cookie jars, profiles and general driver logs are
excluded. Public desktop publication is a separate owner-authorized operation.
