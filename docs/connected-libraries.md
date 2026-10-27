# Connected libraries

Beam Bench 0.2.34 can connect to Craftgineer and PrintCutCarve. Free and paid
accounts can connect. Access is limited to files available to that account.

## Connect and insert

Open the Library and select Craftgineer or PrintCutCarve. Both tabs remain
visible before connecting. Select Connect and finish sign-in and approval in
the browser. Preferences also has a Connected accounts page. Connection changes
apply immediately; Save applies to other preferences.

Choose Remember if the computer should restore the connection after restart.
Credentials stay in the native process and can be saved in the operating
system's password store. If that store is unavailable, use a session-only
connection. Beam Bench does not receive the website password.

Create or edit Craftgineer files on Craftgineer and save them to its library.
On PrintCutCarve, sign in and save a free sample to My Downloads, buy a pack,
or use the files available through a membership. Refresh the corresponding
Beam Bench Library tab, select a file and insert it into the current project.
Saving a sample does not grant the full pack.

Inserted artwork is embedded in the project and remains usable offline.
Editing the inserted artwork does not update the website library. Disconnecting
or revoking access does not delete artwork already saved in a local project.

## Startup ad

A paid subscription verified through either connected service hides the
startup ad. Free files and individual pack purchases do not. If paid access is
remembered securely, its verified end date bounds offline ad suppression.
Disconnecting, expired access or an unavailable verified entitlement restores
the normal ad behavior.

## Validation

Signed candidate 0.2.34-rc.3 covered account isolation, remembered login,
account switching, disconnect and session-only fallback on macOS, Windows,
GNOME Keyring and KDE Wallet. It also covered insert, undo, project save,
offline reopen and paid/free ad behavior. Those results belong to that exact
candidate, not a later binary.

The final release requires the checks in [Releasing](releasing.md), including
actual Frontend ready from the unmodified signed AppImage on Ubuntu 22.04,
24.04 and 26.04. Retain media, fonts and update-relaunch checks. Native KDE
Wayland and physical laser/camera coverage must be recorded separately.
