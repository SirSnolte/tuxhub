# Security policy

## Supported versions

Security fixes are provided for the latest release only. tuxhub updates itself,
so installations stay current unless automatic checks are disabled.

## Reporting a vulnerability

Please do not report vulnerabilities in public issues. Use GitHub's private
vulnerability reporting instead:
[Report a vulnerability](https://github.com/SirSnolte/tuxhub/security/advisories/new).

Include the affected version, a description of the problem and, if possible,
steps to reproduce it. Reports are handled confidentially until a fix is released.

## Security model

- Tools installed through tuxhub are regular applications and run with the
  permissions of the current user. Install only tools you trust. Workshop
  entries are reviewed before they are added, and tools are downloaded only
  from their official sources over HTTPS.
- Updates of tuxhub itself are signed. The application verifies the signature
  before installing an update and rejects updates that are not signed with the
  project key.
- Tool interfaces run in sandboxed frames and can only reach their own tool
  process through the bridge.
- The proxy for embedded web interfaces accepts loopback addresses only.
  Sessions for applications shown inside tuxhub listen on `127.0.0.1` only.
- Content from third-party repositories, such as README files, is sanitized
  before it is displayed.
