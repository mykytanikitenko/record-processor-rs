{
  description = "record-processor development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      nixpkgs,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
        # cargo-llvm-cov 0.9.0's self-test suite is incompatible with cargo
        # 1.99.x (its fixtures/assertions bitrot), so skip its tests — we only
        # need the working binary, verified at runtime by `just test`.
        cargo-llvm-cov = pkgs.cargo-llvm-cov.overrideAttrs (old: {
          doCheck = false;
        });
        # cargo-rudzio 0.2.0 test runner, built from the fix branch of the
        # rudzio git repo (rev c4bf5a7ea4f1b490e27c3cb1c034505549f28d86). Its
        # self-test suite compiles and passes, but doCheck stays
        # disabled: the suite exercises every pluggable runtime (monoio, smol,
        # async-std, embassy, compio, …) as dev-dependencies, which makes nix
        # sandbox builds impractically slow. The binary is verified at runtime
        # by the migrated rudzio suites.
        cargo-rudzio = pkgs.rustPlatform.buildRustPackage {
          pname = "cargo-rudzio";
          version = "0.2.0";
          src = pkgs.fetchFromGitHub {
            owner = "mykytanikitenko";
            repo = "rudzio";
            rev = "c4bf5a7ea4f1b490e27c3cb1c034505549f28d86";
            hash = "sha256-UbkqyMJozG3gN0QwxWZXUjUBn+Vxe+LwezBVnmd9Dhw=";
          };
          cargoHash = "sha256-k7yMIx5YbFDSfZV6gbkgK/4gnAa1y2PmB+2r9UHUBa0=";
          doCheck = false;
          cargoBuildFlags = [
            "-p"
            "cargo-rudzio"
          ];
        };
      in
      {
        packages.cargo-rudzio = cargo-rudzio;

        devShells.default = pkgs.mkShell {
          buildInputs = [
            # Rust toolchain (managed by rustup, pinned via rust-toolchain.toml)
            pkgs.rustup

            # Task runner
            pkgs.just

            # Shell fundamentals the scripts invoke directly (kept explicit so
            # `nix develop --ignore-environment` stays hermetic).
            pkgs.git
            pkgs.coreutils # rm mkdir cp date mktemp wc sort uniq ln cat basename
            pkgs.gnugrep # grep (clippy ndjson filter)

            # Formatters
            pkgs.taplo # TOML
            pkgs.python3Packages.mdformat # Markdown
            pkgs.nixfmt # Nix
            pkgs.yamlfmt # YAML
            pkgs.yamllint # YAML (lint)

            # Test coverage
            cargo-llvm-cov
            cargo-rudzio

            # LLVM tools for cargo-llvm-cov (rustc 1.99.1 = LLVM 23)
            pkgs.llvmPackages_23.llvm

            # Clippy structured output
            pkgs.jq

            # Panic-pattern linter (structural matching, never regex)
            pkgs.ast-grep # ast-grep CLI
            pkgs.fish # fish + fish_indent

            # Supply-chain checks (advisories, licenses/bans/sources, unused
            # deps) — all from the pinned flake so results are reproducible.
            pkgs.cargo-audit # RustSec advisories
            pkgs.cargo-deny # licenses / bans / sources
            pkgs.cargo-machete # unused dependencies
          ];

          shellHook = ''
            export IN_NIX_SHELL=1
            export RECORD_PROCESSOR_DEV_SHELL=1
            export RUST_BACKTRACE=full
            export LLVM_PROFDATA="${pkgs.llvmPackages_23.llvm}/bin/llvm-profdata"
            export LLVM_COV="${pkgs.llvmPackages_23.llvm}/bin/llvm-cov"
            if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
              if [ "''${RECORD_PROCESSOR_HOOKS:-on}" != "off" ]; then
                git config core.hooksPath scripts/hooks
              fi
            fi
            echo "record-processor — dev shell"
            echo "  just build   — cargo build"
            echo "  just test    — run all tests via the shared cargo-rudzio runner (--html/--threshold add coverage)"
            echo "  just lint    — fmt-check + fish-syntax + clippy + panic + non-fish + yaml-lint + recipe + no-suppressions + no-banner-comments + workspace-deps + non-rudzio + cargo-audit + cargo-deny + cargo-machete"
            echo "  just fmt     — apply all formatters"
            echo "  just clean   — remove build artifacts"
          '';
        };
      }
    );
}
