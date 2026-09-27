# Development guide

## Repository map

- `src/`: Rust server, persistence, authorization, and filesystem behavior.
- `web/`: dependency-free browser interface embedded into the executable.
- `tests/`: Rust HTTP integration tests and the Playwright browser test.
- `scripts/`: smoke checks, isolated container checks, browser fixture server, and Pi deployment.
- `.github/workflows/ci.yml`: Rust, container, browser, and release-image jobs.
- `docs/`: architecture, API, and development references.

## Documentation style

Use rustdoc (`///` and `//!`) for Rust contracts, invariants, persistence rules, authorization, and non-obvious failure behavior. Use ordinary comments only when they explain why the implementation takes a particular path. Avoid comments that merely repeat a function or variable name.

Use JSDoc comments for browser functions that coordinate state, network requests, dialogs, or media events. Small one-line DOM assignments do not need line-by-line narration. Python scripts should have a module docstring and function docstrings for reusable or safety-critical steps.

Update the Markdown guides when routes, tables, authentication, storage, deployment, or test commands change.

## Generate Rust documentation

```sh
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --document-private-items
```

Open `target/doc/custom_plex/index.html`. `--document-private-items` includes the internal modules because they contain most server behavior. CI runs the same command and fails on broken rustdoc links or warnings.

## Verification

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
npm run test:browser
python3 -m unittest discover -s scripts -p 'test_deploy_pi.py'
```

The browser test requires FFmpeg and a Playwright Chromium installation. It starts an isolated server on port 18084 and deletes its temporary data afterward.

Container checks intentionally mutate their deployment. Run them only with fresh temporary media and data directories, as described in the main README.

## Adding a server feature

1. Put the behavior in the narrowest Rust module; keep `main.rs` limited to process configuration.
2. Use an additive SQLite migration during `App::open_sources` or the owning module's initializer.
3. Define authorization at the handler boundary and again after expensive asynchronous work when visibility could change during processing.
4. Canonicalize filesystem paths and verify they remain within the configured root.
5. Add an HTTP integration test covering authorization, validation, persistence, and restart behavior.
6. Add or extend the Playwright test when browser events, layout, accessibility, or dialogs are involved.
7. Update rustdoc, `docs/API.md`, `docs/ARCHITECTURE.md`, and the README sections affected by the change.

## Adding a web feature

The interface uses plain JavaScript and browser APIs. `web/app.js` owns shared utilities and movies; `web/photos.js` is an IIFE that owns album state but uses the shared `api`, `show`, `askName`, `askDescription`, and `askDelete` helpers.

All text is assigned with `textContent` unless markup is a fixed, developer-authored icon. Every icon-only button needs an `aria-label` and `data-tooltip`. Dialogs must restore focus when they close. State-changing fetches need the custom request header.

Because web files are embedded at compile time, restart native development servers and rebuild Docker images after every web change.

## Deployment safety

The Pi deployer assumes an existing configured installation. It must preserve `.env`, Compose overrides, movie files, photo files, and the database. It never calls `set-password`. Before changing deployment sequencing, update `scripts/test_deploy_pi.py` and keep these invariants:

- validate Docker, architecture, mounts, and photo writes before downtime;
- stop the service before the database backup;
- retain a previous image and print its rollback tag;
- wait for health and attempt recovery after startup failure;
- never silently fall back from external photo storage to the SD card.
