.PHONY: build vscode

vscode: build
	cd vscode && npm run package

build:
	cd sentry-docs-language-server && cargo build --release
	cp sentry-docs-language-server/target/release/sentry-docs-language-server vscode/server/bin/sentry-docs-language-server-aarch64-apple-darwin

