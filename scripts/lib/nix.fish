# Auto-enter the nix dev shell when not already inside it.
#
# Source this from a top-level script after computing `_self` (the absolute
# path to that script), then call `_nix_reexec "$_self" $argv` before doing
# any work. Re-execution preserves the script's arguments.
function _nix_reexec --argument-names script_path
    if set -q RECORD_PROCESSOR_DEV_SHELL; and test -n "$RECORD_PROCESSOR_DEV_SHELL"
        return 0
    end
    exec nix develop --command fish "$script_path" $argv[2..-1]
end
