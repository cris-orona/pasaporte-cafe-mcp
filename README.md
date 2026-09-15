# Pasaporte Café MCP

An unofficial Rust [Model Context Protocol](https://modelcontextprotocol.io/) server for **Pasaporte del Café de Especialidad**, the CDMX specialty-coffee passport. Explore cafés, see passport progress, and optionally record visits and reviews from an MCP client.

Uses the service's existing HTTPS endpoints, not an official API. Runs locally over stdio (newline-delimited JSON-RPC); no hosted service or HTTP gateway is included. Not affiliated with or endorsed by Pasaporte del Café de Especialidad.

## Build

Install Rust and Cargo. CI uses Rust 1.98.0.

```sh
cargo build --release --locked
cargo test --locked
```

Binary: `target/release/pasaporte-cafe-mcp`. Tests use synthetic inputs and local subprocesses without provider credentials. They cover configuration, write-tool visibility and enforcement, and basic MCP messages—not live provider behavior.

## Connect an MCP client

Add a stdio server to your client's configuration, replacing the absolute binary path:

```json
{
  "mcpServers": {
    "pasaporte-cafe": {
      "command": "/absolute/path/to/target/release/pasaporte-cafe-mcp"
    }
  }
}
```

Client configuration formats vary. Supply account credentials through your client's local environment or credential store; never commit them. A GUI client may not inherit your shell environment. Restart or reload the client after configuration changes.

| Environment variable | Purpose |
| --- | --- |
| `PASAPORTE_LOGIN` | Account email or username; required for account tools |
| `PASAPORTE_PASSWORD` | Account password; required for account tools |
| `PASAPORTE_ALLOW_CHECKIN` | Explicitly enables **all three write tools** when set to `1`, `true`, or `yes`; disabled by default |
| `PASAPORTE_DISABLE_CHECKIN` | Legacy kill switch: `1`, `true`, or `yes` hides and blocks writes even when allow is set |
| `PASAPORTE_BASE_URL` | Optional trusted endpoint override; default `https://pasaportedelcafedeespecialidad.com` |

Flag values are case-sensitive. Unset, empty, or unrecognized allow values do not enable writes. Setting only `PASAPORTE_DISABLE_CHECKIN=0` no longer enables them.

Compatibility aliases: `PASAPORTE_EMAIL`, `PASAPORTE_USER`, `PASAPORTE_IDENTIFIER` for login; `PASAPORTE_PASS` for password. The first nonempty value wins in the order listed, with the primary name first.

Credentials are sent to the configured host's login endpoint; session cookies stay in memory. Use only a trusted base URL. See [SECURITY.md](SECURITY.md) for privacy and operating limits.

## Tools

Available by default:

| Tool | Purpose |
| --- | --- |
| `whoami` | Profile, level, points and rank; **includes raw private profile data** |
| `my_passport` | Visited/remaining café counts by borough |
| `nearby_cafes` | Nearby cafés for supplied coordinates |
| `cafe_directory` | Café directory, optionally filtered by borough |
| `cafe_detail` | Individual café information |
| `user_leaderboard` | Public user rankings |
| `cafe_leaderboard` | Public café rankings |
| `place_report` | Café activity, scores and contact details |
| `busiest_places` | Cafés ranked by review activity |

Optional, with `PASAPORTE_ALLOW_CHECKIN=1`:

| Tool | Purpose |
| --- | --- |
| `check_in` | Records a visit on your account and the shared leaderboard |
| `submit_review` | Posts four ratings (0–100) and an optional comment; no QR or GPS required by this tool |
| `log_visit` | Check-in followed by a review; **not atomic** |

Disabled write tools are both omitted from `tools/list` and rejected when called directly. Opt-in is permission to expose the tools, not per-call confirmation. Configure your MCP client to ask for approval before each write. Only submit visits and ratings the user actually intends; don't invent reviews or automate bulk activity.

### QR and location behavior

A physical QR is not required by this client. If `qr_token` is omitted, it attempts to resolve the token from the café's public `review_url`. This depends on the provider continuing to expose that field.

For check-ins, supply `lat` and `lng`, or explicitly choose `use_cafe_location=true` to use the café's published coordinates when a complete coordinate pair is absent. **Café coordinates are not measured device GPS or proof that the user is there.** This client does not verify physical presence; provider-side acceptance and restrictions are not guaranteed. QR lookup alone does not submit a visit.

Check-ins and reviews create real account activity. Do not automatically retry them: a timeout may occur after a write succeeds. `log_visit` can record a check-in even if review validation or submission fails; inspect account state before attempting recovery.

## Privacy and limitations

- Keep this server and its credentials local. Do not expose raw responses or write tools to public website visitors.
- Responses may contain email, internal IDs, passport details, precise visit times, QR tokens, and other private data. Review client logging and model-provider data handling before use.
- Provider endpoints and field names may change. Review fields remain provisional; automated tests do not prove live review or check-in acceptance.
- This is a small unofficial client, not a hardened public service. Avoid unnecessary requests and respect the provider's rules.
- No automatic sync, public-card exporter, deployment, or change to an existing local installation is included.

## Development

```sh
cargo fmt --check
cargo check --locked
cargo test --locked
```

Keep tests offline, use synthetic data, and never attach real account responses or credentials to issues or pull requests.

## License

[MIT](LICENSE). The license covers this project's code, not the service's branding, café content, or account data.
