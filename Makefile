.PHONY: lint test smoke build validate help i18n check-i18n check-version

CARGO := cargo
VERSION := $(shell sed -nE 's/^version *= *"([^"]+)".*/\1/p' Cargo.toml | head -1)
PO_FILES := $(wildcard po/*.po)
XGETTEXT_FLAGS := --language=Python --keyword=_ --keyword=N_ --from-code=UTF-8 \
	--add-comments=TRANSLATORS --package-name=ilutex \
	--package-version=$(VERSION) --msgid-bugs-address=https://github.com/Soluciones-Alexendros/ilutex/issues

##@ Calidad

lint: ## fmt, clippy, rustdoc y py_compile
	$(CARGO) fmt --check
	$(CARGO) clippy --all-targets -- -D warnings
	RUSTDOCFLAGS="-D warnings" $(CARGO) doc --no-deps
	python3 -m py_compile gui/*.py

test: ## Tests unitarios (hilos 1: pidfile)
	$(CARGO) test -- --test-threads=1

smoke: ## Binario, GUI y packaging
	bash scripts/smoke.sh

build: ## Binario release (artefacto)
	$(CARGO) build --release

check-version: ## Verifica paridad Cargo npm tag CHANGELOG
	bash scripts/check-version.sh

validate: lint test smoke check-version ## Fachada local = quality + test + smoke + version

i18n: ## Regenera po/ilutex.pot y compila los .mo en gui/locale
	xgettext $(XGETTEXT_FLAGS) --output=po/ilutex.pot gui/*.py
	msgmerge --quiet --update --backup=none po/es.po po/ilutex.pot
	msgmerge --quiet --update --backup=none po/en.po po/ilutex.pot
	@for po in $(PO_FILES); do \
	  lang=$$(basename "$$po" .po); \
	  dst="gui/locale/$$lang/LC_MESSAGES/ilutex.mo"; \
	  mkdir -p "$$(dirname "$$dst")"; \
	  msgfmt -c -o "$$dst" "$$po" && echo "    $$lang -> $$dst"; \
	done

check-i18n: ## Verifica los catálogos y que el .pot esté sincronizado
	@for po in $(PO_FILES); do msgfmt -c -o /dev/null "$$po" && echo "    ok: $$po"; done
	@python3 scripts/check-i18n.py --extract

help: ## Muestra esta ayuda
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-12s\033[0m %s\n", $$1, $$2}'