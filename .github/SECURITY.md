# Security Policy

## Supported versions

Objexel is developed on a single release line. Security fixes land on the latest
release; please upgrade to it before reporting an issue found on an older build.

| Version | Supported          |
| ------- | ------------------ |
| 1.0.x   | :white_check_mark: |
| < 1.0   | :x:                |

## Reporting a vulnerability

Please report security vulnerabilities **privately** — do not open a public
issue, pull request, or discussion for them.

Use GitHub's private vulnerability reporting:

1. Open the repository's **Security** tab → **Advisories** → **Report a
   vulnerability**, or go directly to
   <https://github.com/daygle/Objexel/security/advisories/new>.
2. Include the affected version or commit, how it is deployed (Docker Compose,
   native, or Proxmox), a description of the impact, and clear reproduction
   steps or a proof of concept.

We aim to acknowledge a report within 3 business days and to share an initial
assessment within 7 days, then keep you updated while we investigate and
coordinate a fix and disclosure timeline with you.

## Scope

Objexel is self-hosted and operator-controlled. Reports are most relevant for:

- Authentication, sessions, and CSRF in the API and web dashboard
- Role and permission enforcement on API endpoints
- RTSP/media handling, model download and archive extraction, and file serving
- Notification providers and webhook/MQTT/email delivery
- The published Docker image and deployment configuration

Because Objexel runs on infrastructure you control, please report issues in
Objexel itself rather than in your own environment (for example exposed ports,
weak passwords, or an unpatched host).

## Sensitive data in reports

Never include real credentials, RTSP URLs with embedded passwords, camera
footage, or other secrets in a report. Redact or describe them instead.

## Disclosure

We follow coordinated disclosure. Once a fix is available we publish a GitHub
Security Advisory and a release, crediting the reporter unless anonymity is
requested. Please give us reasonable time to ship a fix before any public
disclosure.
