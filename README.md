# WebAuthn caBLE Authenticator Simulator

This project is an experiment that re-implements Google's **cloud-assisted Bluetooth Low Energy (caBLE)** transport so a local authenticator (either a simulated soft token or a physical security key) can fulfil WebAuthn registration or sign-in requests coming from a remote initiator such as Chrome or Android. The binary drives a Bluetooth controller through BlueZ, advertises the caBLE service data over BLE, and then bridges the FIDO2 CTAP2 traffic through the caBLE WebSocket tunnel until registration completes.

## How It Fits Together
- **FIDO URL ingestion** – `src/main.rs` can accept a `FIDO:/...` URL directly or decode it from a QR code screenshot using `bardecoder` and `image`. This URL seeds the caBLE rendezvous parameters.
- **BLE advertising** – `SerialHciAdvertiser` in `src/main.rs:126` implements the `Advertiser` trait from `webauthn-authenticator-rs`. It uses [`bluer`](https://github.com/bluez/bluer) to publish a broadcast advertisement with the caBLE service UUID (`0000fff9-0000-1000-8000-00805f9b34fb`) and the encrypted tunnel metadata required by the initiator to discover us.
- **Authenticator selection** – When the `softtoken` feature is enabled, the code can open a persisted soft token (`SoftTokenFile`) or create a brand-new in-memory authenticator (`SoftToken`). Otherwise it enumerates real devices via `AnyTransport` and `TokenEvent` (HID, CTAP2 over USB, etc.) and attaches to the first device that becomes available.
- **Hybrid WebSocket handshake** – `share_cable_authenticator` from `webauthn-authenticator-rs` carries out the caBLE handshake. It uses the Bluetooth advertisement to bootstrap an encrypted, tunnelled WebSocket channel to Google's relay (or a custom tunnel when compiled with the `cable-override-tunnel` feature). Once the channel is up, the function forwards CTAP2 commands between the browser and the authenticator, so WebAuthn registration flows execute as if the authenticator were directly paired.

## Prerequisites
- Rust toolchain (Rust 1.82+ to match the Rust 2024 edition declared in `Cargo.toml`).
- A Linux host running BlueZ with `bluetoothd` active — `bluer` talks to the system D-Bus daemon.
- Optional: a serial-attached BLE radio if you plan to revive the HCI code path that currently remains commented out.
- Optional: an existing `softtoken` file created via `cargo run --example softtoken --features softtoken`.

## Running the Simulator
Right now the CLI parsing is stubbed out in `src/main.rs`, so the quickest way to test the flow is to hardcode your rendezvous data and run the binary directly:

1. Replace the placeholder QR code path and `FIDO:/...` URL inside `src/main.rs:299` and `src/main.rs:361` with the values taken from Chrome/Android.
2. Run the binary:

   ```bash
   cargo run
   ```

Once the CLI wiring is restored you will be able to pass `--cable-url`, `--qr-image`, or `--softtoken-path` on the command line as originally intended.

During execution you should see:
1. Bluetooth advertising start, exposing the caBLE payload.
2. The initiator discovering the advert and opening the encrypted WebSocket tunnel.
3. CTAP2 requests proxied to the soft token or physical authenticator until registration completes.

## Development Notes
- `SerialHciAdvertiser::start_advertising` currently uses `tokio::task::block_in_place` to bridge the async `bluer` call with the trait's synchronous signature. Refactoring the `Advertiser` trait to be async would remove this workaround.
- Legacy serial HCI support is left in comments. Reviving it would allow running the stack on microcontrollers that expose a raw HCI UART instead of BlueZ.
- Error handling is minimal; instrumenting the code with `tracing` (already partially scaffolded) would improve observability.

## References
- [FIDO2 / WebAuthn caBLE v2 specification](https://fidoalliance.org/specs/fido-v2.1-rd-20210309/fido-client-to-authenticator-protocol-v2.1-rd-20210309.html#cable)
- [`webauthn-authenticator-rs` crate](https://crates.io/crates/webauthn-authenticator-rs)
- [`bluer` crate](https://crates.io/crates/bluer)
