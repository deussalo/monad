.PHONY: ci

PLAYWRIGHT_LIBS ?= /srv/rig/rig/webapp/scripts/playwright-libs/lib

ci:
	LD_LIBRARY_PATH="$(PLAYWRIGHT_LIBS):$$LD_LIBRARY_PATH" node verify.js
