# Emit the git-tracked fish scripts in this repo, one path per line.
# A fish script is any `*.fish` file, plus any extensionless file whose
# first line is a fish shebang (`#!/...fish`) — e.g. scripts/hooks/pre-commit.
# Reads index blobs (sparse-checkout safe) so no untracked or ignored file
# leaks in.
function _fish_sources
    set -l result
    for f in (git ls-files --cached)
        if string match -qr '\.fish$' -- "$f"
            set result $result "$f"
        else if string match -qr '\.[^/]+$' -- "$f"
            # Has an extension; not a candidate for an extensionless fish script.
            continue
        else
            set -l first (git cat-file -p ":$f" | string collect | string split -n \n)[1]
            if string match -qr '^#!.*\bfish\b' -- "$first"
                set result $result "$f"
            end
        end
    end
    printf '%s\n' $result
end
