# Ephemeral CARGO_TARGET_DIR so parallel cargo invocations never contend on
# a shared target directory. Source this, then call `_cargo_env <name>`
# immediately before invoking cargo.
function _cargo_env --argument-names name
    set -g _start_epoch (date +%s)
    set -g _cargo_name "$name"
    set -g _tdir "tmp/targets/$(date +%Y%m%d-%H%M%S)-$fish_pid"
    mkdir -p "$_tdir" tmp/timings

    function _cargo_cleanup --on-event fish_exit
        for _f in "$_tdir"/cargo-timings/cargo-timing*.html
            test -f "$_f"; or continue
            set -l _dest "tmp/timings/$_cargo_name-$(date +%Y%m%d-%H%M%S).html"
            cp "$_f" "$_dest"
            info "Timings saved → $PWD/$_dest"
        end
        rm -rf "$_tdir"
        set -l _elapsed (math (date +%s) - $_start_epoch)
        info "$_cargo_name took $_elapsed"
    end

    set -gx CARGO_TARGET_DIR "$_tdir"
end
