.PHONY: build vscode

vscode: build
	cd vscode && npm run package

build:
	cd sentry-docs-language-server && cargo build --release

