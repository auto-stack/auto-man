#!/bin/bash
cd crates/auto-man/tests/integration/001_simple_app
rm -rf build .am

# 设置默认 port
echo "port: cmake" > .am/state.at

# 运行 cargo build
cargo run --bin auto-man -- build

# 查看生成的文件
ls -la build/

