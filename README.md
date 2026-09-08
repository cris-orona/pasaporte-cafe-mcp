# Pasaporte Café MCP

An unofficial Rust MCP server for Pasaporte del Café de Especialidad, the CDMX specialty-coffee passport. Uses the service's existing HTTPS endpoints; no official API is available. Speaks JSON-RPC MCP over stdio.

## Build and check

```sh
cargo build --release --locked
cargo test --locked
```

Binary: `target/release/pasaporte-cafe-mcp`. This initial import has no Rust unit tests yet; `cargo test` verifies compilation, not live behavior.

## Configuration

Supply secrets through your local MCP client's environment or credential store. Never commit credentials.

| Environment variable | Purpose |
| --- | --- |
| `PASAPORTE_LOGIN` | Account email or username |
| `PASAPORTE_PASSWORD` | Account password |
| `PASAPORTE_DISABLE_CHECKIN` | `1`, `true`, or `yes` hides and blocks check-in/review write tools; otherwise enabled |
| `PASAPORTE_BASE_URL` | Optional endpoint override; default is `https://pasaportedelcafedeespecialidad.com` |

Credentials are sent to the configured host's login endpoint; cookies stay in memory. Only override the base URL with a trusted target. `PASAPORTE_ALLOW_CHECKIN` is not read by the current implementation.

Example Pi registration (replace the binary path, keep secrets in your environment):

```json
{
  "mcpServers": {
    "pasaporte-cafe": {
      "command": "/absolute/path/to/target/release/pasaporte-cafe-mcp",
      "env": {
        "PASAPORTE_LOGIN": "${PASAPORTE_LOGIN}",
        "PASAPORTE_PASSWORD": "${PASAPORTE_PASSWORD}",
        "PASAPORTE_DISABLE_CHECKIN": "0"
      },
      "approveTools": ["check_in", "submit_review", "log_visit"]
    }
  }
}
```

Reload Pi after adding the registration. Require explicit approval before check-ins. Check-ins need the on-site QR token and your real GPS position; never use fabricated coordinates or register a visit you did not make. This import does not verify a live check-in.

## Tools

- `whoami`: profile, level, points, rank and account details. **Includes raw private profile data.**
- `my_passport`: visited/remaining café counts by borough.
- `nearby_cafes`: nearby cafés for supplied coordinates.
- `cafe_directory`, `cafe_detail`: café directory and individual information.
- `user_leaderboard`, `cafe_leaderboard`: public rankings.
- `place_report`, `busiest_places`: café activity and scores.
- `check_in`: registers a visit; enabled by default, can be disabled as above.
- `submit_review`: posts café ratings; uses the same write-tool enable flag.
- `log_visit`: check-in followed by a review, using the same flag. Not atomic: a review failure can leave the check-in recorded.

## Public website boundary

Do not expose this server, its credentials, or raw responses to website visitors. The personal website currently uses a manually curated snapshot containing only approved level, points, visited/total counts, café names, and observation date. Email, internal IDs, passport numbers, precise visit times, QR tokens, login data and write tools stay private.

No HTTP gateway, public-card exporter, automatic sync, hosted server or deployment is included. The existing local Pi installation can continue using its current binary; this source import does not replace or restart it.

## Limitations

This is an initial import of local source, not a hardened public service. The source includes review tools beyond the current Pi registration's ten-tool allowlist; importing it does not widen that registration. Existing code can resolve QR tokens automatically and offers a `use_cafe_location` option that simulates presence. Do not use simulated presence for real visits or expose these tools publicly. The write tools are not safe for unattended retries; `log_visit` may partially succeed. These inherited behaviors have not been changed or exercised during import. Provider endpoints and check-in field names may change. Avoid unnecessary requests. Do not run authenticated smoke tests in CI or publish profile output. The service and its branding belong to their respective owners; this project is not affiliated with or endorsed by them.
