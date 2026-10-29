# Emit the rendered human-readable text for each cargo diagnostic.
# Input: one cargo `--message-format=json` object per line (ndjson).
# Non-diagnostic records (artifacts, build-finished, …) are dropped.
select(.reason == "compiler-message") | .message.rendered
