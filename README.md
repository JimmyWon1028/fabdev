# fabDev

English | [繁體中文](README.zh-TW.md)

fabDev is a cross-platform local environment for ERP Web development. It currently supports macOS 13+ on Apple Silicon ARM64 and Windows 11 on x64. The base App bundles Nginx, PHP 7.4, and PHP 8.2; macOS also bundles dnsmasq. Both platforms support multiple `.test` Sites, independent PHP services selected per Site, and optional PHP 8.4, MariaDB 12.3.2, and Node.js 20/24 runtimes.

## Development Requirements

- Node.js 24 (`/opt/homebrew/opt/node@24/bin` recommended)
- pnpm 11.22+
- Rust stable
- Xcode 26 (full installation, required for the macOS System Helper)

## Development and Validation

```bash
pnpm install
pnpm test
pnpm lint
pnpm dev
pnpm run build:helper:macos
```

Run the Core Agent on unprivileged ports:

```bash
cargo run -p fabdev-agent -- --dns-port 53535 --http-port 8080 --https-port 8443
cargo run -p fabdev-cli -- status
```

When starting Desktop with an isolated data directory, Agent and Desktop must use the same override:

```bash
FABDEV_DATA_DIR=/tmp/fabdev-local-test pnpm dev
```

When Desktop opens, it ensures that the bundled Nginx 1.30.4, PHP 7.4.33, and PHP 8.2.33 runtimes are present; macOS also ensures dnsmasq 2.93 is present. Only missing versions are installed, and existing runtimes, Sites, `php.ini`, and other development data are never overwritten. If the Site Registry is completely empty, fabDev creates the single `demo.test` Site and its own Demo project. If any Site already exists, nothing is added or overwritten. Desktop then starts the bundled Agent of the same version and all development services. “Start services when the App opens” can be disabled in Settings. If every service is already running, fabDev leaves them unchanged; if services are partially running or unhealthy, it cleans them up before restarting. Stale Agent IPC endpoints are safely removed before reconnection. Production data continues to use the existing Application Support directory, and Agent startup logs are stored in `logs/agent-process.log` below that directory. Closing the main window only hides Desktop and does not stop services. Choosing `Quit fabDev` from the menu bar or system tray stops all Web services and MariaDB in order, cleans up managed orphan processes, shuts down Agent, waits for the IPC endpoint to disappear, and then exits Desktop.

Build a local macOS App containing Agent:

```bash
./scripts/run-tauri.sh build --debug --bundles app
codesign --force --deep --sign - target/debug/bundle/macos/fabDev.app
open target/debug/bundle/macos/fabDev.app
```

This creates an ad-hoc signed App for local testing. A future Signed Distribution will still require a Developer ID and notarization.

## Unsigned Community Build

Build the Community DMG without an Apple Developer ID:

```bash
pnpm run build:community:macos
```

