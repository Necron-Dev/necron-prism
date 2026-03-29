PYTHON ?= python3
BUILD_SCRIPT := scripts/build.py

.PHONY: all build dist clean help

# 默认进入交互模式
all: build

build:
	@$(PYTHON) $(BUILD_SCRIPT)

clean:
	@cargo clean

help:
	@echo "Necron-Prism Build Tool"
	@echo "  make         - Start interactive build wizard"
	@echo "  make build   - Same as above"
	@echo "  make clean   - Run cargo clean"
