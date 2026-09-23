set positional-arguments

# Note: help messages should be 1 line long as required by just.

# Print a help message.
help:
    just --list

# Run `cargo hack --feature-powerset` on crates.
powerset *args:
    NEXTEST_NO_TESTS=pass cargo hack --feature-powerset --workspace "$@"

# Run mutation testing against the library.
mutants *args:
    cargo mutants --package oxide-versioned-envelope "$@" -- --package=integration-tests

# Summarize the library's line and region coverage under the integration tests.
coverage *args:
    cargo llvm-cov nextest --workspace --all-features --summary-only "$@"

# Build docs for crates and direct dependencies.
rustdoc *args:
    @cargo tree --depth 1 -e normal --prefix none --workspace --all-features \
        | gawk '{ gsub(" v", "@", $0); printf("%s\n", $1); }' \
        | xargs printf -- '-p %s\n' \
        | RUSTC_BOOTSTRAP=1 RUSTDOCFLAGS='--cfg=doc_cfg' xargs cargo doc --no-deps --all-features {{args}}

# Generate README.md files using `cargo-sync-rdme`.
generate-readmes:
    cargo sync-rdme --toolchain nightly-2026-04-30 --workspace --all-features

# Run cargo release in CI.
ci-cargo-release:
    # cargo-release requires a release off a branch.
    git checkout -B to-release
    cargo release publish --publish --execute --no-confirm --workspace
    git checkout -
    git branch -D to-release
