# Security Policy

## Reporting

Do not open a public issue for a suspected vulnerability, malicious fixture, or
confidential document.

Use [GitHub private vulnerability reporting](https://github.com/CasualOffice/opendoc/security/advisories/new)
for this repository. Include:

- affected revision or release;
- affected subsystem and host mode;
- impact and realistic attack path;
- minimal reproduction without confidential content;
- whether active exploitation is known;
- suggested mitigation when available.

The project will acknowledge a complete report, assess severity, coordinate a
fix and advisory, and credit the reporter when requested. Do not publish details
before coordinated disclosure.

## Supported Versions

OpenDoc has no stable release yet. Security fixes currently target `main`.
Supported release lines and end-of-support dates will be listed here before the
first public preview.

## Security Boundaries

- documents, packages, XML, images, fonts, operation logs, and plugin input are
  untrusted;
- network access and external relationship fetching are denied by default;
- normal diagnostics exclude document text and secrets;
- parser and runtime limits are required behavior;
- macros and embedded executable activation are not supported;
- native plugins are trusted code until a sandboxed plugin model is delivered.

See `docs/07-QUALITY-SECURITY-AND-COMPATIBILITY.md` and
`docs/21-PARSER-LIMITS.md` for the current threat and resource policy.
