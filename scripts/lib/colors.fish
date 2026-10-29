# Shared color output helpers for top-level scripts.
function err
    printf '\033[31m✗ %s\033[0m\n' "$argv" >&2
end
function ok
    printf '\033[32m✓ %s\033[0m\n' "$argv"
end
function info
    printf '\033[34mℹ %s\033[0m\n' "$argv"
end
function warn
    printf '\033[33m⚠ %s\033[0m\n' "$argv"
end