The output is written to `artifacts/fabDev-Community-<version>-macos-arm64.dmg` with a matching `.sha256` file. The Community DMG bundles PHP 7.4.33, PHP 8.2.33, Nginx 1.30.4, and dnsmasq 2.93, along with the single `demo.test` example and double-clickable install and removal tools. PHP 8.4.24, MariaDB 12.3.2, and Node.js 20/24 are optional Runtime Packages published independently by [`fabdev-runtimes`](https://github.com/JimmyWon1028/fabdev-runtimes). They must be installed separately from the dashboard and are not included in the App Release. Bundled PHP uses the currently approved settings as the initial `php.ini`, with both `upload_max_filesize` and `post_max_size` set to 64M. On first launch, fabDev generates the correct Runtime, Log, and Session paths for the current user and does not retain absolute paths from the build machine. Local candidate packages and GitHub Actions Release Assets built from a fixed Tag are both subject to release integrity validation.

The Community installer verifies `SHA256SUMS` inside the DMG before requesting administrator access once to install `/Applications/fabDev.app` and the fixed-function LaunchDaemon. Updates preserve Sites, runtimes, and `php.ini`. The removal tool preserves data by default and moves it to Trash only after an additional user confirmation. Full instructions are available in [`distribution/macos/community/INSTALL.zh-TW.md`](distribution/macos/community/INSTALL.zh-TW.md).

Public App downloads are available from [fabdev GitHub Releases](https://github.com/JimmyWon1028/fabdev/releases). `v0.1.22` is currently the Latest Stable and provides the Windows x64 App, macOS ARM64 App, and fabDev Connect. It was published Windows-first and later supplemented with matching macOS assets from the same tag and commit. This order does not make fabDev a Windows-only product: it remains a single cross-platform product. Starting with `v0.1.21`, an App Release contains only the App Installer, fabDev Connect, App Manifest, and SHA-256 files. Runtime Catalogs and Packages are managed independently through [fabdev-runtimes Releases](https://github.com/JimmyWon1028/fabdev-runtimes/releases). See [`docs/PUBLIC_RELEASE_SPEC.md`](docs/PUBLIC_RELEASE_SPEC.md) for Stable versioning, Asset names, Manifests, SHA-256, Draft/Publish, same-version platform supplements, and rollback contracts. `pnpm run release:prepare -- ...` only prepares an existing App installer and generates its Manifest and checksums. It does not package or publish anything and rejects Runtime Package input. `.github/workflows/release-draft.yml` accepts only manual dual confirmation and an existing Tag, and can only create or supplement a Draft. Publishing Stable still requires separate, explicit approval from the Repository Owner.

Public distribution is split across two repositories with independent version lifecycles:

| Repository | Managed content | Version lifecycle |
| --- | --- | --- |
| [`JimmyWon1028/fabdev`](https://github.com/JimmyWon1028/fabdev) | Desktop App, macOS DMG, Windows Setup, fabDev Connect, App Manifest | App SemVer with `v<version>` Tags |
| [`JimmyWon1028/fabdev-runtimes`](https://github.com/JimmyWon1028/fabdev-runtimes) | Runtime Catalog, optional Runtime Packages, and checksums | Monotonically increasing `catalog-vN`, independent of App versions |

The current Runtime Latest is `catalog-v3`. Its `fabdev-runtime-v2.json` has `catalogSequence=3`, requires App `0.1.21` or later and Agent Protocol `37` or later, and lists seven Windows x64 entries and four macOS ARM64 entries. A Catalog update changes only the installable list and does not rebuild every Package. `catalog-v2` removed Node.js 20.20.2; `catalog-v3` restored the same Package URL, size, and SHA-256 from `catalog-v1` without uploading the Package again.

Windows x64 uses a single-file Current User NSIS installer. The full build environment, Runtime/sidecar preparation, Windows 11 acceptance process, and debugging guidance are documented in [`docs/WINDOWS_X64_PACKAGING.md`](docs/WINDOWS_X64_PACKAGING.md).

The Runtime Catalog manages PHP 7.4.33/8.2.33/8.4.24/8.5.10, MariaDB 12.3.2, and Node.js 20.20.2/24.20.0 for Windows x64, plus PHP 8.4.24, MariaDB 12.3.2, and Node.js 20.20.2/24.20.0 for macOS ARM64, as independent optional packages. Windows packages can be created with `./scripts/build-windows-runtime-packages.sh`, which writes paired Release JSON and `.tar.gz` files under `artifacts/windows-x64/runtimes/`. A new or replaced Package must be published in a new `fabdev-runtimes` Release and referenced by the next `catalog-vN`; it must never be included in an App Release. When only the installable list changes, publish a new Catalog without rebuilding unchanged Packages. MariaDB and Node.js sources are verified against both pinned SHA-256 values and upstream signatures with exact allowed fingerprints.

## Runtime Build and Installation

```bash
./scripts/build-php-runtime.sh
PHP_VERSION=7.4.33 ./scripts/build-php-runtime.sh
PHP_VERSION=8.4.24 ./scripts/build-php-runtime.sh
./scripts/build-nginx-runtime.sh
./scripts/build-dnsmasq-runtime.sh
./scripts/build-mariadb-runtime.sh
./scripts/build-node-runtime.sh
./scripts/generate-runtime-catalog.sh

cargo run -p fabdev-cli -- install-runtime \
  artifacts/php-8.2.33-macos-arm64-dev.tar.gz \
  artifacts/php-8.2.33-macos-arm64-dev.json

cargo run -p fabdev-cli -- install-runtime \
  artifacts/php-7.4.33-macos-arm64-dev.tar.gz \
  artifacts/php-7.4.33-macos-arm64-dev.json

cargo run -p fabdev-cli -- install-runtime \
  artifacts/php-8.4.24-macos-arm64-dev.tar.gz \
  artifacts/php-8.4.24-macos-arm64-dev.json

cargo run -p fabdev-cli -- install-runtime \
  artifacts/mariadb-12.3.2-macos-arm64-dev.tar.gz \
  artifacts/mariadb-12.3.2-macos-arm64-dev.json
```

Runtimes are built from official sources and verified against SHA-256 and upstream signatures. Installation verifies SHA-256 again before atomically switching `<runtime>/current`. Unsigned Community builds use SHA-256 for integrity; a future Signed Distribution will add Developer ID, notarization, and a signed Runtime Catalog.

The macOS MariaDB 12.3.2 Runtime is completely isolated from Homebrew. Its configuration, data, PID, Socket, and Logs are stored under the fabDev data directory by default. MariaDB listens only on `127.0.0.1:3306` by default and creates `root` accounts with an empty password during first-time initialization for local PHP development. The MariaDB card in the dashboard accepts a Release JSON file and matching `.tar.gz` package and provides independent install, start, stop, and removal operations. Web Start All and Stop All do not control MariaDB. MariaDB must be stopped before its Runtime can be removed, and removal deletes only the Runtime while preserving configuration, data, and Logs.

The MariaDB page in the sidebar can persistently change the TCP Port, Data Directory, and platform-specific extra options. MariaDB can be started or stopped independently from either the dashboard or menu bar. The last successful start or stop state is stored in `state/mariadb.json`; on the next launch, the App restores MariaDB independently of the Web service auto-start setting. Structured settings are stored in `config/mariadb.json`. Additional macOS options are stored in `config/mariadb/my.cnf`, while Windows uses `config/mariadb/my.ini`; these options take effect the next time MariaDB starts. MariaDB must be stopped before saving, and the installed MariaDB binary validates the settings first. Port, path, Socket/PID, Log, and loopback listener settings remain managed by fabDev. The Data Directory must be either empty or an existing MariaDB data directory; fabDev never moves or deletes old data automatically.

While MariaDB is running, the same page can set the passwords for `root@127.0.0.1` and `root@localhost` together so PHP projects can connect over TCP and Adminer can connect through the localhost Socket. The current password may be left blank when setting a password for the first time; later changes require the current password. fabDev never stores or refills the password and never places it in MariaDB Client command-line arguments.

```bash
cargo run -p fabdev-cli -- install-maria-db-runtime \
  artifacts/mariadb-12.3.2-macos-arm64-dev.tar.gz \
  artifacts/mariadb-12.3.2-macos-arm64-dev.json
cargo run -p fabdev-cli -- start-maria-db
cargo run -p fabdev-cli -- stop-maria-db
cargo run -p fabdev-cli -- remove-maria-db-runtime
```

If Homebrew MariaDB or another process already uses port 3306, fabDev refuses to start its own MariaDB and never stops or takes over the existing service. Stopping fabDev MariaDB releases only its Port, PID, and Socket; the data directory is preserved. Reinstalling the same Runtime after removal can continue using the existing data.

PHP Runtimes are installed under `runtimes/php/<major>.<minor>.<patch>/`. Agent selects the highest installed patch for each Site's chosen minor version and uses `services/php/<major>.<minor>/php-fpm.sock`. If the requested version is not installed, startup fails explicitly and never silently falls back to another PHP version. The macOS PHP 7.4, 8.2, and 8.4 Runtimes include Imagick, IMAP, and Tidy by default. ImageMagick configuration, Coder Modules, and runtime dylibs are bundled with the Runtime.

The Runtimes page in the dashboard reads the actual installation directories. PHP 7.4 and 8.2 are still labeled as bundled, but they can be removed like other versions. Explicit removal of a bundled version leaves a lightweight marker so the App does not restore it on the next launch; reinstalling the matching Runtime Package clears that marker. A PHP version cannot be removed while it is the global version or is still used by any Site. Switch the global version or update affected Sites first. Other PHP versions can be installed by selecting a Runtime descriptor and matching `.tar.gz` package, with platform, architecture, size, and SHA-256 validation. The first installed version becomes global PHP; later installations do not change the global version automatically.

The Sites page can change the PHP minor version for each Site directly, or select `-` to disable PHP for that Site. Static Sites generate only Nginx static-file rules and do not start PHP-FPM. When switching PHP versions, Agent starts the target PHP-FPM first, validates and reloads Nginx, and only then stops old versions that are no longer used. A failure restores the Registry and Site configuration. The `php.ini` button on the Runtimes page edits persistent settings for each minor version under `config/php/<major>.<minor>/php.ini` in the existing Application Support data directory. A `config/php/default/php.ini` template also exists; it is initially created from the current PHP 8.2 settings and is later used only for minor versions that do not yet have their own configuration. It never overwrites an existing configuration. Saving validates settings with the matching PHP-FPM and performs a safe restart; invalid settings never replace the existing file.

The Node.js page, second from the bottom of the sidebar, provides Node.js 20.20.2 and 24.20.0 on Windows x64. Neither version is installed by default, and both can coexist under `runtimes/node/<version>/`. Catalog Packages are checked for platform, architecture, size, SHA-256, and upstream publisher signature. Installation does not change PATH automatically. Only when the user selects “Set Global” does fabDev enable `node`, `npm`, `npx`, and `corepack` shims in the user PATH; the shims dynamically read `current.version`. Versions can be switched without nvm, and fabDev does not modify Homebrew, Herd, the system Node.js, or any existing nvm installation. Node.js 20 is retained only for legacy project compatibility and is marked as EOL in the UI.

HTTPS can be enabled independently for each Site from the Sites page. fabDev creates its own local CA under `config/tls` and generates certificates under `config/tls/sites` with a SAN containing only the target `.test` domain. Private keys remain in the user's fabDev Application Support directory. On first enablement, the fixed-name fabDev CA is added to the current user's Login Keychain trust. Disabling HTTPS removes that Site's certificate and restores HTTP without deleting a CA still used by other Sites. Once enabled, port 80 performs HTTPS redirects only, Nginx listens for user-level TLS on 8443, and the System Helper forwards fixed `443→8443` traffic.

Sites Home defaults to `~/Sites` in the user directory. Every first-level, non-hidden directory becomes a matching `.test` Site automatically; for example, `~/Sites/site1` maps to `site1.test`. The Sites page can select another Home path and rescan it. Existing manually linked Sites are fully preserved, and linked Sites take precedence on domain conflicts. Remove or move the matching folder to remove a Home Site; fabDev never deletes project files. The Sites page can also export and import settings as versioned JSON. Import skips duplicate domains, and project files are never copied or modified.

The same operations can be validated through the Agent CLI:

```bash
cargo run -p fabdev-cli -- runtimes
cargo run -p fabdev-cli -- set-global-php 8.2.33
cargo run -p fabdev-cli -- remove-php-runtime 7.4.33
cargo run -p fabdev-cli -- set-site-php <site-id> 7.4
cargo run -p fabdev-cli -- node-runtime
cargo run -p fabdev-cli -- set-global-node 24.20.0
cargo run -p fabdev-cli -- enable-terminal-node
cargo run -p fabdev-cli -- disable-terminal-node
cargo run -p fabdev-cli -- remove-node-runtime 20.20.2
cargo run -p fabdev-cli -- secure <site-id>
cargo run -p fabdev-cli -- unsecure <site-id>
cargo run -p fabdev-cli -- php-ini 7.4
```

## Proxy Manager

A fresh installation starts with an empty Proxy list and no preloaded Connections. The Proxy page in the sidebar can add and remove user-configured Remote Connections. Every Connection has its own loopback Listener, remote Target, status, and error state. Connections can be started or stopped together, or started, stopped, and restarted independently. A Port conflict or remote disconnection affects only that Connection. Proxy settings can be exported and imported as versioned JSON. Import skips an entry if its ID, `.test` domain, or Listener Port duplicates an existing Connection; imported entries remain stopped by default.

All Listeners bind only to `127.0.0.1` and are not exposed directly to the local network. Explicit start and stop states are stored in fabDev SQLite. Temporary stops during App Quit or Agent upgrade do not overwrite user preferences. If another process occupies a configured Listener Port, the matching Connection reports Failed, and fabDev never terminates or takes over the existing process.

The CLI uses the same Agent Protocol:

```bash
cargo run -p fabdev-cli -- proxies
cargo run -p fabdev-cli -- add-proxy custom --domain custom.test --port 3020 --target http://api.example.com
cargo run -p fabdev-cli -- remove-proxy custom
cargo run -p fabdev-cli -- start-proxy example
cargo run -p fabdev-cli -- stop-proxy example
cargo run -p fabdev-cli -- start-all-proxies
cargo run -p fabdev-cli -- stop-all-proxies
```

## LAN Site Share

To open `http://site-one.test` from another Windows computer or Parallels VM on the same LAN, start the Web services and then select “LAN Share” for each required Site on the Sites page. All selected Sites share an endpoint such as `192.168.1.10:18080`, and Nginx routes requests by the browser's `.test` domain. Stopping one Site does not affect the others; Stop All, Agent Shutdown, and App Quit stop every share.

Run the standalone `fabdev-connect.exe` on Windows, enter the host `IP:Port` shown in the App, enter domains such as `site-one.test, site-two.test` separated by spaces or commas, and select “Connect.” The program requests UAC access, automatically adds and removes its own managed block in the Windows `hosts` file, and listens only on the Client's `127.0.0.1:80`, so no manual hosts-file editing is required. If IIS or another program occupies Client port 80, the program refuses to start and displays an error.

The CLI can also control host sharing:

```bash
cargo run -p fabdev-cli -- share <site-id> --port 18080
cargo run -p fabdev-cli -- unshare <site-id>
cargo run -p fabdev-cli -- lan-share
cargo run -p fabdev-cli -- stop-share
```

This is an unauthenticated, non-TLS development convenience for short browser tests by one or two Clients. It is not suitable for the public Internet, ten-user access, or production ERP. The `fabDev Server` architecture and acceptance goals for production use are documented in section 15 of [`docs/FABDEV_ARCHITECTURE.md`](docs/FABDEV_ARCHITECTURE.md).

On macOS, the System Helper forwards fixed ports 53/80/443 to the unprivileged 53535/8080/8443 ports. Every listener binds only to `127.0.0.1`. The Helper accepts only fixed service controls and does not execute Runtime binaries, certificate operations, or arbitrary commands. It manages only the fixed `/etc/resolver/test` file and refuses to overwrite or remove it if fabDev did not create it. For development testing, run `helpers/macos/.build/debug/fabdev-system-helper --development` to use entry ports 15353/18080/18443 without changing system settings.

To open `http://site1.test` directly in Chrome, install the local test Helper:

```bash
pnpm run build:helper:macos
sudo ./scripts/install-local-test-helper.sh
```

This temporary LaunchDaemon provides only fixed `53→53535`, `80→8080`, and `443→8443` forwarding. It accepts an existing compatible `/etc/resolver/test` but does not modify or take ownership of that file. On removal, it deletes the resolver only if fabDev created it:

```bash
sudo ./scripts/uninstall-local-test-helper.sh
```

The Unsigned Community Build registers a fixed-function LaunchDaemon through an explicit administrator installation flow and does not rely on an Apple-issued Code Signing Identity. A future terminal-free distribution path will still use Developer ID, notarization, and `SMAppService`. Do not manually overwrite Herd settings. See [`docs/FABDEV_ARCHITECTURE.md`](docs/FABDEV_ARCHITECTURE.md) for security boundaries and full decisions, and [`docs/FABDEV_PROGRESS.md`](docs/FABDEV_PROGRESS.md) for current progress.

## Security

Do not disclose credentials, private keys, customer data, internal domains, IP addresses, or vulnerability details in public Issues. Report security concerns through GitHub Private Vulnerability Reporting as described in [`SECURITY.md`](SECURITY.md). Unsigned Community builds have neither an Apple Developer ID signature nor a formal Windows signature; always verify the SHA-256 files included with the Release before installation.

## License

fabDev is dual-licensed under [MIT](LICENSE-MIT) or [Apache License 2.0](LICENSE-APACHE), at your option.
