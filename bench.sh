#!/usr/bin/env bash
cargo b --release || exit $?
perf stat ./target/release/dynasm_experiments interpreter $1 || exit $?
perf stat ./target/release/dynasm_experiments jit $1 || exit $?
