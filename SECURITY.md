# Security Policy

Beam Bench controls real hardware, so potential vulnerabilities and safety
bypasses are handled separately from ordinary product bugs.

## Supported versions

Security fixes are made on the latest released version and the current `main`
branch. Older releases may not receive separate patches. Users should update to
the newest available release after a fix is published.

## Reporting a vulnerability

Do not open a public issue for a vulnerability.

Use either of these private channels:

- Submit a
  [private vulnerability report](https://github.com/schmnr/beambench/security/advisories/new).
- Email [security@beambench.com](mailto:security@beambench.com).

Include the affected version or service, steps to reproduce, expected impact,
and the smallest safe proof of concept. Redact credentials, private user data,
diagnostic attachments, and machine-specific secrets.

Relevant reports include:

- Bypasses of motion, laser-output, or raw-command safety checks
- Updater, installer, signature-validation, or release-channel problems
- Code execution, privilege escalation, or unsafe file handling
- Exposure of diagnostic reports, project files, credentials, or private data
- Authentication, authorization, injection, request-forgery, or web-service
  vulnerabilities

## Safe research

Avoid destructive testing, denial of service, social engineering, accessing
other users' data, or testing against laser hardware in a way that could cause
motion, fire, eye injury, or property damage. Use the smallest non-destructive
example that demonstrates the issue.

Beam Bench is a small project and does not currently offer a bug bounty. Reports
will be reviewed and coordinated disclosure will be discussed with the reporter
before public details are published.
