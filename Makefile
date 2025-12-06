.PHONY: all build release test bench clean install

all: release

# リリースビルド（最適化）
release:
	@cargo build --release
	@cp target/release/optica .
	@echo "✅ Built: ./optica ($(shell ls -lh target/release/optica | awk '{print $$5}'))"

# デバッグビルド
build:
	@cargo build

# 最小サイズビルド
small:
	@cargo build --profile release-small
	@cp target/release-small/optica ./optica-small
	@echo "✅ Built: ./optica-small ($(shell ls -lh target/release-small/optica | awk '{print $$5}'))"

# テスト
test:
	@cargo test
	@./target/release/optica solve examples/knapsack.optica -v

# ベンチマーク
bench:
	@./target/release/optica bench 100
	@./target/release/optica bench 500

# インストール
install: release
	@cp target/release/optica /usr/local/bin/
	@echo "✅ Installed: /usr/local/bin/optica"

# クリーン
clean:
	@cargo clean
	@rm -f optica optica-small
	@rm -rf native/*.dylib native/*.so

# ヘルプ
help:
	@echo "Optica - Ultra-fast Optimization DSL (Rust)"
	@echo ""
	@echo "Commands:"
	@echo "  make          Build release binary"
	@echo "  make small    Build size-optimized binary"
	@echo "  make test     Run tests"
	@echo "  make bench    Run benchmarks"
	@echo "  make install  Install to /usr/local/bin"
	@echo "  make clean    Clean build artifacts"
