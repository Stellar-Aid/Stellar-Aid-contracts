# Contributing to StellarAid Contracts

Thanks for your interest in improving the **StellarAid Vault** contract. This guide covers how to set up your environment, the conventions we follow, and what we expect in a pull request. Contributions of all sizes — bug fixes, tests, docs, and features — are welcome.

By participating in this project you agree to uphold our [Code of Conduct](#code-of-conduct).

---

## Table of contents

- [Development environment](#development-environment)
- [Building and testing](#building-and-testing)
- [Coding standards](#coding-standards)
- [Branching model](#branching-model)
- [Commit style](#commit-style)
- [Pull requests](#pull-requests)
- [PR checklist](#pr-checklist)
- [Reporting issues](#reporting-issues)
- [Code of Conduct](#code-of-conduct)

---

## Development environment

You need the Rust toolchain, the Soroban Wasm target, and the Stellar CLI.

### 1. Install Rust (stable)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup update stable
```

On Windows, install via [rustup.rs](https://rustup.rs/) and use PowerShell or Git Bash.

### 2. Add the Wasm target

Soroban contracts compile to WebAssembly:

```bash
rustup target add wasm32-unknown-unknown
```

### 3. Install the Stellar CLI

```bash
cargo install --locked stellar-cli --features opt
stellar --version
```

### 4. Install formatting components

```bash
rustup component add rustfmt
```

### 5. Clone and verify

```bash
git clone https://github.com/Stellar-Aid/Stellar-Aid-contracts.git
cd Stellar-Aid-contracts
cargo test
```

---

## Building and testing

The project mirrors the CI pipeline. Run these locally **before** pushing — CI runs the exact same steps on every push and pull request to `main`.

```bash
# Compile the contract to Wasm
stellar contract build

# Run the full unit-test suite
cargo test

# Verify formatting (must pass with no diff)
cargo fmt -- --check
```

To automatically apply formatting fixes:

```bash
cargo fmt
```

> **Tip:** A green `cargo test` plus a clean `cargo fmt -- --check` means your change will pass CI's build/test/format gates.

---

## Coding standards

- **Format with `rustfmt`.** All code must pass `cargo fmt -- --check`. No exceptions — CI enforces this.
- **Keep `#![no_std]` intact.** The contract compiles without the standard library; do not introduce `std`-only dependencies in `contracts/vault/src/lib.rs`.
- **Prefer checked arithmetic.** Use `checked_add` / `checked_sub` with explicit panics for any value accounting, consistent with the existing code and the `overflow-checks` release profile.
- **Authorize state changes.** Any function that moves funds or changes privileged state must call `require_auth()` on the acting address.
- **Test every behavior you add or change.** New logic needs both happy-path and failure-path (`#[should_panic(expected = "...")]`) coverage in `contracts/vault/src/test.rs`.
- **Document public items** with `///` doc comments, including a `# Panics` section where relevant.

---

## Branching model

- `main` is protected and always releasable. Do not commit directly to it.
- Create a topic branch off `main` using a descriptive prefix:
  - `feat/…` — new functionality
  - `fix/…` — bug fixes
  - `test/…` — test-only changes
  - `docs/…` — documentation
  - `chore/…` — tooling, CI, refactors

Example:

```bash
git checkout -b feat/milestone-rejection-flow
```

---

## Commit style

We follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<optional scope>): <short summary>

<optional body explaining what and why>

<optional footer, e.g. "Closes #123">
```

Common types: `feat`, `fix`, `test`, `docs`, `refactor`, `chore`, `ci`.

Examples:

```
feat(vault): add milestone rejection governance path
fix(vault): prevent double refund by clearing deposit atomically
test(vault): cover quorum boundary for approve_milestone
docs(readme): document testnet deploy playbook
```

Keep the summary in the imperative mood and under ~72 characters.

---

## Pull requests

1. Ensure your branch is rebased on the latest `main`.
2. Confirm the [PR checklist](#pr-checklist) below passes locally.
3. Open a PR against `main` with a clear title (Conventional Commit style) and a description of **what** changed and **why**.
4. Link any related issues (e.g. `Closes #42`).
5. A code owner (see `.github/CODEOWNERS`) will review. Address feedback with additional commits; we squash-merge.

Small, focused PRs are reviewed faster than large ones.

---

## PR checklist

Copy this into your pull request description and check every box. Issue and PR templates reference this list.

- [ ] Branch is named with a `feat/`, `fix/`, `test/`, `docs/`, or `chore/` prefix.
- [ ] `stellar contract build` succeeds.
- [ ] `cargo test` passes locally (all tests green).
- [ ] `cargo fmt -- --check` reports no diff.
- [ ] New/changed logic has both happy-path and `#[should_panic]` failure-path tests.
- [ ] Public functions and types have `///` documentation, with `# Panics` where relevant.
- [ ] Commit messages follow Conventional Commits.
- [ ] No existing/protected files were modified without justification in the PR description.
- [ ] Related issues are linked (`Closes #…`).

---

## Reporting issues

Found a bug or have a feature idea? Open an issue and include:

- A clear title and description.
- Steps to reproduce (for bugs), including the exact `stellar`/`cargo` commands and output.
- Expected vs. actual behavior.
- Environment details (OS, Rust version via `rustc --version`, `stellar --version`).

For security-sensitive reports, please avoid filing a public issue — contact the maintainers privately so the vulnerability can be handled responsibly.

---

## Code of Conduct

We are committed to a welcoming, harassment-free community. All contributors are expected to be respectful and constructive in issues, pull requests, and reviews. Unacceptable behavior may be reported to the maintainers, who will review and act on every report in good faith. By contributing, you agree to these expectations. If a dedicated `CODE_OF_CONDUCT.md` is added to the repository, it supersedes this section.

---

Happy building — and thank you for helping make on-chain aid more transparent.
