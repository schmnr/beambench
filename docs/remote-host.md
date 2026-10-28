# Run a laser from a remote Beam Bench host

The desktop can prepare artwork locally and transfer a complete project to a
stationary Beam Bench host. The host owns the USB controller, planning, preflight
and G-code streaming. Sleeping or closing the laptop, losing Wi-Fi, or closing
its desktop app leaves an accepted host job running. Reconnecting reads the
actual host state. It never sends Start automatically.

Use matching Beam Bench versions on both computers. This workflow currently
connects the host to a USB serial GRBL controller. Other controller families,
remote camera capture, jogging, raw console and macros are not exposed by the
remote desktop panel. Selection origin requires **Cut selected graphics** so the
framed selection and executed job agree.

## Set up a Windows host

Use the standalone `beambench-cli.exe` from the same build as the laptop's
Beam Bench app. From a source checkout, `cargo build --release -p beambench-cli`
produces `target\release\beambench-cli.exe` using the Windows Rust/MSVC toolchain.
Run the following commands in PowerShell from the folder containing that executable.

Export the laser's machine profile as a `.bbprofile` from Device Settings, copy
it to the Windows host, and import it once while the host process is stopped:

```powershell
.\beambench-cli.exe host import-profile "C:\Laser\machine.bbprofile"
```

Generate a private token and start the host:

```powershell
$tokenBytes = New-Object byte[] 32
$tokenGenerator = [Security.Cryptography.RandomNumberGenerator]::Create()
$tokenGenerator.GetBytes($tokenBytes)
$tokenGenerator.Dispose()
$env:BEAMBENCH_API_TOKEN = [BitConverter]::ToString($tokenBytes).Replace("-", "")
.\beambench-cli.exe host serve --network --port 5900
```

Save the generated token in your password manager and enter it in the laptop's
Remote host settings. Reuse that token on later starts. PowerShell's `$env:`
assignment applies to that terminal and its children; it does not persist after
the terminal closes. Scope any Windows Firewall inbound rule for TCP port 5900
to the intended private LAN. Use the host's LAN address in the laptop app,
for example `http://laser-host:5900`.

Select the host's COM port, such as `COM3`, in **Host laser setup**. The port list
comes from the Windows host, independently of the laptop's operating system.
Keep the host terminal open and Windows awake while a job runs. Use Ctrl+C or
Ctrl+Break in the host terminal for a clean shutdown that stops streaming and
disconnects the controller. The host is a console application; Windows service
installation is outside this workflow.

The desktop connection and job controls below are the same on Windows, macOS
and Linux. A laptop disconnection leaves the Windows host's job running.

## Set up the Linux host

Build the CLI on the host (no desktop session or webview is required):

```sh
cargo build --release -p beambench-cli
install -m 755 target/release/beambench-cli ~/.local/bin/beambench
```

Install the distribution's serial/USB runtime dependencies and give the service
user access to the laser's serial port, typically through the `dialout` group.
Log out and back in after changing groups. Prefer `/dev/serial/by-id/` device
names when configuring a persistent USB connection.

On the desktop, export the machine profile as a `.bbprofile` from Device
Settings. Copy that file to the Linux host, then import it **once**, while the
host process is stopped:

```sh
beambench host import-profile /home/laser/machine.bbprofile
```

This preserves the machine-specific bed, origin, power scale and other settings
without importing laptop USB paths. The desktop explicitly chooses a host
profile before transferring a job. Configure and verify the profile for the
physical laser; a laptop project never silently selects a machine by UUID.

For a trusted private LAN:

```sh
export BEAMBENCH_API_TOKEN="$(openssl rand -hex 32)"
# Save this token in a private password manager / service environment file.
beambench host serve --network --port 5900
```

The token must have at least 32 ASCII characters with no whitespace. The server
refuses a network listener without a valid token. Every network request,
including status and emergency stop, requires that token. Browser Origin
requests are rejected. Keep the token private and restrict the port to the
trusted LAN. Plain HTTP does not encrypt the token or project. Use a TLS reverse
proxy or an SSH tunnel on an untrusted network; never expose the API directly to
the public Internet. The desktop validates HTTPS certificates and does not
follow redirects or send credentials through an ambient HTTP proxy.

Alternatively, leave off `--network` and tunnel the loopback listener:

