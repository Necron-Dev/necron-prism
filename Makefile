PYTHON ?= python3
BUILD_SCRIPT := scripts/build.py
BENCHMARK_SCRIPT := scripts/benchmark.py

.PHONY: all build clean help bench bench-mc bench-kernel bench-compare

all: build

build:
	@$(PYTHON) $(BUILD_SCRIPT) build

bench: bench-mc

bench-mc:
	@$(PYTHON) $(BENCHMARK_SCRIPT) --bench mc_bench

bench-kernel:
	@$(PYTHON) $(BENCHMARK_SCRIPT) --bench kernel_bench

bench-compare:
	@$(PYTHON) $(BENCHMARK_SCRIPT) --bench relay_compare_bench

clean:
	@cargo clean

help:
	@echo "Necron-Prism Build Tool"
	@echo "Primary entrypoints:"
	@echo "  python scripts/build.py      - Interactive build wizard"
	@echo "  python scripts/benchmark.py  - Interactive benchmark wizard"
	@echo "Compatibility make targets:"
	@echo "  make build        - Run the default binary build wrapper"
	@echo "  make bench        - Run mc benchmark wrapper"
	@echo "  make bench-mc      - Run Minecraft realistic benchmark"
	@echo "  make bench-kernel  - Run relay kernel benchmark"
	@echo "  make bench-compare - Run relay compare benchmark"
	@echo "  make clean   - Run cargo clean"
