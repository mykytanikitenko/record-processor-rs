#!/usr/bin/env fish
# Streaming CSV input generator for the record-processor engine.
#
# Emits a header row followed by `row_count` transaction rows. Rows stream
# through a small fixed-size buffer so memory stays bounded regardless of the
# target size (gigabyte-scale inputs never materialize in full).
#
# Rows match what crates/domain/src/input/transaction.rs accepts:
#   deposit/withdrawal  -> type,client,tx,amount        (4 columns)
#   dispute/resolve/chargeback -> type,client,tx        (3 columns, no amount)
# Client ids cycle 1..=1000 (u16). Deposit/withdrawal rows carry a globally
# unique, incrementing u32 tx id; dispute/resolve/chargeback rows reference the
# most recent deposit's tx id.
#
# Usage:
#   fish scripts/gen-input.fish <row_count> [output_path]
#   fish scripts/gen-input.fish 100000000 > /tmp/input.csv

set -l row_count $argv[1]
set -l out_path ""

if test (count $argv) -ge 2
    set out_path $argv[2]
end

if test -z "$row_count"
    echo "usage: fish scripts/gen-input.fish <row_count> [output_path]" >&2
    exit 2
end

if not string match -qr '^[0-9]+$' "$row_count"
    echo "error: row_count must be a non-negative integer, got '$row_count'" >&2
    exit 2
end

if test -n "$out_path"
    exec >"$out_path"
end

# Emit the header row.
printf '%s\n' 'type,client,tx,amount'

set -l tx 0
set -l last_deposit_tx 0
set -l i 0
set -l buf
set -l emitted 0
set -l flush_every 4096

while test $i -lt $row_count
    set i (math "$i + 1")

    # Cycling client id, bounded to u16.
    set -l client (math "($i - 1) % 1000 + 1")

    # Deterministic type mix: 60% deposit, 25% withdrawal, 5% dispute,
    # 5% resolve, 5% chargeback.
    set -l r (math "$i % 100")

    if test $r -lt 60
        set tx (math "$tx + 1")
        set -l whole (math "1 + $i % 99999")
        set -l frac (math "$i % 10000")
        set -a buf "deposit,$client,$tx,$whole.$frac"
        set last_deposit_tx $tx
    else if test $r -lt 85
        set tx (math "$tx + 1")
        set -l whole (math "1 + $i % 99999")
        set -l frac (math "$i % 10000")
        set -a buf "withdrawal,$client,$tx,$whole.$frac"
    else if test $r -lt 90
        set -a buf "dispute,$client,$last_deposit_tx"
    else if test $r -lt 95
        set -a buf "resolve,$client,$last_deposit_tx"
    else
        set -a buf "chargeback,$client,$last_deposit_tx"
    end

    set emitted (math "$emitted + 1")
    if test $emitted -ge $flush_every
        printf '%s\n' $buf
        set -e buf
        set emitted 0
    end
end

if test (count $buf) -gt 0
    printf '%s\n' $buf
end