```sh
# Linux host
beambench host serve --port 5900
# Laptop, in a separate terminal
ssh -N -L 5901:127.0.0.1:5900 laser@laser-host
```

Use `http://127.0.0.1:5901` as the desktop address. Loopback mode uses the API's
local Host and browser-Origin guards; SSH supplies authentication and encryption.
The desktop still asks for a 32-character token so the same saved connection can
be used with authenticated network mode later.

The existing desktop application's network API also supports remote connections
when configured with `BEAMBENCH_API_TOKEN`. The headless CLI avoids a dependency
on that host's desktop session remaining open.

## Connect from the laptop

1. Disconnect any laser attached to the laptop. In **Run → Laser**, change
   **Laser connection** to **Remote host**.
2. Enter the host address, for example `http://laser-host:5900`, and its token.
   Choose **Save and connect**. Credentials and the reconnect identity are saved
   in the OS credential store, never in browser storage. A locked or unavailable
   credential store must be unlocked before saving; there is no plaintext fallback.
3. Select the host machine profile, refresh the host USB ports and connect the
   host laser. Confirm motion before homing. Unlock only after checking the
   controller's alarm and the physical machine.
4. Edit artwork, layers and materials as usual on the laptop. Choose the project's
   **Start From** mode and job anchor in the remote panel. For a machine without
   homing switches, use **Current Position** and verify placement with a completed
   laser-off frame. For **User Origin**, use **Set user origin here** to capture
   a fresh position from the host before transferring. Choose **Transfer
   and check project**. The complete portable archive, including embedded image
   assets, is transferred. The compressed archive limit is 64 MiB; the existing
   project reader also bounds expanded content. Later laptop edits do not alter
   the transferred snapshot or a running host job.
5. Open the **host preview** and inspect the host preflight. The host plans with
   its own profile and controller state. Confirm motion, frame with the laser
   off, refresh the host review if preflight requires a completed frame, and
   explicitly confirm laser firing and any advisories before **Start on host**.
6. Pause, resume or stop from the same panel. Stop remains available while a
   transfer, planning request or another command is pending.

After a laptop restart, select Remote host and **Connect to host**. The saved
client identity reclaims the same job and its frozen review. Status updates show
the host's acknowledgement progress, including completion or failure. A network
error displays **Host state unknown** with the last update time; a stale display
is not evidence that the laser stopped. The physical stop remains necessary when
the network is unavailable. Attend the laser throughout the job.

A Start receipt is single-use, even if its HTTP response was lost or the job
already completed. A failed or ambiguous Start requires reading host status and
transferring again for a new run. Reconnect never uploads, starts or resumes a job.

## Ownership and host lifecycle

One desktop claims write control. Conflicting API clients cannot replace its
project or change machine state. The owner survives network loss while a job is
running, paused or preparing. An idle claim can be replaced after 30 seconds
without that client's requests. **Release host controls** releases an idle host
immediately. Any authenticated client can send emergency stop; it bypasses the
ownership gate. The token is the security boundary, so share it only with trusted
operators. Ownership IDs coordinate clients; they are not separate user accounts.

The host process, host computer and USB connection must remain running. On Linux,
use a systemd service for a stationary installation, with the same unprivileged user
and settings directory used for profile import. For example:

```ini
[Unit]
Description=Beam Bench laser host
After=network.target

[Service]
Type=simple
User=laser
SupplementaryGroups=dialout
EnvironmentFile=/home/laser/.config/beambench/host.env
ExecStart=/home/laser/.local/bin/beambench host serve --network --port 5900
Restart=on-failure
TimeoutStopSec=30

[Install]
WantedBy=multi-user.target
```

Create the environment file with user-only permissions (`0600`) and a
`BEAMBENCH_API_TOKEN=...` assignment, then install and enable the service using
the distribution's systemd tools. `BEAMBENCH_CONFIG_DIR` can isolate a host's
settings when needed; set it consistently during import and service startup.

Stopping or restarting the host cancels streaming and disconnects its controller.
A crash, power loss or USB failure cannot guarantee a physical stop. After any
host restart the project, receipt and active job are absent; the operator must
reconnect the laser, transfer and review again, and explicitly start a new job.
There is no automatic crash recovery or replay of motion.
