PORT ?= 8091

.PHONY: install build build-web start stop restart status test check

install:
	pnpm install
	pnpm --dir web install

build:
	pnpm build

build-web:
	pnpm build:web

start: build
	@PORT=$(PORT) node dist/cli.js start --data-dir .data

stop:
	@PORT=$(PORT) node dist/cli.js stop --data-dir .data

restart: build
	@PORT=$(PORT) node dist/cli.js restart --data-dir .data

status:
	@PORT=$(PORT) node dist/cli.js status --data-dir .data

test:
	pnpm test

check:
	pnpm check
