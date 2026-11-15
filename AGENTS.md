# Repository Guidelines

## Workspace

- `cli/`: `kinetics` package.
- `api/`: shared request/response contracts consumed by the backend.
- `lib/`: runtime helpers and re-exported workload macros.
- `macro/`: attribute macros. They only validate arguments and return the
  function unchanged.
- `parser/`: workload parsing and argument validation.
- `examples/`: deployable example project (`publish = false`).
- `.github/actions/`: shared CI actions. `.github/workflows/`: checks and
  release.

## Validation

Verify changes from the repository root with the following commands:

```sh
cargo fmt --all --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
```

- Documentation-only changes: review the diff; skip Cargo checks.

## Conventions

- Use short imperative commit subjects without prefixes such as `fix:` or
  `feat:`.

## Guardrails

- Never read, log, or commit `.envrc`, `.env`, `.env.secrets`, `local.env`,
  `credentials.json`, or other credential files.
