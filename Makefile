.PHONY: ci

PLAYWRIGHT_LIBS ?= /srv/rig/rig/webapp/scripts/playwright-libs/lib

ci:
	@for test in verify.js verify-audio.js verify-nodepool.js verify-reverb.js verify-refinements.js verify-engine.js; do \
		LD_LIBRARY_PATH="$(PLAYWRIGHT_LIBS):$$LD_LIBRARY_PATH" node $$test || exit $$?; \
	done

.PHONY: cogs
cogs:
	@node cogs/build.js
	@export LD_LIBRARY_PATH="$(PLAYWRIGHT_LIBS):$$LD_LIBRARY_PATH"; node cogs/verify.js; phone=$$?; node cogs/verify-desk.js; desk=$$?; exit $$((phone|desk))
