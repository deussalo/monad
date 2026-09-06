.PHONY: ci

PLAYWRIGHT_LIBS ?= /srv/rig/rig/webapp/scripts/playwright-libs/lib

ci:
	@for test in verify.js verify-audio.js verify-nodepool.js verify-reverb.js verify-refinements.js; do \
		LD_LIBRARY_PATH="$(PLAYWRIGHT_LIBS):$$LD_LIBRARY_PATH" node $$test || exit $$?; \
	done
