# Build the project
build:
    cargo build

# Run clippy lints
lint:
    cargo clippy -- -D warnings

# Run tests
test:
    cargo test
