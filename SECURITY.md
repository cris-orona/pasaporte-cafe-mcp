# Security and privacy

This is an unofficial local stdio client, not a multi-user service. Do not expose it through an unauthenticated gateway or distribute configured binaries with credentials.

## Safe operation

- Store credentials in your MCP client's local environment or credential store. Never commit `.env`, client configuration, session cookies, or raw account output.
- Use the default HTTPS endpoint. An overridden `PASAPORTE_BASE_URL` receives your credentials; only use targets you control or trust.
- Treat tool responses as private. Profile and provider responses can contain personal details; café reports can include contact addresses. Client logs and connected model providers may receive this data.
- Writes are disabled by default. `PASAPORTE_ALLOW_CHECKIN=1` enables check-ins, reviews and combined visits. `PASAPORTE_DISABLE_CHECKIN=1` overrides that opt-in. Neither flag replaces client-side approval of each write.
- Automatic QR lookup reads a public provider field; it is not proof of an on-site scan. Explicit café-location selection submits published café coordinates, not measured device GPS. This client cannot attest physical presence.
- Writes may partially succeed, especially `log_visit`. Do not retry automatically or treat a transport error as proof that nothing happened.
- Do not run authenticated smoke tests in CI. Use synthetic data when reporting bugs.

## Reporting a vulnerability

Do not post credentials, private account data, or exploit details in public issues. Use GitHub's private vulnerability reporting feature if it is enabled for the repository. Otherwise, open a minimal issue requesting a private reporting channel without sensitive details.

If a credential was exposed, rotate it with the provider. Deleting it from the latest source does not remove it from Git history, logs, or existing copies.
