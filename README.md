# OCPI learning hub

This project contains a small **local demonstration**, not a production-ready or fully OCPI-compliant hub. It models one eMSP sending a request through a hub to one CPO using OCPI 2.2-shaped discovery and responses.

## Run the demo

You need Rust and Cargo. Start the mock CPO in one terminal:

```sh
cargo run --example mock_cpo
```

Start the hub in another terminal:

```sh
cargo run -- examples/routing-demo.json
```

The mock listens on `127.0.0.1:9001` and the hub on `127.0.0.1:8080`. Stop either process with Ctrl+C. The example configuration and tokens are **public fake values**; never use them as real credentials. The demo only accepts loopback IP addresses and uses HTTP without TLS.

In a third terminal, request the available version:

```sh
TOKEN=$(printf %s 'demo-inbound-token' | base64 | tr -d '\n')
curl -sS -H "Authorization: Token $TOKEN" \
  http://127.0.0.1:8080/ocpi/versions
```

The JSON response contains `status_code: 1000` and a `data` array advertising version `2.2`. Its details are available at:

```sh
curl -sS -H "Authorization: Token $TOKEN" \
  http://127.0.0.1:8080/ocpi/2.2
```

Fetch one location through the hub:

```sh
curl -sS -H "Authorization: Token $TOKEN" \
  -H 'OCPI-from-country-code: NL' \
  -H 'OCPI-from-party-id: MSP' \
  -H 'OCPI-to-country-code: NL' \
  -H 'OCPI-to-party-id: CPO' \
  -H 'X-Request-ID: example-request' \
  -H 'X-Correlation-ID: example-correlation' \
  http://127.0.0.1:8080/ocpi/2.2/locations/LOC1
```

The mock returns an OCPI-shaped envelope with `status_code: 1000` and a `data` object with `id: "LOC1"`. Its Location payload is deliberately minimal, not a complete OCPI Location. The hub verifies the inbound token and routing headers, substitutes the separately configured outbound token, and forwards the request to the configured mock CPO. It preserves the correlation ID and generates a new request ID for the upstream request. If trace IDs are omitted, the hub generates them.

## Code structure

`main` starts the server. A location request then follows this path, and the response returns in the opposite direction:

```text
router → handler → service → CPO client → mock CPO
```

- `src/main.rs` loads the configuration and starts the server.
- `src/router.rs` creates the hub and connects URLs to handlers.
- `src/handler.rs` extracts HTTP requests and builds HTTP responses.
- `src/service.rs` authenticates requests, checks the route, manages trace IDs, and asks the CPO client to forward valid requests. It uses HTTP headers and URIs directly because OCPI is an HTTP protocol.
- `src/client.rs` sends requests to the configured CPO and reads its responses with time and size limits.
- `src/config.rs` validates the demo configuration. `src/db.rs` is a separate SQLite helper and is not used by the hub.

`src/hub.rs` re-exports `Hub`, so both `ocpi::hub::Hub` and `ocpi::router::Hub` work. The lower-level service methods are implementation-oriented and their Rust signatures changed in the architectural refactor; the HTTP endpoints remain the same.

## Configuration and limitations

`examples/routing-demo.json` defines the hub's bind and public URLs, one sender identity and inbound token, one destination identity and outbound token, and the CPO Locations URL. The hub validates the configuration on startup. URLs must use explicit loopback IPs and HTTP; the public URL must match the bind address, and the Locations URL must end in `/`. Keep tokens distinct. No tokens should be considered secret in this example.

The demo supports authenticated version discovery and only `GET /ocpi/2.2/locations/{location_id}` for a single configured route. It does not implement credentials onboarding, other modules, writes, collection queries, pagination, broadcast routing, or general OCPI interoperability. Request queries are rejected. Redirects and environment proxies are disabled; upstream calls have timeouts and a 1 MiB response limit. These restrictions do not make the example suitable for production.

The hub reads its routing configuration from JSON and does **not** use `ocpi.db` or the migration seed credentials. The existing SQLite helper and migrations remain available separately; database tests use isolated in-memory databases.

Run the checks with:

```sh
cargo fmt --check
cargo check --all-targets
cargo test
```
